# NodeSpace Persistence System - Refactoring Plan

**Date:** 2025-11-12
**Status:** Proposal
**Depends On:** persistence-system-review.md

## Overview

This document outlines a **phased refactoring** to simplify the NodeSpace persistence system by eliminating ephemeral node complexity during editing while maintaining the same user experience.

### Core Changes

1. **Eliminate ephemeral during editing** - All new nodes persist immediately (with debounce)
2. **Single source of truth** - One field tracks "ever persisted to database"
3. **Central debounce** - 500ms timer that resets on each change
4. **Clean up complexity** - Remove deferred updates, dependency tracking

---

## Phase 1: Add `databaseId` Field (Identity Separation)

**Goal:** Separate "node exists in database" from "node has pending operation"

**Duration:** 2-3 hours

**Risk:** Low (additive only)

### Changes

#### 1.1 Update Node Interface

**File:** `packages/desktop-app/src/lib/types/index.ts`

```typescript
export interface Node {
  id: string;  // Client-generated UUID (immutable)
  content: string;
  parentId: string | null;
  beforeSiblingId: string | null;
  containerNodeId: string | null;
  nodeType: string;
  properties: Record<string, unknown>;
  mentions: string[];

  // NEW: Database identity (populated after first successful CREATE)
  databaseId?: string | null;  // null = never persisted, string = persisted with this ID

  // Keep for now (will be simplified in Phase 3)
  persistenceState: 'ephemeral' | 'pending' | 'persisting' | 'persisted' | 'failed';

  createdAt: string;
  modifiedAt: string;
  version: number;
}
```

**Why `databaseId`?**

- Clear semantic: If `databaseId` exists, node was persisted
- Immutable once set: Never goes back to null
- Orthogonal to `persistenceState`: Can have `databaseId` while `persistenceState === 'pending'`

#### 1.2 Update CREATE Logic

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**BEFORE** (lines 645-658):

```typescript
const wasPersistedBefore =
  currentNode?.persistenceState === 'persisted' ||
  (currentNode?.createdAt !== undefined && currentNode.createdAt !== null);
const isPersistedToDatabase = wasPersistedBefore;
```

**AFTER:**

```typescript
// Simple, reliable check
const isPersistedToDatabase = currentNode?.databaseId !== undefined && currentNode.databaseId !== null;
```

#### 1.3 Set databaseId on Successful CREATE

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**Location:** After `tauriNodeService.createNode()` succeeds

```typescript
// Line ~697 (after CREATE success)
await tauriNodeService.createNode(updatedNode);

// NEW: Mark that node exists in database
const nodeInStore = this.nodes.get(nodeId);
if (nodeInStore) {
  nodeInStore.databaseId = nodeId;  // Use same ID (UUID-based)
  this.nodes.set(nodeId, nodeInStore);
}

this.updatePersistenceState(nodeId, 'persisted');
```

#### 1.4 Set databaseId on Database Load

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**Location:** `loadChildrenForParent()` method (lines 1090-1110)

```typescript
const nodes = await tauriNodeService.getNodesByContainerId(parentId);

for (const node of nodes) {
  // NEW: Nodes from database always have databaseId
  if (!node.databaseId) {
    node.databaseId = node.id;  // Ensure field is set
  }

  this.setNode(node, databaseSource, true); // skipPersistence = true
}
```

### Acceptance Criteria

- [ ] `databaseId` field added to Node interface
- [ ] `databaseId` set on successful CREATE
- [ ] `databaseId` set when loading from database
- [ ] CREATE vs UPDATE decision uses `databaseId` check
- [ ] All existing tests still pass (field is optional)
- [ ] No changes to user-facing behavior

### Rollback Strategy

If this phase causes issues:
1. Remove `databaseId` field from Node interface
2. Revert to `createdAt`-based checks
3. This phase is purely additive - no breaking changes

---

## Phase 2: Simplify CREATE vs UPDATE Decision

**Goal:** Use `databaseId` as single source of truth for CREATE vs UPDATE

**Duration:** 2-3 hours

**Risk:** Medium (changes core persistence logic)

### Changes

#### 2.1 Simplify Decision Logic

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**BEFORE** (lines 645-694):

```typescript
// DIAGNOSTIC #450: Log decision-making for CREATE vs UPDATE
console.log(
  `[DIAGNOSTIC #450 processPendingUpdate] Node ${nodeId.slice(0, 8)} - persistenceState: ${currentNode?.persistenceState}, wasPersistedBefore: ${wasPersistedBefore}, createdAt: ${currentNode?.createdAt}, will use: ${isPersistedToDatabase ? 'UPDATE' : 'CREATE'}`
);

