# NodeSpace Hierarchy Persistence and Reactivity Architecture Review

**Date**: 2025-01-18
**Status**: ✅ **COMPLETED** - Implemented in Issue #614, PR #616
**Context**: Post-Graph Migration (PR #523)
**Purpose**: Establish foundation for multi-client consistency and eliminate reactivity anti-patterns

> **📋 Implementation Complete (2025-11-22)**
>
> This architectural analysis led to **Issue #614** which was implemented in **PR #616**.
> The `beforeSiblingId` field has been removed from the node model and replaced with
> fractional ordering on `has_child` edges. References to `beforeSiblingId` in this
> document represent the **original problem analysis**, not the current architecture.
>
> **Current Architecture**: Edge-based fractional ordering on `has_child.order` field.

---

## Executive Summary

### Critical Recommendation: **Database-Driven Reactivity with Complete Data-Structure Separation**

**Adopt SurrealDB LIVE SELECT + Complete Separation of Node Data from Structure** to fundamentally solve reactivity, multi-client consistency, and state synchronization.

**Key Architectural Decisions:**
1. ✅ **Database-Driven Reactivity** - Leverage LIVE SELECT for automatic UI updates
2. ✅ **Complete Separation** - Node data and structure are completely separate concerns
3. ✅ **Fractional Ordering** - Replace `beforeSiblingId` linked-list with `order` field on edges
4. ✅ **Dual Debouncing** - Data changes debounced (400ms), structure changes immediate (<10ms)
5. ✅ **Fire-and-Forget with Rollback** - Optimistic UI, rollback on failure, user notification
6. ✅ **Multi-Client Ready** - MCP server and Tauri frontend share same event streams
7. ✅ **Parallel Implementation** - Independent workstreams, not phased rollout
8. ✅ **Massive Simplification** - Delete 15 services (~9,000 lines), 73% code reduction

---

## 1. The Fundamental Problem

### 1.1 What Changed (Graph Migration - PR #523)

**Before:**
```typescript
// Parent stored as field on node
node: {
  id: 'child-1',
  parentId: 'parent-1',  // ← Reactive field
  beforeSiblingId: 'sibling-2'
}

// Svelte reactivity worked:
$: children = nodes.filter(n => n.parentId === parentId)
```

**After:**
```sql
-- Parent stored as graph edge (separate from nodes)
CREATE node:child-1 CONTENT { id: 'child-1', beforeSiblingId: 'sibling-2' };
RELATE node:parent-1->has_child->node:child-1;
```

**The Break:**
- **Node object no longer contains parent relationship**
- Frontend can't subscribe to `node.parentId` changes (doesn't exist!)
- Must query `has_child` edges separately to determine hierarchy
- Svelte reactivity broken - mutations to graph edges don't trigger `$derived` recalculations
- **HYBRID MESS**: Structure split between `node.beforeSiblingId` (field) and `has_child` (edge)

### 1.2 Current Band-Aid Solution

**Manual Cache Management:**
```typescript
// shared-node-store.ts:88-106
private nodes = new Map<string, Node>();           // ✅ Reactive (Map)
private childrenCache = new Map<string, string[]>(); // ❌ Non-reactive (plain Map)
private parentsCache = new Map<string, string[]>();  // ❌ Non-reactive (plain Map)
```

**Manual Triggers:**
```typescript
// reactive-node-service.svelte.ts:67
let _updateTrigger = $state(0);  // ← Increment to force re-renders

sharedNodeStore.subscribeAll((node) => {
  _updateTrigger++;  // ← Manual reactivity trigger
});
```

**Why This Fails:**
1. **Race Conditions** - Cache updates vs DB writes vs UI updates can interleave incorrectly
2. **Temporary Orphans** - Rapid indent/outdent can create brief periods where hierarchy is inconsistent
3. **Multi-Client Blind** - Tauri frontend can't see MCP server changes (no DB event stream)
4. **Developer Burden** - Every hierarchy operation requires manual cache synchronization
5. **Silent Failures** - Cache desync leads to incorrect rendering, no error surfaced
6. **Hybrid Confusion** - Structure split between fields and edges (inconsistent model)

### 1.3 Critical Multi-Client Context

**Two consumers of the same data:**

1. **Tauri Desktop App** (Svelte 5 frontend)
   - Primary user interface
   - Manual cache management (current approach)
   - No visibility into MCP server operations

2. **MCP Server** (Model Context Protocol)
   - AI agents programmatically manipulating nodes
   - Direct database writes
   - No notification to frontend when changes occur

**Current Architecture Fails This:**
```
MCP Server                Tauri Frontend
     |                          |
     v                          v
  [SurrealDB]              [Manual Cache]

# MCP creates node → Tauri cache stale → UI shows wrong state
# Frontend indents → MCP queries → Sees inconsistent hierarchy
```

---

## 2. Proposed Architecture: Complete Data-Structure Separation

### 2.1 Core Principle: Two Independent Reactive Systems

**Current (Coupled - Problematic):**
```typescript
interface Node {
  id: string;
  content: string;           // ← Data
  nodeType: string;          // ← Data
  beforeSiblingId: string;   // ← Structure (WHY IS THIS HERE?!)
}

// Problem: Updating content must preserve beforeSiblingId
// Structure scattered across nodes and edges
```

**Proposed (Separated - Clean):**
```typescript
// Node: ONLY data, NO structure
interface Node {
  id: string;
  content: string;
  nodeType: string;
  properties: Record<string, unknown>;
  version: number;
  // NO beforeSiblingId, NO parentId!
}

// Structure: ONLY in edges with fractional ordering
interface HasChildEdge {
  in: string;      // parent node ID
  out: string;     // child node ID
  order: number;   // ← Fractional ordering (replaces linked-list)
}

// Frontend: Two separate reactive stores
class ReactiveNodeData {
  nodes: Map<string, Node>;  // Data only
}

class ReactiveStructureTree {
  children: Map<string, ChildInfo[]>;  // Structure only
}
```

### 2.2 Benefits of Complete Separation

**1. Independent Debouncing Strategies**
```typescript
// Data: Debounced (typing can batch)
updateContent = debounce((id, content) => {
  db.update(id).merge({ content });
}, 400);  // 400ms = ~3-4 characters for fast typist

// Structure: Immediate (hierarchy changes must be instant)
async indentNode(id, newParent) {
  await db.moveNode(id, newParent);  // No debounce!
}
```

**2. Cleaner LIVE SELECT Subscriptions**
```typescript
// Subscribe separately to data vs structure
nodeStream.on('update', (node) => {
  nodeData.set(node.id, node);  // Data only
});

edgeStream.on('update', (edge) => {
  structureTree.updateEdge(edge);  // Structure only
});
```

**3. No Cascading Sibling Updates (Fractional Ordering)**
```typescript
// Current (linked-list): Insert between B and C requires 2 updates
UPDATE node:B SET beforeSiblingId = 'new';
UPDATE node:new SET beforeSiblingId = 'C';

// Proposed (fractional order): Insert without touching siblings
RELATE parent->has_child->new CONTENT { order: 1.5 };
// B (order: 1.0), new (order: 1.5), C (order: 2.0)
// NO sibling updates needed!
```

**4. Simpler Atomicity**
```rust
// Outdent node C (transfers sibling D to be child of C)
BEGIN TRANSACTION;
  -- Move C up one level
  DELETE has_child WHERE out = 'C';
  RELATE grandparent->has_child->C CONTENT { order: 2.5 };

  -- Transfer D to be child of C
  DELETE has_child WHERE out = 'D';
  RELATE C->has_child->D CONTENT { order: 1.0 };
COMMIT TRANSACTION;

// NO node fields to update, ONLY edges!
```

