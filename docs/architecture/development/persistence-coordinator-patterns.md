# PersistenceCoordinator Patterns & Best Practices

## Overview

The `PersistenceCoordinator` manages asynchronous database persistence with dependency tracking, ensuring operations execute in the correct order to prevent FOREIGN KEY violations and race conditions.

**Key Design Principles:**
- **Declarative Dependencies**: Specify what must complete before an operation
- **Automatic Sequencing**: Coordinator handles waiting and error recovery
- **Separation of Concerns**: Service layer stays synchronous, persistence layer is async

---

## Core API

### `persist(nodeId, operation, options)`

```typescript
PersistenceCoordinator.getInstance().persist(
  nodeId: string,                    // Unique identifier for this operation
  operation: () => Promise<void>,    // Async database operation
  options: {
    mode?: 'immediate' | 'debounce', // Execution mode (default: debounce)
    debounceMs?: number,             // Debounce delay (default: 500ms)
    dependencies?: PersistenceDependency[] // Operations that must complete first
  }
);
```

**Dependencies** can be:
- `string` - Node ID of operation that must complete first
- `() => Promise<void>` - Async function that must complete first

---

## Pattern 1: Simple Immediate Persistence

**Use Case:** Creating or updating a node without dependencies.

```typescript
sharedNodeStore.updateNode(nodeId, { content: 'New content' }, source);
```

**What Happens:**
1. In-memory state updated immediately
2. `SharedNodeStore.updateNode()` calls `PersistenceCoordinator.persist()`
3. Database write queued based on mode (immediate vs debounce)

**When to Use:**
- Single node operations
- No foreign key dependencies
- No ordering requirements

---

## Pattern 2: Deletion with Dependencies (Recommended)

**Use Case:** Deleting a node that other nodes reference. Must ensure updates complete before deletion to prevent FOREIGN KEY violations.

### Example: Node Combining (Issue #229)

**Scenario:** Combining two nodes involves:
1. Update previous node content (must persist)
2. Update children parentId (must persist)
3. Update sibling beforeSiblingId (must persist)
4. Delete current node (must wait for all above)

**Implementation:**

```typescript
function combineNodes(currentNodeId: string, previousNodeId: string): void {
  // 1. Update content
  sharedNodeStore.updateNode(previousNodeId, { content: combinedContent }, source);

  // 2. Promote children
  promoteChildren(currentNodeId, previousNodeId); // Updates multiple nodes

  // 3. Repair sibling chain
  removeFromSiblingChain(currentNodeId); // Updates next sibling

  // 4. Collect all nodes that must persist before deletion
  const deletionDependencies = [
    previousNodeId,           // Content update must complete
    ...childIds,              // Child promotions must complete
    nextSiblingId             // Sibling chain repair must complete
  ];

  // 5. Delete with dependencies - PersistenceCoordinator ensures correct order
  sharedNodeStore.deleteNode(currentNodeId, source, false, deletionDependencies);
}
```

**How It Works:**
1. All `updateNode()` calls queue persistence operations
2. `deleteNode()` passes dependencies to `PersistenceCoordinator.persist()`
3. Coordinator waits for all dependency operations to complete
4. Only then executes the deletion
5. If any dependency fails, deletion is automatically delayed/retried

**Benefits:**
- ✅ Prevents FOREIGN KEY violations
- ✅ Handles race conditions automatically
- ✅ No manual `await` or error handling needed
- ✅ Service layer stays synchronous

---

## Pattern 3: Ancestor Chain Dependencies

**Use Case:** Creating a deeply nested node. Must ensure all ancestors exist in database before creating the child.

**Implementation:**

```typescript
// Creating: Grandparent → Parent → Child
sharedNodeStore.setNode(grandparent, source); // No dependencies
sharedNodeStore.setNode(parent, source);      // Auto-depends on grandparent (via ensureAncestorChainPersisted)
sharedNodeStore.setNode(child, source);       // Auto-depends on parent + grandparent
```

**How It Works:**
- `SharedNodeStore.setNode()` automatically walks ancestor chain (lines 393-398)
- Adds async function dependency: `() => this.ensureAncestorChainPersisted(parentId)`
- PersistenceCoordinator resolves entire chain recursively

**Handled Automatically** - No explicit dependency array needed.

---

## Pattern 4: Structural vs Content Updates

**Use Case:** Different persistence strategies for structural changes vs content edits.

### Structural Changes (Immediate Mode)
```typescript
sharedNodeStore.updateNode(nodeId, {
  parentId: newParent,
  beforeSiblingId: newSibling
}, viewerSource);
// Persists immediately (mode: 'immediate')
```

### Content Changes (Debounce Mode)
```typescript
sharedNodeStore.updateNode(nodeId, {
  content: 'User is typing...'
}, viewerSource);
// Debounced 500ms to avoid excessive writes
```

**Implementation Details** (shared-node-store.ts:252-254):
```typescript
const isStructuralChange = 'parentId' in changes ||
                          'beforeSiblingId' in changes ||
                          'containerNodeId' in changes;
const mode = isStructuralChange ? 'immediate' : 'debounce';
```

