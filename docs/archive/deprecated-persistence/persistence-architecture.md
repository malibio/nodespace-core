# Persistence Architecture

> **⚠️ DEPRECATED**: This document has been consolidated into [`persistence-system.md`](persistence-system.md) which provides comprehensive coverage of the entire persistence system including frontend service layers, placeholder nodes, and complete code examples.
>
> **Please use the consolidated documentation:**
> - **New Location**: [`/docs/architecture/persistence-system.md`](persistence-system.md)
> - **Why Updated**: Eliminates confusion from overlapping docs, adds missing service layer explanations and placeholder node documentation
> - **Migration Date**: 2025-01-21
>
> This file is kept for historical reference only.

---

## Overview

NodeSpace uses a **dual-pattern architecture** for data persistence and service coordination. This document explains when to use each pattern and why both are necessary.

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                      NodeSpace Architecture                  │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Pattern 1: Direct Persistence                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  UI Component ($effect) → Database Service           │   │
│  │  Purpose: Save UI state directly to database         │   │
│  │  Example: base-node-viewer.svelte                    │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  Pattern 2: Event Bus Coordination                           │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Service → Event Bus → Multiple Services             │   │
│  │  Purpose: Coordinate between services                │   │
│  │  Example: nodeReferenceService, hierarchyService     │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Pattern 1: Direct Persistence ($effect → Database)

### When to Use

Use direct database calls when:
- ✅ UI components saving their own state
- ✅ Simple CRUD operations from UI layer
- ✅ High-frequency operations (debounced saves during editing)
- ✅ Clear ownership: component owns the data it displays

### Implementation

**File**: `src/lib/design/components/base-node-viewer.svelte`

```typescript
// Watch for content changes and save
$effect(() => {
  if (!parentId) return;

  const nodes = nodeManager.visibleNodes;

  for (const node of nodes) {
    // Skip placeholder nodes
    if (node.isPlaceholder) continue;

    // Only save if content has changed since last save
    const lastContent = lastSavedContent.get(node.id);
    if (node.content.trim() && node.content !== lastContent) {
      debounceSave(node.id, node.content, node.nodeType);
    }
  }
});

// Watch for deletions and persist
$effect(() => {
  if (!parentId) return;

  const currentNodeIds = new Set(nodeManager.visibleNodes.map((n) => n.id));

  // Skip first run
  if (previousNodeIds.size > 0) {
    // Detect deleted nodes
    for (const prevId of previousNodeIds) {
      if (!currentNodeIds.has(prevId) && !nodeManager.findNode(prevId)) {
        // Node was deleted - persist to database
        databaseService.deleteNode(prevId);
      }
    }
  }

  // Update tracking (mutate Set to avoid re-triggering)
  previousNodeIds.clear();
  for (const id of currentNodeIds) {
    previousNodeIds.add(id);
  }
});
```

### Why This Pattern

**Advantages**:
- **Simple**: Direct path from UI change to database (2 steps)
- **Performant**: No event overhead (~0.1ms per operation)
- **Svelte-native**: Leverages $effect exactly as designed
- **Clear responsibility**: Component owns its persistence
- **Easy debugging**: Direct call stack
- **Type-safe**: Full TypeScript checking on function calls

**Performance**:
```
UI change → $effect → databaseService.save() → Tauri IPC
Overhead: ~0.1ms (one function call)
```

## Pattern 2: Event Bus Coordination (Service ↔ Event Bus)

### When to Use

Use event bus when:
- ✅ Services coordinating with each other
- ✅ Cache invalidation across multiple services
- ✅ Graph relationship maintenance (references, hierarchy)
- ✅ Future extensibility (AI features, analytics, telemetry)
- ✅ Multiple handlers need to react to same event

### Implementation

**Emitting Events** (from `reactiveNodeService.svelte.ts`):

```typescript
function combineNodes(currentNodeId: string, previousNodeId: string): void {
  // ... perform in-memory operations ...

  // Remove from sibling chain
  removeFromSiblingChain(currentNodeId);

  // Delete from memory
  delete _nodes[currentNodeId];
  delete _uiState[currentNodeId];

  // Invalidate caches
  invalidateSortedChildrenCache(currentNode.parentId);

  // Emit coordination events
  events.nodeDeleted(currentNodeId);      // Services can react
  events.hierarchyChanged();              // Hierarchy changed
}
```

**Subscribing to Events** (from various services):

