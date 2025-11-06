# Singleton Testing Pattern: Dynamic getInstance() vs Static Import

**Date**: November 2024
**Issue**: #424 - Node type conversions persistence
**Category**: Testing Infrastructure, Design Pattern

## Problem

When tests call `resetInstance()` on singletons to ensure test isolation, **static imports retain stale references** to the old singleton instance. This causes service methods to interact with the wrong instance, leading to test failures or incorrect behavior.

## Root Cause

TypeScript/JavaScript module imports are evaluated **once at module load time**. When you import a singleton instance directly:

```typescript
// ❌ PROBLEMATIC PATTERN
import { sharedNodeStore } from './shared-node-store';

export function createReactiveNodeService(events: NodeManagerEvents) {
  // sharedNodeStore is bound to the FIRST instance created
  // If tests call SharedNodeStore.resetInstance(), this reference becomes stale
  sharedNodeStore.updateNode(nodeId, { content }, source); // ❌ Updates wrong instance!
}
```

The problem manifests as:

1. **Test 1** creates service, uses singleton A
2. **Test 2** calls `resetInstance()`, creates singleton B
3. **Test 2's service still references singleton A** (stale!)
4. Updates go to the wrong instance, tests fail mysteriously

## Solution

Import the **class** instead of the singleton instance, and call `getInstance()` **dynamically** inside the function:

```typescript
// ✅ CORRECT PATTERN
import { SharedNodeStore } from './shared-node-store'; // Import class, not instance

export function createReactiveNodeService(events: NodeManagerEvents) {
  // Get instance dynamically - important for tests that call resetInstance()
  const sharedNodeStore = SharedNodeStore.getInstance();

  // Now sharedNodeStore refers to the CURRENT singleton instance
  sharedNodeStore.updateNode(nodeId, { content, nodeType }, source); // ✅ Correct instance!
}
```

## Why This Works

**Dynamic instantiation** ensures the singleton reference is resolved **at function execution time**, not at module load time:

```typescript
// Module load time (happens once)
import { SharedNodeStore } from './shared-node-store'; // ✅ Import class definition

// Function execution time (happens every call)
export function createReactiveNodeService() {
  const store = SharedNodeStore.getInstance(); // ✅ Gets CURRENT instance
}
```

**Timeline**:
1. Module loads → `SharedNodeStore` class imported (no instance created yet)
2. Test 1: `createReactiveNodeService()` → calls `getInstance()` → gets singleton A
3. Test 2: `resetInstance()` → destroys singleton A
4. Test 2: `createReactiveNodeService()` → calls `getInstance()` → gets fresh singleton B ✅

## When to Use This Pattern

**Use dynamic `getInstance()` when**:
- ✅ Service is used in tests that call `resetInstance()`
- ✅ Service needs to respect singleton resets for proper test isolation
- ✅ Multiple test files interact with the same singleton

**Static import is OK when**:
- ✅ No tests call `resetInstance()` (singleton lives for entire test suite)
- ✅ Service is never tested in isolation
- ✅ You're importing a constant/utility, not a singleton

## Real-World Example: Issue #424

**Before Fix** (`reactive-node-service.svelte.ts`):
```typescript
import { sharedNodeStore } from './shared-node-store'; // ❌ Stale reference

export function createReactiveNodeService(events: NodeManagerEvents) {
  // ... 500 lines later ...

  updateNodeContent(nodeId: string, content: string) {
    sharedNodeStore.updateNode(nodeId, { content }, source);
    // ❌ In tests, this updates the OLD instance after resetInstance()
  }
}
```

**After Fix**:
```typescript
import { SharedNodeStore } from './shared-node-store'; // ✅ Import class

export function createReactiveNodeService(events: NodeManagerEvents) {
  // Get instance dynamically (important for tests that call resetInstance())
  const sharedNodeStore = SharedNodeStore.getInstance(); // ✅ Fresh reference

  updateNodeContent(nodeId: string, content: string) {
    sharedNodeStore.updateNode(nodeId, { content, nodeType }, source);
    // ✅ Always uses current singleton instance
  }
}
```

