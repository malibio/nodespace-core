# Issue #228 Handoff - Indent/Outdent Race Condition Fixes

## Quick Context

**Issue:** #228 - Fix Indent/Outdent Operations (12 test failures)
**Branch:** `feature/issue-228-indent-outdent-fixes`
**PR:** #253
**Current Status:** 8/14 tests passing, 6 failing with HTTP 500 errors

## What's Been Completed

### ✅ PersistenceCoordinator Queue-Based Sequencing (Merged to Main)

We successfully implemented and merged queue-based dependency sequencing infrastructure:

**Commit:** `a9d233a` on main branch
**Files:**
- `src/lib/services/persistence-coordinator.svelte.ts` - Queue blocking mechanism
- `src/lib/types/update-protocol.ts` - Added `persistenceDependencies` field
- `src/lib/services/shared-node-store.ts` - Forwards persistenceDependencies

**How It Works:**
1. When `persist(nodeId, operation, { dependencies: [...] })` is called
2. Check if any string dependencies are pending/in-progress
3. If yes, add operation to `waitingQueue` instead of scheduling
4. When dependency completes, call `unblockDependentOperations()`
5. Blocked operations schedule automatically when all deps resolve

**This infrastructure is working correctly!** The remaining issues are in how indent/outdent declare their dependencies.

### ⚠️ What's Still Broken

**Test Results:** 8/14 passing
**Failures:** HTTP 500 errors indicating FOREIGN KEY constraint violations
**Root Cause:** Circular dependencies in consecutive indent/outdent operations

## The Actual Problem

### Circular Dependency Chain Example

**Test:** "should handle multiple consecutive indents"

**Setup:**
```
- node-1 (beforeSiblingId: null)
- node-2 (beforeSiblingId: 'node-1')
- node-3 (beforeSiblingId: 'node-2')
```

**Operations:**
```typescript
service.indentNode('node-2'); // node-2 becomes child of node-1
service.indentNode('node-3'); // node-3 becomes child of node-2
```

**What Happens:**
1. First indent: `removeFromSiblingChain('node-2')` updates node-3's beforeSiblingId
2. Returns 'node-3' as the updated sibling
3. We declare dependency: `persistenceDependencies: ['node-3', 'node-1']`
4. Second indent: `removeFromSiblingChain('node-3')` might update node-2
5. Creates circular: node-2 depends on node-3, node-3 depends on node-2

**Test Output:**
```
[PersistenceCoordinator] Operation blocked: { nodeId: 'node-2', blockingDeps: [ 'node-3' ] }
[PersistenceCoordinator] Operation blocked: { nodeId: 'node-3', blockingDeps: [ 'node-2' ] }
```

### Current Implementation Issues

**Location:** `src/lib/services/reactive-node-service.svelte.ts`

**indentNode() - Lines 862-907:**
```typescript
// Step 1: Remove from sibling chain
removeFromSiblingChain(nodeId); // Updates next sibling, returns its ID

// Step 2: Build dependencies
const persistenceDependencies: string[] = [];
persistenceDependencies.push(prevSiblingId); // Only parent

// We removed updatedSiblingId to avoid circular deps
// But this means sibling update and main update happen in parallel!

// Step 3: Update main node
sharedNodeStore.updateNode(nodeId, { parentId, beforeSiblingId }, ..., {
  persistenceDependencies
});
```

**outdentNode() - Lines 927-1069:**
- Similar pattern with removeFromSiblingChain
- Additional complexity: transfers siblings below as children
- Each transferred sibling declares dependencies but they execute in a loop
- Creates dependency chains that might have circular references

## What You Need to Fix

### Primary Task: Eliminate Circular Dependencies

**The core issue:** `removeFromSiblingChain()` updates siblings, and we're trying to declare those as dependencies, but consecutive operations create circles.

**Possible Solutions:**

#### Solution 1: Don't Depend on Sibling Updates (Current Attempt)

The sibling chain cleanup is independent from parent updates - they can happen in parallel.

**Problem:** We tried this (removed `updatedSiblingId` from dependencies), but still getting HTTP 500s. Why?

**Next Investigation:**
- Are the HTTP 500s from the sibling updates themselves, or from other operations?
- Check if SharedNodeStore's automatic dependencies (lines 272-302 in shared-node-store.ts) are creating conflicts
- Maybe the sibling updates ARE causing violations because beforeSiblingId references nodes being moved?

#### Solution 2: Sequential Indent/Outdent Calls

Make indent/outdent operations wait for ALL pending persistence before starting.

**Implementation:**
```typescript
async function indentNode(nodeId: string): Promise<boolean> {
  // Wait for ALL pending operations to complete first
  await PersistenceCoordinator.getInstance().waitForPersistence(
    Array.from(sharedNodeStore.nodes.keys()),
    5000
  );

  // Now proceed with indent logic...
}
```

**Pros:** Eliminates all race conditions
**Cons:** Serializes everything, loses parallelism

#### Solution 3: Batch Operations

Create a single atomic operation for all updates in indent/outdent.