// Update persistence state to 'persisting'
this.updatePersistenceState(nodeId, 'persisting');

if (isPersistedToDatabase) {
  // IMPORTANT: For UPDATE, only send the changes (Partial<Node>), not the full node
  const updatePayload = { ...changes };
  const currentVersion = currentNode?.version ?? 1;

  try {
    await tauriNodeService.updateNode(nodeId, currentVersion, updatePayload);
    this.updatePersistenceState(nodeId, 'persisted');
  } catch (updateError) {
    // If UPDATE fails because node doesn't exist, try CREATE instead
    if (
      updateError instanceof Error &&
      (updateError.message.includes('NodeNotFound') ||
        updateError.message.includes('does not exist'))
    ) {
      console.warn(
        `[SharedNodeStore] Node ${nodeId} not found in database, creating instead of updating`
      );
      await tauriNodeService.createNode(updatedNode);
      this.updatePersistenceState(nodeId, 'persisted');
    } else {
      this.updatePersistenceState(nodeId, 'failed');
      throw updateError;
    }
  }
} else {
  await tauriNodeService.createNode(updatedNode);
  this.updatePersistenceState(nodeId, 'persisted');
  this.updateDeferredSiblingReferences(nodeId);
}
```

**AFTER:**

```typescript
this.updatePersistenceState(nodeId, 'persisting');

// Simple, reliable decision
const hasDatabaseId = currentNode?.databaseId !== undefined && currentNode.databaseId !== null;