### 2.3 Database Schema: Hub-and-Spoke with Complete Separation

**Architecture Pattern**: Hub-and-Spoke with Bidirectional Record Links

```sql
-- ============================================================================
-- HUB TABLE (Universal metadata for ALL nodes)
-- ============================================================================

DEFINE TABLE node SCHEMAFULL;

-- NOTE: 'id' field is automatically managed by SurrealDB as Record ID (Thing type)
-- Do NOT redefine it - causes type conflicts and deserialization errors
DEFINE FIELD content ON TABLE node TYPE string;
DEFINE FIELD nodeType ON TABLE node TYPE string;
DEFINE FIELD data ON TABLE node TYPE option<record>;  -- Record Link to spoke (composition)
DEFINE FIELD version ON TABLE node TYPE int;
DEFINE FIELD createdAt ON TABLE node TYPE datetime;
DEFINE FIELD modifiedAt ON TABLE node TYPE datetime;

-- NO beforeSiblingId - structure lives in has_child edges!
-- NO parentId - hierarchy lives in has_child edges!

-- ============================================================================
-- SPOKE TABLES (Type-specific queryable data)
-- ============================================================================

-- Task spoke: Indexed fields for efficient queries
DEFINE TABLE task SCHEMAFULL;
-- NOTE: 'id' is automatically the Record ID - do NOT redefine it
DEFINE FIELD node ON TABLE task TYPE option<record>;  -- Reverse link to hub
DEFINE FIELD status ON TABLE task TYPE string DEFAULT 'todo';
DEFINE FIELD priority ON TABLE task TYPE option<string>;
DEFINE FIELD due_date ON TABLE task TYPE option<datetime>;
DEFINE FIELD assignee ON TABLE task TYPE option<record>;
DEFINE FIELD * ON TABLE task FLEXIBLE;  -- User extensions

-- Indexes for efficient spoke queries
DEFINE INDEX idx_task_status ON TABLE task COLUMNS status;
DEFINE INDEX idx_task_priority ON TABLE task COLUMNS priority;
DEFINE INDEX idx_task_due_date ON TABLE task COLUMNS due_date;

-- Date spoke: Timezone, holiday tracking
DEFINE TABLE date SCHEMAFULL;
-- NOTE: 'id' is automatically the Record ID - do NOT redefine it
DEFINE FIELD node ON TABLE date TYPE option<record>;
DEFINE FIELD timezone ON TABLE date TYPE string DEFAULT 'UTC';
DEFINE FIELD is_holiday ON TABLE date TYPE bool DEFAULT false;
DEFINE FIELD * ON TABLE date FLEXIBLE;

-- Schema spoke: Type definitions
DEFINE TABLE schema SCHEMAFULL;
-- NOTE: 'id' is automatically the Record ID - do NOT redefine it
DEFINE FIELD node ON TABLE schema TYPE option<record>;
DEFINE FIELD is_core ON TABLE schema TYPE bool DEFAULT false;
DEFINE FIELD version ON TABLE schema TYPE int DEFAULT 1;
DEFINE FIELD description ON TABLE schema TYPE string DEFAULT "";
DEFINE FIELD fields ON TABLE schema TYPE array DEFAULT [];
DEFINE FIELD * ON TABLE schema FLEXIBLE;

-- ============================================================================
-- GRAPH RELATIONS (Node-to-node relationships)
-- ============================================================================

-- Structure: Hierarchy with fractional ordering
DEFINE TABLE has_child SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD order ON TABLE has_child TYPE float;
DEFINE FIELD createdAt ON TABLE has_child TYPE datetime;
DEFINE FIELD version ON TABLE has_child TYPE int DEFAULT 1;  -- OCC

DEFINE INDEX idx_child_order ON TABLE has_child COLUMNS in, order;
DEFINE INDEX idx_unique_child ON TABLE has_child COLUMNS in, out UNIQUE;

-- References: Bidirectional mentions
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD createdAt ON TABLE mentions TYPE datetime;
DEFINE FIELD context ON TABLE mentions TYPE string DEFAULT "";
DEFINE FIELD offset ON TABLE mentions TYPE int DEFAULT 0;

DEFINE INDEX idx_mentions_in ON TABLE mentions COLUMNS in;
DEFINE INDEX idx_mentions_out ON TABLE mentions COLUMNS out;
DEFINE INDEX idx_unique_mention ON TABLE mentions COLUMNS in, out UNIQUE;
```

**Key Architectural Decisions:**

1. **Record Links (not RELATE) for Hub-Spoke**:
   - `node.data` → spoke (composition: node "contains" task data)
   - `task.node` → hub (reverse access for context)
   - Faster than RELATE edges (no graph traversal for 1-to-1)
   - Direct field access: `node.data.status`

2. **RELATE Edges for Relationships**:
   - `has_child`: Parent-child hierarchy
   - `mentions`: Cross-node references
   - Use when entities exist independently

3. **Bidirectional Links**:
   - Hub → Spoke: Fast spoke data access
   - Spoke → Hub: Efficient queries with hub context
   - Example: `SELECT *, node.content FROM task WHERE status = 'in_progress'`

**Query Patterns:**

```sql
-- Hub → Spoke (generic node context)
SELECT id, content, data.status, data.priority
FROM node WHERE nodeType = 'task';

-- Spoke → Hub (type-specific query)
SELECT *, node.content, node.createdAt
FROM task WHERE status = 'in_progress';

-- Hierarchy (via graph edges)
SELECT ->has_child->(node ORDER BY ->has_child.order).*
FROM node:parent;
```

**See Issue #560 for complete hub-and-spoke architecture specification.**

### 2.4 Fractional Ordering Algorithm

**Key Insight: Insert between siblings without updating them**

```typescript
class FractionalOrderManager {
  // Calculate order for new node inserted between prev and next
  calculateOrder(prevOrder: number | null, nextOrder: number | null): number {
    if (prevOrder === null && nextOrder === null) {
      return 1.0;  // First child
    }
    if (prevOrder === null) {
      return nextOrder - 1.0;  // Before all siblings
    }
    if (nextOrder === null) {
      return prevOrder + 1.0;  // After all siblings
    }

    // Between two siblings: average their orders
    return (prevOrder + nextOrder) / 2;
  }

  // Example usage:
  // Insert between B (1.0) and C (2.0) → order: 1.5
  // Insert between B (1.0) and new (1.5) → order: 1.25
  // Insert between new (1.5) and C (2.0) → order: 1.75
}
```

**Precision Management:**
```typescript
class StructureTree {
  async rebalanceIfNeeded(parentId: string) {
    const children = this.getChildren(parentId);

    // Check if orders are too close (< 0.0001 apart)
    const minGap = Math.min(...children.map((c, i, arr) =>
      i === 0 ? Infinity : c.order - arr[i-1].order
    ));

    if (minGap < 0.0001) {
      // Rebalance: Spread orders evenly from 1.0 to N.0
      await this.rebalanceChildren(parentId);
    }
  }
}
```

---

## 3. Frontend Architecture: Dual Reactive Stores

### 3.1 ReactiveNodeData (Content Store)

