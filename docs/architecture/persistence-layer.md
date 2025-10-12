# Persistence Layer Architecture

## Overview

NodeSpace's persistence layer coordinates asynchronous database operations with FOREIGN KEY constraint enforcement and multi-source update handling. This document describes the three-tier architecture for managing node persistence.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    UI Layer (Svelte 5)                          │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  BaseNodeViewer ($effect watchers)                       │  │
│  │  - Content watcher: Detects content changes             │  │
│  │  - Structural watcher: Detects hierarchy changes        │  │
│  │  - Deletion watcher: Detects node deletions             │  │
│  └──────────────────────────────────────────────────────────┘  │
│                            ↓                                    │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  ReactiveNodeService                                     │  │
│  │  - Orchestrates multi-step operations                    │  │
│  │  - Example: combineNodes() → merge + promote + delete   │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│              Coordination Layer (NEW)                           │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  PersistenceCoordinator.svelte.ts                        │  │
│  │                                                           │  │
│  │  Responsibilities:                                        │  │
│  │  • Dependency tracking (declarative)                     │  │
│  │  • Debouncing coordination (configurable delays)         │  │
│  │  • Operation ordering (topological sort)                 │  │
│  │  • Conflict detection (version checking)                 │  │
│  │  • Status tracking (reactive via SvelteMap)              │  │
│  │                                                           │  │
│  │  API:                                                     │  │
│  │    persist(nodeId, operation, {                          │  │
│  │      mode: 'immediate' | 'debounce',                     │  │
│  │      dependencies: [nodeIds, lambdas, handles]           │  │
│  │    })                                                     │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│                Data Layer                                        │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  SharedNodeStore.ts                                       │  │
│  │  - In-memory reactive state (SvelteMap)                  │  │
│  │  - Multi-source update handling                          │  │
│  │  - Subscriber notifications                              │  │
│  │  - Conflict resolution (Last-Write-Wins)                 │  │
│  │                                                           │  │
│  │  Delegates persistence to PersistenceCoordinator         │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│              Backend Layer (Rust + Tauri)                       │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  TauriNodeService.ts                                      │  │
│  │  - IPC adapter for Tauri commands                        │  │
│  │  - Serialization queue (queueDatabaseWrite)              │  │
│  └──────────────────────────────────────────────────────────┘  │
│                            ↓                                    │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Rust Backend (src-tauri)                                │  │
│  │  - SQLite database operations                            │  │
│  │  - FOREIGN KEY constraint enforcement                    │  │
│  │  - Transaction management                                │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Layer Responsibilities

### 1. UI Layer (BaseNodeViewer + ReactiveNodeService)

**Role:** Orchestrator - decides WHEN and in WHAT ORDER operations happen

**Responsibilities:**
- Detect user actions (typing, indent, delete, merge)
- Trigger multi-step operations (e.g., merge node → promote children → delete)
- Provide immediate UI feedback (optimistic updates)

**Key Files:**
- `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte`
- `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`

**Example:**
```typescript
// ReactiveNodeService.combineNodes()
function combineNodes(deletedNodeId, targetNodeId) {
  // Step 1: Merge content (in-memory, immediate)
  sharedNodeStore.updateNode(targetNodeId, { content: merged }, source);

  // Step 2: Promote children (in-memory, immediate)
  for (const child of children) {
    sharedNodeStore.updateNode(child.id, { parentId: newParent }, source);
  }

  // Step 3: Delete node (in-memory, immediate)
  sharedNodeStore.deleteNode(deletedNodeId, source);

  // All changes visible in UI instantly (optimistic)
  // Persistence happens asynchronously via coordinator
}
```

### 2. Coordination Layer (PersistenceCoordinator) **[NEW]**

**Role:** Coordinator - manages asynchronous persistence with dependency tracking

**Responsibilities:**
- Accept persistence requests from UI layer
- Track dependencies between operations
- Enforce execution order (topological sorting)
- Manage debouncing (configurable delays)
- Detect conflicts (version checking)
- Provide reactive status updates

**Key Features:**
- **Declarative dependencies** - Operations declare what they depend on
- **Lambda support** - Complex dependencies via functions
- **Reactive state** - Uses `SvelteMap` for Svelte 5 reactivity
- **Automatic coordination** - System enforces ordering, no manual tracking

