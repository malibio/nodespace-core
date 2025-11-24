# Placeholder Node Pattern in BaseNodeViewer

**Component**: `base-node-viewer.svelte`
**Related ADR**: [ADR 023: Eliminate Ephemeral Nodes During Editing](../decisions/023-eliminate-ephemeral-nodes-during-editing.md)
**Issue**: #628 (Svelte 5 Effect Elimination)

## Overview

The placeholder node is a **viewer-local temporary node** that appears when a parent node has no children. It provides users with an immediate place to start typing without requiring explicit "create node" actions.

### Key Principle

**Only ONE ephemeral node exists in the entire system:** The viewer-local placeholder in BaseNodeViewer. This placeholder:
- Never enters SharedNodeStore
- Never persists to the database
- Exists purely in component memory
- Transitions to a real node when user types

All other nodes—even blank nodes created during editing—are real nodes that persist immediately.

## Architecture

### Lifecycle States

```
┌─────────────────────────────────────────────────────────────┐
│ Parent Node Has No Children                                  │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  1. EMPTY STATE                                              │
│     visibleNodesFromStores = []                              │
│     shouldShowPlaceholder = true                             │
│     viewerPlaceholder = { id: <uuid>, content: '', ... }     │
│     nodesToRender = [viewerPlaceholder]                      │
│                                                               │
│     User sees: Empty editable field                          │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  2. USER TYPES (Promotion Trigger)                           │
│     Event: contentChanged with non-empty content             │
│     Action: promotePlaceholderToNode()                       │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  3. PROMOTION (Atomic Transition)                            │
│     isPromoting = true (blocks new placeholders)             │
│     sharedNodeStore.setNode(promotedNode) ← Same ID!         │
│     placeholderId = null (cleanup)                           │
│     tick().then(() => isPromoting = false)                   │
│                                                               │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  4. REAL NODE STATE                                          │
│     visibleNodesFromStores = [promotedNode]                  │
│     shouldShowPlaceholder = false                            │
│     viewerPlaceholder = null                                 │
│     nodesToRender = [promotedNode]                           │
│                                                               │
│     User sees: Same field, now a real persisted node         │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Implementation (Svelte 5 Pattern)

### 1. Stable Placeholder ID

**Pattern**: Lazy initialization via getter function

```svelte
// State: null when no placeholder exists
let placeholderId = $state<string | null>(null);