```typescript
// src/lib/stores/reactive-node-data.svelte.ts

import { listen } from '@tauri-apps/api/event';
import type { Node } from '$lib/types';

class ReactiveNodeData {
  // Reactive map (Svelte 5 $state)
  private nodes = $state(new Map<string, Node>());

  async initialize() {
    // Initial bulk load
    const initialNodes = await tauriNodeService.getAllNodes();
    initialNodes.forEach(node => this.nodes.set(node.id, node));

    // Subscribe to LIVE SELECT data events
    await listen<Node>('node:created', (event) => {
      this.nodes.set(event.payload.id, event.payload);
    });

    await listen<Node>('node:updated', (event) => {
      const existing = this.nodes.get(event.payload.id);
      if (existing) {
        this.nodes.set(event.payload.id, { ...existing, ...event.payload });
      }
    });

    await listen<string>('node:deleted', (event) => {
      this.nodes.delete(event.payload);
    });
  }

  // Reactive getter (works with $derived)
  getNode(id: string): Node | undefined {
    return this.nodes.get(id);
  }

  // Debounced content updates (400ms = optimal batching)
  updateContent = debounce((id: string, content: string) => {
    tauriNodeService.updateNode(id, { content });
    // LIVE SELECT confirms after 400ms
  }, 400);

  // Immediate property updates (no debounce for critical data)
  async updateProperties(id: string, properties: Record<string, unknown>) {
    await tauriNodeService.updateNode(id, { properties });
    // LIVE SELECT confirms immediately
  }
}

export const nodeData = new ReactiveNodeData();
```

### 3.2 ReactiveStructureTree (Hierarchy Store)

```typescript
// src/lib/stores/reactive-structure-tree.svelte.ts

import { listen } from '@tauri-apps/api/event';
import type { HasChildEdge } from '$lib/types';

interface ChildInfo {
  nodeId: string;
  order: number;
}

class ReactiveStructureTree {
  // Reactive map: parent → ordered children
  private children = $state(new Map<string, ChildInfo[]>());

  async initialize() {
    // Initial bulk load
    const initialEdges = await tauriNodeService.getAllEdges();
    this.buildTree(initialEdges);

    // Subscribe to LIVE SELECT structure events
    await listen<HasChildEdge>('edge:created', (event) => {
      this.addChild(event.payload);
    });

    await listen<HasChildEdge>('edge:deleted', (event) => {
      this.removeChild(event.payload);
    });

    await listen<HasChildEdge>('edge:updated', (event) => {
      // Order changed (rare, but possible during rebalancing)
      this.updateChildOrder(event.payload);
    });
  }

  // Reactive getter: Returns ordered child IDs
  getChildren(parentId: string): string[] {
    const childInfos = this.children.get(parentId) || [];
    // Already sorted on insertion (binary search)
    return childInfos.map(c => c.nodeId);
  }

  private buildTree(edges: HasChildEdge[]) {
    const tree = new Map<string, ChildInfo[]>();

    for (const edge of edges) {
      if (!tree.has(edge.in)) {
        tree.set(edge.in, []);
      }
      tree.get(edge.in)!.push({ nodeId: edge.out, order: edge.order });
    }

    // Sort all children by order
    for (const [parentId, children] of tree) {
      children.sort((a, b) => a.order - b.order);
      tree.set(parentId, children);
    }

    this.children = tree;
  }

  private addChild(edge: HasChildEdge) {
    const children = this.children.get(edge.in) || [];

    // Binary search insertion (keeps array sorted)
    const insertIndex = this.findInsertPosition(children, edge.order);
    children.splice(insertIndex, 0, { nodeId: edge.out, order: edge.order });

    this.children.set(edge.in, children);
  }

  private removeChild(edge: HasChildEdge) {
    const children = this.children.get(edge.in) || [];
    const filtered = children.filter(c => c.nodeId !== edge.out);
    this.children.set(edge.in, filtered);
  }

  private findInsertPosition(children: ChildInfo[], order: number): number {
    let left = 0;
    let right = children.length;

    while (left < right) {
      const mid = Math.floor((left + right) / 2);
      if (children[mid].order < order) {
        left = mid + 1;
      } else {
        right = mid;
      }
    }

    return left;
  }

  // Take snapshot for rollback on error
  snapshot(): Map<string, ChildInfo[]> {
    return new Map(this.children);
  }

  // Restore from snapshot (rollback on error)
  restore(snapshot: Map<string, ChildInfo[]>) {
    this.children = snapshot;
  }
}

export const structureTree = new ReactiveStructureTree();
```

### 3.3 Component Usage: Combining Data + Structure

```svelte
<!-- BaseNodeViewer.svelte -->
<script lang="ts">
  import { nodeData } from '$lib/stores/reactive-node-data.svelte';
  import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';

  let { parentId } = $props();

  // Reactive: Get ordered child IDs from structure
  let childIds = $derived(structureTree.getChildren(parentId));

  // Reactive: Get node data for each child
  let childNodes = $derived(
    childIds
      .map(id => nodeData.getNode(id))
      .filter(n => n !== undefined)
  );
</script>

{#each childNodes as child (child.id)}
  <BaseNode nodeId={child.id} content={child.content}>
    {#if structureTree.getChildren(child.id).length > 0}
      <svelte:self parentId={child.id} />
    {/if}
  </BaseNode>
{/each}
```

**Key Benefits:**
- ✅ Zero `$effect` blocks (pure `$derived`)
- ✅ Zero `_updateTrigger` hacks
- ✅ Automatic reactivity when either data or structure changes
- ✅ LIVE SELECT updates both stores independently

---

## 4. Backend Implementation: LIVE SELECT + Fractional Ordering

### 4.1 LIVE SELECT Service (Rust)

```rust
// src-tauri/src/services/live_query_service.rs

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use futures::StreamExt;
use tauri::Manager;

pub struct LiveQueryService {
    db: Surreal<Db>,
    app_handle: tauri::AppHandle,
}

impl LiveQueryService {
    pub async fn subscribe_to_changes(&self) -> Result<()> {
        // Subscribe to node data changes
        let mut node_stream = self.db
            .query("LIVE SELECT * FROM node")
            .await?
            .stream::<surrealdb::Notification<Node>>(0)?;

        // Subscribe to structure (edge) changes
        let mut edge_stream = self.db
            .query("LIVE SELECT * FROM has_child")
            .await?
            .stream::<surrealdb::Notification<HasChildEdge>>(0)?;

        // Spawn task for node events
        let app_handle = self.app_handle.clone();
        tokio::spawn(async move {
            while let Some(notification) = node_stream.next().await {
                match notification.action {
                    Action::Create => {
                        app_handle.emit_all("node:created", &notification.result).ok();
                    }
                    Action::Update => {
                        app_handle.emit_all("node:updated", &notification.result).ok();
                    }
                    Action::Delete => {
                        app_handle.emit_all("node:deleted", &notification.result.id).ok();
                    }
                    _ => {}
                }
            }
        });

        // Spawn task for edge events
        let app_handle = self.app_handle.clone();
        tokio::spawn(async move {
            while let Some(notification) = edge_stream.next().await {
                match notification.action {
                    Action::Create => {
                        app_handle.emit_all("edge:created", &notification.result).ok();
                    }
                    Action::Update => {
                        app_handle.emit_all("edge:updated", &notification.result).ok();
                    }
                    Action::Delete => {
                        app_handle.emit_all("edge:deleted", &notification.result).ok();
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }
}
```

### 4.2 Move Node Operation (Fractional Ordering)