```typescript
// nodeReferenceService.ts - Clean up references
eventBus.subscribe('node:deleted', (event) => {
  const nodeEvent = event as NodeDeletedEvent;
  // Find all nodes that reference the deleted node
  cleanupDeletedNodeReferences(nodeEvent.nodeId);
});

// hierarchyService.ts - Update hierarchy caches
eventBus.subscribe('node:deleted', (event) => {
  const nodeEvent = event as NodeDeletedEvent;
  // Remove from all hierarchy caches
  invalidateNodeCache(nodeEvent.nodeId);
});

// contentProcessor.ts - Update content references
eventBus.subscribe('node:deleted', (event) => {
  const nodeEvent = event as NodeDeletedEvent;
  // Invalidate reference cache
  invalidateReferenceCache(nodeEvent.nodeId);
});

// cacheCoordinator.ts - Coordinate cache invalidation
eventBus.subscribe('node:deleted', (event) => {
  handleNodeLifecycleEvent(event as NodeDeletedEvent);
});
```

### Why This Pattern

**Advantages**:
- **Decoupled**: Services don't directly depend on each other
- **Extensible**: Add new services without modifying existing code
- **Observable**: Multiple services react to same event
- **Coordination**: Complex operations across service boundaries
- **Future-ready**: Easy to add AI, sync, analytics features

**Example - Adding New Feature**:
```typescript
// Add real-time sync in the future - zero changes to existing code
class SyncService {
  constructor() {
    eventBus.subscribe('node:updated', this.syncToCloud);
    eventBus.subscribe('node:deleted', this.syncDeletion);
  }
}
```

## How They Work Together

### Example: User Backspaces to Combine Nodes

```
1. User action: Backspace from Node B onto Node A
         ↓
2. combineNodes() executes (in reactiveNodeService)
         ↓
   ┌─────┴─────────────────────────────────────────┐
   │  IN-MEMORY OPERATIONS (Pattern 2 context)     │
   ├─→ Combine content                             │
   ├─→ Transfer children                           │
   ├─→ Remove from sibling chain                   │
   ├─→ Delete from _nodes map                      │
   ├─→ Invalidate caches                           │
   ├─→ events.nodeDeleted(nodeB.id) ← EMIT EVENT   │
   └─────┬─────────────────────────────────────────┘
         │
         ├─→ Event Bus broadcasts 'node:deleted'
         │         ↓
         │    ┌────┴────┬──────────┬─────────┬──────────┐
         │    ↓         ↓          ↓         ↓          ↓
         │  cache    content   hierarchy  nodeRef   (future)
         │  Coord.   Proc.     Service    Service   services
         │    ↓         ↓          ↓         ↓          ↓
         │  Clear    Clear      Clear     Clean up   React
         │  cache    cache      cache     refs       here
         │
         ↓
3. UI updates (Svelte reactivity)
   visibleNodes no longer includes Node B
         ↓
4. $effect detects change (Pattern 1)
         ↓
   ┌─────┴──────────────────────────────┐
   │  DATABASE PERSISTENCE              │
   ├─→ previousNodeIds has Node B       │
   ├─→ currentNodeIds doesn't have B    │
   ├─→ databaseService.deleteNode(B)    │
   └────────────────────────────────────┘
         ↓
5. ✅ Complete: Memory clean, services coordinated, database updated
```

## Decision Rationale

### Why Not Pure Event Bus?

We could theoretically consolidate everything to event bus:

```typescript
// All persistence through events (NOT RECOMMENDED)
$effect(() => {
  if (contentChanged) {
    eventBus.emit('node:updated', node);  // Instead of direct save
  }
});

// New DatabaseSyncService would handle persistence
eventBus.subscribe('node:updated', async (event) => {
  await databaseService.save(event.node);
});
```

**Why This Is Worse**:

1. ❌ **Extra indirection**: 4 steps instead of 2 (UI → Event → Handler → Database)
2. ❌ **Performance overhead**: 5-10x slower (~0.5-1ms vs ~0.1ms)
3. ❌ **Added complexity**: New DatabaseSyncService to maintain and test
4. ❌ **Harder debugging**: Async event flow vs direct call stack
5. ❌ **Fighting the framework**: Svelte $effect is designed for direct side effects
6. ❌ **Unclear ownership**: Who's responsible for persistence?
7. ❌ **No real benefit**: UI is the natural owner of its state persistence

### Performance Comparison

**Text editing scenario** (typing in a node):

```
Direct Persistence ($effect):
  Keystroke → debounce → $effect → databaseService.save()
  Time: ~0.1ms

Event Bus Persistence:
  Keystroke → debounce → $effect → eventBus.emit() →
  handler lookup → DatabaseSyncService → databaseService.save()
  Time: ~0.5-1ms (5-10x slower)
```

For a knowledge management system with frequent edits, this overhead matters.

### Architectural Principles

The dual approach correctly separates concerns:

- **Persistence concerns**: Component owns saving its state ✅
- **Coordination concerns**: Services coordinate via events ✅

