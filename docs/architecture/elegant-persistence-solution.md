# Elegant Persistence Solution: Priority-Based Flush Control

## Problem Statement

Current architecture has dual persistence paths:
1. `updateNode()` - Debounced persistence (500ms)
2. `saveNodeImmediately()` - Immediate persistence (bypass debounce)

This creates a "backdoor" that bypasses the main update flow.

## Root Cause

The real issue is **lack of control over persistence timing**. We have three timing requirements:

1. **Debounced** - User typing content (500ms batching for performance)
2. **Immediate** - New nodes that will be referenced (FOREIGN KEY coordination)
3. **Ordered** - Structural updates must wait for content saves (dependency tracking)

## Elegant Solution: Single Path with Flush Control

### Core Principle

**Replace "immediate vs debounced" with "flush priority"**

All updates go through ONE method with a `flushStrategy` option:

```typescript
interface UpdateOptions {
  skipConflictDetection?: boolean;
  skipPersistence?: boolean;
  force?: boolean;
  forceNotify?: boolean;

  // NEW: Flush control (replaces the need for saveNodeImmediately)
  flushStrategy?: 'debounce' | 'immediate' | 'manual';
  flushPriority?: 'high' | 'normal' | 'low';
}
```

### Implementation

```typescript
class SharedNodeStore {
  // Single debounce timer per node
  private persistenceTimers = new Map<string, {
    timer: ReturnType<typeof setTimeout>;
    pendingUpdates: NodeUpdate[];
    priority: 'high' | 'normal' | 'low';
  }>();

  // Track pending persistence promises for dependency tracking
  private pendingPersistence = new Map<string, Promise<void>>();

  updateNode(
    nodeId: string,
    changes: Partial<Node>,
    source: UpdateSource,
    options: UpdateOptions = {}
  ): void {
    // 1. Apply update to in-memory store (always immediate)
    const updatedNode = this.applyUpdateToMemory(nodeId, changes, source, options);

    // 2. Determine persistence strategy
    const flushStrategy = options.flushStrategy || 'debounce';
    const flushPriority = options.flushPriority || 'normal';

    // 3. Queue persistence based on strategy
    if (!options.skipPersistence && source.type !== 'database') {
      switch (flushStrategy) {
        case 'immediate':
          // Cancel debounce timer and persist NOW
          this.flushImmediately(nodeId, updatedNode);
          break;

        case 'debounce':
          // Schedule for later (500ms)
          this.schedulePersistence(nodeId, updatedNode, flushPriority);
          break;

        case 'manual':
          // Don't persist until flush() is called explicitly
          this.queueForManualFlush(nodeId, updatedNode);
          break;
      }
    }
  }

  /**
   * Flush pending updates immediately
   * Used for FOREIGN KEY coordination
   */
  async flush(nodeIds?: string[]): Promise<void> {
    const nodesToFlush = nodeIds || Array.from(this.persistenceTimers.keys());

    await Promise.all(
      nodesToFlush.map(nodeId => this.flushImmediately(nodeId))
    );
  }

  /**
   * Wait for specific nodes to be persisted
   * Used for dependency tracking
   */
  async waitForPersistence(nodeIds: string[], timeoutMs = 5000): Promise<Set<string>> {
    const failedNodeIds = new Set<string>();
    const relevantPersistence = nodeIds
      .map(id => this.pendingPersistence.get(id))
      .filter((p): p is Promise<void> => p !== undefined);

    if (relevantPersistence.length === 0) return failedNodeIds;

    try {
      await Promise.race([
        Promise.all(relevantPersistence),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Timeout')), timeoutMs)
        )
      ]);
    } catch (error) {
      console.error('[SharedNodeStore] Timeout waiting for persistence:', error);

      // Check which nodes failed
      for (const nodeId of nodeIds) {
        if (this.pendingPersistence.has(nodeId) && !this.persistedNodeIds.has(nodeId)) {
          failedNodeIds.add(nodeId);
        }
      }
    }

    return failedNodeIds;
  }

  private flushImmediately(nodeId: string, node?: Node): Promise<void> {
    // Cancel any pending timer
    const pending = this.persistenceTimers.get(nodeId);
    if (pending) {
      clearTimeout(pending.timer);
      this.persistenceTimers.delete(nodeId);
    }

    // Get node if not provided
    const nodeToSave = node || this.nodes.get(nodeId);
    if (!nodeToSave) return Promise.resolve();

    // Create persistence promise and track it
    const persistPromise = queueDatabaseWrite(nodeId, async () => {
      const isPersistedToDatabase = this.persistedNodeIds.has(nodeId);
      if (isPersistedToDatabase) {
        await tauriNodeService.updateNode(nodeId, nodeToSave);
      } else {
        await tauriNodeService.createNode(nodeToSave);
        this.persistedNodeIds.add(nodeId);
      }
    });

    // Track for dependency waiting
    this.pendingPersistence.set(nodeId, persistPromise);

    persistPromise.finally(() => {
      this.pendingPersistence.delete(nodeId);
    });

    return persistPromise;
  }

  private schedulePersistence(
    nodeId: string,
    node: Node,
    priority: 'high' | 'normal' | 'low'
  ): void {
    // Cancel existing timer
    const existing = this.persistenceTimers.get(nodeId);
    if (existing) {
      clearTimeout(existing.timer);
    }

    // Determine debounce delay based on priority
    const delay = priority === 'high' ? 100 : priority === 'normal' ? 500 : 1000;

    // Schedule new timer
    const timer = setTimeout(() => {
      this.flushImmediately(nodeId, node);
      this.persistenceTimers.delete(nodeId);
    }, delay);

    this.persistenceTimers.set(nodeId, {
      timer,
      pendingUpdates: [],
      priority
    });
  }
}
```