```rust
// packages/core/src/db/surreal_store.rs

pub async fn move_node(
    &self,
    node_id: &str,
    new_parent_id: Option<&str>,
    insert_after: Option<&str>,  // Insert after this sibling
) -> Result<()> {
    // Calculate fractional order for new position
    let new_order = if let Some(after_id) = insert_after {
        // Get sibling before and after insertion point
        let siblings = self.get_children_ordered(new_parent_id).await?;
        let after_index = siblings.iter().position(|s| s.id == after_id).unwrap();

        let prev_order = siblings[after_index].order;
        let next_order = siblings.get(after_index + 1).map(|s| s.order);

        self.calculate_fractional_order(Some(prev_order), next_order)
    } else {
        // Insert at beginning
        let siblings = self.get_children_ordered(new_parent_id).await?;
        let first_order = siblings.first().map(|s| s.order);

        self.calculate_fractional_order(None, first_order)
    };

    // Single atomic transaction
    let transaction = if let Some(parent) = new_parent_id {
        r#"
            BEGIN TRANSACTION;

            -- Delete old parent edge
            DELETE has_child WHERE out = $node_id;

            -- Create new parent edge with fractional order
            RELATE $parent->has_child->$node_id CONTENT { order: $order };

            COMMIT TRANSACTION;
        "#
    } else {
        r#"
            BEGIN TRANSACTION;

            -- Delete parent edge (make root node)
            DELETE has_child WHERE out = $node_id;

            COMMIT TRANSACTION;
        "#
    };

    self.db.query(transaction)
        .bind(("node_id", node_id))
        .bind(("parent", new_parent_id))
        .bind(("order", new_order))
        .await?;

    Ok(())
}

fn calculate_fractional_order(
    &self,
    prev_order: Option<f64>,
    next_order: Option<f64>,
) -> f64 {
    match (prev_order, next_order) {
        (None, None) => 1.0,
        (None, Some(next)) => next - 1.0,
        (Some(prev), None) => prev + 1.0,
        (Some(prev), Some(next)) => (prev + next) / 2.0,
    }
}
```

### 4.3 Outdent Operation (Multi-Node Atomicity)

```rust
// packages/core/src/operations/mod.rs

pub async fn outdent_node(&self, node_id: &str) -> Result<()> {
    // Get current hierarchy
    let parent = self.get_parent(node_id).await?;
    let grandparent = self.get_parent(&parent).await?;
    let siblings_after = self.get_siblings_after(node_id).await?;

    // Calculate new order for node at grandparent level
    let new_order = self.calculate_order_after_sibling(&parent, &grandparent).await?;

    // Atomic transaction: Move node + transfer siblings
    self.db.query(r#"
        BEGIN TRANSACTION;

        -- Move node to grandparent level
        DELETE has_child WHERE out = $node_id;
        RELATE $grandparent->has_child->$node_id CONTENT { order: $new_order };

        -- Transfer siblings to be children of node
        FOR $sibling IN $siblings_after {
            DELETE has_child WHERE out = $sibling;
            RELATE $node_id->has_child->$sibling CONTENT { order: $sibling.order };
        };

        COMMIT TRANSACTION;
    "#)
    .bind(("node_id", node_id))
    .bind(("grandparent", grandparent))
    .bind(("new_order", new_order))
    .bind(("siblings_after", siblings_after))
    .await?;

    Ok(())
}
```

---

## 5. Fire-and-Forget with Rollback Strategy

### 5.1 Optimistic Operation Manager

```typescript
// src/lib/services/optimistic-operation-manager.svelte.ts

class OptimisticOperationManager {
  async executeStructuralChange(
    operation: () => Promise<void>,
    rollback: () => void,
  ) {
    // Take snapshot for rollback
    const snapshot = structureTree.snapshot();

    try {
      // Apply optimistic change immediately (UI updates)
      operation();

      // Fire-and-forget: Persist to backend
      // Don't await - let LIVE SELECT confirm

    } catch (error) {
      // Backend transaction failed - rollback + notify
      structureTree.restore(snapshot);

      eventBus.emit({
        type: 'error:operation-failed',
        message: 'Failed to update hierarchy',
        error,
        canRetry: true,
        action: operation  // Allow retry
      });
    }
  }
}

// Usage in component
async function handleIndent(nodeId: string) {
  await operationManager.executeStructuralChange(
    // Optimistic: Update local structure immediately
    () => structureTree.moveNode(nodeId, newParent),

    // Backend: Persist (fire-and-forget)
    async () => {
      await backend.moveNode(nodeId, newParent);
      // LIVE SELECT confirms success
    }
  );
}
```

### 5.2 Error Notification UI

```svelte
<!-- ErrorToast.svelte -->
<script lang="ts">
  import { eventBus } from '$lib/services/event-bus';

  let errors = $state<Array<{ id: string; message: string; action?: () => void }>>([]);

  $effect(() => {
    const unsubscribe = eventBus.on('error:operation-failed', (event) => {
      errors.push({
        id: crypto.randomUUID(),
        message: event.message,
        action: event.action
      });

      // Auto-dismiss after 5 seconds
      setTimeout(() => {
        errors = errors.filter(e => e.id !== id);
      }, 5000);
    });

    return () => unsubscribe();
  });
</script>

{#each errors as error (error.id)}
  <div class="error-toast">
    <p>{error.message}</p>
    {#if error.action}
      <button onclick={() => error.action()}>Retry</button>
    {/if}
  </div>
{/each}
```

---

## 6. Implementation Plan: Parallel Workstreams

**No phased rollout - work can proceed in parallel on independent components.**

### Workstream A: Backend Schema & LIVE SELECT (3-5 days)

**Can start immediately - foundation for all other work**

```bash
# Tasks:
- [ ] A1: Add order field to has_child edges (migration script)
- [ ] A2: Remove beforeSiblingId from node table (migration script)
- [ ] A3: Update move_node() to use fractional ordering
- [ ] A4: Set up LIVE SELECT event streams (node + edge)
- [ ] A5: Tauri event bridge (emit node:*, edge:* events)

# Deliverable:
- Database schema migrated
- LIVE SELECT streams emitting events to frontend
```

**Migration Script:**
```sql
BEGIN TRANSACTION;

-- Add order field to existing edges
FOR $edge IN (SELECT * FROM has_child) {
  LET $parent = $edge.in;
  LET $children = (SELECT * FROM has_child WHERE in = $parent ORDER BY out);
  LET $index = array::position($children, $edge);

  UPDATE has_child
    SET order = ($index + 1) * 1.0
    WHERE id = $edge.id;
};

-- Remove beforeSiblingId from nodes
UPDATE node UNSET beforeSiblingId;

COMMIT TRANSACTION;
```

### Workstream B: Frontend Structure Store (3-4 days)

**Can start in parallel with A - uses mock data initially**

```bash
# Tasks:
- [ ] B1: Create ReactiveStructureTree store
- [ ] B2: Implement fractional order insertion logic
- [ ] B3: LIVE SELECT event listeners (edge:created, edge:deleted)
- [ ] B4: Snapshot/restore for rollback
- [ ] B5: Binary search insertion (keep children sorted)

# Deliverable:
- Reactive structure store
- Works with mock edges initially
- Ready to integrate with real LIVE SELECT (from A5)
```

### Workstream C: Frontend Data Store (2-3 days)

**Can start in parallel with A & B - independent of structure**

```bash
# Tasks:
- [ ] C1: Create ReactiveNodeData store
- [ ] C2: Implement debounced content updates (200ms)
- [ ] C3: LIVE SELECT event listeners (node:created, node:updated, node:deleted)
- [ ] C4: Immediate property updates (no debounce)

# Deliverable:
- Reactive data store
- Debounced content persistence
- Ready to integrate with real LIVE SELECT (from A5)
```

### Workstream D: Component Integration (2-3 days)

**Depends on B & C completing - integrates stores with UI**

