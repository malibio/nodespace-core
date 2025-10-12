# Migrating Operations to Pattern A (Declarative Dependencies)

## Overview

This guide helps developers migrate sibling chain operations from **Pattern B** (manual async/await coordination) to **Pattern A** (declarative PersistenceCoordinator dependencies).

**Pattern A** is the **recommended approach** for all operations that modify sibling chains, as it:
- Reduces code complexity by ~42%
- Eliminates race conditions
- Provides automatic dependency sequencing
- Removes manual error handling burden

## When to Use This Guide

Use this migration guide if your operation:
- Modifies the sibling chain (`beforeSiblingId` relationships)
- Currently uses `async function` with `await` keywords
- Manually calls `waitForNodeSaves()` or `persist()`
- Requires try-catch blocks for coordination errors

## Pattern Comparison

### ❌ Pattern B (Old - Manual Awaits)

```typescript
async function indentNode(nodeId: string): Promise<boolean> {
  const node = sharedNodeStore.getNode(nodeId);
  if (!node) return false;

  // Identify affected nodes
  const nodesToWaitFor = [];
  const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
  if (nextSibling) {
    nodesToWaitFor.push(nextSibling.id);
  }

  // Repair sibling chain
  removeFromSiblingChain(nodeId);

  // ❌ Manual coordination - bypasses PersistenceCoordinator!
  try {
    if (nodesToWaitFor.length > 0) {
      await sharedNodeStore.waitForNodeSaves(nodesToWaitFor);
    }
  } catch (error) {
    console.error('Failed to wait for saves:', error);
    // Manual error handling required
  }

  // Update node
  sharedNodeStore.updateNode(nodeId, updates, source);

  return true;
}
```

**Problems**:
- 🔴 Bypasses PersistenceCoordinator's queue system
- 🔴 Concurrent requests flood backend
- 🔴 Requires manual timeout and error handling
- 🔴 Makes operation async (harder to use)
- 🔴 ~30 lines of boilerplate per operation

### ✅ Pattern A (New - Declarative Dependencies)

```typescript
function indentNode(nodeId: string): boolean {
  const node = sharedNodeStore.getNode(nodeId);
  if (!node) return false;

  // Identify affected nodes
  const dependencies: string[] = [];
  const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
  if (nextSibling) {
    dependencies.push(nextSibling.id);
  }

  // Repair sibling chain
  removeFromSiblingChain(nodeId); // Auto-queues persistence

  // ✅ Declarative coordination - PersistenceCoordinator handles everything!
  sharedNodeStore.updateNode(
    nodeId,
    updates,
    source,
    { persistenceDependencies: dependencies }
  );

  return true;
}
```

**Benefits**:
- ✅ Declarative: Dependencies explicit in code
- ✅ Automatic: Coordinator sequences all operations
- ✅ Efficient: No manual timeout/error handling
- ✅ Synchronous: Simpler API, no async/await
- ✅ ~15 lines (50% less code)

## Migration Steps

### Step 1: Remove Async/Await

**Before**:
```typescript
async function myOperation(nodeId: string): Promise<void> {
  // ...
}
```

**After**:
```typescript
function myOperation(nodeId: string): void {
  // ...
}
```

**Checklist**:
- [ ] Remove `async` keyword from function signature
- [ ] Change return type from `Promise<T>` to `T`
- [ ] Remove all `await` keywords from function body

### Step 2: Identify Dependencies

Determine which nodes will be **modified by preparatory operations** (like `removeFromSiblingChain()`).

**Common dependencies**:
- **Next sibling**: Updated by `removeFromSiblingChain()`
- **Children**: If operation moves/deletes them
- **Previous operations**: If chaining multiple updates

**Before**:
```typescript
const nodesToWaitFor = [];
const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
if (nextSibling) {
  nodesToWaitFor.push(nextSibling.id);
}
```

**After**:
```typescript
const dependencies: string[] = [];
const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
if (nextSibling) {
  dependencies.push(nextSibling.id);
}
```

**Checklist**:
- [ ] Rename `nodesToWaitFor` → `dependencies`
- [ ] Add TypeScript type annotation: `const dependencies: string[] = []`
- [ ] Keep the same logic for identifying affected nodes

