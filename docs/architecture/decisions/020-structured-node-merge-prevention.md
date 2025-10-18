# ADR-020: Structured Node Merge Prevention

## Status
**Accepted** - October 2025

## Context

NodeSpace supports merging nodes when users press Backspace at the start of a node. This allows natural text editing where two text nodes can be combined. However, some node types have structured content with specific formatting requirements (e.g., code blocks with fence markers, quote blocks with line prefixes) that cannot accept arbitrary content merges.

### Existing Behavior

Prior to this decision, pressing Backspace at the start of any node would attempt to merge it with the previous node:

```
Code Block:           Quote Block:
```javascript        > Quote text
console.log()        > More text
```

Text Node:           Text Node:
Hello world          Some content
↑ cursor here        ↑ cursor here
```

Pressing Backspace would merge "Hello world" into the code block or quote block, corrupting the structured formatting.

### Problem Statement

We needed to decide:
1. Should structured nodes (code-block, quote-block) accept merges from arbitrary content?
2. If not, how should we prevent such merges?
3. Should prevention be implicit (built into node logic) or explicit (configuration-based)?

## Decision

**Structured nodes (code-block, quote-block) prevent arbitrary content from merging into them via Backspace.**

Implementation uses a multi-layered approach:

1. **Configuration-Based Prevention** (`allowMergeInto` config)
   - Node components declare merge policy via `editableConfig`
   - Example: `{ allowMultiline: true, allowMergeInto: false }`
   - Provides explicit documentation of node behavior

2. **Viewer-Level Safety Guards** (defense in depth)
   - BaseNodeViewer checks `isStructuredNode(previousNode.nodeType)`
   - Prevents merges even if configuration is missed
   - Centralized constant: `STRUCTURED_NODE_TYPES = ['code-block', 'quote-block']`

3. **Silent Prevention** (UX decision)
   - No error messages or visual feedback
   - Cursor stays in current node
   - Users can still delete empty nodes manually

### Rationale

#### 1. **Data Integrity**

Merging arbitrary content into structured nodes corrupts their format:

```typescript
// Before merge:
codeBlock.content = "```javascript\nconsole.log()\n```"
textNode.content = "Hello world"

// After merge (if allowed):
codeBlock.content = "```javascript\nconsole.log()\n```Hello world"
// ❌ Closing fence no longer at end, content outside fences

// Quote block example:
quoteBlock.content = "> Quote text\n> More text"
textNode.content = "Regular text"

// After merge (if allowed):
quoteBlock.content = "> Quote text\n> More textRegular text"
// ❌ Last line lacks "> " prefix, breaks quote structure
```

#### 2. **Backend Validation Failures**

The Rust backend validates node structure. Corrupted content causes HTTP 500 errors:

```rust
// code-block validation expects:
// - Opening fence: ```
// - Content lines
// - Closing fence: ```

// quote-block validation expects:
// - All lines start with "> " or ">"
```

Allowing merges would create invalid nodes that fail persistence.

#### 3. **User Experience Clarity**

Structured nodes are visually distinct (monospace font for code, blockquote styling for quotes). Users understand these are "special containers" that don't accept random merges.

#### 4. **Configuration-Based Extensibility**

The `allowMergeInto` config provides:
- **Self-documenting code**: Each node declares its merge policy
- **Easy extension**: Future structured nodes (tables, diagrams) can opt out
- **Type safety**: Configuration interface enforced by TypeScript

#### 5. **Defense in Depth**

Two-layer prevention (config + viewer checks) ensures:
- New node types can't accidentally allow merges by forgetting config
- Centralized `STRUCTURED_NODE_TYPES` constant = single source of truth
- Refactoring safety: hard to break merge prevention accidentally

## Alternative Considered

### Option A: Allow Merge with Content Stripping

**Approach:**
- Accept the merge
- Strip formatting from incoming content
- Append to structured node

**Example:**
```typescript
// Merging text into code block:
codeBlock.content = "```\ncode here\n```"
textNode.content = "Hello world"

// After merge:
codeBlock.content = "```\ncode here\nHello world\n```"
// Added new line with merged content inside fences
```

**Why Rejected:**

1. **Unexpected Behavior**: User didn't intend to add code, they pressed Backspace
2. **Auto-Prefix Burden**: Would need to add `> ` prefix for quote blocks automatically
3. **Complexity**: Different merge logic per structured type
4. **Unclear Intent**: Did user want to merge, or just delete current node?
5. **Formatting Conflicts**: What if merged content has newlines, special characters, etc.?

### Option B: Allow Merge with Conversion

**Approach:**
- Merge triggers node type conversion
- Previous node converts to text before accepting merge

**Example:**
```typescript
// User presses Backspace in text node below code block:
// 1. Convert code-block to text (strip fences)
// 2. Merge text node into converted text node
```

**Why Rejected:**

1. **Data Loss**: Loses structured formatting user created
2. **Unexpected**: User didn't request conversion
3. **Irreversible**: Can't undo the conversion easily
4. **Violates Principle of Least Surprise**: Backspace shouldn't convert node types

## Consequences

### Positive

✅ **Data Integrity**: Prevents corrupted node content from reaching database
✅ **Clear Mental Model**: Structured nodes are "solid blocks" that don't merge
✅ **Extensible Pattern**: Easy to add new structured node types
✅ **Self-Documenting**: Config makes merge policy explicit
✅ **Fail-Safe**: Multiple prevention layers reduce bugs
✅ **Simple UX**: Silent prevention avoids error dialogs

### Negative

⚠️ **Slightly Different UX**: Text nodes merge, structured nodes don't (acceptable trade-off)
⚠️ **Manual Deletion Required**: Users must explicitly delete empty nodes after structured nodes
⚠️ **No Visual Feedback**: Could add hover hint showing "no merge" (future enhancement)

### Neutral

ℹ️ **Selection Detection**: Merge command now checks for text selection
ℹ️ **Stripping Quote Prefixes**: `stripFormattingSyntax()` now handles `> ` from all lines

## Implementation Notes

### 1. Configuration Interface

```typescript
// packages/desktop-app/src/lib/design/components/textarea-controller.ts
export interface TextareaControllerConfig {
  allowMultiline?: boolean;
  /**
   * Whether other nodes can merge into this node via Backspace
   * Set to false for structured nodes that can't accept arbitrary merges
   * @default true
   * @example
   * const editableConfig = { allowMultiline: true, allowMergeInto: false };
   */
  allowMergeInto?: boolean;
}
```

### 2. Structured Node Types Registry

```typescript
// packages/desktop-app/src/lib/design/components/base-node-viewer.svelte
const STRUCTURED_NODE_TYPES = ['code-block', 'quote-block'] as const;