**API Design:**
```typescript
interface PersistenceCoordinator {
  /**
   * Request persistence with dependency tracking
   */
  persist(
    nodeId: string,
    operation: () => Promise<void>,
    options: {
      mode: 'immediate' | 'debounce';
      debounceMs?: number;  // Default 500ms
      dependencies?: Array<
        | string                    // Node ID to wait for
        | (() => Promise<void>)    // Lambda function
        | { nodeIds: string[] }    // Batch of nodes
        | PersistenceHandle        // Handle from previous operation
      >;
    }
  ): void;

  /**
   * Check if node is persisted (reactive)
   */
  isPersisted(nodeId: string): boolean;

  /**
   * Check if node has pending persistence (reactive)
   */
  isPending(nodeId: string): boolean;

  /**
   * Wait for specific nodes to be persisted
   */
  waitForPersistence(nodeIds: string[], timeoutMs?: number): Promise<Set<string>>;
}
```

**Implementation File:**
- `packages/desktop-app/src/lib/services/persistence-coordinator.svelte.ts` (NEW)

**Example Usage:**
```typescript
// Content change with debouncing
persistenceCoordinator.persist(
  nodeId,
  () => sharedNodeStore.saveToDatabase(node),
  { mode: 'debounce' }
);

// New node - needs immediate persistence for FOREIGN KEY
persistenceCoordinator.persist(
  childId,
  () => sharedNodeStore.saveToDatabase(child),
  {
    mode: 'immediate',
    dependencies: [parentId]  // Wait for parent to be persisted first
  }
);

// Complex dependency with lambda
persistenceCoordinator.persist(
  nodeId,
  () => sharedNodeStore.saveToDatabase(node),
  {
    mode: 'immediate',
    dependencies: [
      async () => {
        // Ensure entire ancestor chain is persisted
        await ensureAncestorChainPersisted(nodeId);
      }
    ]
  }
);
```

### 3. Data Layer (SharedNodeStore)

**Role:** Source of Truth - manages in-memory reactive state

**Responsibilities:**
- Store all node data in reactive Map
- Handle multi-source updates (viewer, database, MCP)
- Notify subscribers of changes
- Delegate persistence to PersistenceCoordinator

**Key Files:**
- `packages/desktop-app/src/lib/services/shared-node-store.ts`

**Refactored API:**
```typescript
class SharedNodeStore {
  // In-memory reactive state
  private nodes = new Map<string, Node>();

  /**
   * Update node (in-memory only, delegates persistence)
   */
  updateNode(
    nodeId: string,
    changes: Partial<Node>,
    source: UpdateSource
  ): void {
    // 1. Apply update to in-memory Map (synchronous)
    this.nodes.set(nodeId, updatedNode);

    // 2. Notify subscribers (triggers UI re-render)
    this.notifySubscribers(nodeId, updatedNode, source);

    // 3. Emit event
    eventBus.emit({ type: 'node:updated', nodeId, ... });

    // 4. Delegate persistence to coordinator
    // (coordinator handles debouncing, dependencies, ordering)
  }
}
```

### 4. Backend Layer (TauriNodeService + Rust)

**Role:** Executor - performs actual database operations

**Responsibilities:**
- Execute SQL queries via Tauri IPC
- Enforce FOREIGN KEY constraints
- Manage transactions
- Serialize writes per node (queueDatabaseWrite)

**Key Files:**
- `packages/desktop-app/src/lib/services/tauri-node-service.ts`
- `packages/desktop-app/src-tauri/src/commands.rs`

## Data Flow Example: Merge Node with Children

