# NodeSpace Architecture: Target State

**Version**: 3.0 (Universal Graph Architecture)
**Date**: December 2025
**Status**: Approved for Implementation

---

## Executive Summary

NodeSpace is transitioning to a **Universal Graph Architecture** powered by SurrealDB. This architecture eliminates spoke tables in favor of **Pure JSON Properties with Partial Indexes**, enabling:

- **AI-Native Context**: Agents traverse arbitrary relationships via MCP without schema friction
- **Playbook Marketplace**: Users install new methodologies (Schema-as-Node) without database migrations
- **Local-First Performance**: Zero-latency local reads with SurrealDB Cloud handling sync and RLS
- **Simplified Sync**: Single-record atomic operations eliminate "ghost node" problems

### Architecture Evolution

| Era | Pattern | Status |
|-----|---------|--------|
| Pre-2025 | Turso/libSQL with JSON properties | Archived |
| Early 2025 | Hub-and-Spoke with Record Links | **Being Retired** |
| **Target State** | Universal Graph with Pure JSON | **Implementing** |

---

## 1. Core Data Model: The Universal Node

### 1.1 Single Table Polymorphism

Every entity in NodeSpace—Task, Document, Schema, Workflow—is a record in the `node` table:

```sql
DEFINE TABLE node SCHEMAFULL;

-- The Discriminator
DEFINE FIELD node_type ON TABLE node TYPE string ASSERT $value != NONE;

-- Primary Content
DEFINE FIELD content ON TABLE node TYPE string DEFAULT "";

-- Pure JSON Properties (ALL type-specific data lives here)
DEFINE FIELD properties ON TABLE node TYPE object DEFAULT {};

-- System Metadata
DEFINE FIELD created_at ON TABLE node TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE node TYPE datetime DEFAULT time::now();
DEFINE FIELD version ON TABLE node TYPE int DEFAULT 1;

-- Indexes
DEFINE INDEX idx_node_type ON TABLE node COLUMNS node_type;
DEFINE INDEX idx_node_modified ON TABLE node COLUMNS modified_at;
```

### 1.2 Why Pure JSON Properties

| Aspect | Hub-and-Spoke (Old) | Pure JSON (New) | Benefit |
|--------|---------------------|-----------------|---------|
| **Querying** | Requires JOINs (node + task) | Single Read | Zero latency for UI |
| **Sorting** | Hard (Sort by spoke.priority + hub.created) | Instant | Solves "split-brain" sorting |
| **Sync** | Complex (2 tables atomic) | Atomic (1 Record) | Eliminates "ghost nodes" |
| **Flexibility** | Requires CREATE TABLE | Zero DDL | Enables Playbooks |
| **Property Access** | N+1 fetch from spoke | Direct from properties | Eliminates query overhead |

### 1.3 Partial Indexes for Performance

Type-specific indexes without spoke tables:

```sql
-- Task indexes (only index tasks, ignore text/headers)
DEFINE INDEX idx_task_status ON node
  COLUMNS properties.status
  WHERE node_type = 'task';

DEFINE INDEX idx_task_priority ON node
  COLUMNS properties.priority
  WHERE node_type = 'task';

-- Schema indexes
DEFINE INDEX idx_schema_is_core ON node
  COLUMNS properties.isCore
  WHERE node_type = 'schema';
```

### 1.4 Rust Type Enforcement

Validation happens at application layer via Serde, not database DDL:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub node_type: String,
    pub content: String,
    pub properties: serde_json::Value,  // Pure JSON
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}
```

---

## 2. Relationship Model: Universal Edge Table

Following the same principle as Universal Nodes, all relationships are stored in a **single `edge` table** with type discrimination. This enables custom relationship types without DDL, supporting the Playbook Marketplace vision.

### 2.1 Single Edge Table

```sql
DEFINE TABLE edge SCHEMAFULL TYPE RELATION IN node OUT node;

-- The Discriminator
DEFINE FIELD edge_type ON TABLE edge TYPE string ASSERT $value != NONE;

-- Common Fields
DEFINE FIELD properties ON TABLE edge TYPE object DEFAULT {};
DEFINE FIELD version ON TABLE edge TYPE int DEFAULT 1;
DEFINE FIELD created_at ON TABLE edge TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE edge TYPE datetime DEFAULT time::now();