### Step 3: Remove Manual Awaits

**Before**:
```typescript
removeFromSiblingChain(nodeId);

// ❌ Manual await
try {
  if (nodesToWaitFor.length > 0) {
    await sharedNodeStore.waitForNodeSaves(nodesToWaitFor);
  }
} catch (error) {
  console.error('Coordination error:', error);
}

sharedNodeStore.updateNode(nodeId, updates, source);
```

**After**:
```typescript
removeFromSiblingChain(nodeId); // Auto-queues persistence

// ✅ Pass dependencies to coordinator
sharedNodeStore.updateNode(
  nodeId,
  updates,
  source,
  { persistenceDependencies: dependencies }
);
```

**Checklist**:
- [ ] Remove all `await waitForNodeSaves()` calls
- [ ] Remove try-catch blocks for coordination errors
- [ ] Pass dependencies via options parameter

### Step 4: Pass Dependencies to SharedNodeStore

**For updateNode()**:
```typescript
sharedNodeStore.updateNode(
  nodeId,
  updates,
  source,
  { persistenceDependencies: dependencies }  // ← Pass here
);
```

**For deleteNode()**:
```typescript
sharedNodeStore.deleteNode(
  nodeId,
  source,
  false,      // skipPersistence
  dependencies // ← Pass here
);
```

**Checklist**:
- [ ] Add dependencies parameter to `updateNode()` calls
- [ ] Add dependencies parameter to `deleteNode()` calls
- [ ] Verify dependencies array is populated before passing

### Step 5: Update Call Sites

Since the operation is no longer async, update all call sites to remove `await`:

**Before**:
```typescript
await nodeManager.indentNode('node-2');
```

**After**:
```typescript
nodeManager.indentNode('node-2');
```

**Checklist**:
- [ ] Search for all calls to your operation
- [ ] Remove `await` keywords
- [ ] Remove `async` from calling functions if no longer needed
- [ ] Update tests to not await the operation

### Step 6: Update Tests

**Before**:
```typescript
await service.indentNode('node-2');
await waitForDatabaseWrites();
```

**After**:
```typescript
service.indentNode('node-2');
await waitForDatabaseWrites();
```

**Checklist**:
- [ ] Remove `await` from operation calls in tests
- [ ] Keep `await waitForDatabaseWrites()` for verification
- [ ] Ensure tests still pass

## Complete Example

### Before Migration (Pattern B)

```typescript
async function outdentNode(nodeId: string): Promise<boolean> {
  const node = sharedNodeStore.getNode(nodeId);
  if (!node || !node.parentId) return false;

  const parent = sharedNodeStore.getNode(node.parentId);
  if (!parent) return false;

  // Identify affected nodes
  const nodesToWaitFor = [];
  const siblings = sharedNodeStore.getNodesForParent(node.parentId);
  const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
  if (nextSibling) {
    nodesToWaitFor.push(nextSibling.id);
  }

  // Repair sibling chain
  removeFromSiblingChain(nodeId);

  // ❌ Manual coordination
  try {
    if (nodesToWaitFor.length > 0) {
      const persistedNodes = await sharedNodeStore.waitForNodeSaves(nodesToWaitFor);
      if (persistedNodes.size !== nodesToWaitFor.length) {
        console.error('Some nodes failed to persist');
      }
    }
  } catch (error) {
    console.error('Wait timeout:', error);
  }

  // Update node
  const newParentId = parent.parentId || null;
  sharedNodeStore.updateNode(
    nodeId,
    { parentId: newParentId, beforeSiblingId: parent.id },
    source
  );

  return true;
}
```

### After Migration (Pattern A)

```typescript
function outdentNode(nodeId: string): boolean {
  const node = sharedNodeStore.getNode(nodeId);
  if (!node || !node.parentId) return false;

  const parent = sharedNodeStore.getNode(node.parentId);
  if (!parent) return false;

  // Identify affected nodes
  const dependencies: string[] = [];
  const siblings = sharedNodeStore.getNodesForParent(node.parentId);
  const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
  if (nextSibling) {
    dependencies.push(nextSibling.id);
  }

  // Repair sibling chain
  removeFromSiblingChain(nodeId); // Auto-queues persistence

  // ✅ Declarative coordination
  const newParentId = parent.parentId || null;
  sharedNodeStore.updateNode(
    nodeId,
    { parentId: newParentId, beforeSiblingId: parent.id },
    source,
    { persistenceDependencies: dependencies }
  );

  return true;
}
```