// Lazy getter - idempotent after first call
function getOrCreatePlaceholderId(): string {
  return placeholderId ??= globalThis.crypto.randomUUID();
}
```

**Why this pattern:**
- ✅ Generates ID only when first needed (lazy)
- ✅ Returns same ID on subsequent calls (stable)
- ✅ Safe to call from `$derived` blocks (idempotent mutation)
- ✅ No `$effect` needed for initialization

**Rationale**: Svelte 5 allows lazy initialization in getters because the mutation is idempotent (happens once, then stable). This differs from reactive mutations that would trigger infinite re-evaluation loops.

### 2. Derived Placeholder Node

**Pattern**: Compute placeholder from reactive condition

```svelte
const viewerPlaceholder = $derived.by<Node | null>(() => {
  if (!shouldShowPlaceholder) {
    return null;
  }

  return {
    id: getOrCreatePlaceholderId(),  // Lazy ID via getter
    nodeType: 'text',
    content: '',
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1,
    properties: {},
    mentions: []
  };
});
```

**Why derived:**
- ✅ Automatically recomputes when `shouldShowPlaceholder` changes
- ✅ No manual coordination needed
- ✅ Returns `null` when not needed (clean reactivity)
- ✅ Lazy ID generation only happens when placeholder is shown

### 3. Placeholder Visibility Condition

**Pattern**: Derive from actual child count

```svelte
const shouldShowPlaceholder = $derived.by(() => {
  if (!nodeId) return false;
  if (isPromoting) return false;  // Block during promotion
  const realChildren = visibleNodesFromStores;
  return realChildren.length === 0;
});
```

**Why this works:**
- ✅ Pure computation from observable state
- ✅ Automatically updates when children are added/removed
- ✅ `isPromoting` flag prevents race conditions
- ✅ No manual toggle needed

### 4. Nodes to Render (The Binding Switch)

**Pattern**: Priority-based rendering decision

```svelte
const nodesToRender = $derived(() => {
  const realChildren = visibleNodesFromStores;

  // Priority 1: Real children (from store/database)
  if (realChildren.length > 0) {
    return realChildren;
  }

  // Priority 2: Viewer placeholder (temporary, in-memory only)
  if (viewerPlaceholder) {
    return [{
      ...viewerPlaceholder,
      depth: 0,
      children: [],
      expanded: false,
      autoFocus: true,  // Auto-focus for immediate typing
      inheritHeaderLevel: 0,
      isPlaceholder: true
    }];
  }

  // Priority 3: Empty
  return [];
});
```

**The Reactive Switch**:
- **Before promotion**: `realChildren.length === 0` → returns `[viewerPlaceholder]`
- **After promotion**: `realChildren.length > 0` → returns `realChildren`
- **Automatic**: No manual coordination - reactivity handles the switch

## Promotion Flow

### Trigger (User Input Event)

Location: `base-node-viewer.svelte:1605-1638`

```svelte
on:contentChanged={(e) => {
  const { content, cursorPosition } = e.detail;

  // Check if this is placeholder promotion
  if (node.isPlaceholder && content.trim() !== '' && nodeId) {
    // ATOMIC PROMOTION: Set flag to block new placeholder creation
    isPromoting = true;

    // Promote placeholder to real node
    const promotedNode = promotePlaceholderToNode(viewerPlaceholder, nodeId, {
      content
    });

    // Set editing state BEFORE store update
    focusManager.setEditingNodeFromTypeConversion(
      promotedNode.id,
      cursorPosition,
      paneId
    );

    // Add to shared store (in-memory only, don't persist yet)
    sharedNodeStore.setNode(promotedNode, { type: 'viewer', viewerId }, true);

    // Clear placeholder ID so fresh one is created if needed later
    placeholderId = null;

    // Clear promotion flag after Svelte's microtask queue flushes
    tick().then(() => {
      isPromoting = false;
    });
  }
}}
```

### Promotion Helper Function

Location: `base-node-viewer.svelte:643-665`

```typescript
function promotePlaceholderToNode(
  placeholder: Node,
  parentNodeId: string,
  overrides: { content?: string; nodeType?: string }
): Node & { parentId?: string } {
  return {
    id: placeholder.id,  // ← CRITICAL: Reuses placeholder ID
    nodeType: overrides.nodeType ?? placeholder.nodeType,
    content: overrides.content ?? placeholder.content,
    version: placeholder.version,
    createdAt: placeholder.createdAt,
    modifiedAt: new Date().toISOString(),
    properties: placeholder.properties,
    mentions: placeholder.mentions || [],
    parentId: parentNodeId  // Transient field for backend edge creation
  };
}
```

**Critical Design Decision**: The promoted node **reuses the placeholder ID**. This ensures:
- Same DOM element transitions from placeholder to real node
- No component remount/unmount
- Focus and cursor position are preserved
- Svelte's keyed rendering ({#each} with `{key: node.id}`) works correctly

## Reactivity Flow

### The Reactive Cascade

When promotion happens, Svelte's reactivity automatically propagates:

```
User types content
  ↓
sharedNodeStore.setNode(promotedNode)
  ↓