-- Core indexes
DEFINE INDEX idx_edge_type ON TABLE edge COLUMNS edge_type;
DEFINE INDEX idx_edge_in ON TABLE edge COLUMNS in, edge_type;
DEFINE INDEX idx_edge_out ON TABLE edge COLUMNS out, edge_type;
```

### 2.2 Core Edge Types

| edge_type | Purpose | Key Properties |
|-----------|---------|----------------|
| `has_child` | Document tree hierarchy | `order` (float for fractional ordering) |
| `mentions` | @references and [[links]] | `context`, `offset`, `root_id` |
| `member_of` | Collection membership | - |

### 2.3 Partial Indexes for Core Types

```sql
-- has_child: Ordered children lookup
DEFINE INDEX idx_edge_child_order ON edge
  COLUMNS in, properties.order
  WHERE edge_type = 'has_child';

-- has_child: Unique parent-child constraint
DEFINE INDEX idx_edge_unique_child ON edge
  COLUMNS in, out UNIQUE
  WHERE edge_type = 'has_child';

-- mentions: Backlinks lookup
DEFINE INDEX idx_edge_mentions_target ON edge
  COLUMNS out
  WHERE edge_type = 'mentions';
```

### 2.4 Edge Properties by Type

**has_child** (structural):
```json
{
  "edge_type": "has_child",
  "properties": {
    "order": 1.5
  }
}
```

**mentions** (semantic):
```json
{
  "edge_type": "mentions",
  "properties": {
    "context": "See also",
    "offset": 42,
    "root_id": "node:abc123"
  }
}
```

**Custom relationship** (user-defined):
```json
{
  "edge_type": "blocked_by",
  "properties": {
    "reason": "Waiting for API approval",
    "blocking_since": "2025-01-06"
  }
}
```

### 2.5 Graph Traversal Queries

```sql
-- Get children ordered
SELECT out.* FROM edge
WHERE in = $parent_id AND edge_type = 'has_child'
ORDER BY properties.order ASC;

-- Get all nodes that mention this node (backlinks)
SELECT in.* FROM edge
WHERE out = $node_id AND edge_type = 'mentions';

-- Find tasks blocked by high-priority bugs (single traversal)
SELECT * FROM node
WHERE node_type = 'task'
  AND ->edge[WHERE edge_type = 'blocked_by']->node[WHERE properties.priority = 'high'];

-- Get all relationships for a node (any type)
SELECT * FROM edge WHERE in = $node_id OR out = $node_id;
```

### 2.6 Why Universal Edge Table

| Aspect | Separate Tables (Old) | Single Edge Table (New) |
|--------|----------------------|-------------------------|
| **Custom relationships** | Requires DDL | Zero DDL |
| **Playbook Marketplace** | Can't add relationship types | Install adds new edge_types |
| **Consistency** | Nodes unified, edges fragmented | Fully unified model |
| **Query all relationships** | UNION across tables | Single table query |
| **Sync complexity** | Multiple edge tables | One edge table |

### 2.7 Fractional Ordering

The `order` property in `has_child` edges uses floats for O(1) inserts:
- Insert between 1.0 and 2.0 → use 1.5
- Insert between 1.0 and 1.5 → use 1.25
- Periodic rebalancing when precision degrades

---

## 3. Schema & Playbooks: Schema-as-Node

### 3.1 Schema Definition

Schemas are stored as nodes with `node_type = 'schema'`:

```json
{
  "id": "task",
  "node_type": "schema",
  "content": "Task",
  "properties": {
    "isCore": true,
    "schemaVersion": 1,
    "description": "Task tracking with status and due dates",
    "fields": [
      {
        "name": "status",
        "type": "enum",
        "protection": "core",
        "coreValues": [
          {"value": "open", "label": "Open"},
          {"value": "in_progress", "label": "In Progress"},
          {"value": "done", "label": "Done"},
          {"value": "cancelled", "label": "Cancelled"}
        ],
        "userValues": [],
        "extensible": true,
        "indexed": true,
        "required": true,
        "default": "open"
      },
      {
        "name": "priority",
        "type": "enum",
        "protection": "user",
        "coreValues": [
          {"value": "urgent", "label": "Urgent"},
          {"value": "high", "label": "High"},
          {"value": "medium", "label": "Medium"},
          {"value": "low", "label": "Low"}
        ],
        "indexed": true
      }
    ],
    "relationships": []
  }
}
```

### 3.2 Playbook Marketplace Flow

1. **Install**: User installs "Scrum Playbook"
2. **Action**: System inserts schema nodes (Sprint, UserStory, Epic)
3. **Result**: UI immediately renders new types—no database restart or migration

### 3.3 Schema Protection

```sql
-- Only admins can modify schema nodes
DEFINE TABLE node SCHEMAFULL
  PERMISSIONS
    FOR create, update, delete WHERE
       (node_type != 'schema') OR ($auth.role = 'admin');