```bash
# Tasks:
- [ ] D1: Update BaseNodeViewer to use dual stores
- [ ] D2: Replace $effect blocks with $derived queries
- [ ] D3: Remove _updateTrigger pattern
- [ ] D4: Update all node components to use new stores

# Deliverable:
- All components using reactive stores
- Zero manual cache management
- Zero _updateTrigger hacks
```

### Workstream E: Operations Refactor (3-4 days)

**Depends on A, B, C completing - uses new backend + stores**

```bash
# Tasks:
- [ ] E1: Refactor indentNode() to use new structure store
- [ ] E2: Refactor outdentNode() with sibling transfer
- [ ] E3: Refactor createNode() with fractional ordering
- [ ] E4: Implement optimistic operation manager
- [ ] E5: Error handling + rollback + retry
- [ ] E6: Remove all old cache management code

# Deliverable:
- All operations using new architecture
- Fire-and-forget with rollback
- Old cache code deleted
```

### Workstream F: Testing & Documentation (2-3 days)

**Can start in parallel with D & E - independent verification**

```bash
# Tasks:
- [ ] F1: Multi-client integration tests (Tauri + MCP)
- [ ] F2: Rapid operation tests (Enter→Tab→Tab→Tab)
- [ ] F3: Error handling tests (rollback, retry)
- [ ] F4: Performance benchmarks (1000+ nodes)
- [ ] F5: Update architecture documentation
- [ ] F6: Developer migration guide

# Deliverable:
- Comprehensive test suite
- Performance validated
- Documentation complete
```

### Workstream Dependencies

```
Timeline (work in parallel):

Week 1:
├─ A (Backend) ──────────────────┐
├─ B (Structure Store) ──────────┤─► D (Integration)
└─ C (Data Store) ───────────────┘

Week 2:
├─ D (Integration) ──────────────┐
├─ E (Operations) ───────────────┤
└─ F (Testing) ──────────────────┘

Week 3:
└─ E (Operations) + F (Testing) finish
```

**Total: 2-3 weeks with 2-3 developers working in parallel**

---

## 7. Multi-Client Consistency Guarantees

### 7.1 Scenario: MCP Server Creates Node

```
T=0ms:   MCP server calls createNode()
T=1ms:   Backend creates node in database
T=2ms:   COMMIT TRANSACTION
T=3ms:   LIVE SELECT notification fires
T=4ms:   Tauri receives node:created event
T=5ms:   ReactiveNodeData adds node to Map
T=6ms:   Svelte re-renders with new node
T=7ms:   User sees node appear in UI

Result: 7ms latency for MCP → Tauri sync
```

### 7.2 Scenario: Tauri Indents, MCP Queries

```
T=0ms:   User presses Tab in Tauri
T=1ms:   Optimistic: structureTree.moveNode() (UI updates instantly)
T=2ms:   Backend: moveNode() transaction starts
T=3ms:   COMMIT TRANSACTION (edge updated)
T=4ms:   LIVE SELECT notification fires
T=5ms:   Tauri receives edge:updated event (confirms optimistic update)

T=5ms:   MCP server queries getChildren(parentId)
T=6ms:   Backend queries has_child edges (sees updated order)
T=7ms:   MCP receives correct hierarchy

Result: MCP sees change 7ms after user action
```

### 7.3 Conflict Resolution (Concurrent Edits)

```typescript
// Scenario: User A and User B both indent node C simultaneously

// User A (T=0ms):
indentNode('C', 'parent-1');  // Optimistic: order = 1.5

// User B (T=2ms):
indentNode('C', 'parent-2');  // Optimistic: order = 2.5

// Database (last-write-wins):
// T=3ms: Transaction A commits (C → parent-1)
// T=5ms: Transaction B commits (C → parent-2, overwrites A)

// LIVE SELECT notifications:
// T=6ms: User A receives edge:deleted (old edge)
// T=6ms: User A receives edge:created (User B's edge)
// Result: User A sees their change overwritten, structure syncs to User B's

// Optional: Conflict notification
eventBus.emit({
  type: 'warning:concurrent-edit',
  message: 'Another user modified this node',
  nodeId: 'C'
});
```

---

## 8. Performance Characteristics

### 8.1 Latency Benchmarks

| Operation | Current | Proposed | Improvement |
|-----------|---------|----------|-------------|
| Content edit (typing) | 200ms debounce | 400ms debounce | Better batching (50% fewer writes) |
| Indent/outdent | 50ms + cache sync | 10ms (fire-and-forget) | **5x faster** |
| Multi-node outdent (C + D) | 100ms + cache | 10ms (atomic) | **10x faster** |
| MCP → Tauri sync | ∞ (manual refresh) | 7ms (LIVE SELECT) | **∞ improvement** |

### 8.2 Memory Usage

**Current (Manual Cache):**
```
- nodes: Map<string, Node>          (100 nodes × 1KB = 100KB)
- childrenCache: Map<string, []>    (100 entries × 500B = 50KB)
- parentsCache: Map<string, []>     (100 entries × 500B = 50KB)
Total: ~200KB for 100 nodes
```

**Proposed (Separated):**
```
- nodeData.nodes: Map<string, Node>           (100 nodes × 800B = 80KB)  ← Smaller (no beforeSiblingId)
- structureTree.children: Map<string, []>     (100 entries × 300B = 30KB)
Total: ~110KB for 100 nodes (45% reduction!)
```

### 8.3 Scalability

**Fractional ordering precision:**
- Initial orders: 1.0, 2.0, 3.0, ... (1.0 spacing)
- After 10 inserts: 1.0, 1.5, 1.75, 1.875, ... (~0.06 spacing)
- After 20 inserts: ~0.0003 spacing
- After 30 inserts: **Rebalance needed** (< 0.0001 spacing)

**Rebalancing strategy:**
- Trigger: When any two siblings have gap < 0.0001
- Action: Redistribute all siblings evenly (1.0, 2.0, 3.0, ...)
- Frequency: ~1 rebalance per 30 rapid insertions at same location
- Cost: Single batch transaction updating N edges

---

## 9. Migration from Current Architecture

### 9.1 Database Schema Migration

```sql
-- Step 1: Add order field to all existing edges
BEGIN TRANSACTION;

-- For each parent, order children by beforeSiblingId linked-list
FOR $parent IN (SELECT VALUE DISTINCT in FROM has_child) {
  -- Rebuild linked-list order
  LET $children = (SELECT * FROM has_child WHERE in = $parent);
  LET $ordered = fn::rebuild_sibling_order($children);

  -- Assign fractional orders
  FOR $i, $child IN $ordered {
    UPDATE has_child
      SET order = ($i + 1) * 1.0
      WHERE in = $parent AND out = $child.out;
  };
};

COMMIT TRANSACTION;
```

```rust
// Custom function: Rebuild linked-list order
fn rebuild_sibling_order(children: Vec<Edge>) -> Vec<Edge> {
    let mut ordered = Vec::new();
    let mut current = children.iter().find(|c| c.before_sibling_id.is_none());

    while let Some(node) = current {
        ordered.push(node.clone());
        current = children.iter().find(|c|
            c.before_sibling_id.as_ref() == Some(&node.out)
        );
    }

    ordered
}
```

```sql
-- Step 2: Remove beforeSiblingId from all nodes
UPDATE node UNSET beforeSiblingId;
```

### 9.2 Frontend Migration

**Files to Delete (After Migration):**
```bash
# Manual cache management code
- childrenCache, parentsCache from SharedNodeStore
- updateChildrenCache, addChildToCache, removeChildFromCache methods
- _updateTrigger from ReactiveNodeService
- All $effect blocks watching hierarchy changes
```