function isStructuredNode(nodeType: string): boolean {
  return STRUCTURED_NODE_TYPES.includes(
    nodeType as (typeof STRUCTURED_NODE_TYPES)[number]
  );
}
```

**Benefits:**
- Single source of truth for structured types
- Easy to extend: add new types to array
- Type-safe: TypeScript enforces valid values

### 3. Merge Prevention Points

```typescript
// Point 1: handleCombineWithPrevious (Backspace with content)
if (isStructuredNode(previousNode.nodeType)) {
  return; // Silently prevent merge
}

// Point 2: handleDeleteNode (Backspace in empty node)
if (isStructuredNode(previousNode.nodeType)) {
  return; // Don't delete, don't merge, don't focus
}
```

**Why Both Points:**
- `handleCombineWithPrevious`: Node has content, user wants to merge
- `handleDeleteNode`: Node is empty, user wants to delete it
- Both blocked to maintain consistency

### 4. Quote Prefix Stripping

Enhanced `stripFormattingSyntax()` to handle quote blocks during merges into non-structured nodes:

```typescript
function stripFormattingSyntax(content: string): string {
  let cleaned = content;
  cleaned = cleaned.replace(/^#{1,6}\s+/, ''); // Headers
  cleaned = cleaned.replace(/^\[\s*[x\s]*\]\s*/, ''); // Tasks
  // Strip quote-block prefixes from all lines
  cleaned = cleaned
    .split('\n')
    .map((line) => line.replace(/^>\s?/, ''))
    .join('\n');
  return cleaned.trim();
}
```

This ensures quote-block content can merge INTO text nodes (stripping prefixes), but text cannot merge INTO quote-blocks (prevented by `isStructuredNode`).

## Testing Strategy

### Unit Tests

1. **Merge Prevention Helpers** (`merge-prevention.test.ts`)
   - `isStructuredNode()` returns true for code-block, quote-block
   - Returns false for text, header, task, date nodes
   - `STRUCTURED_NODE_TYPES` has expected values

2. **Selection Detection** (`merge-nodes.command.test.ts`)
   - Backspace with selection → don't merge (browser deletes selection)
   - Backspace at start without selection → merge (as expected)

3. **Quote Prefix Stripping** (`reactive-node-service.svelte.ts`)
   - Validates `stripFormattingSyntax()` removes `> ` from all lines

### Integration Tests (Future)

When base-node-viewer test infrastructure is available:

```typescript
it('should prevent merging text node into code-block via Backspace', async () => {
  // Setup: code-block followed by text node
  // Action: cursor at start of text node, press Backspace
  // Expect: text node unchanged, code-block unchanged
});
```

See `merge-prevention.test.ts` for full integration test plan.

## Related Decisions

- **ADR-019**: Quote Block Prefix Persistence (why prefixes stored in database)
- **ADR-015**: Text Editor Architecture Refactor (textarea-based editing foundation)
- Issue #277: QuoteBlockNode Implementation
- Issue #275: HeaderNode Implementation

## References

- `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte:87-96` (helpers)
- `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte:1147-1151` (prevention)
- `packages/desktop-app/src/lib/design/components/textarea-controller.ts:99-107` (config)
- `packages/desktop-app/src/lib/commands/keyboard/merge-nodes.command.ts:106-135` (selection detection)
- `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts:815-828` (stripping)
- `packages/desktop-app/src/tests/components/merge-prevention.test.ts` (tests)

## Future Considerations

### Visual Feedback

Could add visual indicator when user attempts merge into structured node:

```typescript
// Potential enhancement:
if (isStructuredNode(previousNode.nodeType)) {
  // Brief border highlight on code-block/quote-block?
  // Tooltip: "Can't merge into code blocks"?
  return;
}
```

**Trade-off:** Adds complexity vs. silent behavior that "just works"

### Additional Structured Types

As new structured node types are added (tables, diagrams, embeds):

1. Add to `STRUCTURED_NODE_TYPES` constant
2. Set `allowMergeInto: false` in component config
3. Add test cases to `merge-prevention.test.ts`

### Manual Deletion Enhancement

Future keyboard command (e.g., Cmd+Backspace) could force-delete empty nodes even below structured nodes, providing explicit deletion path.
