# Dependency-Based Persistence Architecture

> **Note (Issue #614)**: This document references `beforeSiblingId` field which has been
> removed from the node model. Sibling ordering now uses fractional `order` field on
> `has_child` edges. The dependency coordination patterns still apply, but the specific
> fields mentioned need to be updated to reflect edge-based ordering.

## Core Concept

Instead of manually coordinating with `waitForPersistence()`, **declare dependencies upfront** and let the system enforce ordering automatically.

## Design: Lambda-Based Dependency Declaration

### API Design

```typescript
interface PersistenceOptions {
  skipConflictDetection?: boolean;
  skipPersistence?: boolean;
  force?: boolean;
  forceNotify?: boolean;

  // NEW: Declarative dependency management
  persistenceMode?: 'debounce' | 'immediate';
  debounceMs?: number; // Default 500ms

  // Declare what must complete BEFORE this operation
  dependsOn?: Array<
    | string                          // Simple: node ID
    | (() => Promise<void>)          // Lambda: any async operation
    | { nodeIds: string[] }          // Batch: multiple nodes
    | PersistenceHandle              // Handle: returned from previous operation
  >;
}

// Handle returned from updateNode - can be used as dependency
interface PersistenceHandle {
  nodeId: string;
  promise: Promise<void>;
  isPersisted: () => boolean;
}
```

### Implementation

```typescript
class SharedNodeStore {
  // Track all pending persistence operations
  private pendingOperations = new Map<string, PersistenceHandle>();

  /**
   * Update node with declarative dependency management
   */
  updateNode(
    nodeId: string,
    changes: Partial<Node>,
    source: UpdateSource,
    options: PersistenceOptions = {}
  ): PersistenceHandle {
    // 1. Apply in-memory update immediately
    const updatedNode = this.applyUpdateToMemory(nodeId, changes, source);

    // 2. Create persistence operation
    const persistOperation = this.createPersistenceOperation(
      nodeId,
      updatedNode,
      options
    );

    // 3. Return handle for chaining
    return persistOperation;
  }

  private createPersistenceOperation(
    nodeId: string,
    node: Node,
    options: PersistenceOptions
  ): PersistenceHandle {
    // Create the actual persistence work
    const doPersistence = async () => {
      // Wait for dependencies FIRST
      if (options.dependsOn) {
        await this.resolveDependencies(options.dependsOn);
      }

      // Then persist this node
      if (!options.skipPersistence) {
        const isPersistedToDatabase = this.persistedNodeIds.has(nodeId);
        if (isPersistedToDatabase) {
          await tauriNodeService.updateNode(nodeId, node);
        } else {
          await tauriNodeService.createNode(node);
          this.persistedNodeIds.add(nodeId);
        }
      }
    };

    // Wrap with debouncing if requested
    const persistPromise = options.persistenceMode === 'immediate'
      ? this.executeImmediately(nodeId, doPersistence)
      : this.executeDebounced(nodeId, doPersistence, options.debounceMs || 500);

    // Create handle
    const handle: PersistenceHandle = {
      nodeId,
      promise: persistPromise,
      isPersisted: () => this.persistedNodeIds.has(nodeId)
    };

    // Track it
    this.pendingOperations.set(nodeId, handle);

    // Cleanup when done
    persistPromise.finally(() => {
      if (this.pendingOperations.get(nodeId) === handle) {
        this.pendingOperations.delete(nodeId);
      }
    });

    return handle;
  }

  /**
   * Resolve all dependencies before proceeding
   */
  private async resolveDependencies(
    dependencies: Array<string | (() => Promise<void>) | { nodeIds: string[] } | PersistenceHandle>
  ): Promise<void> {
    const promises: Promise<void>[] = [];

    for (const dep of dependencies) {
      if (typeof dep === 'string') {
        // Simple node ID dependency
        const handle = this.pendingOperations.get(dep);
        if (handle) promises.push(handle.promise);
      }
      else if (typeof dep === 'function') {
        // Lambda function dependency
        promises.push(dep());
      }
      else if ('nodeIds' in dep) {
        // Batch node IDs
        for (const nodeId of dep.nodeIds) {
          const handle = this.pendingOperations.get(nodeId);
          if (handle) promises.push(handle.promise);
        }
      }
      else if ('promise' in dep) {
        // PersistenceHandle
        promises.push(dep.promise);
      }
    }

    // Wait for all dependencies in parallel
    if (promises.length > 0) {
      await Promise.all(promises);
    }
  }

  private executeImmediately(
    nodeId: string,
    operation: () => Promise<void>
  ): Promise<void> {
    // Cancel any pending debounce
    this.cancelDebounce(nodeId);

    // Execute via queue to prevent concurrent writes to same node
    return queueDatabaseWrite(nodeId, operation);
  }

  private executeDebounced(
    nodeId: string,
    operation: () => Promise<void>,
    delayMs: number
  ): Promise<void> {
    // Cancel existing debounce
    this.cancelDebounce(nodeId);

    // Create new debounced promise
    return new Promise((resolve, reject) => {
      const timer = setTimeout(async () => {
        try {
          await queueDatabaseWrite(nodeId, operation);
          resolve();
        } catch (error) {
          reject(error);
        }
      }, delayMs);

      this.debounceTimers.set(nodeId, timer);
    });
  }
}
```

## Usage Examples

### Example 1: Simple Dependency (Node ID)

```typescript
// Create a new parent node
const parentHandle = sharedNodeStore.updateNode(
  parentId,
  { content: 'Parent', nodeType: 'text' },
  viewerSource,
  { persistenceMode: 'immediate' }
);

// Create child - depends on parent being persisted first
const childHandle = sharedNodeStore.updateNode(
  childId,
  { content: 'Child', parentId },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [parentId]  // Simple: just pass the parent's ID
  }
);

// System automatically waits for parent before persisting child!
```

### Example 2: Lambda Function Dependency

```typescript
// Complex pre-condition: ensure ALL ancestors are persisted
const handle = sharedNodeStore.updateNode(
  nodeId,
  { content, parentId },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [
      // Lambda: custom logic
      async () => {
        // Recursively persist ancestor chain
        await ensureAncestorChainPersisted(parentId);
      }
    ]
  }
);
```

### Example 3: Multiple Dependencies (Batch)

```typescript
// Structural update depends on multiple nodes
const handle = sharedNodeStore.updateNode(
  nodeId,
  { parentId, beforeSiblingId },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [
      { nodeIds: [parentId, beforeSiblingId].filter(Boolean) }
    ]
  }
);

// System waits for BOTH parent and sibling before updating structure
```

### Example 4: Handle-Based Dependency (Chaining)

```typescript
// Create nodes in sequence with explicit chaining
const handle1 = sharedNodeStore.updateNode(
  node1Id,
  { content: 'First' },
  viewerSource,
  { persistenceMode: 'immediate' }
);

const handle2 = sharedNodeStore.updateNode(
  node2Id,
  { content: 'Second', beforeSiblingId: node1Id },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [handle1]  // Explicit handle dependency
  }
);

const handle3 = sharedNodeStore.updateNode(
  node3Id,
  { content: 'Third', beforeSiblingId: node2Id },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [handle2]  // Chain continues
  }
);

// Guaranteed order: node1 → node2 → node3
```

### Example 5: Complex Lambda with Error Handling

```typescript
const handle = sharedNodeStore.updateNode(
  nodeId,
  { content, parentId, beforeSiblingId },
  viewerSource,
  {
    persistenceMode: 'immediate',
    dependsOn: [
      // Complex pre-condition with validation
      async () => {
        // Validate parent exists
        const parent = await tauriNodeService.getNode(parentId);
        if (!parent) {
          throw new Error(`Parent ${parentId} not found`);
        }

        // Ensure sibling is persisted
        const siblingHandle = sharedNodeStore.pendingOperations.get(beforeSiblingId);
        if (siblingHandle) {
          await siblingHandle.promise;
        }

        // Custom validation logic
        if (!isValidStructure(parent, nodeId)) {
          throw new Error('Invalid hierarchy structure');
        }
      }
    ]
  }
);
```

### Example 6: Real-World - BaseNodeViewer Content Watcher

```typescript
// Current complex code:
if (isNewNode) {
  const savePromise = (async () => {
    await ensureAncestorsPersisted(node.id);
    await sharedNodeStore.saveNodeImmediately(...);
  })();
  pendingContentSavePromises.set(node.id, savePromise);
}

// Elegant version with dependencies:
if (isNewNode) {
  const handle = sharedNodeStore.updateNode(
    node.id,
    { content: node.content, nodeType: node.nodeType, parentId, ... },
    VIEWER_SOURCE,
    {
      persistenceMode: 'immediate',
      dependsOn: [
        // Lambda: ensure ancestors
        async () => await ensureAncestorsPersisted(node.id)
      ]
    }
  );

  // Store handle for structural watcher to depend on
  pendingContentSavePromises.set(node.id, handle.promise);
}
```

### Example 7: Real-World - Structural Watcher

```typescript
// Current complex code:
const nodeIdsToWaitFor: string[] = [];
for (const update of updates) {
  if (update.parentId) nodeIdsToWaitFor.push(update.parentId);
  if (update.beforeSiblingId) nodeIdsToWaitFor.push(update.beforeSiblingId);
}
const failedNodeIds = await waitForNodeSavesIfPending(nodeIdsToWaitFor);

// Elegant version with dependencies:
const result = await sharedNodeStore.updateStructuralChangesValidated(
  updates.map(update => ({
    nodeId: update.nodeId,
    changes: { parentId: update.parentId, beforeSiblingId: update.beforeSiblingId },
    options: {
      persistenceMode: 'immediate',
      dependsOn: [
        { nodeIds: [update.parentId, update.beforeSiblingId].filter(Boolean) }
      ]
    }
  })),
  VIEWER_SOURCE,
  parentId
);

// System automatically handles all dependency coordination!
```

## Advanced Pattern: Dependency Graph

For even more complex scenarios, dependencies can form a graph:

```typescript
// Build a dependency graph
const operations = [
  { id: 'root', deps: [] },
  { id: 'child1', deps: ['root'] },
  { id: 'child2', deps: ['root'] },
  { id: 'grandchild1', deps: ['child1'] },
  { id: 'grandchild2', deps: ['child1', 'child2'] },
];

const handles = new Map<string, PersistenceHandle>();

// Execute with automatic topological ordering
for (const op of operations) {
  const handle = sharedNodeStore.updateNode(
    op.id,
    { content: `Node ${op.id}` },
    viewerSource,
    {
      persistenceMode: 'immediate',
      dependsOn: op.deps.map(depId => handles.get(depId)!).filter(Boolean)
    }
  );

  handles.set(op.id, handle);
}

// System automatically resolves the graph:
// root → child1 → grandchild1
//     ↘ child2 ↗ grandchild2
```

## Benefits

### 1. **Declarative, Not Imperative**

```typescript
// ❌ Imperative (current)
await waitForNodeSaves([parentId]);
if (failed.size === 0) {
  await updateNode(...);
}

// ✅ Declarative (elegant)
updateNode(..., {
  dependsOn: [parentId]
});
```

### 2. **No Manual Coordination**

```typescript
// ❌ Manual tracking (current)
const savePromise = saveNodeImmediately(...);
pendingContentSavePromises.set(nodeId, savePromise);
// ... later ...
const failed = await waitForNodeSavesIfPending([nodeId]);

// ✅ Automatic tracking (elegant)
const handle = updateNode(..., {
  persistenceMode: 'immediate'
});
// dependsOn: [handle] - system tracks it!
```

### 3. **Type-Safe Dependencies**

```typescript
// ✅ Lambda dependencies are type-checked
updateNode(..., {
  dependsOn: [
    async () => {
      // TypeScript ensures this returns Promise<void>
      await customValidation();
    }
  ]
});
```

### 4. **Composable Dependencies**

```typescript
// Mix different dependency types
updateNode(..., {
  dependsOn: [
    'node-id-1',                    // String
    handle2,                         // Handle
    { nodeIds: ['n1', 'n2'] },      // Batch
    async () => { await custom() }   // Lambda
  ]
});
```

### 5. **Self-Documenting Code**

```typescript
// Intent is crystal clear
updateNode(childId, { parentId }, source, {
  persistenceMode: 'immediate',
  dependsOn: [parentId]  // "This child depends on parent being saved first"
});
```

### 6. **Testable Dependencies**

```typescript
// Easy to test dependency resolution
const mockDependency = vi.fn(async () => {
  // Simulate slow dependency
  await sleep(100);
});

updateNode(..., {
  dependsOn: [mockDependency]
});

expect(mockDependency).toHaveBeenCalledBefore(persistenceCall);
```

## Implementation Strategy

### Phase 1: Add Dependency Support (Non-Breaking)

```typescript
// Extend UpdateOptions
interface PersistenceOptions extends UpdateOptions {
  persistenceMode?: 'debounce' | 'immediate';
  dependsOn?: Array<...>;
}

// Add handle return type
updateNode(...): PersistenceHandle {
  // Implementation
}
```

### Phase 2: Refactor BaseNodeViewer

```typescript
// Replace manual coordination with dependencies
- const savePromise = saveNodeImmediately(...);
- pendingContentSavePromises.set(nodeId, savePromise);
+ const handle = updateNode(..., {
+   persistenceMode: 'immediate',
+   dependsOn: [ancestorLambda]
+ });
```

### Phase 3: Remove `saveNodeImmediately`

```typescript
// No longer needed - updateNode handles everything
- saveNodeImmediately(...)
+ updateNode(..., { persistenceMode: 'immediate' })
```

### Phase 4: Remove Manual Coordination

```typescript
// No longer needed - dependencies handle it
- waitForNodeSavesIfPending(...)
- pendingContentSavePromises
+ // System tracks dependencies automatically
```

## Error Handling

Dependencies can fail - system handles this gracefully:

```typescript
const handle = updateNode(nodeId, changes, source, {
  persistenceMode: 'immediate',
  dependsOn: [
    async () => {
      const result = await riskyOperation();
      if (!result.success) {
        throw new Error('Dependency failed');
      }
    }
  ]
});

// Caller can handle dependency failures
try {
  await handle.promise;
} catch (error) {
  console.error('Failed due to dependency:', error);
  // Handle rollback, retry, etc.
}
```

### Operation Cancellation

When operations are superseded (e.g., rapid typing triggers multiple debounced saves), the system cancels pending operations using a custom error type:

```typescript
import { OperationCancelledError } from './persistence-coordinator.svelte';

// Capture persistence handle
const handle = updateNode(nodeId, changes, source, {
  persistenceMode: 'debounce'
});

// Handle cancellation separately from real errors
handle.promise.catch((err) => {
  if (err instanceof OperationCancelledError) {
    // Operation was superseded by a newer operation - this is expected
    console.debug('Operation cancelled:', err.message);
    return;
  }
  // Real error - handle appropriately
  console.error('Persistence failed:', err);
  showErrorToUser(err);
});
```

**Why OperationCancelledError?**

- **Distinguishes control flow from failures**: Cancellation is expected behavior, not an error
- **Type-safe error handling**: `instanceof` checks allow filtering cancellations from real errors
- **Prevents unhandled rejections**: Tests and production code can catch cancellations explicitly
- **Maintains promise semantics**: Operations still reject (maintaining API contract) but with explicit intent

**When operations are cancelled:**
- Newer operation supersedes pending debounced operation
- Test cleanup calls `reset()` to cancel all pending operations
- Node is deleted while its persistence is pending

## Timeout Handling

Dependencies can timeout:

```typescript
const handle = updateNode(nodeId, changes, source, {
  persistenceMode: 'immediate',
  dependsOn: [parentId],
  dependencyTimeout: 5000  // 5 second timeout
});

// System automatically times out if dependencies take too long
```

## Comparison

### Current Architecture (Manual)

```typescript
// 1. Save node immediately
const savePromise = saveNodeImmediately(nodeId, ...);
pendingContentSavePromises.set(nodeId, savePromise);

// 2. Later, manually wait
const failed = await waitForNodeSavesIfPending([nodeId]);

// 3. Check failures manually
if (failed.size === 0) {
  // OK to proceed
}

// Problems:
// - Manual tracking of promises
// - Manual checking of failures
// - Easy to forget to wait
// - No type safety
// - Hard to test
```

### Elegant Architecture (Declarative)

```typescript
// 1. Declare dependency upfront
const handle = updateNode(childId, { parentId }, source, {
  persistenceMode: 'immediate',
  dependsOn: [parentId]
});

// 2. System handles everything automatically
// - Waits for parent
// - Handles failures
// - Manages ordering
// - Type-safe
// - Testable

// Benefits:
// ✅ Automatic tracking
// ✅ Automatic waiting
// ✅ Type-safe lambdas
// ✅ Self-documenting
// ✅ Easy to test
```

## Conclusion

Lambda-based dependency declaration is elegant because it:

1. **Separates concerns** - Declare WHAT depends on WHAT, not HOW to coordinate
2. **Type-safe** - Lambdas are checked by TypeScript
3. **Flexible** - Mix node IDs, handles, batches, and custom logic
4. **Self-documenting** - Intent is clear from the declaration
5. **Testable** - Dependencies can be mocked easily
6. **Eliminates boilerplate** - No manual promise tracking

The key insight: **Dependencies are configuration, not code.**