A pure event bus would **conflate** these concerns, making persistence a coordination problem when it's actually an ownership problem.

## Guidelines for Developers

### Use Direct Persistence When:

```typescript
// ✅ UI component saving its own state
class MyComponent {
  $effect(() => {
    if (myDataChanged) {
      databaseService.saveMyData(data);
    }
  });
}
```

### Use Event Bus When:

```typescript
// ✅ Coordinating between services
class MyService {
  doSomething() {
    // Perform action
    const result = performAction();

    // Let other services know
    eventBus.emit('action:completed', result);
  }
}

// Other services react
class OtherService {
  constructor() {
    eventBus.subscribe('action:completed', (result) => {
      this.handleActionCompleted(result);
    });
  }
}
```

### Never Mix Concerns:

```typescript
// ❌ BAD: UI emitting coordination events
$effect(() => {
  eventBus.emit('user:typing', content);  // Don't do this
});

// ❌ BAD: Service calling UI persistence
class MyService {
  doSomething() {
    componentRef.saveToDatabase();  // Don't do this
  }
}
```

## Testing Strategies

### Testing Direct Persistence

```typescript
test('base-node-viewer saves on content change', async () => {
  const mockDatabase = createMockDatabase();

  const viewer = mount(BaseNodeViewer, {
    props: {
      databaseService: mockDatabase,
      nodeManager: mockNodeManager
    }
  });

  await updateNodeContent(viewer, 'new content');

  expect(mockDatabase.saveNodeWithParent).toHaveBeenCalledWith(
    nodeId,
    expect.objectContaining({ content: 'new content' })
  );
});
```

### Testing Event Bus Coordination

```typescript
test('services coordinate on node deletion', async () => {
  const events: NodeDeletedEvent[] = [];

  eventBus.subscribe('node:deleted', (event) => {
    events.push(event as NodeDeletedEvent);
  });

  nodeManager.combineNodes('node2', 'node1');

  expect(events).toContainEqual({
    type: 'node:deleted',
    nodeId: 'node2',
    namespace: 'state',
    source: 'ReactiveNodeService'
  });
});
```

## Future Extensibility

The dual architecture is ready for future features:

### Real-time Sync (Phase 3+)

```typescript
class SyncService {
  constructor(eventBus) {
    // Subscribe to existing events - zero changes needed
    eventBus.subscribe('node:updated', this.syncToCloud);
    eventBus.subscribe('node:deleted', this.syncDeletion);
  }

  async syncToCloud(event) {
    await cloudAPI.sync(event);
  }
}
```

### AI Embeddings (Phase 3+)

```typescript
class EmbeddingService {
  constructor(eventBus) {
    // Subscribe to existing events
    eventBus.subscribe('node:updated', this.generateEmbedding);
  }

  async generateEmbedding(event) {
    const embedding = await aiModel.embed(event.content);
    await vectorDB.store(event.nodeId, embedding);
  }
}
```

### Analytics (Future)

```typescript
class AnalyticsService {
  constructor(eventBus) {
    eventBus.subscribe('node:created', this.trackCreation);
    eventBus.subscribe('node:deleted', this.trackDeletion);
  }
}
```

**No architectural changes needed** - just add new subscribers.

## Common Patterns

### Debounced Saves

```typescript
const saveTimeouts = new Map<string, NodeJS.Timeout>();

function debounceSave(nodeId: string, content: string) {
  // Clear existing timeout
  const existing = saveTimeouts.get(nodeId);
  if (existing) clearTimeout(existing);

  // Debounce 500ms
  const timeout = setTimeout(async () => {
    await databaseService.saveNodeWithParent(nodeId, { content });
    lastSavedContent.set(nodeId, content);
    saveTimeouts.delete(nodeId);
  }, 500);

  saveTimeouts.set(nodeId, timeout);
}
```

### Event Coordination with Error Handling

```typescript
class MyService {
  constructor() {
    eventBus.subscribe('node:deleted', async (event) => {
      try {
        await this.handleDeletion(event);
      } catch (error) {
        console.error('MyService: Error handling deletion', {
          error,
          nodeId: event.nodeId
        });
        // Service errors shouldn't crash the app
      }
    });
  }
}
```

## Related Documentation

- [Frontend Architecture](./frontend-architecture.md) - Overall frontend design
- [Event Bus Design](./core/event-bus.md) - Event system details
- [Database Layer](./data/database-architecture.md) - Database design
- [Component Architecture](./components/component-architecture-guide.md) - Component patterns

## Changelog

| Date | Change | Author |
|------|--------|--------|
| 2025-10-07 | Initial documentation of dual-pattern architecture | Claude Code |
| 2025-10-07 | Added node deletion fix context and rationale | Claude Code |