## Usage Examples

### Example 1: Content Editing (Debounced)

```typescript
// User typing - debounce for performance
sharedNodeStore.updateNode(
  nodeId,
  { content: newContent },
  viewerSource,
  { flushStrategy: 'debounce' }  // Default behavior
);
```

### Example 2: New Node Creation (Immediate)

```typescript
// New node that will be referenced - flush immediately
sharedNodeStore.updateNode(
  nodeId,
  { content, parentId, beforeSiblingId },
  viewerSource,
  { flushStrategy: 'immediate' }  // Persist NOW
);

// Track the pending persistence
const persistPromise = sharedNodeStore.getPendingPersistence(nodeId);
```

### Example 3: Structural Updates (Ordered)

```typescript
// Wait for dependencies to be persisted
const failedNodes = await sharedNodeStore.waitForPersistence([parentId, beforeSiblingId]);

if (failedNodes.size === 0) {
  // Safe to update structure
  sharedNodeStore.updateNode(
    nodeId,
    { parentId, beforeSiblingId },
    viewerSource,
    { flushStrategy: 'immediate' }  // Structural changes are immediate
  );
}
```

### Example 4: Batch Operations (Manual Flush)

```typescript
// Queue multiple updates
for (const node of nodesToUpdate) {
  sharedNodeStore.updateNode(
    node.id,
    node.changes,
    viewerSource,
    { flushStrategy: 'manual' }  // Don't persist yet
  );
}

// Flush all at once in correct order
await sharedNodeStore.flush();
```

## Benefits

### 1. **Single Code Path**
- No `saveNodeImmediately()` backdoor
- All persistence goes through `updateNode()`
- Easier to understand, test, and maintain

### 2. **Explicit Control**
```typescript
// Clear intent - "this needs to persist NOW"
{ flushStrategy: 'immediate' }

// vs ambiguous current approach
saveNodeImmediately(...)  // Why immediate? What's special?
```

### 3. **Flexible Priority System**
```typescript
// High priority content (100ms debounce)
{ flushStrategy: 'debounce', flushPriority: 'high' }

// Normal edits (500ms debounce)
{ flushStrategy: 'debounce', flushPriority: 'normal' }

// Low priority metadata (1000ms debounce)
{ flushStrategy: 'debounce', flushPriority: 'low' }
```

### 4. **Dependency Tracking Built-In**
```typescript
// Dependencies are explicit in the API
await sharedNodeStore.waitForPersistence([parentId, siblingId]);
```

### 5. **Batch Operations Support**
```typescript
// Useful for bulk imports, migrations, etc.
{ flushStrategy: 'manual' }
await sharedNodeStore.flush();
```

## Migration Path

### Phase 1: Add New Options (Non-Breaking)
```typescript
// Old code still works
saveNodeImmediately(...)  // Internally calls updateNode with flushStrategy: 'immediate'

// New code can use elegant API
updateNode(..., { flushStrategy: 'immediate' })
```

### Phase 2: Migrate Call Sites
```typescript
// Replace saveNodeImmediately calls
- await sharedNodeStore.saveNodeImmediately(nodeId, content, ...);
+ sharedNodeStore.updateNode(nodeId, { content, ... }, source, {
+   flushStrategy: 'immediate'
+ });
+ await sharedNodeStore.waitForPersistence([nodeId]);
```

### Phase 3: Remove Deprecated Method
```typescript
// Delete saveNodeImmediately() entirely
// All code now uses unified updateNode() API
```

## Comparison

### Current Architecture (Backdoor)
```typescript
// Two separate code paths
updateNode(...)           // Debounced, goes through one path
saveNodeImmediately(...)  // Immediate, goes through different path

// Unclear why immediate is needed
// Code duplication between paths
// Hard to test both paths consistently
```

### Elegant Architecture (Unified)
```typescript
// Single code path with explicit control
updateNode(..., { flushStrategy: 'immediate' })   // Clear intent
updateNode(..., { flushStrategy: 'debounce' })    // Default behavior
updateNode(..., { flushStrategy: 'manual' })      // Batch control

// Clear separation of concerns:
// - updateNode() handles in-memory updates (always immediate)
// - flushStrategy controls persistence timing (configurable)
// - waitForPersistence() handles dependencies (explicit)
```

## Implementation Checklist

- [ ] Add `flushStrategy` and `flushPriority` to `UpdateOptions`
- [ ] Implement `flushImmediately()` method
- [ ] Implement `schedulePersistence()` method
- [ ] Implement `flush()` method for manual control
- [ ] Refactor `updateNode()` to use flush strategies
- [ ] Add `waitForPersistence()` method (rename from `waitForNodeSaves`)
- [ ] Update BaseNodeViewer to use new API
- [ ] Add comprehensive tests for all flush strategies
- [ ] Deprecate `saveNodeImmediately()` with migration guide
- [ ] Remove `saveNodeImmediately()` after migration complete

## Conclusion

The elegant solution is to **unify the persistence path** while providing **explicit control** over timing and ordering. This eliminates the "backdoor" by making persistence strategy a **first-class option** instead of requiring a separate method.

Key insight: **Immediate persistence isn't a special case - it's just a different flush strategy.**
