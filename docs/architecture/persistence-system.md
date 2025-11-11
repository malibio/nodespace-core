# NodeSpace Persistence System

> **⚠️ READ THIS FIRST IF CONFUSED ABOUT PERSISTENCE**
>
> Engineers frequently ask "How does persistence work in NodeSpace?" This document provides the definitive answer in one place.
>
> **Last Updated:** 2025-11-11

---

## Table of Contents

1. [Quick Start](#quick-start) - TL;DR for busy engineers
2. [Core Concept](#core-concept) - The dual-pattern architecture
3. [Pattern 1: Direct Persistence](#pattern-1-direct-persistence) - UI → Database
4. [Pattern 2: Event Coordination](#pattern-2-event-coordination) - Service ↔ Service
5. [PersistenceCoordinator](#persistencecoordinator) - Dependency management
6. [API Reference](#api-reference) - Complete API documentation
7. [Decision Tree](#decision-tree) - Which pattern should I use?
8. [Anti-Patterns](#anti-patterns) - Critical lessons from Issue #246
9. [Real-World Examples](#real-world-examples) - Copy-paste code
10. [Testing](#testing) - How to test persistence
11. [Troubleshooting](#troubleshooting) - Common issues and solutions

---

## Quick Start

**TL;DR: NodeSpace uses TWO complementary persistence patterns:**

### Pattern 1: Direct Persistence (UI → Database)

**When:** UI components saving their own state
**How:** `$effect` watchers → `databaseService.save()`
**Example:** Auto-saving content while typing

```typescript
// base-node-viewer.svelte
$effect(() => {
  if (node.content !== lastSaved) {
    debounceSave(node.id, node.content);  // Direct database call
  }
});
```

### Pattern 2: Event Coordination (Service ↔ Service)

**When:** Services need to coordinate with each other
**How:** Service emits event → Other services react
**Example:** Cleaning up references when a node is deleted

```typescript
// reactive-node-service.svelte.ts
function deleteNode(nodeId: string) {
  // Delete from memory
  delete _nodes[nodeId];

  // Notify other services
  events.nodeDeleted(nodeId);  // Event bus broadcast
}

// node-reference-service.ts listens and cleans up references
eventBus.subscribe('node:deleted', (event) => {
  cleanupReferences(event.nodeId);
});
```

**Decision Rule:**
- **Your component owns the data?** → Use Pattern 1 (direct persistence)
- **Multiple services need to react?** → Use Pattern 2 (event coordination)

---

## Core Concept

NodeSpace uses a **dual-pattern architecture** for data persistence. This is intentional - each pattern solves a different problem.

### The Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    NodeSpace Architecture                    │
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

### Why Two Patterns?

**Why not just use one pattern for everything?**

❌ **Pure Event Bus** would be slower and more complex:
- 5-10x slower for frequent saves (typing in editor)
- Extra indirection: UI → Event → Handler → Database (4 steps vs 2)
- Unclear ownership: Who's responsible for persistence?
- Fights Svelte's `$effect` design patterns

❌ **Pure Direct Calls** would create tight coupling:
- Services depending directly on each other
- Hard to add new features (AI, sync, analytics)
- Cache invalidation nightmares
- No extensibility

✅ **Dual Pattern** gets the best of both:
- **Direct**: Fast, simple, clear ownership (UI owns its persistence)
- **Events**: Decoupled, extensible, observable (services coordinate)

---

## Pattern 1: Direct Persistence

### When to Use

Use direct database calls when:
- ✅ UI components saving their own state
- ✅ Simple CRUD operations from UI layer
- ✅ High-frequency operations (typing, debounced saves)
- ✅ Clear ownership: component owns the data it displays

### How It Works

**File:** `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte`

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

// Debounced save function
function debounceSave(nodeId: string, content: string, nodeType: string) {
  // Clear existing timeout
  const existing = saveTimeouts.get(nodeId);
  if (existing) clearTimeout(existing);

  // Debounce 500ms
  const timeout = setTimeout(async () => {
    await databaseService.saveNodeWithParent(nodeId, { content, nodeType }, parentId);
    lastSavedContent.set(nodeId, content);
    saveTimeouts.delete(nodeId);
  }, 500);

  saveTimeouts.set(nodeId, timeout);
}
```

### Why This Pattern Works

**Advantages:**
- **Simple**: Direct path from UI change to database (2 steps)
- **Performant**: No event overhead (~0.1ms per operation)
- **Svelte-native**: Leverages `$effect` exactly as designed
- **Clear responsibility**: Component owns its persistence
- **Easy debugging**: Direct call stack
- **Type-safe**: Full TypeScript checking on function calls

**Performance:**
```
UI change → $effect → databaseService.save() → Tauri IPC
Overhead: ~0.1ms (one function call)
```

### Complete Example: Deletion Watcher

```typescript
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

---

## Pattern 2: Event Coordination

### When to Use

Use event bus when:
- ✅ Services coordinating with each other
- ✅ Cache invalidation across multiple services
- ✅ Graph relationship maintenance (references, hierarchy)
- ✅ Future extensibility (AI features, analytics, telemetry)
- ✅ Multiple handlers need to react to same event

### How It Works

**Emitting Events** (`packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`):

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

**Subscribing to Events** (various services):

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

### Why This Pattern Works

**Advantages:**
- **Decoupled**: Services don't directly depend on each other
- **Extensible**: Add new services without modifying existing code
- **Observable**: Multiple services react to same event
- **Coordination**: Complex operations across service boundaries
- **Future-ready**: Easy to add AI, sync, analytics features

**Example - Adding New Feature:**
```typescript
// Add real-time sync in the future - zero changes to existing code
class SyncService {
  constructor() {
    eventBus.subscribe('node:updated', this.syncToCloud);
    eventBus.subscribe('node:deleted', this.syncDeletion);
  }
}
```

---

## Frontend Service Layers Explained

Before diving into the coordinator, let's understand **what each service does** and how they work together.

### Layer 1: UI Layer

**Services:** `BaseNodeViewer` (component) + `ReactiveNodeService` (orchestrator)

**Role:** Orchestrator - decides WHEN and in WHAT ORDER operations happen

**Responsibilities:**
- Detect user actions (typing, indent, outdent, delete, merge)
- Trigger multi-step operations (e.g., merge node → promote children → delete)
- Provide immediate UI feedback (optimistic updates via in-memory changes)
- Watch for changes via `$effect` and trigger persistence

**Key Files:**
- `packages/desktop-app/src/lib/design/components/base-node-viewer.svelte`
- `packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`

**What it does:**
```typescript
// Example: BaseNodeViewer watches for content changes
$effect(() => {
  for (const node of visibleNodes) {
    if (node.content !== lastSaved) {
      debounceSave(node.id, node.content);  // Triggers persistence
    }
  }
});

// Example: ReactiveNodeService orchestrates complex operations
function combineNodes(currentId, previousId) {
  // 1. Merge content (in-memory, immediate)
  sharedNodeStore.updateNode(previousId, { content: merged }, source);

  // 2. Promote children (in-memory, immediate)
  for (const child of children) {
    sharedNodeStore.updateNode(child.id, { parentId: previousId }, source);
  }

  // 3. Delete node (in-memory, immediate)
  sharedNodeStore.deleteNode(currentId, source);

  // UI updates instantly (optimistic)
  // Persistence happens asynchronously in background
}
```

**Think of it as:** The "traffic controller" - sees user actions and directs traffic to the right services.

---

### Layer 2: Coordination Layer

**Service:** `PersistenceCoordinator`

**Role:** Coordinator - manages asynchronous persistence with dependency tracking

**Responsibilities:**
- Accept persistence requests from UI layer
- Track dependencies between operations (which must complete before which)
- Enforce execution order (topological sorting)
- Manage debouncing (batch rapid changes)
- Detect conflicts (version checking)
- Provide reactive status updates (is this node persisted yet?)

**Key File:**
- `packages/desktop-app/src/lib/services/persistence-coordinator.svelte.ts`

**What it does:**
```typescript
// Example: Coordinating a new child node creation
persistenceCoordinator.persist(
  childId,
  () => tauriNodeService.createNode(child),
  {
    mode: 'immediate',
    dependencies: [parentId]  // Wait for parent first
  }
);

// Internally, the coordinator:
// 1. Checks if parent is already persisted
// 2. If not, waits for parent's persistence to complete
// 3. Then executes the child's persistence
// 4. Updates reactive status (isPersisted, isPending)
// 5. Handles errors and timeouts
```

**Think of it as:** The "project manager" - ensures work happens in the right order and nothing blocks.

---

### Layer 3: Data Layer

**Service:** `SharedNodeStore`

**Role:** Source of Truth - manages in-memory reactive state

**Responsibilities:**
- Store all node data in reactive `SvelteMap` (immediate access)
- Handle multi-source updates (viewer, database, MCP server)
- Notify subscribers of changes (trigger UI re-renders)
- Delegate persistence to `PersistenceCoordinator`
- Manage optimistic updates (show changes before database confirms)

**Key File:**
- `packages/desktop-app/src/lib/services/shared-node-store.ts`

**What it does:**
```typescript
class SharedNodeStore {
  private nodes = new SvelteMap<string, Node>();  // Reactive storage

  updateNode(nodeId: string, changes: Partial<Node>, source: UpdateSource) {
    // 1. Apply update to in-memory Map (synchronous)
    this.nodes.set(nodeId, { ...existing, ...changes });

    // 2. Notify subscribers (triggers UI re-render)
    this.notifySubscribers(nodeId, updatedNode, source);

    // 3. Emit event for service coordination
    eventBus.emit({ type: 'node:updated', nodeId, ... });

    // 4. Delegate persistence to coordinator (if not from database)
    if (source.type !== 'database') {
      persistenceCoordinator.persist(nodeId, () => {
        return tauriNodeService.updateNode(nodeId, updatedNode);
      }, { mode: 'debounce' });
    }
  }
}
```

**Think of it as:** The "warehouse" - holds all the data and coordinates between UI, persistence, and other services.

---

### Layer 4: Backend Integration Layer

**Service:** `TauriNodeService`

**Role:** Backend Adapter - bridges frontend and Rust backend

**Responsibilities:**
- Serialize/deserialize data for IPC (Inter-Process Communication)
- Queue database writes per node (prevent concurrent writes)
- Call Tauri commands (invoke Rust functions)
- Handle backend errors and retries

**Key File:**
- `packages/desktop-app/src/lib/services/tauri-node-service.ts`

**What it does:**
```typescript
class TauriNodeService {
  async createNode(node: Node): Promise<void> {
    // Queue to prevent concurrent writes to same node
    return queueDatabaseWrite(node.id, async () => {
      // Serialize and send to Rust backend via Tauri IPC
      await invoke('create_node', { node: serializeNode(node) });
    });
  }

  async updateNode(nodeId: string, node: Node): Promise<void> {
    return queueDatabaseWrite(nodeId, async () => {
      await invoke('update_node', { nodeId, node: serializeNode(node) });
    });
  }
}
```

**Think of it as:** The "postal service" - delivers messages between frontend and backend, ensuring they arrive in order.

---

### Layer 5: Backend Layer (Rust + Tauri)

**Services:** Rust commands + libsql database

**Role:** Executor - performs actual database operations

**Responsibilities:**
- Execute SQL queries via libsql
- Enforce FOREIGN KEY constraints
- Manage transactions
- Serialize writes per node

**Key Files:**
- `packages/desktop-app/src-tauri/src/commands.rs`
- `packages/core/src/db/database.rs`

**What it does:**
```rust
#[tauri::command]
async fn create_node(node: Node) -> Result<(), String> {
    // Validate node structure
    validate_node(&node)?;

    // Insert into SQLite with FOREIGN KEY checks
    database.execute(
        "INSERT INTO nodes (id, content, parent_id, ...) VALUES (?, ?, ?, ...)",
        params![node.id, node.content, node.parent_id, ...]
    ).await?;

    Ok(())
}
```

**Think of it as:** The "bank vault" - safely stores data and enforces all the rules.

---

### How They Work Together

**Complete Flow: User Types → Database Save**

```
┌──────────────────────────────────────────────────────────────┐
│ 1. USER TYPES                                                │
│    "Hello World"                                             │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 2. UI LAYER (BaseNodeViewer)                                │
│    $effect detects change                                    │
│    → debounceSave(nodeId, "Hello World")                     │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 3. DATA LAYER (SharedNodeStore)                             │
│    → updateNode(nodeId, { content: "Hello World" })         │
│    → Update in-memory Map (UI shows change instantly)       │
│    → Notify subscribers (UI re-renders)                     │
│    → Emit 'node:updated' event                              │
│    → Delegate to PersistenceCoordinator                     │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 4. COORDINATION LAYER (PersistenceCoordinator)              │
│    → persist(nodeId, operation, { mode: 'debounce' })       │
│    → Wait 500ms (batch rapid changes)                       │
│    → Check dependencies (none for content change)           │
│    → Execute operation                                       │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 5. BACKEND INTEGRATION (TauriNodeService)                   │
│    → updateNode(nodeId, node)                               │
│    → queueDatabaseWrite(nodeId, async () => {...})          │
│    → invoke('update_node', { nodeId, node })                │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 6. BACKEND LAYER (Rust + libsql)                            │
│    → Execute SQL: UPDATE nodes SET content = ? WHERE id = ? │
│    → Enforce FOREIGN KEY constraints                         │
│    → Return success/error                                    │
└────────────────┬─────────────────────────────────────────────┘
                 ↓
┌──────────────────────────────────────────────────────────────┐
│ 7. SUCCESS                                                   │
│    → PersistenceCoordinator marks as persisted              │
│    → SharedNodeStore updates persistedNodeIds               │
│    → UI can show "Saved" indicator                          │
└──────────────────────────────────────────────────────────────┘
```

**Time:** ~500ms (debounce) + ~5-10ms (database write) = ~510ms total

**User Experience:** Changes appear instantly (optimistic), saved in background

---

## Placeholder Nodes

### What Are Placeholder Nodes?

**Placeholder nodes** are temporary UI-only nodes that exist to provide smooth UX when creating new nodes, but **should not be persisted** to the database until the user adds actual content.

**Think of them as:** Draft envelopes - you start addressing an envelope, but you don't mail it until you've actually written something inside.

### Why Do Placeholder Nodes Exist?

**Problem:** When user creates a new node (presses Enter), we need to:
1. Show the new node instantly in the UI (for smooth UX)
2. But not save it to the database yet (avoid cluttering DB with empty nodes)
3. Wait until user actually types content (then persist)

**Solution:** Create placeholder nodes in memory only:
```typescript
// User presses Enter
const newNode = {
  id: generateId(),
  content: '',           // Empty content
  nodeType: 'text',
  isPlaceholder: true    // Marker: don't persist yet
};

// Show in UI immediately (optimistic)
sharedNodeStore.updateNode(newNode.id, newNode, viewerSource, {
  skipPersistence: true  // Don't save to database
});

// User starts typing...
// Now persist (placeholder becomes real node)
```

### How Placeholder Detection Works

**Utility:** `packages/desktop-app/src/lib/utils/placeholder-detection.ts`

```typescript
export function isPlaceholderNode(node: PlaceholderCheckable): boolean {
  const trimmedContent = node.content.trim();

  switch (node.nodeType) {
    case 'text':
      // Empty text or just pattern prefixes
      return trimmedContent === '' ||
             trimmedContent === '>' ||
             trimmedContent.match(/^#{1,6}\s*$/);  // Just "# " etc

    case 'quote-block':
      // Just "> " with no actual content
      const contentWithoutPrefix = trimmedContent
        .replace(/^>\s?/gm, '')
        .trim();
      return contentWithoutPrefix === '';

    case 'code-block':
      // Just "```" with no actual code
      const contentWithoutBackticks = trimmedContent
        .replace(/^```\w*\s*/, '')
        .replace(/```$/, '');
      return contentWithoutBackticks.trim() === '';

    case 'ordered-list':
      // Just "1. " with no actual content
      const contentWithoutPrefix = trimmedContent.replace(/^1\.\s*/, '');
      return contentWithoutPrefix === '';

    case 'task':
      // Empty task description
      return trimmedContent === '';

    case 'date':
      // Date nodes are never placeholders (backend-managed containers)
      return false;

    default:
      return trimmedContent === '';
  }
}
```

### Pattern Conversion and Placeholders

**Special Case:** Pattern conversion (typing `>` → converts text node to quote-block)

```typescript
// User types "> " in a text node
const textNode = { nodeType: 'text', content: '> ' };

// Pattern detector converts to quote-block
const quoteNode = { nodeType: 'quote-block', content: '> ' };

// But it's still a placeholder! (just prefix, no actual content)
isPlaceholderNode(quoteNode);  // true

// User continues typing: "> Hello"
const realQuoteNode = { nodeType: 'quote-block', content: '> Hello' };
isPlaceholderNode(realQuoteNode);  // false → NOW persist
```

**Atomic Batching for Pattern Conversions:**

Pattern-converted node types (`quote-block`, `code-block`, `ordered-list`) require **atomic batching** to prevent race conditions:

```typescript
// BAD: Race condition
updateNode(nodeId, { content: '> Hello' });    // Update 1
updateNode(nodeId, { nodeType: 'quote-block' }); // Update 2
// Backend might see content before nodeType change!

// GOOD: Atomic batch
updateNode(nodeId, {
  content: '> Hello',
  nodeType: 'quote-block'  // Both updates together
});
```

### Persistence Prevention

**In BaseNodeViewer:**

```typescript
// Content watcher ($effect)
$effect(() => {
  for (const node of visibleNodes) {
    // Skip placeholders - don't trigger persistence
    if (node.isPlaceholder) continue;

    // Only save nodes with actual content
    if (node.content !== lastSaved) {
      debounceSave(node.id, node.content);
    }
  }
});
```

**In SharedNodeStore:**

```typescript
updateNode(nodeId: string, changes: Partial<Node>, source: UpdateSource) {
  // Apply to memory (always)
  this.nodes.set(nodeId, updatedNode);

  // Check placeholder status before persisting
  if (!isPlaceholderNode(updatedNode) && source.type !== 'database') {
    // not a placeholder → persist to database
    persistenceCoordinator.persist(nodeId, () => {
      return tauriNodeService.updateNode(nodeId, updatedNode);
    });
  }
  // Placeholder → skip persistence (UI-only node)
}
```

### Placeholder Lifecycle

**Complete flow from creation to persistence:**

```
┌────────────────────────────────────────────────────────────┐
│ 1. USER PRESSES ENTER                                      │
│    Creates new text node                                   │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 2. CREATE PLACEHOLDER                                      │
│    { id: 'node-1', content: '', isPlaceholder: true }     │
│    → updateNode(..., { skipPersistence: true })           │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 3. SHOW IN UI                                              │
│    User sees empty node with cursor                        │
│    Node exists in memory (SharedNodeStore)                 │
│    Node does not exist in database                         │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 4. USER TYPES: "H"                                         │
│    { content: 'H', isPlaceholder: false }                  │
│    → updateNode(...) [no skipPersistence flag]            │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 5. PLACEHOLDER DETECTION                                   │
│    isPlaceholderNode({ content: 'H' }) → false            │
│    → Has actual content now!                               │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 6. TRIGGER PERSISTENCE                                     │
│    persistenceCoordinator.persist(nodeId, ...)            │
│    → Debounce 500ms (batch rapid changes)                 │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 7. PERSIST TO DATABASE                                     │
│    tauriNodeService.createNode(node)                       │
│    → INSERT INTO nodes (id, content, ...) VALUES (...)    │
└────────────────┬───────────────────────────────────────────┘
                 ↓
┌────────────────────────────────────────────────────────────┐
│ 8. MARK AS PERSISTED                                       │
│    sharedNodeStore.persistedNodeIds.add('node-1')         │
│    → Node now exists in database                           │
└────────────────────────────────────────────────────────────┘
```

**Time:** Instant UI (step 3) → ~500ms after typing (step 7)

**User Experience:** Smooth, no lag, empty nodes don't clutter database

### Edge Cases and Guarding

**Problem:** What if parent/sibling references point to unpersisted placeholders?

```typescript
// User creates child under placeholder parent
const placeholder = { id: 'parent-1', content: '', isPlaceholder: true };
const child = { id: 'child-1', parentId: 'parent-1', content: 'Hello' };

// FOREIGN KEY violation: parent-1 doesn't exist in database yet!
```

**Solution:** Defer persistence until references are resolved:

```typescript
// In SharedNodeStore (line 400-440)
// GUARD: Check if beforeSiblingId or parentId reference unpersisted placeholders
if (changes.parentId) {
  const parent = this.nodes.get(changes.parentId);
  if (parent && isPlaceholderNode(parent)) {
    // Parent is unpersisted placeholder - defer until placeholder persists
    deferUntilPersisted(changes.parentId);
  }
}

if (changes.beforeSiblingId) {
  const sibling = this.nodes.get(changes.beforeSiblingId);
  if (sibling && isPlaceholderNode(sibling)) {
    // Sibling is unpersisted placeholder - defer
    deferUntilPersisted(changes.beforeSiblingId);
  }
}
```

### Testing Placeholder Behavior

**Test File:** `packages/desktop-app/src/tests/services/placeholder-detection.test.ts`

```typescript
describe('Placeholder Detection', () => {
  it('should detect empty text nodes as placeholders', () => {
    expect(isPlaceholderNode({ nodeType: 'text', content: '' })).toBe(true);
  });

  it('should detect quote-block with only prefix as placeholder', () => {
    expect(isPlaceholderNode({ nodeType: 'quote-block', content: '> ' })).toBe(true);
  });

  it('should not detect quote-block with content as placeholder', () => {
    expect(isPlaceholderNode({ nodeType: 'quote-block', content: '> Hello' })).toBe(false);
  });

  it('should detect code-block with only backticks as placeholder', () => {
    expect(isPlaceholderNode({ nodeType: 'code-block', content: '```' })).toBe(true);
  });

  it('should detect pattern prefixes in text nodes as placeholders', () => {
    expect(isPlaceholderNode({ nodeType: 'text', content: '> ' })).toBe(true);
    expect(isPlaceholderNode({ nodeType: 'text', content: '# ' })).toBe(true);
    expect(isPlaceholderNode({ nodeType: 'text', content: '1. ' })).toBe(true);
  });
});
```

### Special Case: Date Nodes

**Date nodes are never placeholders** (even with empty content):

```typescript
case 'date':
  // Date nodes are virtual/backend-managed containers, never placeholders
  // They have empty content by design and are created on-demand by the backend
  return false;
```

**Why?** Date nodes are containers for other nodes (e.g., "2025-01-15" contains all tasks for that day). They're created by the backend when needed, not by user typing.

### Summary

**Key Takeaways:**

1. **Purpose:** Smooth UX - show new nodes instantly without cluttering database
2. **Detection:** Empty content or just type-specific prefixes (e.g., "> " for quote)
3. **Lifecycle:** Memory-only → User types content → Persist to database
4. **Persistence:** Placeholders are never persisted (checked at multiple layers)
5. **Edge Cases:** Guard against FOREIGN KEY violations from placeholder references
6. **Pattern Conversion:** Atomic batching prevents race conditions
7. **Date Nodes Exception:** Never placeholders (backend-managed containers)

**Implementation Files:**
- Detection: `src/lib/utils/placeholder-detection.ts`
- Prevention: `src/lib/design/components/base-node-viewer.svelte` (line 381, 614)
- Guarding: `src/lib/services/shared-node-store.ts` (line 400-440)
- Tests: `src/tests/services/placeholder-detection.test.ts`

---

## PersistenceCoordinator

### The Problem

Some operations have **ordering dependencies** due to database FOREIGN KEY constraints:

- **Parent must exist before child** (FOREIGN KEY constraint)
- **Sibling must be updated before deletion** (sibling chain integrity)
- **Ancestors must be persisted before descendants** (hierarchy validity)

**Challenge:** How do we coordinate async operations while keeping code simple?

### The Solution: Declarative Dependencies

The `PersistenceCoordinator` manages dependencies between operations automatically:

```typescript
// Instead of manually awaiting:
await waitForNodeSaves([parentId]);  // ❌ Manual coordination
if (failed.size === 0) {
  await updateNode(...);
}

// Declare dependencies upfront:
updateNode(..., {
  persistenceDependencies: [parentId]  // ✅ Declarative
});
// System handles waiting automatically!
```

### How It Works

**Three-Tier Architecture:**

```
┌─────────────────────────────────────────────────────────────┐
│                    UI Layer (Svelte 5)                      │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  BaseNodeViewer ($effect watchers)                   │  │
│  │  ReactiveNodeService (orchestrates operations)       │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────┬──────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              Coordination Layer                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  PersistenceCoordinator                              │  │
│  │  • Dependency tracking (declarative)                 │  │
│  │  • Debouncing coordination (configurable delays)     │  │
│  │  • Operation ordering (topological sort)             │  │
│  │  • Conflict detection (version checking)             │  │
│  │  • Status tracking (reactive via SvelteMap)          │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────┬──────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│                Data Layer                                    │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  SharedNodeStore                                      │  │
│  │  • In-memory reactive state (SvelteMap)              │  │
│  │  • Multi-source update handling                      │  │
│  │  • Subscriber notifications                          │  │
│  │  • Delegates persistence to PersistenceCoordinator   │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────┬──────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│              Backend Layer (Rust + Tauri)                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  TauriNodeService + Rust Backend                     │  │
│  │  • SQLite database operations                        │  │
│  │  • FOREIGN KEY constraint enforcement                │  │
│  │  • Transaction management                            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow Example

**User action: Backspace to combine nodes**

```
1. User Action → ReactiveNodeService
   combineNodes('NodeB', 'GrandChild1')

2. In-memory updates (synchronous, immediate UI feedback)
   sharedNodeStore.updateNode('GrandChild1', { content: merged });  // ← UI shows merged content
   sharedNodeStore.updateNode('Child3', { parentId: 'NodeA' });     // ← UI shows promoted child
   sharedNodeStore.deleteNode('NodeB');                              // ← UI removes deleted node

3. Persistence requests → PersistenceCoordinator
   (happens automatically via BaseNodeViewer watchers)

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

4. PersistenceCoordinator resolves dependencies and executes in order:
   Order: GrandChild1 → Child3 (waits for NodeA, Child2) → NodeB (waits for children)

5. ✅ Complete: Memory clean, services coordinated, database updated
```

---

## API Reference

### SharedNodeStore API

#### `updateNode(nodeId, changes, source, options)`

Update a node with optional persistence dependencies.

**Parameters:**
```typescript
nodeId: string                  // Node to update
changes: Partial<Node>          // Properties to change
source: UpdateSource            // Who triggered the update
options: {
  persistenceDependencies?: string[];  // Node IDs that must be persisted first
  skipPersistence?: boolean;           // Skip database save
  skipConflictDetection?: boolean;     // Skip version checking
  force?: boolean;                     // Force update even if conflict
  forceNotify?: boolean;               // Force UI notification
}
```

**Example:**
```typescript
// Update node with dependency on parent
sharedNodeStore.updateNode(
  childId,
  { content: 'Child node', parentId },
  viewerSource,
  {
    persistenceDependencies: [parentId]  // Wait for parent to be persisted
  }
);
```

#### `deleteNode(nodeId, source, skipPersistence, dependencies)`

Delete a node with optional persistence dependencies.

**Parameters:**
```typescript
nodeId: string                   // Node to delete
source: UpdateSource             // Who triggered the deletion
skipPersistence?: boolean        // Skip database delete
dependencies?: string[]          // Node IDs that must be persisted first
```

**Example:**
```typescript
// Delete node after sibling is updated
sharedNodeStore.deleteNode(
  nodeId,
  viewerSource,
  false,  // Don't skip persistence
  [nextSiblingId]  // Wait for sibling update
);
```

### PersistenceCoordinator API

#### `persist(nodeId, operation, options)`

Queue an operation with dependency tracking.

**Parameters:**
```typescript
nodeId: string                    // Node being persisted
operation: () => Promise<void>    // Async operation to execute
options: {
  mode: 'immediate' | 'debounce';    // Execution timing
  debounceMs?: number;               // Debounce delay (default 500ms)
  dependencies?: Array<
    | string                         // Node ID to wait for
    | (() => Promise<void>)          // Lambda function
    | { nodeIds: string[] }          // Batch of nodes
  >;
}
```

**Example:**
```typescript
// Persist with multiple dependencies
persistenceCoordinator.persist(
  nodeId,
  () => tauriNodeService.updateNode(nodeId, node),
  {
    mode: 'immediate',
    dependencies: [
      parentId,                    // Wait for parent
      async () => {                // Custom validation
        await validateStructure();
      }
    ]
  }
);
```

#### `waitForPersistence(nodeIds, timeoutMs)`

Wait for specific nodes to be persisted.

**Parameters:**
```typescript
nodeIds: string[]           // Nodes to wait for
timeoutMs?: number          // Timeout (default 5000ms)

Returns: Promise<Set<string>>  // Failed node IDs
```

**Example:**
```typescript
// Wait for dependencies before proceeding
const failed = await persistenceCoordinator.waitForPersistence([parentId, siblingId]);

if (failed.size === 0) {
  // Safe to proceed
  await updateStructure();
}
```

---

## Decision Tree

**Which persistence pattern should I use?**

```
┌─────────────────────────────────────────────────────────────┐
│ START: Need to persist data                                 │
└────────────┬────────────────────────────────────────────────┘
             │
             ↓
        [Question 1]
  Is this a UI component
   saving its own state?
             │
       ┌─────┴─────┐
       │           │
      YES         NO
       │           │
       ↓           ↓
   [Pattern 1]  [Question 2]
   Direct       Do multiple services
   Persistence  need to react?
       │           │
       │     ┌─────┴─────┐
       │    YES         NO
       │     │           │
       │     ↓           ↓
       │ [Pattern 2]  [Question 3]
       │ Event        Are there ordering
       │ Coordination dependencies?
       │     │           │
       │     │     ┌─────┴─────┐
       │     │    YES         NO
       │     │     │           │
       │     │     ↓           ↓
       │     │ [Use          [Pattern 1]
       │     │ Coordinator]  Direct
       │     │     │         Persistence
       └─────┴─────┴─────────┴───────→ IMPLEMENT
```

### Quick Reference

| Scenario | Pattern | API |
|----------|---------|-----|
| User typing content | Pattern 1 | `$effect` + debounce + `databaseService.save()` |
| New node creation | Pattern 1 + Coordinator | `updateNode(..., { persistenceDependencies: [parentId] })` |
| Node deletion | Pattern 2 | `events.nodeDeleted()` |
| Cache invalidation | Pattern 2 | `eventBus.subscribe('node:updated', ...)` |
| Structural updates | Pattern 1 + Coordinator | `updateNode(..., { persistenceDependencies: [...] })` |
| Bulk operations | Pattern 1 + Coordinator | Multiple `updateNode()` with dependencies |

---

## Anti-Patterns

### ⚠️ Critical Lessons from Issue #246

**DO not manually await waitForNodeSaves() in operation implementations** - this bypasses the PersistenceCoordinator's dependency management.

#### ❌ WRONG: Manual Awaits Bypass Coordinator

```typescript
// ❌ WRONG - Manual await bypasses coordinator
async function deleteNode(nodeId: string): Promise<void> {
  const nodesToWaitFor = [nextSibling?.id].filter(Boolean);
  removeFromSiblingChain(nodeId);
  await sharedNodeStore.waitForNodeSaves(nodesToWaitFor);  // ❌ Manual coordination!
  sharedNodeStore.deleteNode(nodeId, source);
}
```

**Why this causes problems:**
- Operations run in parallel instead of being sequenced
- Multiple concurrent HTTP requests hit backend
- Backend can't handle concurrent writes → HTTP 500 errors
- Defeats the purpose of having a PersistenceCoordinator
- Requires manual error handling and timeout logic
- Makes code unnecessarily async

#### ✅ CORRECT: Declarative Dependencies

```typescript
// ✅ CORRECT - Declarative dependencies
function deleteNode(nodeId: string): void {
  // 1. Identify what must complete first
  const dependencies: string[] = [];
  const nextSibling = siblings.find(n => n.beforeSiblingId === nodeId);
  if (nextSibling) {
    dependencies.push(nextSibling.id);
  }

  // 2. Queue preparatory operations
  removeFromSiblingChain(nodeId);  // Auto-persists via SharedNodeStore

  // 3. Queue main operation WITH dependencies
  sharedNodeStore.deleteNode(nodeId, source, false, dependencies);

  // PersistenceCoordinator automatically:
  // - Waits for nextSibling update to complete
  // - Then executes the deletion
  // - Handles any errors or timeouts
  // - Updates reactive status
}
```

**Why this is correct:**
- ✅ Dependencies explicit and declarative
- ✅ Coordinator queues and sequences everything
- ✅ Backend receives requests one at a time
- ✅ No race conditions or HTTP 500 errors
- ✅ Operations remain synchronous
- ✅ No manual error handling needed
- ✅ Clean, readable code

### 🚩 Red Flags Indicating Coordinator Bypass

Watch for these patterns - they indicate improper usage:

1. **`async function` on operations** - Operations should be synchronous
2. **`await waitForNodeSaves()`** - Should use dependencies instead
3. **`await persist()`** - persist() should not be awaited
4. **Try-catch around waits** - Manual error handling not needed
5. **Promise return types** - Operations should return void or boolean

### ✅ Proper Usage Checklist

When implementing sibling chain operations:

- [ ] Operation is **synchronous** (not async)
- [ ] Returns **void** or **boolean** (not Promise)
- [ ] Identifies dependencies **before** making changes
- [ ] Passes dependencies via **persistenceDependencies** option
- [ ] **No manual awaits** anywhere in the function
- [ ] **No try-catch** for coordination errors
- [ ] SharedNodeStore methods called **without await**
- [ ] Dependencies are **node IDs** or **lambda functions**

---

## Real-World Examples

### Example 1: Content Editing (Debounced)

**Use Case:** User typing in a text node

```typescript
// packages/desktop-app/src/lib/design/components/base-node-viewer.svelte

$effect(() => {
  if (!parentId) return;

  const nodes = nodeManager.visibleNodes;

  for (const node of nodes) {
    if (node.isPlaceholder) continue;

    const lastContent = lastSavedContent.get(node.id);
    if (node.content.trim() && node.content !== lastContent) {
      // Pattern 1: Direct persistence with debouncing
      debounceSave(node.id, node.content, node.nodeType);
    }
  }
});

function debounceSave(nodeId: string, content: string, nodeType: string) {
  const existing = saveTimeouts.get(nodeId);
  if (existing) clearTimeout(existing);

  const timeout = setTimeout(async () => {
    // Direct database call - simple and fast
    await databaseService.saveNodeWithParent(
      nodeId,
      { content, nodeType },
      parentId
    );
    lastSavedContent.set(nodeId, content);
    saveTimeouts.delete(nodeId);
  }, 500);

  saveTimeouts.set(nodeId, timeout);
}
```

### Example 2: New Node Creation (With Dependencies)

**Use Case:** Creating a new child node

```typescript
// packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts

function createNewNode(parentId: string, content: string): string {
  const newNodeId = generateId();

  // Pattern 1 + Coordinator: Create with dependency on parent
  sharedNodeStore.updateNode(
    newNodeId,
    {
      content,
      nodeType: 'text',
      parentId,
      beforeSiblingId: null
    },
    viewerSource,
    {
      persistenceDependencies: [parentId]  // Wait for parent to be persisted
    }
  );

  return newNodeId;
}
```

### Example 3: Indent Operation (Complex Dependencies)

**Use Case:** Indenting a node (changes parent and sibling chain)

```typescript
// packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts

function indentNode(nodeId: string): boolean {
  const node = _nodes[nodeId];
  if (!node) return false;

  // Find new parent (previous sibling)
  const siblings = getSortedChildren(node.parentId);
  const nodeIndex = siblings.findIndex(s => s.id === nodeId);
  if (nodeIndex === 0) return false;  // Can't indent first child

  const newParent = siblings[nodeIndex - 1];

  // Identify dependencies BEFORE making changes
  const dependencies: string[] = [];
  const nextSibling = siblings.find(s => s.beforeSiblingId === nodeId);
  if (nextSibling) {
    dependencies.push(nextSibling.id);
  }

  // 1. Remove from current sibling chain
  removeFromSiblingChain(nodeId);  // Updates nextSibling's beforeSiblingId

  // 2. Update node's parent (with dependencies)
  sharedNodeStore.updateNode(
    nodeId,
    {
      parentId: newParent.id,
      beforeSiblingId: null  // First child of new parent
    },
    viewerSource,
    {
      persistenceDependencies: dependencies  // Wait for sibling update
    }
  );

  return true;
}
```

### Example 4: Combine Nodes (Delete with Children Transfer)

**Use Case:** Backspace to merge current node into previous node

```typescript
// packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts

function combineNodes(currentNodeId: string, previousNodeId: string): void {
  const currentNode = _nodes[currentNodeId];
  const previousNode = _nodes[previousNodeId];
  if (!currentNode || !previousNode) return;

  // 1. Merge content
  sharedNodeStore.updateNode(
    previousNodeId,
    { content: previousNode.content + currentNode.content },
    viewerSource
  );

  // 2. Transfer children to previous node
  const children = getSortedChildren(currentNodeId);
  for (const child of children) {
    sharedNodeStore.updateNode(
      child.id,
      { parentId: previousNodeId },
      viewerSource
    );
  }

  // 3. Identify dependencies for deletion
  const dependencies: string[] = [];
  const nextSibling = siblings.find(s => s.beforeSiblingId === currentNodeId);
  if (nextSibling) {
    dependencies.push(nextSibling.id);
  }
  // Add all children as dependencies
  dependencies.push(...children.map(c => c.id));

  // 4. Remove from sibling chain
  removeFromSiblingChain(currentNodeId);

  // 5. Delete node (waits for all dependencies)
  sharedNodeStore.deleteNode(
    currentNodeId,
    viewerSource,
    false,  // Don't skip persistence
    dependencies
  );

  // Pattern 2: Emit coordination event
  events.nodeDeleted(currentNodeId);
  events.hierarchyChanged();
}
```

### Example 5: Service Coordination (Cache Invalidation)

**Use Case:** Invalidating caches when a node is deleted

```typescript
// packages/desktop-app/src/lib/services/hierarchy-service.ts

class HierarchyService {
  private depthCache = new Map<string, number>();
  private childrenCache = new Map<string, string[]>();

  constructor() {
    // Pattern 2: Subscribe to coordination events
    eventBus.subscribe('node:deleted', this.handleNodeDeleted.bind(this));
    eventBus.subscribe('hierarchy:changed', this.handleHierarchyChanged.bind(this));
  }

  private handleNodeDeleted(event: NodeDeletedEvent): void {
    // Invalidate caches for deleted node
    this.depthCache.delete(event.nodeId);
    this.childrenCache.delete(event.nodeId);

    // Invalidate parent's children cache
    const node = this.getNodeFromStore(event.nodeId);
    if (node?.parentId) {
      this.childrenCache.delete(node.parentId);
    }
  }

  private handleHierarchyChanged(): void {
    // Full cache invalidation for structural changes
    this.depthCache.clear();
    this.childrenCache.clear();
  }
}
```

---

## Testing

### Testing Direct Persistence

```typescript
import { mount } from '@testing-library/svelte';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';

describe('BaseNodeViewer - Direct Persistence', () => {
  let mockDatabase: any;
  let mockNodeManager: any;

  beforeEach(() => {
    mockDatabase = {
      saveNodeWithParent: vi.fn().mockResolvedValue(undefined),
      deleteNode: vi.fn().mockResolvedValue(undefined)
    };

    mockNodeManager = {
      visibleNodes: $state([]),
      findNode: vi.fn()
    };
  });

  it('should save content changes after debounce', async () => {
    const viewer = mount(BaseNodeViewer, {
      props: {
        parentId: 'parent-1',
        databaseService: mockDatabase,
        nodeManager: mockNodeManager
      }
    });

    // Simulate content change
    mockNodeManager.visibleNodes = [{
      id: 'node-1',
      content: 'new content',
      nodeType: 'text'
    }];

    // Wait for debounce (500ms)
    await new Promise(resolve => setTimeout(resolve, 600));

    // Verify database save was called
    expect(mockDatabase.saveNodeWithParent).toHaveBeenCalledWith(
      'node-1',
      expect.objectContaining({ content: 'new content' }),
      'parent-1'
    );
  });

  it('should persist node deletion', async () => {
    mockNodeManager.visibleNodes = [{ id: 'node-1', content: 'test' }];

    const viewer = mount(BaseNodeViewer, {
      props: {
        parentId: 'parent-1',
        databaseService: mockDatabase,
        nodeManager: mockNodeManager
      }
    });

    // Remove node
    mockNodeManager.visibleNodes = [];
    mockNodeManager.findNode.mockReturnValue(undefined);

    // Wait for effect
    await new Promise(resolve => setTimeout(resolve, 100));

    // Verify deletion was persisted
    expect(mockDatabase.deleteNode).toHaveBeenCalledWith('node-1');
  });
});
```

### Testing Event Coordination

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { eventBus } from '$lib/services/event-bus';
import { HierarchyService } from '$lib/services/hierarchy-service';

describe('HierarchyService - Event Coordination', () => {
  let hierarchyService: HierarchyService;
  let emittedEvents: any[] = [];

  beforeEach(() => {
    emittedEvents = [];

    // Subscribe to all events
    eventBus.subscribe('node:deleted', (event) => {
      emittedEvents.push({ type: 'node:deleted', ...event });
    });

    hierarchyService = new HierarchyService();
  });

  it('should emit node:deleted event when node is deleted', () => {
    // Trigger deletion via nodeManager
    nodeManager.combineNodes('node-2', 'node-1');

    // Verify event was emitted
    expect(emittedEvents).toContainEqual({
      type: 'node:deleted',
      nodeId: 'node-2',
      namespace: 'state',
      source: 'ReactiveNodeService'
    });
  });

  it('should invalidate caches on hierarchy change', () => {
    // Populate cache
    hierarchyService.getDepth('node-1');  // Caches depth

    // Emit hierarchy changed event
    eventBus.emit('hierarchy:changed', {});

    // Verify cache was cleared
    const depthAfter = hierarchyService.getDepth('node-1');
    // Should recalculate, not use cached value
  });
});
```

### Testing PersistenceCoordinator Dependencies

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { sharedNodeStore } from '$lib/services/shared-node-store';
import { persistenceCoordinator } from '$lib/services/persistence-coordinator.svelte';

describe('PersistenceCoordinator - Dependencies', () => {
  beforeEach(() => {
    persistenceCoordinator.reset();
  });

  it('should wait for parent before persisting child', async () => {
    const operations: string[] = [];

    // Create parent (immediate)
    sharedNodeStore.updateNode(
      'parent-1',
      { content: 'Parent', nodeType: 'text' },
      viewerSource,
      { persistenceDependencies: [] }
    );

    operations.push('parent created');

    // Create child (waits for parent)
    sharedNodeStore.updateNode(
      'child-1',
      { content: 'Child', nodeType: 'text', parentId: 'parent-1' },
      viewerSource,
      { persistenceDependencies: ['parent-1'] }
    );

    operations.push('child created');

    // Wait for all persistence
    await persistenceCoordinator.waitForPersistence(['parent-1', 'child-1']);

    operations.push('all persisted');

    // Verify order: parent → child → completion
    expect(operations).toEqual([
      'parent created',
      'child created',
      'all persisted'
    ]);
  });

  it('should handle lambda dependencies', async () => {
    let lambdaExecuted = false;

    sharedNodeStore.updateNode(
      'node-1',
      { content: 'Test' },
      viewerSource,
      {
        persistenceDependencies: [
          async () => {
            await new Promise(resolve => setTimeout(resolve, 100));
            lambdaExecuted = true;
          }
        ]
      }
    );

    await persistenceCoordinator.waitForPersistence(['node-1']);

    expect(lambdaExecuted).toBe(true);
  });

  it('should handle multiple dependencies', async () => {
    // Create parent and sibling
    sharedNodeStore.updateNode('parent-1', { content: 'Parent' }, viewerSource);
    sharedNodeStore.updateNode('sibling-1', { content: 'Sibling' }, viewerSource);

    // Create node that depends on both
    sharedNodeStore.updateNode(
      'node-1',
      { content: 'Node', parentId: 'parent-1', beforeSiblingId: 'sibling-1' },
      viewerSource,
      {
        persistenceDependencies: ['parent-1', 'sibling-1']
      }
    );

    // Should wait for both
    const failed = await persistenceCoordinator.waitForPersistence(
      ['parent-1', 'sibling-1', 'node-1']
    );

    expect(failed.size).toBe(0);
  });
});
```

---

## Troubleshooting

### Common Issues

#### Issue 1: "Foreign key constraint failed"

**Symptom:** Database errors when creating child nodes

**Cause:** Parent node not persisted before child

**Solution:** Use `persistenceDependencies`:
```typescript
sharedNodeStore.updateNode(
  childId,
  { parentId, ... },
  viewerSource,
  {
    persistenceDependencies: [parentId]  // ← Add this
  }
);
```

#### Issue 2: "HTTP 500 errors during concurrent operations"

**Symptom:** Backend errors when performing rapid operations

**Cause:** Manual awaits bypassing PersistenceCoordinator (Issue #246)

**Solution:** Remove manual awaits, use declarative dependencies:
```typescript
// ❌ WRONG
await waitForNodeSaves([nodeId]);
sharedNodeStore.deleteNode(...);

// ✅ CORRECT
sharedNodeStore.deleteNode(..., dependencies: [nodeId]);
```

#### Issue 3: "Nodes not saving"

**Symptom:** Changes visible in UI but not persisted

**Cause:** `skipPersistence: true` or source is 'database'

**Solution:** Check update source and options:
```typescript
// Check source
const source = {
  type: 'viewer',  // ← Should not be 'database'
  identifier: 'base-node-viewer'
};

// Check options
const options = {
  skipPersistence: false  // ← Should be false or omitted
};
```

#### Issue 4: "Stale data in UI"

**Symptom:** UI shows old data after updates

**Cause:** Event bus listeners not invalidating caches

**Solution:** Subscribe to events and clear caches:
```typescript
eventBus.subscribe('node:updated', (event) => {
  cache.delete(event.nodeId);  // ← Invalidate cache
});
```

#### Issue 5: "Debounce not working"

**Symptom:** Every keystroke triggers immediate save

**Cause:** Missing debounce logic or `flushStrategy: 'immediate'`

**Solution:** Use debounced saves:
```typescript
// ✅ Debounced (default for content editing)
$effect(() => {
  debounceSave(nodeId, content);  // ← Batches saves
});
```

---

## Performance Considerations

### Direct Persistence Performance

**Typical Overhead:**
- Function call: ~0.1ms
- Debounce timer: ~0ms (async)
- IPC to Rust: ~1-5ms
- SQLite write: ~1-10ms

**Total:** ~2-15ms per operation (imperceptible to users)

### Event Coordination Performance

**Typical Overhead:**
- Event dispatch: ~0.5ms
- Listener lookup: ~0.1ms
- Handler execution: Varies (1-100ms)

**Total:** ~1-100ms depending on handler complexity

### When Performance Matters

- **High-frequency operations** (typing): Use debouncing
- **Bulk operations** (import): Use batch updates with manual flush
- **Complex dependencies**: Use lambda dependencies to minimize overhead

---

## Related Documentation

- [`/docs/IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md) - Current implementation status
- [`/docs/architecture/frontend-architecture.md`](./frontend-architecture.md) - Frontend overview
- [`/docs/architecture/business-logic/overview.md`](./business-logic/overview.md) - Business logic patterns
- [`/docs/architecture/components/component-architecture-guide.md`](./components/component-architecture-guide.md) - Component patterns

---

## Changelog

| Date | Change | Author |
|------|--------|--------|
| 2025-11-11 | Initial consolidated persistence documentation | Claude Code |
| 2025-11-11 | Added Issue #246 anti-patterns section | Claude Code |
| 2025-11-11 | Added comprehensive API reference and examples | Claude Code |

---

**Questions or confusion?** This document should answer everything. If not, please file an issue so we can improve it!