**Test Impact**:
- Before: Tests calling `resetInstance()` failed intermittently
- After: All tests pass reliably with proper isolation

## Testing the Pattern

To verify your singleton respects resets, add this test:

```typescript
describe('Singleton Reset Pattern', () => {
  it('should use fresh instance after resetInstance()', () => {
    // Create first instance and service
    const service1 = createReactiveNodeService(events);
    const store1 = SharedNodeStore.getInstance();

    // Reset singleton
    SharedNodeStore.resetInstance();

    // Create second instance and service
    const service2 = createReactiveNodeService(events);
    const store2 = SharedNodeStore.getInstance();

    // Verify they're different instances
    expect(store1).not.toBe(store2);

    // Verify service2 uses store2, not store1
    service2.updateNodeContent('test-id', 'New content');
    expect(store2.getNode('test-id')).toBeDefined();
    expect(store1.getNode('test-id')).toBeUndefined(); // ✅ Store1 not affected
  });
});
```

## Alternative Solutions Considered

### Option 1: Dependency Injection
```typescript
export function createReactiveNodeService(
  events: NodeManagerEvents,
  store: SharedNodeStore // ✅ Pass as parameter
) {
  // Use injected store
}
```

**Pros**: Most explicit, easier to mock
**Cons**: Breaks existing API, requires changing all callers

### Option 2: No Singleton, Create Fresh Instances
```typescript
export function createReactiveNodeService(events: NodeManagerEvents) {
  const store = new SharedNodeStore(); // ❌ Not a singleton anymore
}
```

**Pros**: No stale reference issues
**Cons**: Defeats purpose of singleton pattern (shared state)

### Option 3: Never Call resetInstance()
```typescript
// Just don't reset between tests
beforeEach(() => {
  // Don't call resetInstance() - reuse singleton
  store.clearAll(); // Only clear data
});
```

**Pros**: Simple, no code changes needed
**Cons**: Less test isolation, can leak state between tests

**Decision**: Dynamic `getInstance()` (our solution) is the best balance of:
- ✅ Preserves singleton pattern
- ✅ Maintains test isolation
- ✅ No breaking API changes
- ✅ Minimal code changes

## Related Patterns

**Lazy Module Pattern** (similar issue):
```typescript
// ❌ Module-level singleton creation
const logger = new Logger(); // Happens at import time
export { logger };

// ✅ Lazy instantiation
let logger: Logger | null = null;
export function getLogger(): Logger {
  if (!logger) logger = new Logger();
  return logger;
}
```

**React Hook Analogy**:
```typescript
// ❌ Create outside component (stale closure)
const store = SharedNodeStore.getInstance();

function MyComponent() {
  useEffect(() => {
    store.updateNode(...); // Stale reference!
  }, []);
}

// ✅ Get inside component (fresh reference)
function MyComponent() {
  useEffect(() => {
    const store = SharedNodeStore.getInstance(); // Fresh!
    store.updateNode(...);
  }, []);
}
```

## Checklist: When You Add a New Singleton

- [ ] Does the singleton have a `resetInstance()` method?
- [ ] Are tests calling `resetInstance()` for isolation?
- [ ] Do services import the singleton at module level?
- [ ] If YES to all: Use dynamic `getInstance()` pattern

## References

- **Issue**: #424 - Fix node type conversions persistence
- **PR**: #425 - Implemented dynamic getInstance() for ReactiveNodeService
- **Files Changed**:
  - `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts:25,57`
  - Test: `slash-command-type-persistence.test.ts`

## Summary

**Problem**: Static singleton imports become stale after `resetInstance()` in tests
**Solution**: Import singleton **class**, call `getInstance()` **dynamically**
**Impact**: Proper test isolation, reliable singleton behavior
**Scope**: Any service using singletons with `resetInstance()` support

---

**Last Updated**: November 6, 2024
**Author**: AI Development Agent
**Status**: Active Pattern (use in new code)