```typescript
// 1. User Action → ReactiveNodeService
combineNodes('NodeB', 'GrandChild1')

// 2. In-memory updates (synchronous, immediate UI feedback)
sharedNodeStore.updateNode('GrandChild1', { content: merged });  // ← UI shows merged content
sharedNodeStore.updateNode('Child3', { parentId: 'NodeA' });     // ← UI shows promoted child
sharedNodeStore.deleteNode('NodeB');                              // ← UI removes deleted node

// 3. Persistence requests → PersistenceCoordinator
// (happens automatically via BaseNodeViewer watchers)

// Content watcher detects change:
persistenceCoordinator.persist(
  'GrandChild1',
  () => tauriNodeService.updateNode('GrandChild1', node),
  { mode: 'immediate' }  // Merge is explicit action, persist immediately
);

// Structural watcher detects change:
persistenceCoordinator.persist(
  'Child3',
  () => tauriNodeService.updateNode('Child3', node),
  {
    mode: 'immediate',
    dependencies: ['NodeA', 'Child2']  // Wait for parent and sibling
  }
);

// Deletion watcher detects change:
persistenceCoordinator.persist(
  'NodeB',
  () => tauriNodeService.deleteNode('NodeB'),
  {
    mode: 'immediate',
    dependencies: [
      async () => {
        // Wait for all children to be reassigned
        await waitForChildren(['Child3', 'Child4', 'Child5']);
      }
    ]
  }
);

// 4. PersistenceCoordinator resolves dependencies and executes in order:
// Order: GrandChild1 → Child3 (waits for NodeA, Child2) → NodeB (waits for children)
```

## Benefits of the Three-Tier Architecture

### 1. Separation of Concerns
- **UI layer** - User interaction and operation orchestration
- **Coordination layer** - Asynchronous persistence management
- **Data layer** - Reactive state management
- **Backend layer** - Database operations

### 2. Declarative Dependencies
```typescript
// Before (imperative):
await waitForNodeSaves([parentId]);
if (failed.size === 0) {
  await updateStructure();
}

// After (declarative):
persist(nodeId, operation, {
  dependencies: [parentId]  // System handles waiting
});
```

### 3. Testability
```typescript
// Mock the coordinator for tests
const mockCoordinator = {
  persist: vi.fn(),
  isPersisted: vi.fn(() => true)
};

// Test operations without database
reactiveNodeService.combineNodes('a', 'b');
expect(mockCoordinator.persist).toHaveBeenCalledWith('b', ...);
```

### 4. Multi-Source Update Handling
- Viewer updates → coordinated persistence
- Database updates → skip persistence (already persisted)
- MCP updates → conflict detection + resolution

### 5. Performance Optimization
- Debouncing for frequent updates (typing)
- Immediate persistence for critical operations (FOREIGN KEY)
- Batching for bulk operations
- Parallel execution where possible

## Migration from Current Architecture

### Current Problems

1. **Dual persistence paths**
   - `updateNode()` - debounced (500ms)
   - `saveNodeImmediately()` - immediate (backdoor)

2. **Manual coordination**
   - `pendingContentSavePromises` map
   - `waitForNodeSavesIfPending()` function
   - Easy to forget dependencies

3. **Hard to test**
   - Promise tracking spread across files
   - No clear separation of concerns

### Migration Strategy

**Phase 1: Add PersistenceCoordinator (non-breaking)**
- Create new coordinator service
- Keep existing methods working
- Add new dependency-based API

**Phase 2: Refactor BaseNodeViewer**
- Replace manual tracking with coordinator
- Use declarative dependencies
- Remove `pendingContentSavePromises` map

**Phase 3: Refactor SharedNodeStore**
- Remove `saveNodeImmediately()` method
- Delegate all persistence to coordinator
- Simplify `updateNode()` to in-memory only

**Phase 4: Remove deprecated code**
- Remove `waitForNodeSavesIfPending()`
- Remove manual promise tracking
- Update tests

## Related Documentation

- [Dependency-Based Persistence](./dependency-based-persistence.md) - Detailed API design
- [Elegant Persistence Solution](./elegant-persistence-solution.md) - Flush strategies
- [MCP Integration Gaps](./mcp-integration-gaps.md) - Multi-source coordination
- [Frontend Architecture](./frontend-architecture.md) - Overall UI architecture
- [Component Architecture Guide](./components/component-architecture-guide.md) - Component patterns

## Open Questions

1. **MCP Server Integration**
   - Should MCP server run inside Tauri app (shared state)?
   - Or separate process with event-based coordination?

2. **Conflict Resolution**
   - Last-Write-Wins sufficient for single user?
   - Need Operational Transform for multi-user?

3. **Performance**
   - What's the cost of dependency resolution?
   - Should we cache dependency graphs?

4. **Testing**
   - How to test complex dependency chains?
   - Need visual dependency graph inspector?