```

---

## 4. Semantic Search: Root-Aggregate Embeddings

### 4.1 Embedding Table

```sql
DEFINE TABLE embedding SCHEMAFULL;
DEFINE FIELD node ON TABLE embedding TYPE record<node>;
DEFINE FIELD vector ON TABLE embedding TYPE array<float>;
DEFINE FIELD dimension ON TABLE embedding TYPE int DEFAULT 768;
DEFINE FIELD model_name ON TABLE embedding TYPE string DEFAULT 'nomic-embed-text-v1.5';
DEFINE FIELD chunk_index ON TABLE embedding TYPE int DEFAULT 0;
DEFINE FIELD total_chunks ON TABLE embedding TYPE int DEFAULT 1;
DEFINE FIELD content_hash ON TABLE embedding TYPE option<string>;
DEFINE FIELD stale ON TABLE embedding TYPE bool DEFAULT true;
DEFINE FIELD created_at ON TABLE embedding TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE embedding TYPE datetime DEFAULT time::now();

-- MTREE vector index for fast KNN search
DEFINE INDEX idx_embedding_vector ON TABLE embedding
  COLUMNS vector MTREE DIMENSION 768 DIST COSINE TYPE F32;
```

### 4.2 Root-Aggregate Model

- Only **root nodes** (no parent edge) of embeddable types get embedded
- Embeddings represent semantic content of entire subtree
- Embeddable types: `text`, `header`, `code-block`, `schema`
- NOT embeddable: `task`, `date`, child nodes

### 4.3 MTREE Index Stability

> **⚠️ Testing Requirement**: MTREE vector indexes are a newer SurrealDB feature. The test suite must include heavy-load scenarios to validate stability.

**Required test coverage:**
- Insert 10,000+ vectors and query immediately (tests rebalancing under load)
- Concurrent insert/query operations
- Index recovery after application restart

These tests should be part of the performance test suite (`src/tests/performance/`) and run before major releases or SurrealDB version upgrades.

---

## 5. Security & Access: ABAC + RLS

### 5.1 The "Key-Ring" Pattern

Permissions compiled onto User record for O(1) lookups:

```javascript
user: {
  id: "user-123",
  role: "member",
  type_grants: {
    "task": "write",
    "invoice": "read",
    "hr_*": "none"
  }
}
```

### 5.2 Row-Level Security

Cloud database enforces security before data leaves the server:

```sql
DEFINE TABLE node SCHEMAFULL
  PERMISSIONS
    FOR select, update, delete WHERE
      -- Admin Override
      $auth.role = 'owner'
      OR
      -- Key-Ring Check (O(1) Lookup)
      ($auth.type_grants[node_type] != NONE);
```

### 5.3 Type Field Protection

Prevent permission bypass via type mutation:

```sql
DEFINE FIELD node_type ON node TYPE string
  PERMISSIONS FOR update WHERE $auth.role = 'admin' OR node_type = $value;