**Files to Create:**
```bash
+ src/lib/stores/reactive-node-data.svelte.ts       (new)
+ src/lib/stores/reactive-structure-tree.svelte.ts  (new)
+ src/lib/services/optimistic-operation-manager.svelte.ts (new)
+ src-tauri/src/services/live_query_service.rs      (new)
```

**Files to Modify:**
```bash
~ src/lib/design/components/base-node-viewer.svelte  (use dual stores)
~ src/lib/services/reactive-node-service.svelte.ts   (remove cache logic)
~ packages/core/src/db/surreal_store.rs              (add fractional ordering)
```

---

## 10. Advantages Over Original LIVE SELECT Proposal

### Original Proposal: LIVE SELECT with beforeSiblingId

```typescript
// Still had structure mixed with data
interface Node {
  content: string;
  beforeSiblingId: string;  // ← Still a structural field on node
}

// Problems:
// - Updating content must preserve beforeSiblingId
// - Cascading updates to siblings when inserting
// - Structure still partially in node fields
```

### This Proposal: Complete Separation

```typescript
// Clean separation
interface Node {
  content: string;
  // NO structural fields!
}

interface HasChildEdge {
  in: string;
  out: string;
  order: number;  // ← All structure in edges
}

// Benefits:
// ✅ Updating content never touches structure
// ✅ No cascading sibling updates (fractional ordering)
// ✅ Clear mental model (data vs structure)
// ✅ Independent debouncing strategies
// ✅ Cleaner LIVE SELECT subscriptions
```

---

## 11. Risk Assessment

### Low Risk

**1. SurrealDB LIVE SELECT Maturity**
- Well-documented, stable feature (v1.1.0+)
- Works with embedded database (our use case)
- Automatic reconnection on disconnect
- Community adoption proven

**2. Fractional Ordering Proven**
- Used by: Figma, Linear, Notion
- Well-understood algorithm
- Periodic rebalancing handles precision loss
- Predictable performance characteristics

**3. Incremental Validation**
- Each workstream independently testable
- Can validate LIVE SELECT with POC before full migration
- Feature flags allow A/B testing
- Rollback to current architecture if needed

### Medium Risk (Mitigated)

**1. LIVE SELECT Event Throughput**
- **Risk**: 1000 rapid changes → 1000 events?
- **Mitigation**: Frontend microtask coalescing + debouncing
- **Validation**: Performance benchmarks before full rollout

**2. Fractional Order Precision**
- **Risk**: 100 rapid insertions → rebalance too often?
- **Mitigation**: Rebalance only when gap < 0.0001 (~30 insertions)
- **Validation**: Stress tests with rapid operations

**3. Multi-Client Conflicts**
- **Risk**: Two users indent same node simultaneously
- **Mitigation**: Last-write-wins + conflict notification
- **Future**: CRDT-based conflict resolution if needed

---

## 12. Success Metrics

### Quantitative

- ✅ **Zero `_updateTrigger++` calls** in codebase
- ✅ **Zero `$effect` blocks** for hierarchy reactivity
- ✅ **Zero `beforeSiblingId` fields** on nodes
- ✅ **<10ms latency** for structural changes
- ✅ **<100ms latency** for MCP→Tauri updates
- ✅ **<500ms initial render** for 1000 node documents
- ✅ **45% memory reduction** (110KB vs 200KB for 100 nodes)

### Qualitative

- ✅ **Developers find operations easier** to reason about
- ✅ **Multi-client consistency "just works"**
- ✅ **Fewer bug reports** related to cache desync
- ✅ **Clear mental model** (data vs structure)
- ✅ **Optimistic UI feels instant** (<50ms perceived latency)

---

## 13. Frontend Service Simplification

### 13.1 Current Architecture Complexity

**Current State: 30+ Services, 22,215 Lines of Code**

NodeSpace's current frontend has accumulated significant complexity through manual state management:

```
Data Management (14 services, ~8,000 lines):
├─ SharedNodeStore              # Manual cache management
├─ ReactiveNodeService          # _updateTrigger hacks
├─ PersistenceCoordinator       # Debouncing + queue management
├─ CacheCoordinator             # Cache invalidation
├─ NodeManager                  # Node CRUD operations
├─ BackendAdapter               # Tauri command wrapper
├─ TauriNodeService             # Another backend wrapper
├─ ConflictResolvers            # Version conflict detection
├─ VersionConflictResolver      # More conflict logic
├─ EventBus                     # Manual event propagation
├─ EventTypes                   # Event definitions
├─ HierarchyService             # Child/parent queries
├─ NodeReferenceService         # Reference tracking
└─ MentionSyncService           # Mention synchronization

UI State (5 services, ~2,000 lines):
├─ FocusManager                 # Focus + cursor positioning
├─ NavigationService            # Navigation state
├─ LayoutPersistenceService     # Tab/pane layout
├─ TabPersistenceService        # Open tabs state
└─ NodeExpansionCoordinator     # Expand/collapse state

Content Processing (6 services, ~4,500 lines):
├─ ContentProcessor             # AST processing
├─ WYSIWYGProcessor             # WYSIWYG for contenteditable (OBSOLETE)
├─ MarkdownPatternDetector      # Pattern detection
├─ SlashCommandService          # Slash commands
├─ SchemaService                # Schema validation
└─ CursorPositioningService     # Cursor geometry

Persistence (3 services, ~1,500 lines):
├─ DecorationCoordinator        # Visual decorations
├─ BaseNodeDecoration           # Decoration logic
└─ ComponentHydrationSystem     # Component hydration

Dev Tools (2 services, ~500 lines):
├─ DeveloperInspector           # Debug tools
└─ PerformanceTracker           # Metrics
```

### 13.2 Proposed Architecture (Simplified)

**New State: 14 Services, ~6,000 Lines (73% Reduction)**

```
Data Layer (2 services, ~500 lines - NEW):
├─ ReactiveNodeData             # LIVE SELECT subscriber (nodes)
└─ ReactiveStructureTree        # LIVE SELECT subscriber (edges)

UI State (4 services, ~1,500 lines - KEEP):
├─ FocusManager                 # Focus tracking + cursor positioning
├─ NavigationService            # Routing/navigation
├─ LayoutPersistenceService     # Tab/pane layout settings
└─ TabPersistenceService        # Open tabs state

Content/Domain Logic (6 services, ~3,500 lines - KEEP):
├─ ContentProcessor             # AST processing, validation
├─ MarkdownPatternDetector      # Pattern detection
├─ SlashCommandService          # Slash command handling
├─ SchemaService                # Schema validation
├─ CursorPositioningService     # Cursor geometry calculations
└─ NodeExpansionCoordinator     # Expand/collapse (simplified)

Dev Tools (2 services, ~500 lines - KEEP):
├─ DeveloperInspector           # Debug tools
└─ PerformanceTracker           # Metrics
```

### 13.3 Services to DELETE (15 services, ~9,000 lines)

**All manual data/cache/event management replaced by LIVE SELECT:**

```bash
❌ SharedNodeStore              # → ReactiveNodeData (LIVE SELECT)
❌ ReactiveNodeService          # → ReactiveStructureTree (LIVE SELECT)
❌ PersistenceCoordinator       # → LIVE SELECT auto-persists
❌ CacheCoordinator             # → No manual cache needed
❌ NodeManager                  # → Direct DB queries
❌ BackendAdapter               # → Simplified Tauri commands
❌ TauriNodeService             # → Merged into simplified adapter
❌ ConflictResolvers            # → Database version control
❌ VersionConflictResolver      # → LIVE SELECT handles conflicts
❌ EventBus                     # → Tauri events from LIVE SELECT
❌ EventTypes                   # → Tauri event types
❌ HierarchyService             # → ReactiveStructureTree
❌ NodeReferenceService         # → Database queries
❌ MentionSyncService           # → LIVE SELECT auto-syncs
❌ WYSIWYGProcessor             # → Obsolete (moved from contenteditable to textarea)
```