**Would require:**
- New `batchUpdate()` method in SharedNodeStore
- Collect all node updates (sibling chain, main node, transferred siblings)
- Execute as single PersistenceCoordinator operation
- Single dependency declaration for entire batch

**Pros:** Clean dependency model, atomic operations
**Cons:** More complex implementation, new APIs needed

### Secondary Tasks

1. **Update tests to wait for persistence** - Tests currently check database immediately after calling indent/outdent synchronously. They might need to wait for persistence to complete before assertions.

2. **Fix beforeSiblingId circular references** - The error logs show nodes pointing to each other as beforeSiblings. This shouldn't happen with correct logic.

3. **Handle sibling transfers in outdent** - Lines 1020-1069 transfer siblings in a loop, each with dependencies. This complex chain might need simplification.

## Files You'll Be Modifying

### Primary File
**`packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`**
- Lines 844-907: `indentNode()` function
- Lines 927-1069: `outdentNode()` function
- Lines 824-842: `removeFromSiblingChain()` helper

### Supporting Files (Reference Only)
- `src/lib/services/persistence-coordinator.svelte.ts` - Queue implementation (DON'T MODIFY - already on main)
- `src/lib/services/shared-node-store.ts` - Dependency forwarding (already correct)
- `src/tests/integration/indent-outdent-operations.test.ts` - Test scenarios

## How to Start

### 1. Checkout and Setup

```bash
cd /Users/malibio/nodespace/nodespace-core
git checkout feature/issue-228-indent-outdent-fixes
bun install
```

### 2. Run Tests to See Current State

```bash
bun run test src/tests/integration/indent-outdent-operations.test.ts
```

**Expected Output:** 8 passing, 6 failing with HTTP 500 errors

### 3. Investigate Circular Dependencies

Add temporary logging to see dependency chains:

```typescript
// In indentNode() before calling updateNode:
console.log('[DEBUG] Indent dependencies:', {
  nodeId,
  persistenceDependencies,
  parentId: prevSiblingId,
  beforeSiblingId
});
```

Run tests and look for:
- "Operation blocked" messages with circular patterns
- beforeSiblingId values that point to nodes being simultaneously updated
- Which specific test creates the node-2 ↔ node-3 circular dependency

### 4. Test Your Fix

After making changes:

```bash
# Run the specific tests
bun run test src/tests/integration/indent-outdent-operations.test.ts

# Should show:
# - No HTTP 500 errors
# - No "Operation blocked" with circular dependencies
# - 14/14 tests passing
```

### 5. Quality Check and Commit

```bash
bun run quality:fix
git add -A
git commit -m "Fix circular dependencies in indent/outdent operations

<describe what you changed and why it fixes the circular deps>

Test Results: 14/14 passing, no HTTP 500 errors

Related: Issue #228"
```

### 6. Update PR

```bash
git push
# PR #253 will automatically update
```

## Key Insights for Your Investigation

### 1. The Queue Sequencing IS Working

Don't modify PersistenceCoordinator - it's correctly blocking operations. The problem is we're declaring circular dependencies.

### 2. SharedNodeStore Adds Automatic Dependencies

Lines 272-302 in shared-node-store.ts automatically add dependencies for:
- containerNodeId (if set and not persisted)
- beforeSiblingId (if set and not persisted)

This means even if we don't explicitly declare beforeSiblingId as a dependency, SharedNodeStore might add it! This could be creating unexpected dependency chains.

### 3. removeFromSiblingChain() Is Innocent

The helper function just updates one sibling's beforeSiblingId to "splice out" the moved node. This is correct logic. The issue is how we're treating that update in the dependency chain.

### 4. Tests Don't Need Modification

The tests are correctly calling operations synchronously and checking database state. Once the race conditions are fixed, the tests should pass as-is. Don't add await calls to tests.

## Expected Timeline

**Complexity:** Medium - requires careful analysis of dependency chains
**Estimated Time:** 2-4 hours for an AI agent with fresh context
**Blocker Status:** HIGH - This is blocking other issues codebase-wide

## Success Criteria

✅ All 14 tests in `indent-outdent-operations.test.ts` pass
✅ No HTTP 500 errors in test output
✅ No circular dependency blocks in PersistenceCoordinator
✅ Database state matches expected hierarchy
✅ In-memory state matches database state
✅ Quality checks pass (`bun run quality:fix`)

## Additional Context

**Related Issues:**
- Issue #245 (Completed) - PersistenceCoordinator setup
- Issue #246 - Sibling chain integrity (might have similar patterns)
- Issue #229 - Backspace/delete operations (different but related)

**Main Branch Changes Since This Branch:**
- PersistenceCoordinator queue sequencing is now on main
- containerNodeId and beforeSiblingId automatic dependencies added to SharedNodeStore
- You may need to rebase this branch on main before final merge

**Architecture Docs:**
- `/docs/architecture/development/process/startup-sequence.md` - Development workflow
- No specific docs for PersistenceCoordinator yet (was just implemented)

---

## Final Notes

The infrastructure for solving this is in place. What remains is careful analysis of the dependency chains being created by indent/outdent operations and fixing the circular references. The queue-based sequencing will handle the rest automatically once the dependencies are declared correctly.

Good luck! 🚀