```

---

## 6. Sync Architecture

### 6.1 Local-First with SurrealDB Cloud

```
┌──────────────────┐         WebSocket          ┌──────────────────┐
│   Desktop App    │◄─────────────────────────►│  SurrealDB Cloud │
│  (Local RocksDB) │       LIVE SELECT          │  (Coordination)  │
└──────────────────┘                            └──────────────────┘
        │                                               │
        │ Source of Truth                               │ RLS Enforcement
        │ (user's data)                                 │ Rate Limiting
        ▼                                               ▼
   Full Offline                                   Managed Scaling
   Functionality                                  (their problem)
```

### 6.2 Authentication Flow

1. **Login**: Desktop app authenticates against SurrealDB Cloud Scope
2. **Token**: Receives JWT with `id`, `role`, `type_grants`
3. **Sync**: Opens `LIVE SELECT * FROM node` stream
4. **Security**: SurrealDB checks JWT against RLS for every event

### 6.3 Conflict Resolution: Conflicted Copies

Using the Dropbox model—don't auto-merge, create copies:

1. **Detection**: OCC via `version` field
2. **Rejection**: If cloud version higher, update fails
3. **Resolution**: Create new node with `(Conflicted Copy YYYY-MM-DD)` suffix
4. **User Action**: Manual merge, user stays in control

---

## 7. Backup & Recovery: 3-2-1 Strategy

### 7.1 Three-Tier Approach

| Tier | Scenario | Strategy |
|------|----------|----------|
| **Local-Only** | No cloud sync | RocksDB snapshots + JSON export |
| **Local + Cloud** | Syncing user | Local is source of truth, cloud holds sync log |
| **Cloud-Only** | Web app (future) | Cloud backup with RLS-aware export |

### 7.2 Local Snapshots

```rust
// Daily automated backup
#[tauri::command]
pub async fn export_to_json(path: String) -> Result<ExportResult> {
    // Export nodes, edges, embeddings to JSON
}

#[tauri::command]
pub async fn backup_rocksdb(path: String) -> Result<BackupResult> {
    // RocksDB native snapshot
}
```

### 7.3 GDPR Compliance

- **Personal Space**: Hard delete (`DELETE WHERE owner = $user_id`)
- **Shared Space**: Anonymization (overwrite PII, preserve node for team)

---

## 8. Workflow Automation: Hybrid Reactors

### 8.1 Workflow as Data

```json
{
  "node_type": "workflow",
  "content": "Notify on Review",
  "properties": {
    "execution": "cloud",
    "trigger": {"event": "update", "target_type": "task"},
    "condition": "node.properties.status == 'Ready' && node.properties.priority == 'High'",
    "actions": [{"type": "send_email", "template": "review_req"}]
  }
}
```

### 8.2 Two Reactors

| Reactor | Location | Scope | Tier |
|---------|----------|-------|------|
| **Local** | Desktop App | Data integrity, create_task, update_record | Free |
| **Cloud** | Management API | Side effects, send_email, slack_notify | Premium |

### 8.3 Rate Limiting (Cloud Reactor)

```
Free Tier:  0 cloud executions (feature gated)
Pro Tier:   1,000 executions/day
Enterprise: Unlimited
```

---

## 9. Schema Validation & Deletion

### 9.1 Application-Layer Validation

```rust
impl SchemaValidator {
    pub fn validate(&self, node: &Node) -> Result<(), ValidationErrors> {
        let schema = self.get_schema(&node.node_type)?;

        for field in &schema.fields {
            let value = node.properties.get(&field.name);

            if field.required && value.is_none() {
                return Err(ValidationError::MissingRequired(field.name.clone()));
            }

            if let Some(value) = value {
                self.validate_field_type(value, &field.field_type)?;
            }
        }
        Ok(())
    }
}
```

### 9.2 Schema Soft Delete

Cannot delete schema if nodes of that type exist:

```rust
pub async fn delete_schema(&self, schema_id: &str) -> Result<DeleteSchemaResult> {
    let count = self.count_nodes_of_type(schema_id).await?;

    if count > 0 {
        return Err(SchemaError::NodesExist {
            schema_id: schema_id.to_string(),
            node_count: count,
            suggestion: "Archive the schema instead",
        });
    }

    // Safe to delete
    self.db.delete_node(schema_id).await
}

pub async fn archive_schema(&self, schema_id: &str) -> Result<()> {
    self.db.update_properties(schema_id, json!({"active": false})).await
}
```

---

## 10. Auto-Optimization: The Janitor

### 10.1 Query-Based Index Tuning

Since users define custom types/queries, we cannot pre-index everything:

1. **Observer**: Rust client logs query latency locally
2. **Janitor**: Background process analyzes logs
3. **Action**: Creates local indexes for slow, frequent queries
4. **Isolation**: These indexes are NEVER synced to cloud

### 10.2 Index Creation Safety

> **⚠️ Operational Note**: `DEFINE INDEX` operations can block queries during creation. Index operations must NOT execute during active user sessions.

**Safe execution windows:**
- Application startup (before UI is interactive)
- Explicit user-triggered "Optimize Database" action
- Extended idle periods (no user activity for 60+ seconds)

The Janitor observes and recommends indexes, but actual creation is deferred to safe windows.

---

## 11. Technology Stack

### 11.1 Current Versions

| Technology | Version | Role |
|------------|---------|------|
| **Rust** | 1.80+ | Backend, Core Library |
| **Bun** | 1.0+ | Package manager, runtime |
| **Svelte** | 5.39+ | Frontend framework |
| **SvelteKit** | 2.42+ | Routing |
| **Tauri** | 2.8+ | Desktop framework |
| **SurrealDB** | 2.3+ | Embedded database |
| **TypeScript** | 5.6 | Frontend type safety |

### 11.2 Key Dependencies

**Rust:**
- `surrealdb` - Database driver
- `tokio` - Async runtime
- `serde/serde_json` - Serialization
- `candle` - Local ML inference

**Frontend:**
- `@tauri-apps/api` - IPC
- `bits-ui` - UI components
- `tailwindcss` - Styling

---

## 12. Migration from Hub-and-Spoke

### 12.1 What Changes

| Component | Action |
|-----------|--------|
| Spoke tables (`task`, `schema`) | **Delete** - data moves to `node.properties` |
| `data` field (Record Link) | **Delete** - no longer needed |
| Edge tables (`has_child`, `mentions`, `member_of`) | **Consolidate** - migrate to single `edge` table |
| `SchemaTableManager` | **Simplify** - index management only |
| Spoke queries | **Remove** - single-table queries |
| N+1 property fetching | **Eliminate** - properties inline |

### 12.2 Migration Script

> **Note on Migration Phases:**
> - **Current phase (zero users)**: No batching needed—can reset database or run migrations directly
> - **Future phase (with users)**: Use batched updates to avoid long-running transactions

```sql
-- 1. Move spoke data to properties (one-time migration)
UPDATE node SET properties = (
  SELECT * OMIT id, node FROM task WHERE node = $parent.id
) WHERE node_type = 'task';

-- 2. Delete spoke tables
REMOVE TABLE task;
REMOVE TABLE schema;  -- Schema data already in properties

-- 3. Consolidate edge tables into single 'edge' table
-- Migrate has_child
INSERT INTO edge SELECT
  in, out,
  'has_child' AS edge_type,
  { order: order } AS properties,
  version,
  created_at,
  time::now() AS modified_at
FROM has_child;

-- Migrate mentions
INSERT INTO edge SELECT
  in, out,
  'mentions' AS edge_type,
  { context: context, offset: offset, root_id: root_id } AS properties,
  1 AS version,
  created_at,
  time::now() AS modified_at
FROM mentions;

-- Migrate member_of
INSERT INTO edge SELECT
  in, out,
  'member_of' AS edge_type,
  {} AS properties,
  1 AS version,
  created_at,
  time::now() AS modified_at
FROM member_of;

-- 4. Delete old edge tables
REMOVE TABLE has_child;
REMOVE TABLE mentions;
REMOVE TABLE member_of;

-- 5. Add partial indexes for nodes
DEFINE INDEX idx_task_status ON node
  COLUMNS properties.status
  WHERE node_type = 'task';

-- 6. Add partial indexes for edges (see Section 2.3)
```

**For future migrations with users** (batched pattern):
```sql
-- Process in batches to avoid blocking
LET $batch = (SELECT id FROM node WHERE node_type = 'task' LIMIT 1000);
FOR $id IN $batch {
  UPDATE $id SET properties.new_field = <computed_value>;
}
-- Repeat until no records remain
```

### 12.3 Estimated Effort

| Phase | Work | Duration |
|-------|------|----------|
| Node consolidation | Remove spokes, move to properties | 1 day |
| Edge consolidation | Merge edge tables into single `edge` table | 1 day |
| Rust code | Remove spoke/edge-related code, update queries | 2-3 days |
| Testing | Validate all queries work | 1 day |
| **Total** | | ~1 week |

---

## 13. Implementation Roadmap

### Phase 1: Foundation (P0)
- [ ] Schema validation at application layer
- [ ] Schema soft delete/archive
- [ ] Type field protection (prevent permission bypass)

### Phase 2: Data Migration (P0)
- [ ] Eliminate spoke tables
- [ ] Add partial indexes
- [ ] Simplify Rust code

### Phase 3: Backup & Recovery (P1)
- [ ] Local backup commands
- [ ] JSON export/import
- [ ] RocksDB snapshots

### Phase 4: Sync Preparation (P1)
- [ ] Conflict detection (OCC)
- [ ] Conflicted copies creation
- [ ] Offline operation queue

### Phase 5: Cloud Integration (P2)
- [ ] SurrealDB Cloud connection
- [ ] RLS implementation
- [ ] LIVE SELECT sync

### Phase 6: Advanced Features (P3)
- [ ] Cloud workflow reactor
- [ ] Rate limiting
- [ ] Auto-optimization janitor

---

## References

- [Node Behavior System](business-logic/node-behavior-system.md)
- [Schema Management Guide](development/schema-management-implementation-guide.md)
- [MCP Integration](components/mcp-server-architecture.md)
- [NLP Embedding Service](components/nlp-embedding-service.md)

---

**Document History:**
- 2025-12: Created as target state for Universal Graph transition
- Supersedes: `archived/architecture-overview-2025-hub-spoke.md`
- Supersedes: `archived/surrealdb-schema-design-hub-spoke.md`