**Why these can be deleted:**
- LIVE SELECT provides automatic cache updates
- Tauri events replace manual event bus
- Database is single source of truth
- No manual synchronization needed

### 13.4 Services to KEEP (Critical Functionality)

**UI State (Keep - Not Related to Data Persistence):**

```bash
✅ FocusManager (focus-manager.svelte.ts)
   Purpose: Track which node has focus + cursor positioning
   Why Keep: Complex domain logic for cursor geometry
   - Vertical navigation (Up/Down between nodes)
   - Horizontal position preservation (accounting for indentation/font size)
   - Click-to-cursor position mapping (markdown syntax offsets)
   - NOT related to data persistence

✅ NavigationService (navigation-service.ts)
   Purpose: Routing and navigation state
   Why Keep: UI-level routing logic

✅ LayoutPersistenceService (layout-persistence-service.ts)
   Purpose: Remember which tabs are in which panes
   Why Keep: UI settings, not node data
   Persists to: localStorage or settings table

✅ TabPersistenceService (tab-persistence-service.ts)
   Purpose: Remember which tabs were open
   Why Keep: UI settings, not node data
   Persists to: localStorage or settings table
```

**Content/Domain Logic (Keep - Core Functionality):**

```bash
✅ ContentProcessor (content-processor.ts)
   Purpose: AST processing, wikilinks, validation
   Why Keep: Core content processing infrastructure
   - Source markdown → AST → Display HTML
   - Wikilink detection [[page]]
   - Node reference handling ns://node-id

✅ MarkdownPatternDetector (markdown-pattern-detector.ts)
   Purpose: Detect markdown patterns (headers, bold, italic, etc.)
   Why Keep: Core parsing logic for markdown
   - Detects: # headers, **bold**, *italic*, ` code`, etc.
   - Used by: ContentProcessor for AST generation

✅ SlashCommandService (slash-command-service.ts)
   Purpose: Handle /task, /header, etc. commands
   Why Keep: Core user interaction feature

✅ SchemaService (schema-service.ts)
   Purpose: Schema validation and management
   Why Keep: Core data validation logic

✅ CursorPositioningService (cursor-positioning-service.ts)
   Purpose: Complex cursor geometry calculations
   Why Keep: Critical for keyboard navigation
   - Calculate cursor position across different indentation levels
   - Account for font size differences between nodes
   - Map click position to content offset (with markdown syntax)
   Example: User presses Down arrow → calculate equivalent column in next node

✅ NodeExpansionCoordinator (node-expansion-coordinator.ts)
   Purpose: Track expand/collapse state of nodes
   Why Keep: UI state (could be simplified to component-level $state)
```

**Dev Tools (Keep):**

```bash
✅ DeveloperInspector (developer-inspector.ts)
   Purpose: Debug tools for development
   Why Keep: Developer productivity

✅ PerformanceTracker (performance-tracker.ts)
   Purpose: Performance metrics and monitoring
   Why Keep: Production monitoring
```

### 13.5 Why EventBus is Deleted

**Current EventBus Purpose:**
```typescript
// Manual event propagation across services
eventBus.emit({ type: 'node:updated', node });
eventBus.emit({ type: 'hierarchy:changed', nodeId });

// Every service must subscribe and emit manually
sharedNodeStore.on('node:updated', (node) => { /* update cache */ });
hierarchyService.on('hierarchy:changed', () => { /* invalidate cache */ });
```

**LIVE SELECT Replacement:**
```rust
// Backend automatically emits Tauri events via LIVE SELECT
impl LiveQueryService {
    pub async fn subscribe_to_changes(&self) {
        let mut node_stream = db.query("LIVE SELECT * FROM node").await?;

        while let Some(notification) = node_stream.next().await {
            // Automatically emit to frontend
            app_handle.emit_all("node:updated", &notification.result).ok();
        }
    }
}
```

```typescript
// Frontend subscribes directly to Tauri events
await listen('node:updated', (event) => {
  nodeData.set(event.payload.id, event.payload);
  // Svelte $state triggers re-render automatically
});

// NO manual event bus needed!
// NO manual cache invalidation needed!
// NO manual synchronization needed!
```

### 13.6 Complexity Comparison

| Category | Current | Proposed | Reduction |
|----------|---------|----------|-----------|
| **Data Management** | 14 services, ~8,000 lines | 2 services, ~500 lines | **94% reduction** |
| **Event System** | 2 services, ~500 lines | 0 services | **100% deletion** |
| **UI State** | 5 services, ~2,000 lines | 4 services, ~1,500 lines | **25% reduction** |
| **Content Logic** | 6 services, ~4,500 lines | 6 services, ~3,500 lines | **22% reduction** (delete WYSIWYGProcessor) |
| **Dev Tools** | 2 services, ~500 lines | 2 services, ~500 lines | **No change** |
| **TOTAL** | **30 services, 22,215 lines** | **14 services, ~6,000 lines** | **73% reduction** |

### 13.7 Component Usage (Before vs After)

**Current (Complex):**
```svelte
<script lang="ts">
  import { sharedNodeStore } from '$lib/services/shared-node-store';
  import { reactiveNodeService } from '$lib/services/reactive-node-service.svelte';
  import { hierarchyService } from '$lib/services/hierarchy-service';
  import { eventBus } from '$lib/services/event-bus';

  let { parentId } = $props();
  let _updateTrigger = $state(0);

  // Manual subscriptions
  $effect(() => {
    const unsubscribe = eventBus.on('hierarchy:changed', () => {
      _updateTrigger++;
    });
    return () => unsubscribe();
  });

  // Manual cache queries
  let children = $derived.by(() => {
    void _updateTrigger;  // Force re-computation
    return hierarchyService.getChildren(parentId).map(id =>
      sharedNodeStore.getNode(id)
    );
  });
</script>

{#each children as child}
  <BaseNode {child} />
{/each}
```

**Proposed (Simple):**
```svelte
<script lang="ts">
  import { nodeData } from '$lib/stores/reactive-node-data.svelte';
  import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';

  let { parentId } = $props();

  // Pure reactivity - NO manual subscriptions, NO _updateTrigger!
  let childIds = $derived(structureTree.getChildren(parentId));
  let children = $derived(childIds.map(id => nodeData.getNode(id)));
</script>

{#each children as child}
  <BaseNode {child} />
{/each}
```

### 13.8 Migration Strategy

**Phase 1: Add New Stores (Week 1)**
```bash
+ src/lib/stores/reactive-node-data.svelte.ts       (~200 lines)
+ src/lib/stores/reactive-structure-tree.svelte.ts  (~300 lines)
+ src-tauri/src/services/live_query_service.rs      (~200 lines)
```

**Phase 2: Update Components (Week 2)**
```bash
# Update all components to use new stores
~ src/lib/design/components/base-node-viewer.svelte
~ src/lib/design/components/base-node.svelte
~ src/lib/components/**/*.svelte
```