if (hasDatabaseId) {
  // Node exists in database → UPDATE
  const updatePayload = { ...changes };
  const currentVersion = currentNode?.version ?? 1;

  await tauriNodeService.updateNode(nodeId, currentVersion, updatePayload);
  this.updatePersistenceState(nodeId, 'persisted');

} else {
  // Node doesn't exist in database → CREATE
  await tauriNodeService.createNode(updatedNode);

  // Set databaseId to mark as persisted
  currentNode.databaseId = nodeId;
  this.nodes.set(nodeId, currentNode);

  this.updatePersistenceState(nodeId, 'persisted');
}
```

**Key Changes:**

1. Remove complex logic checking `createdAt`, `persistenceState`, etc.
2. Single check: Does node have `databaseId`?
3. Remove fallback CREATE logic (no longer needed - decision is reliable)
4. Set `databaseId` after successful CREATE

#### 2.2 Remove Diagnostic Logging

Remove all `[DIAGNOSTIC #450]` logs - no longer needed with simple decision logic

### Acceptance Criteria

- [ ] CREATE vs UPDATE decision uses only `databaseId` check
- [ ] No fallback logic needed (decision is reliable)
- [ ] All diagnostic logging removed
- [ ] All existing tests still pass
- [ ] Manual testing: Create node → update node → verify UPDATE used
- [ ] Manual testing: Create node → reload page → update → verify UPDATE used

### Rollback Strategy

If this phase causes issues:
1. Revert decision logic to use `createdAt` checks
2. Keep `databaseId` field (Phase 1 is still valuable)
3. Investigate why `databaseId` is unreliable

---

## Phase 3: Eliminate Ephemeral During Editing

**Goal:** New blank nodes persist immediately instead of staying ephemeral

**Duration:** 4-6 hours

**Risk:** High (changes core creation behavior)

### Changes

#### 3.1 Update Node Creation

**File:** `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`

**BEFORE** (lines 445-454):

```typescript
const isPlaceholder = initialContent.trim() === '' || /^#{1,6}\s*$/.test(initialContent.trim());

// Skip persistence for placeholder nodes
const skipPersistence = isPlaceholder;
sharedNodeStore.setNode(newNode, viewerSource, skipPersistence);
_uiState[nodeId] = newUIState;
```

**AFTER:**

```typescript
const isEmptyOnInitialLoad = initialContent.trim() === '' || /^#{1,6}\s*$/.test(initialContent.trim());

// NEW: Only skip persistence if this is the INITIAL placeholder on viewer load
// After that, all nodes (even empty) persist immediately
const isInitialPlaceholder = isEmptyOnInitialLoad && /* TODO: detect initial load */;

const skipPersistence = isInitialPlaceholder;
sharedNodeStore.setNode(newNode, viewerSource, skipPersistence);
_uiState[nodeId] = newUIState;
```

**Implementation Note:**

Need to detect "initial load with no children" case. Options:

1. Pass flag from BaseNodeViewer when creating initial placeholder
2. Check if parent has zero children
3. Add `isInitialPlaceholder` parameter to `createNode()`

**Recommended:** Add parameter to `createNode()` for clarity

```typescript
function createNode(
  afterNodeId: string,
  content: string = '',
  nodeType: string = 'text',
  headerLevel?: number,
  insertAtBeginning?: boolean,
  originalNodeContent?: string,
  focusNewNode?: boolean,
  paneId: string = DEFAULT_PANE_ID,
  isInitialPlaceholder: boolean = false  // NEW: Only skip persistence for initial placeholder
): string {
  // ... existing logic ...

  const skipPersistence = isInitialPlaceholder;
  sharedNodeStore.setNode(newNode, viewerSource, skipPersistence);

  // ...
}
```

#### 3.2 Update BaseNodeViewer

**File:** `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte`

**Location:** Where initial placeholder is created

```typescript
// Create initial placeholder ONLY if no children loaded
const children = await loadChildrenForParent(parentId);

if (children.length === 0) {
  // This is the ONE case where we use ephemeral placeholder
  const placeholderId = service.createNode(
    parentNodeId,
    '',
    'text',
    undefined,
    false,
    undefined,
    false,
    paneId,
    true  // NEW: Mark as initial placeholder
  );
}
```

#### 3.3 Update Placeholder Detection

**File:** `packages/desktop-app/src/lib/utils/placeholder-detection.ts`

**Logic stays same** - but semantic changes:

- Placeholder detection still needed for UI (show empty state styling)
- But placeholder nodes are now persisted (not ephemeral)

**No code changes needed** - just semantic difference

#### 3.4 Update Persistence State Logic

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**BEFORE** (lines 779-806):

```typescript
// CRITICAL: Preserve existing persistence state for nodes not yet persisted
const existingState = existingNode?.persistenceState;
const shouldPreserveState = existingState === 'ephemeral' || existingState === 'pending';

if (shouldPreserveState && !isNewNode) {
  // Don't change the persistence state - it's still waiting to be persisted
} else if (shouldMarkAsPersisted) {
  // Node loaded from database - but placeholders stay ephemeral
  if (isPlaceholder) {
    this.updatePersistenceState(node.id, 'ephemeral');
  } else {
    this.updatePersistenceState(node.id, 'persisted');
  }
} else if (isNewNode && !skipPersistence) {
  if (isPlaceholder) {
    this.updatePersistenceState(node.id, 'ephemeral');
  } else {
    this.updatePersistenceState(node.id, 'pending');
  }
}
```

**AFTER:**

```typescript
// Simplified: No special case for placeholders during editing

if (shouldMarkAsPersisted) {
  this.updatePersistenceState(node.id, 'persisted');
} else if (isNewNode && !skipPersistence) {
  this.updatePersistenceState(node.id, 'pending');
}

// Only initial placeholder is ephemeral (handled by skipPersistence flag)
```

### Acceptance Criteria

- [ ] New blank nodes created with `persistenceState: 'pending'` (not ephemeral)
- [ ] Initial placeholder on empty viewer still ephemeral
- [ ] All nodes (even empty) persist to database after debounce
- [ ] Backend allows empty text nodes (already does)
- [ ] No more `'ephemeral'` state after initial load
- [ ] All existing tests updated to reflect new behavior

### Rollback Strategy

If this phase causes issues:
1. Revert node creation to use `skipPersistence` for empty nodes
2. Keep Phases 1-2 (they're still valuable)
3. Document why ephemeral during editing is required

---

## Phase 4: Remove Deferred Update Queue

**Goal:** Eliminate complexity now that all nodes persist immediately

**Duration:** 3-4 hours

**Risk:** Low (removing unused code)

### Changes

#### 4.1 Remove Deferred Update Types and Maps

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**Remove:** Lines 91-97 (DeferredUpdate interface)
**Remove:** Lines 149-151 (deferredUpdates Map)

```typescript
// DELETE THESE
interface DeferredUpdate {
  nodeId: string;
  changes: Partial<Node>;
  source: UpdateSource;
  waitingFor: string;
}

private deferredUpdates = new Map<string, DeferredUpdate[]>();
```

#### 4.2 Remove Deferred Update Methods

**Remove:** Lines 238-264 (`addDeferredUpdate`, `processDeferredUpdates`)
**Remove:** Lines 269-292 (`updatePersistenceState` deferred update trigger)

```typescript
// DELETE THESE METHODS
private addDeferredUpdate(update: DeferredUpdate): void { ... }
private processDeferredUpdates(nodeId: string): void { ... }
```

#### 4.3 Remove Deferred Update Logic from updateNode

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**BEFORE** (lines 491-537):

```typescript
// Check if beforeSiblingId reference ephemeral placeholders
if ('beforeSiblingId' in changes && changes.beforeSiblingId) {
  const siblingNode = this.nodes.get(changes.beforeSiblingId);
  if (siblingNode?.persistenceState === 'ephemeral') {
    this.addDeferredUpdate({
      nodeId,
      changes,
      source,
      waitingFor: changes.beforeSiblingId
    });
    // Still update in-memory for immediate UI feedback
    this.nodes.set(nodeId, updatedNode);
    this.notifySubscribers(nodeId, updatedNode, source);
    return; // Skip persistence for now
  }
}

// Check parentId
if ('parentId' in changes && changes.parentId) {
  const parentNode = this.nodes.get(changes.parentId);
  if (parentNode?.persistenceState === 'ephemeral') {
    this.addDeferredUpdate({
      nodeId,
      changes,
      source,
      waitingFor: changes.parentId
    });
    this.nodes.set(nodeId, updatedNode);
    this.notifySubscribers(nodeId, updatedNode, source);
    return; // Skip persistence for now
  }
}
```

**AFTER:**

```typescript
// DELETE ALL OF THIS - no longer needed
// All nodes persist immediately, so references are always valid
```

#### 4.4 Remove Dependency Blocking Logic

**File:** `packages/desktop-app/src/lib/services/persistence-coordinator.svelte.ts`

**Remove:** Ephemeral blocking logic (lines 308-327)

```typescript
// DELETE THIS
for (const dep of op.dependencies) {
  if (typeof dep === 'string') {
    const depOp = this.operations.get(dep);
    if (depOp && (depOp.status === 'pending' || depOp.status === 'in-progress')) {
      blockingDeps.add(dep);
    } else if (this.sharedNodeStore) {
      const depNode = this.sharedNodeStore.getNode(dep);
      if (depNode && depNode.persistenceState === 'ephemeral') {
        console.warn(`[DIAGNOSTIC #450] Blocking operation...`);
        blockingDeps.add(dep);
      }
    }
  }
}
```

**AFTER:**

```typescript
// Simplified: Only check for pending operations, not ephemeral state
for (const dep of op.dependencies) {
  if (typeof dep === 'string') {
    const depOp = this.operations.get(dep);
    if (depOp && (depOp.status === 'pending' || depOp.status === 'in-progress')) {
      blockingDeps.add(dep);
    }
  }
}
```

### Acceptance Criteria

- [ ] All deferred update code removed
- [ ] No references to `deferredUpdates` Map
- [ ] No ephemeral blocking in PersistenceCoordinator
- [ ] All tests updated to remove deferred update expectations
- [ ] All existing functionality works without deferred updates

### Rollback Strategy

If this phase causes issues:
1. Revert removal of deferred update code
2. Investigate what scenario requires deferred updates
3. Document why they're needed

---

## Phase 5: Central Debounce Timer

**Goal:** Implement 500ms debounce that resets on each change

**Duration:** 2-3 hours

**Risk:** Low (already have debouncing, just centralizing)

### Changes

#### 5.1 Update PersistenceCoordinator Debouncing

**File:** `packages/desktop-app/src/lib/services/persistence-coordinator.svelte.ts`

**Current:** 500ms default (line 166)
**Behavior:** Timer does NOT reset on new changes (new operation cancels old)

**Desired:** Timer RESETS on new changes for same node

**BEFORE** (lines 271-277):

```typescript
persist(nodeId: string, operation: () => Promise<void>, options: PersistenceOptions = {}): PersistenceHandle {
  // Cancel any existing pending operation for this node
  this.cancelPending(nodeId);

  // Create new operation with new timer
  // ...
}
```

**AFTER:**

```typescript
persist(nodeId: string, operation: () => Promise<void>, options: PersistenceOptions = {}): PersistenceHandle {
  const existingOp = this.operations.get(nodeId);

  if (existingOp && existingOp.status === 'pending' && existingOp.timer) {
    // RESET timer instead of cancelling
    clearTimeout(existingOp.timer);

    // Update operation callback
    existingOp.operation = operation;
    existingOp.options = options;

    // Restart timer with fresh 500ms (or configured debounceMs)
    const debounceMs = options.debounceMs || this.defaultDebounceMs;
    const mode = options.mode || 'debounce';

    if (mode === 'debounce') {
      existingOp.timer = setTimeout(() => {
        this.executeOperation(existingOp);
      }, debounceMs);
    }

    return {
      nodeId,
      promise: existingOp.promise,
      isPersisted: () => this.isPersisted(nodeId)
    };
  }

  // No existing operation, create new one (existing logic)
  // ...
}
```

**Key Change:** Instead of cancelling and creating new operation, RESET the timer on existing operation

### Acceptance Criteria

- [ ] Rapid changes to same node reset debounce timer
- [ ] Example: indent then outdent within 500ms → only ONE update fires
- [ ] Timer behavior matches user's requirement
- [ ] No spam of updates for rapid operations

### Rollback Strategy

If this phase causes issues:
1. Revert to cancel-and-create-new behavior
2. Investigate what scenario requires new operation instead of timer reset

---

## Phase 6: Simplify persistenceState Field

**Goal:** Remove `'ephemeral'` state entirely (except initial placeholder)

**Duration:** 2-3 hours

**Risk:** Low (mostly cleanup)

### Changes

#### 6.1 Update PersistenceState Type

**File:** `packages/desktop-app/src/lib/types/index.ts`

**BEFORE:**

```typescript
export type PersistenceState =
  | 'ephemeral'    // Exists in UI only, never been persisted (placeholders)
  | 'pending'      // Queued for persistence
  | 'persisting'   // Currently being written to database
  | 'persisted'    // Successfully saved to database
  | 'failed';      // Persistence attempt failed