**Code Reduction**: 38 lines → 22 lines (42% reduction)

## Verification Checklist

After migration, verify:

### Code Changes
- [ ] No `async` keyword on operation function
- [ ] No `await` keywords in function body
- [ ] No `try-catch` blocks for coordination
- [ ] No `waitForNodeSaves()` calls
- [ ] Dependencies passed via options
- [ ] All call sites updated

### Testing
- [ ] All sibling-chain-integrity tests pass
- [ ] No "database is locked" errors in logs
- [ ] No FOREIGN KEY constraint violations
- [ ] Database state matches in-memory state
- [ ] Quality checks pass: `bun run quality:fix`

### Documentation
- [ ] JSDoc updated to remove async references
- [ ] Race condition handling documented
- [ ] Dependency identification explained

## Common Pitfalls

### ❌ Pitfall 1: Forgetting to Pass Dependencies

```typescript
// ❌ WRONG - Dependencies identified but not passed
const dependencies = [nextSibling.id];
removeFromSiblingChain(nodeId);
sharedNodeStore.updateNode(nodeId, updates, source);
// Missing: { persistenceDependencies: dependencies }
```

**Fix**: Always pass dependencies via options:
```typescript
sharedNodeStore.updateNode(nodeId, updates, source, { persistenceDependencies: dependencies });
```

### ❌ Pitfall 2: Passing Empty Dependencies

```typescript
// ❌ WRONG - Always passing empty array
sharedNodeStore.updateNode(nodeId, updates, source, { persistenceDependencies: [] });
```

**Fix**: Only pass dependencies if the array has items:
```typescript
const options = dependencies.length > 0 ? { persistenceDependencies: dependencies } : {};
sharedNodeStore.updateNode(nodeId, updates, source, options);
```

### ❌ Pitfall 3: Not Updating Call Sites

```typescript
// ❌ WRONG - Operation is now synchronous but call site still awaits
await nodeManager.indentNode('node-2');
```

**Fix**: Remove await from call sites:
```typescript
nodeManager.indentNode('node-2');
```

## Performance Impact

**Before (Pattern B)**:
- ~130 lines of coordination code across 4 operations
- Manual timeout handling (2000ms per operation)
- Async/await overhead (~2ms per operation)
- Explicit try-catch blocks

**After (Pattern A)**:
- ~75 lines of business logic
- No timeout handling needed
- No async overhead
- No error handling needed

**Improvement**: 55 lines saved (~42% reduction), 2ms faster per operation

## Real-World Examples

See these successfully migrated operations in `reactive-node-service.svelte.ts`:

1. **deleteNode()** (lines 1275-1310): Simplest case, single dependency
2. **indentNode()** (lines 853-977): Moderate complexity, sibling transfer
3. **outdentNode()** (lines 979-1191): Complex, multiple sibling transfers
4. **combineNodes()** (lines 768-825): Child promotion with dependencies

## Related Documentation

- [Persistence Layer Architecture](../../persistence-layer.md#critical-usage-patterns-issue-246-lessons-learned)
- [Dependency-Based Persistence](../../dependency-based-persistence.md)
- [PersistenceCoordinator API](../../api/persistence-coordinator.md)

## Support

If you encounter issues during migration:

1. Review the [anti-patterns section](../../persistence-layer.md#anti-pattern-manual-awaits-bypass-coordinator)
2. Check the [red flags checklist](../../persistence-layer.md#red-flags-indicating-coordinator-bypass)
3. Compare your code to the [real-world examples](../../persistence-layer.md#real-world-examples)
4. Run the sibling-chain-integrity test suite to validate correctness

## Summary

**Migration is straightforward**:
1. Remove async/await
2. Identify dependencies
3. Pass dependencies to SharedNodeStore
4. Update call sites
5. Test thoroughly

**Benefits are significant**:
- 42% less code
- Automatic sequencing
- No race conditions
- Simpler API

**The result**: More maintainable, more correct, and more performant operations.