---

## Anti-Patterns to Avoid

### ❌ Anti-Pattern 1: Manual Await

**Bad:**
```typescript
async function deleteNodeWithWait(nodeId: string, dependencies: string[]) {
  // Manual coordination
  const persistedNodes = await sharedNodeStore.waitForNodeSaves(dependencies);

  if (persistedNodes.size !== dependencies.length) {
    const failed = dependencies.filter(id => !persistedNodes.has(id));
    console.error('Failed to persist:', failed);
    // What do we do now? Retry? Skip?
  }

  sharedNodeStore.deleteNode(nodeId, source);
}
```

**Problems:**
- Service layer becomes async (cascades to entire call chain)
- Manual error handling (duplicates PersistenceCoordinator logic)
- Unclear retry strategy
- Harder to test

**Good:**
```typescript
function deleteNodeWithDependencies(nodeId: string, dependencies: string[]) {
  // Declarative - let PersistenceCoordinator handle coordination
  sharedNodeStore.deleteNode(nodeId, source, false, dependencies);
}
```

---

### ❌ Anti-Pattern 2: Direct Database Calls

**Bad:**
```typescript
async function updateNodeAndSave(nodeId: string, updates: Partial<Node>) {
  sharedNodeStore.updateNode(nodeId, updates, source);
  await tauriNodeService.updateNode(nodeId, updates); // Direct DB call
}
```

**Problems:**
- Bypasses PersistenceCoordinator (no dependency tracking)
- Duplicate persistence (SharedNodeStore already persists)
- Race conditions possible

**Good:**
```typescript
function updateNode(nodeId: string, updates: Partial<Node>) {
  // SharedNodeStore handles persistence via PersistenceCoordinator
  sharedNodeStore.updateNode(nodeId, updates, source);
}
```

---

### ❌ Anti-Pattern 3: Ignoring Containerization

**Bad:**
```typescript
function promoteChild(childId: string, newParentId: string) {
  sharedNodeStore.updateNode(childId, {
    parentId: newParentId
    // Missing: containerNodeId update!
  }, source);
}
```

**Problems:**
- Child still references old containerNodeId
- Deleting old container causes FOREIGN KEY violation

**Good:**
```typescript
function promoteChild(childId: string, newParentId: string) {
  const newParent = sharedNodeStore.getNode(newParentId);
  sharedNodeStore.updateNode(childId, {
    parentId: newParentId,
    containerNodeId: newParent?.containerNodeId ?? null // Inherit container
  }, source);
}
```

---

## Testing with PersistenceCoordinator

### Test Pattern: Wait for Database Writes

```typescript
import { waitForDatabaseWrites } from '../../utils/test-helpers';

test('should delete node after updates persist', async () => {
  // Arrange
  service.updateNode(node1, { content: 'Update 1' });
  service.updateNode(node2, { content: 'Update 2' });

  // Act
  service.deleteNode(nodeToDelete, source, false, [node1, node2]);

  // Wait for all async persistence to complete
  await waitForDatabaseWrites();

  // Assert
  expect(sharedNodeStore.getTestErrors()).toHaveLength(0);
  expect(await databaseService.getNode(nodeToDelete)).toBeNull();
});
```

**Key Points:**
- `waitForDatabaseWrites()` polls PersistenceCoordinator until all operations complete
- Check `getTestErrors()` to verify no persistence failures
- Verify final database state

---

## When to Use Each Pattern

| Scenario | Pattern | Example |
|----------|---------|---------|
| Single node create/update | Simple Immediate | `sharedNodeStore.updateNode(id, {content}, source)` |
| Delete with updates | Deletion with Dependencies | `deleteNode(id, source, false, [dep1, dep2])` |
| Nested node creation | Ancestor Chain (Auto) | `setNode()` walks ancestors automatically |
| UI-driven edits | Debounce Mode (Auto) | Viewer source + content change = debounce |
| Hierarchy changes | Immediate Mode (Auto) | Structural changes = immediate |

---

## Key Takeaways

1. **Use Dependencies, Not Await**: Pass dependency array to operations instead of manual waiting
2. **Stay Synchronous**: Service layer should be synchronous; PersistenceCoordinator handles async
3. **Trust the Coordinator**: It handles retries, logging, and error recovery
4. **Update All References**: When moving/deleting nodes, update parentId AND containerNodeId
5. **Test with Helpers**: Use `waitForDatabaseWrites()` to properly test async persistence

---

## References

- **PersistenceCoordinator Implementation**: `packages/desktop-app/src/lib/services/persistence-coordinator.svelte.ts`
- **SharedNodeStore Integration**: `packages/desktop-app/src/lib/services/shared-node-store.ts` (lines 251-340, 461-522)
- **Example Implementation**: `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts` (combineNodes, lines 744-785)
- **Test Helpers**: `packages/desktop-app/src/tests/helpers/test-helpers.ts` (waitForDatabaseWrites)

---

**Last Updated:** 2025-10-12
**Related Issues:** #229 (Backspace/Delete Operations)