```

**AFTER:**

```typescript
// Simplified: Only track operation state (identity tracked by databaseId)
export type PersistenceState =
  | 'pending'      // Queued for persistence
  | 'persisting'   // Currently being written to database
  | 'persisted'    // Successfully saved to database
  | 'failed';      // Persistence attempt failed

// Special case: Initial placeholder on empty viewer load
// Can still use 'ephemeral' OR create separate flag
```

**Option A:** Keep `'ephemeral'` for initial placeholder only
**Option B:** Add separate `isInitialPlaceholder` boolean field

**Recommended:** Option A (minimal changes)

#### 6.2 Update State Transition Logic

**All state transitions should be:**

```
Initial Placeholder: ephemeral (transient)
All Other Nodes:     pending → persisting → persisted
                                  ↓
                                failed
```

**Key:** `'ephemeral'` only appears on initial placeholder, never during editing

### Acceptance Criteria

- [ ] `'ephemeral'` state only used for initial placeholder
- [ ] All other nodes use `'pending'` as initial state
- [ ] State transitions simplified
- [ ] No confusion between identity and operation state

### Rollback Strategy

N/A - This is final cleanup after Phases 1-5 succeed

---

## Phase 7: Remove Obsolete Code and Comments

**Goal:** Clean up legacy code, comments, and complexity

**Duration:** 2-3 hours

**Risk:** Low (cleanup only)

### Changes

#### 7.1 Remove `everPersisted` Map

**File:** `packages/desktop-app/src/lib/services/shared-node-store.ts`

**Remove:** Lines 136-140

```typescript
// DELETE THIS
// Track which nodes have ever been successfully persisted to database
// CRITICAL #450: This solves the CREATE vs UPDATE decision bug
private everPersisted = new Map<string, boolean>();
```

**Why:** Now redundant with `databaseId` field

#### 7.2 Remove Diagnostic Logging

**Remove all:**
- `[DIAGNOSTIC #450]` logs
- `console.warn()` for ephemeral blocking
- Complex state transition logs

**Keep:**
- Error logs
- Important state transitions

#### 7.3 Remove Obsolete Documentation Comments

**Update:**
- Remove "CRITICAL" and "IMPORTANT" comments that reference old patterns
- Update comments to reflect new simplified architecture
- Add links to this refactoring plan

#### 7.4 Remove Unused Imports and Types

**Check for:**
- Unused `DeferredUpdate` type references
- Unused `isPlaceholderNode` imports (if no longer needed)
- Obsolete test utilities

### Acceptance Criteria

- [ ] No references to removed concepts (`everPersisted`, `deferredUpdates`)
- [ ] All diagnostic logging removed
- [ ] Comments updated to reflect new architecture
- [ ] No unused imports or types
- [ ] Codebase is clean and maintainable

### Rollback Strategy

N/A - This is final cleanup after all phases succeed

---

## Testing Strategy

### Unit Tests

**Add tests for:**

1. **databaseId behavior:**
   - Node created → no `databaseId`
   - Node persisted successfully → has `databaseId`
   - Node with `databaseId` updated → uses UPDATE operation

2. **Empty node persistence:**
   - Create empty node → persists after debounce
   - Rapid creates/deletes → coalesce correctly

3. **Debounce timer reset:**
   - Change node twice rapidly → only one update fires
   - Indent then outdent → only final state persists

### Integration Tests

**Add tests for:**

1. **End-to-end persistence:**
   - Create node → verify in database
   - Update node → verify UPDATE used
   - Delete node → verify removed from database

2. **Timing scenarios:**
   - Rapid operations → verify coalescing
   - Structural changes → verify immediate persistence
   - Content changes → verify debounced persistence

3. **Edge cases:**
   - Create node → reload page → update → verify UPDATE used
   - Network failure → verify retry logic
   - Concurrent operations → verify ordering

### Manual Testing

**Test scenarios:**

1. **Empty node workflow:**
   - Create blank node → verify persists
   - Type content → verify UPDATE used
   - Delete while empty → verify removed

2. **Rapid operations:**
   - Indent then immediately outdent → verify only one update
   - Type then immediately delete → verify coalescing

3. **Initial placeholder:**
   - Open empty viewer → verify placeholder is ephemeral
   - Type in placeholder → verify transitions to pending → persisted

---

## Migration Path

### Feature Flag (Optional)

```typescript
const USE_SIMPLIFIED_PERSISTENCE = import.meta.env.VITE_SIMPLIFIED_PERSISTENCE === 'true';
```

**Why:** Allows testing new system alongside old system

**Rollout:**
1. Phases 1-2: No feature flag needed (safe changes)
2. Phase 3: Enable feature flag in dev environment
3. Phase 4-7: Feature flag controls entire new system
4. After validation: Remove feature flag, delete old code

### Backward Compatibility

**Not needed** - This is internal refactoring with no API changes

**Database schema:** No changes needed (nodes already store empty content)

---

## Rollback Plan

### Per-Phase Rollback

Each phase has specific rollback strategy (see above)

### Full Rollback

If entire refactoring needs to be rolled back:

1. Revert all commits from this refactoring
2. Return to current system (with known bugs)
3. Document why refactoring failed
4. Propose alternative approach

### Partial Success

If some phases succeed but others fail:

- Phases 1-2: Keep (valuable even without full refactoring)
- Phase 3+: Revert if causing issues

---

## Success Metrics

### Code Metrics

- Lines of code reduced by 30%+
- Cyclomatic complexity reduced by 50%+
- Number of state tracking fields: 1 (`databaseId`)

### Bug Metrics

- Zero UNIQUE constraint violations after refactoring
- Zero persistence state confusion bugs
- Zero deferred update bugs

### Developer Experience

- New developers understand persistence in <1 hour
- No fear of changing persistence code
- Clear mental model: "All nodes persist immediately"

---

## Estimated Timeline

| Phase | Duration | Dependencies | Risk |
|-------|----------|--------------|------|
| Phase 1 | 2-3 hours | None | Low |
| Phase 2 | 2-3 hours | Phase 1 | Medium |
| Phase 3 | 4-6 hours | Phase 2 | High |
| Phase 4 | 3-4 hours | Phase 3 | Low |
| Phase 5 | 2-3 hours | Phase 4 | Low |
| Phase 6 | 2-3 hours | Phase 5 | Low |
| Phase 7 | 2-3 hours | Phase 6 | Low |
| **Total** | **18-27 hours** | Sequential | Mixed |

**Recommendation:**
- Week 1: Phases 1-3 (foundation)
- Week 2: Phases 4-7 (cleanup)
- Week 3: Testing and validation

---

## Next Steps

1. **Review this plan with team**
2. **Create GitHub issues for each phase**
3. **Implement Phase 1** (low risk, high value)
4. **Validate Phase 1** before proceeding
5. **Continue with remaining phases**

---

## References

- `persistence-system-review.md` - Root cause analysis
- Issue #450 - Original bug report
- `docs/architecture/placeholder-dependency-pattern.md` - Current complexity