visibleNodesFromStores re-evaluates (it's $derived from store)
  ↓
realChildren.length changes: 0 → 1
  ↓
shouldShowPlaceholder re-evaluates: true → false
  ↓
viewerPlaceholder re-evaluates: { ... } → null
  ↓
nodesToRender re-evaluates: [placeholder] → [realNode]
  ↓
Template re-renders with real node instead of placeholder
```

**No manual coordination needed** - the entire state transition is declarative via `$derived` chains.

### ID Cleanup Strategy

**Event-based cleanup only** (no effects needed):

```svelte
// Option 1: Cleanup on promotion (implemented)
function handlePromotion() {
  sharedNodeStore.setNode(promotedNode);
  placeholderId = null;  // Reset for next placeholder cycle
}

// Option 2: Cleanup on hide - NOT RECOMMENDED
// Would require $effect watching shouldShowPlaceholder
```

**Why event-based is better:**
- ✅ Explicit and traceable
- ✅ No effects needed (aligns with Issue #628 goal)
- ✅ Cleanup happens exactly when placeholder lifecycle ends
- ✅ Maintains ID stability if visibility toggles rapidly

## DOM Identity Preservation

### Why Stable IDs Matter

**Problem being solved**: Without stable IDs, rapid placeholder visibility toggles would:
1. Destroy placeholder component
2. Create new placeholder with different ID
3. Lose focus state
4. Reset cursor position
5. Break user experience

**Solution**: Stable `placeholderId` ensures:
- Same ID across visibility toggles
- Component remains mounted
- Focus/cursor preserved
- Smooth UX

### Example Scenario

```
User clicks on empty parent node
  → placeholder appears with ID: abc-123
User clicks elsewhere (focus lost)
  → placeholder hides BUT placeholderId = "abc-123" (kept in memory)
User clicks back on empty parent
  → placeholder reappears with SAME ID: abc-123
  → Same DOM element, no remount
  → Cursor position/state preserved
```

## Testing Considerations

### Key Test Scenarios

1. **Placeholder Creation**
   - Empty parent → placeholder appears
   - Placeholder has stable ID
   - Placeholder auto-focuses

2. **Placeholder Promotion**
   - User types → placeholder becomes real node
   - Same ID preserved (no DOM remount)
   - Node persists to database
   - Focus/cursor maintained

3. **Visibility Toggles**
   - Focus in/out doesn't change ID
   - Component doesn't remount
   - State preserved across toggles

4. **Multiple Promotions**
   - After promotion, new placeholder gets NEW ID
   - Each promotion cycle is independent
   - No ID conflicts

### Test Locations

- Unit tests: `src/tests/integration/blank-node-creation.test.ts`
- Browser tests: `src/tests/browser/focus-management.test.ts`
- Integration: `src/tests/integration/parent-child-edge-creation.test.ts`

## Common Pitfalls

### ❌ Don't: Manually Assign viewerPlaceholder

```svelte
// WRONG - viewerPlaceholder is derived (const)
viewerPlaceholder = { id: newId, ... };  // ❌ Error!
```

The placeholder is automatically computed via `$derived.by`. Let reactivity handle it.

### ❌ Don't: Reset ID on Every Hide

```svelte
// WRONG - requires $effect, violates Issue #628
$effect(() => {
  if (!shouldShowPlaceholder) {
    placeholderId = null;  // ❌ Unnecessary
  }
});
```

ID should only reset when placeholder is promoted, not when visibility toggles.

### ❌ Don't: Mutate State in $derived

```svelte
// WRONG - Svelte 5 prohibits this
const placeholder = $derived.by(() => {
  if (shouldShow) {
    placeholderId = crypto.randomUUID();  // ❌ Direct mutation forbidden
    return { id: placeholderId, ... };
  }
});
```

Use a getter function instead to encapsulate the initialization logic.

### ✅ Do: Use Getter Pattern

```svelte
// CORRECT - Idiomatic Svelte 5
function getOrCreatePlaceholderId(): string {
  return placeholderId ??= crypto.randomUUID();
}

const placeholder = $derived.by(() => {
  if (shouldShow) {
    return { id: getOrCreatePlaceholderId(), ... };  // ✅ Lazy init
  }
});
```

## Performance Considerations

### Minimal Re-evaluation

The derived placeholder only re-evaluates when:
- `shouldShowPlaceholder` changes (child added/removed)
- Component is re-rendered by parent

It does **not** re-evaluate on:
- Content changes in other nodes
- Focus state changes
- Scroll position changes

### Lazy ID Generation

The ID is generated:
- **Once**: When placeholder is first shown
- **Never**: If parent always has children
- **Reset**: Only on promotion (intentional cleanup)

This prevents unnecessary UUID generation.

## Edge Cases

### Multiple Viewers for Same Node

Each BaseNodeViewer instance maintains its **own** `placeholderId`:

```
Tab 1: viewing NodeA → placeholderA (id: abc-123)
Tab 2: viewing NodeA → placeholderB (id: def-456)
```

When user types in Tab 1:
- Tab 1's placeholder promotes to real node
- Tab 2's placeholder **automatically switches** to show the real node (reactive binding)
- Both viewers now show the same real child

### Rapid Promotion

If user types very fast:
1. `isPromoting = true` blocks new placeholder creation
2. Subsequent keystrokes update the promoted node (not placeholder)
3. After `tick()`, `isPromoting = false`
4. No duplicate nodes created

### Component Unmount During Promotion

If component unmounts while promotion is in progress:
1. `isDestroyed = true` flag prevents stale updates
2. `placeholderId` is garbage collected with component
3. No memory leaks

## Related Documentation

- **ADR 023**: High-level decision to eliminate ephemeral nodes
- **Component Architecture Guide**: BaseNodeViewer role and responsibilities
- **Persistence Layer**: How real nodes are persisted vs. placeholder
- **Issue #628**: Svelte 5 effect elimination patterns

## Code References

| Concept | Location | Lines |
|---------|----------|-------|
| Placeholder ID state | base-node-viewer.svelte | 104 |
| Lazy ID getter | base-node-viewer.svelte | 106-110 |
| Derived placeholder | base-node-viewer.svelte | 115-132 |
| Visibility condition | base-node-viewer.svelte | 1360-1366 |
| Nodes to render | base-node-viewer.svelte | 1330-1356 |
| Promotion trigger | base-node-viewer.svelte | 1605-1638 |
| Promotion helper | base-node-viewer.svelte | 643-665 |

## Future Considerations

### Potential Enhancements

1. **Placeholder Templates by Node Type**
   - Custom placeholder content for different parent types
   - e.g., "Add task..." for task parents, "Add note..." for text parents

2. **Placeholder Customization**
   - Allow parent nodes to specify placeholder text
   - Schema-driven placeholder configuration

3. **Multi-Placeholder Support**
   - Multiple placeholders for different sibling positions
   - Would require more complex ID management

These are **not currently implemented** but the architecture supports future extension.