**Phase 3: Delete Old Services (Week 2)**
```bash
# Delete 15 obsolete services (~9,000 lines)
- src/lib/services/shared-node-store.ts
- src/lib/services/reactive-node-service.svelte.ts
- src/lib/services/persistence-coordinator.svelte.ts
- src/lib/services/cache-coordinator.ts
- src/lib/services/event-bus.ts
- src/lib/services/event-types.ts
- src/lib/services/node-manager.ts
- src/lib/services/backend-adapter.ts
- src/lib/services/tauri-node-service.ts
- src/lib/services/hierarchy-service.ts
- src/lib/services/node-reference-service.ts
- src/lib/services/mention-sync-service.ts
- src/lib/services/conflict-resolvers.ts
- src/lib/services/version-conflict-resolver.ts
- src/lib/services/wysiwyg-processor.ts
```

**Phase 4: Cleanup Legacy Naming (Week 2)**
```bash
# Fix legacy contenteditable naming (now using textarea)
# Find: id="contenteditable-{nodeId}"
# Replace: id="editor-{nodeId}"

# Update FocusManager references
~ src/lib/services/focus-manager.svelte.ts
~ src/lib/design/components/base-node-viewer.svelte
```

**Phase 5: Simplify Remaining Services (Week 3)**
```bash
# Simplify NodeExpansionCoordinator to component-level $state
# Remove DecorationCoordinator (move to components)
# Refactor ContentProcessor (remove contenteditable methods)
```

### 13.9 Future Enhancement: Hybrid Editing (Post-Fundamentals)

**After completing this architecture refactor**, NodeSpace will implement **Option D: Hybrid textarea with inline decorations** for @mentions:

```svelte
<!-- Future enhancement (not in this refactor) -->
<div class="editor-wrapper">
  <textarea bind:value={content} />
  <div class="decorations-overlay">
    <!-- Inline decorations for @mentions, wikilinks -->
    {#each decorations as decoration}
      <span class="mention-decoration" style="top: {decoration.y}px; left: {decoration.x}px">
        @{decoration.target}
      </span>
    {/each}
  </div>
</div>
```

This enhancement will be tackled AFTER the fundamental architecture is stable.

---

## 14. Conclusion

### Critical Architectural Decision

**Adopt Database-Driven Reactivity with Complete Data-Structure Separation** for NodeSpace hierarchy management.

### Why This Is The Right Foundation

1. **Multi-Client Consistency** - MCP + Tauri stay in sync automatically via LIVE SELECT
2. **Complete Separation** - Data and structure are independent concerns (cleaner than original proposal)
3. **Fractional Ordering** - No cascading sibling updates (O(1) inserts vs O(n) linked-list)
4. **Independent Debouncing** - Content (400ms) vs structure (immediate) strategies
5. **Fire-and-Forget** - Optimistic UI with rollback on failure
6. **Svelte 5 Best Practices** - `$derived` "just works" without hacks
7. **Future-Proof** - Enables collaborative editing, AI agents, real-time features
8. **Developer Experience** - 10x simpler code, fewer bugs
9. **Performance** - 5-10x faster operations, 45% memory reduction
10. **Massive Simplification** - Delete 15 services, 9,000 lines (73% reduction)

### Implementation Timeline

**2-3 weeks with 2-3 developers working in parallel**

- **Workstreams A-C** (week 1): Backend schema + LIVE SELECT + Frontend stores
- **Workstreams D-E** (week 2): Integration + Operations refactor
- **Workstream F** (week 3): Testing + Documentation

### Risk Assessment

**Low Risk:**
- SurrealDB LIVE SELECT is well-documented, stable feature
- Fractional ordering is proven algorithm (Figma, Linear, Notion)
- Embedded DB (same process) minimizes failure modes
- Independent workstreams allow parallel development
- Each workstream independently testable

**High Reward:**
- **73% code reduction** - Delete 15 services, 9,000 lines of accidental complexity
- Eliminates entire class of reactivity bugs
- Unblocks multi-client use cases (MCP server + Tauri + future sync client)
- Establishes foundation for future features (collaboration, AI agents, sync)
- 45% memory reduction, 5-10x faster operations
- Developer velocity increases dramatically (simpler architecture = faster iteration)

### Next Steps

1. **Approval** - Review this document with team
2. **Workstream Kickoff** - Assign developers to parallel workstreams
3. **A1-A5** - Backend team starts schema migration + LIVE SELECT
4. **B1-B5, C1-C4** - Frontend team starts reactive stores (use mocks initially)
5. **Integration** - Combine backend + frontend when both ready
6. **Testing** - Validate multi-client consistency, performance benchmarks
7. **Deployment** - No phased rollout (zero users), deploy when complete

---

**Document Status**: Final Recommendation - Complete Separation Architecture
**Reviewers**: NodeSpace Architecture Team
**Approval Required**: Yes
**Implementation Start**: Upon approval

---

## Appendix A: Code Removal Checklist

**Estimated 2000+ lines to delete:**

```bash
shared-node-store.ts:
  ❌ childrenCache, parentsCache (lines 88-106)
  ❌ updateChildrenCache, addChildToCache, removeChildFromCache
  ❌ clearChildrenCache, getNodesForParent

reactive-node-service.svelte.ts:
  ❌ _updateTrigger (line 67)
  ❌ _sortedChildrenCache (lines 101-107)
  ❌ invalidateSortedChildrenCache (lines 119-121)
  ❌ Manual cache updates in indentNode (15+ calls)
  ❌ Manual cache updates in outdentNode (12+ calls)

base-node-viewer.svelte:
  ❌ Manual watchers for structural changes
  ❌ Cache synchronization logic
  ❌ $effect blocks for hierarchy watching

Node interface:
  ❌ beforeSiblingId field
```

**Codebase Simplification:**
- **Before**: ~15 files with `$effect`, 2000+ lines of cache management
- **After**: ~5 files with lifecycle hooks, 200 lines of reactive queries

---

## Appendix B: Fractional Ordering Examples

```typescript
// Initial state:
// A
// ├─ B (order: 1.0)
// ├─ C (order: 2.0)
// └─ D (order: 3.0)

// Insert X between B and C:
const order = (1.0 + 2.0) / 2;  // 1.5
// A
// ├─ B (order: 1.0)
// ├─ X (order: 1.5)  ← Inserted without updating B or C!
// ├─ C (order: 2.0)
// └─ D (order: 3.0)

// Insert Y between B and X:
const order = (1.0 + 1.5) / 2;  // 1.25
// A
// ├─ B (order: 1.0)
// ├─ Y (order: 1.25)
// ├─ X (order: 1.5)
// ├─ C (order: 2.0)
// └─ D (order: 3.0)

// After 30 insertions at same location:
// A
// ├─ B (order: 1.0)
// ├─ ... (many nodes with close orders)
// ├─ Z (order: 1.00009765625)  ← Getting too close!
// └─ D (order: 3.0)

// Rebalance triggered (gap < 0.0001):
// A
// ├─ B (order: 1.0)  ← Redistributed
// ├─ ... (order: 2.0, 3.0, 4.0, ...)
// ├─ Z (order: 31.0)
// └─ D (order: 32.0)
```

---

## Appendix C: SurrealDB LIVE SELECT Documentation

**Official Docs**: https://surrealdb.com/docs/surrealql/statements/live

**Key Features:**
- Real-time notifications for CREATE/UPDATE/DELETE
- Works with embedded database (local-first)
- Automatic reconnection on disconnect
- Filter by WHERE clause (e.g., `LIVE SELECT * FROM node WHERE nodeType = 'task'`)

**Performance:**
- Notification latency: <1ms (embedded DB, same process)
- Throughput: 10,000+ events/second (embedded)
- Memory: Minimal (stream metadata only, not data snapshot)

---

**End of Document**
