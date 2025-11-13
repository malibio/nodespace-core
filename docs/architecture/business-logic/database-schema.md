# NodeSpace Database Schema Design

## Overview

NodeSpace uses a **Pure JSON schema** approach with Turso (SQLite-compatible) that provides:
- **Universal nodes table**: ALL entities stored in single table (no complementary tables)
- **Schema-as-node pattern**: Schemas stored as nodes with `node_type = "schema"`
- **JSON path indexes**: Dynamic index creation based on query frequency (rule-based)
- **Zero migration risk**: No ALTER TABLE required on user machines (critical for desktop apps)

## Core Schema Architecture

### Universal Node Table (Pure JSON Schema)

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,                    -- UUID from frontend (or YYYY-MM-DD for date nodes)
    node_type TEXT NOT NULL,                -- "task", "invoice", "schema", "date", etc.
    content TEXT NOT NULL,                  -- Primary content/text
    parent_id TEXT,                         -- Hierarchy parent (NULL = root-level node)
    origin_node_id TEXT,                    -- Which viewer/page created this (for bulk fetch)
    before_sibling_id TEXT,                 -- Single-pointer sibling ordering (linked list)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    properties JSON NOT NULL DEFAULT '{}',  -- ALL entity-specific fields (no complementary tables)
    embedding_vector BLOB,                  -- F32_BLOB vector embeddings

    FOREIGN KEY (parent_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (origin_node_id) REFERENCES nodes(id)
);

-- Core indexes (no ALTER TABLE ever required)
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_nodes_parent ON nodes(parent_id);
CREATE INDEX idx_nodes_origin ON nodes(origin_node_id);
CREATE INDEX idx_nodes_modified ON nodes(modified_at);
CREATE INDEX idx_nodes_content ON nodes(content); -- For text search
```

**Design Rationale:**
- **Pure JSON approach**: ALL entity data in properties field (no complementary tables)
- **Zero migration risk**: No ALTER TABLE on user machines (critical for desktop: 10,000+ installations)
- **Schema-as-node**: Schemas stored as nodes (`node_type = "schema"`, `id = type_name`)
- **Hierarchy semantics**:
  - `parent_id`: Hierarchical parent relationship (enables indent/outdent, tree structure)
  - `origin_node_id`: Which viewer/page originally created this node (enables bulk fetch via single query)
  - `before_sibling_id`: Linked-list sibling ordering (maintains node order independent of creation time)
- **Bulk fetch optimization**: Single query fetches all nodes by `origin_node_id`, hierarchy built in-memory
- **Dynamic indexes**: JSON path indexes created based on query frequency (rule-based)

### Mentions Relation Table

```sql
CREATE TABLE node_mentions (
    node_id TEXT NOT NULL,                  -- Node that contains the mention
    mentions_node_id TEXT NOT NULL,         -- Node being mentioned
    PRIMARY KEY (node_id, mentions_node_id),

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (mentions_node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Indexes for bidirectional queries
CREATE INDEX idx_mentions_source ON node_mentions(node_id);      -- "What does X mention?"
CREATE INDEX idx_mentions_target ON node_mentions(mentions_node_id); -- "What mentions X?"
```

**Usage Examples:**
```sql
-- Find all nodes that mention a specific node
SELECT n.* FROM nodes n
JOIN node_mentions m ON n.id = m.node_id
WHERE m.mentions_node_id = 'target-node-id';

-- Find all backlinks to a node
SELECT n.* FROM nodes n
JOIN node_mentions m ON n.id = m.mentions_node_id
WHERE m.node_id = 'source-node-id';
```

## Schema-as-Node Pattern

### Schema Field Definition

Every schema field includes type information, protection levels, and optional constraints:

```typescript
interface SchemaField {
  name: string;                // Field name (e.g., "status", "due_date")
  type: string;                // Field type: 'text', 'number', 'boolean', 'date', 'enum', 'array', 'json', or schema reference
  protection: 'core' | 'user' | 'system';  // Protection level

  // Enum-specific properties
  core_values?: string[];      // Protected enum values (cannot be removed)
  user_values?: string[];      // User-extensible enum values

  indexed: boolean;            // Whether to create JSON path index
  required?: boolean;          // Field is required
  extensible?: boolean;        // Allow extending enum values
  default?: unknown;           // Default value
  description?: string;        // Field description
  item_type?: string;          // For array types
}
```

**Protection Levels:**

| Level | Can Delete? | Can Modify Type? | Can Remove Enum Values? | Use Case |
|-------|-------------|------------------|------------------------|----------|
| `core` | ❌ No | ❌ No | ❌ No (core_values only) | Fields UI components depend on |
| `user` | ✅ Yes | ✅ Yes | ✅ Yes | User-added custom fields |
| `system` | ❌ No | ❌ No | ❌ No | Auto-managed read-only fields |

**Field Types:**

NodeSpace supports the following field types for schema definitions:

| Type | Description | Example Value | Notes |
|------|-------------|---------------|-------|
| `text` | String value | `"Hello World"` | No size limit (SQLite TEXT supports up to ~2GB) |
| `number` | Numeric value (integer or float) | `42`, `3.14`, `15000.00` | No currency formatting built-in |
| `boolean` | True/false value | `true`, `false` | Stored as JSON boolean |
| `date` | Reference to date node | `"2025-01-15"` | Date node ID (YYYY-MM-DD format) |
| `enum` | Validated string from allowed values | `"OPEN"`, `"IN_PROGRESS"` | Requires `core_values` and/or `user_values` |
| `array` | List of values | `["tag1", "tag2"]` | Requires `item_type` field |
| `json` | Arbitrary JSON object | `{"key": "value"}` | No size limit - can store large objects |
| **Schema Reference** | Reference to another entity | `"person-uuid-123"` | Any schema name (e.g., `person`, `project`, `invoice`) |

**Primitive Types** (auto-detected, no schema lookup required):
```rust
const PRIMITIVE_TYPES: &[&str] = &["text", "number", "boolean", "date", "json"];
```

**Size Limits:**
- **`text` fields**: No enforced limit. SQLite TEXT type supports up to ~2GB per field.
- **`json` fields**: No enforced limit. Can store arbitrarily large JSON objects.
- **`content` column**: No enforced limit (also SQLite TEXT).
- **`properties` column**: No enforced limit (SQLite JSON/TEXT).
- **Practical considerations**:
  - Very large text (>1MB) may impact query performance
  - Consider chunking large documents into multiple nodes
  - Use `content` for primary text, `properties` for structured metadata

**Special Type: enum**
- Requires `core_values` (protected) and/or `user_values` (extensible)
- Validation ensures only allowed values are accepted
- `extensible: true` allows users to add new values

**Special Type: array**
- Requires `item_type` field specifying element type
- `item_type` can be any primitive, enum, or schema reference
- Examples:
  - `{"type": "array", "item_type": "text"}` → `["tag1", "tag2"]`
  - `{"type": "array", "item_type": "person"}` → `["person-uuid-1", "person-uuid-2"]`

**Schema References** (auto-detected):
- Any type name that matches an existing schema becomes a reference
- Auto-reference detection: if type is not primitive and schema exists, it's a reference
- Examples: `person`, `project`, `invoice`, `task`

**Complete Field Type Examples:**

```json
{
  "fields": [
    // Primitive types
    {
      "name": "title",
      "type": "text",
      "protection": "core",
      "indexed": true
    },
    {
      "name": "price",
      "type": "number",
      "protection": "user",
      "indexed": true
    },
    {
      "name": "is_active",
      "type": "boolean",
      "protection": "user",
      "indexed": false
    },
    {
      "name": "due_date",
      "type": "date",
      "protection": "core",
      "indexed": true,
      "description": "Date node ID (YYYY-MM-DD)"
    },
    {
      "name": "metadata",
      "type": "json",
      "protection": "user",
      "indexed": false,
      "description": "Arbitrary unstructured data"
    },

    // Enum type
    {
      "name": "status",
      "type": "enum",
      "protection": "core",
      "core_values": ["OPEN", "IN_PROGRESS", "DONE"],
      "user_values": [],
      "indexed": true,
      "required": true,
      "extensible": true,
      "default": "OPEN"
    },

    // Array types
    {
      "name": "tags",
      "type": "array",
      "item_type": "text",
      "protection": "user",
      "indexed": false
    },
    {
      "name": "assignees",
      "type": "array",
      "item_type": "person",
      "protection": "user",
      "indexed": true,
      "description": "Multiple person node IDs"
    },

    // Schema references
    {
      "name": "owner",
      "type": "person",
      "protection": "user",
      "indexed": true,
      "description": "Person node ID"
    },
    {
      "name": "project",
      "type": "project",
      "protection": "user",
      "indexed": true,
      "description": "Project node ID"
    }
  ]
}
```

**Enum Field Extension:**

Users can extend enum values through the SchemaService API:
- Add new values to `user_values` array
- Cannot remove `core_values` (UI depends on these)
- Cannot delete core-protected enum fields

```typescript
// User extends status enum
await schemaService.extendEnumField('task', 'status', 'BLOCKED');
// Result: user_values = ["BLOCKED"]

// User adds another value
await schemaService.extendEnumField('task', 'status', 'WAITING');
// Result: user_values = ["BLOCKED", "WAITING"]

// Valid values now: ["OPEN", "IN_PROGRESS", "DONE", "BLOCKED", "WAITING"]
```

### Core Built-In Schemas

NodeSpace includes pre-seeded core schemas stored as nodes. **Convention**: Schema nodes have `id = type_name` and `node_type = "schema"`.

#### Task Schema

**Schema Definition with Protection Levels:**

```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    parent_id,
    origin_node_id,
    properties
) VALUES (
    'task',                 -- Schema id = type name
    'schema',              -- Identifies this as a schema definition
    'Task',
    NULL,
    NULL,
    '{
        "is_core": true,
        "version": 1,
        "description": "Task tracking with status and due dates",
        "fields": [
            {
                "name": "status",
                "type": "enum",
                "protection": "core",
                "core_values": ["OPEN", "IN_PROGRESS", "DONE"],
                "user_values": [],
                "indexed": true,
                "required": true,
                "extensible": true,
                "default": "OPEN",
                "description": "Task status - core values protected, users can extend"
            },
            {
                "name": "due_date",
                "type": "date",
                "protection": "core",
                "indexed": true,
                "description": "Date node ID (YYYY-MM-DD format)"
            },
            {
                "name": "completed_at",
                "type": "date",
                "protection": "core",
                "indexed": true,
                "description": "Date node ID when task was completed"
            },
            {
                "name": "started_at",
                "type": "date",
                "protection": "core",
                "indexed": true,
                "description": "Date node ID when task was started"
            }
        ]
    }'
);
```

**Protection Levels:**
- `core`: Cannot be deleted, type cannot change. Core enum values cannot be removed. (TaskNode UI depends on these)
- `user`: User-added fields, fully modifiable/deletable
- `system`: Auto-managed fields, read-only (future use)

**Enum Field Structure:**
- `core_values`: Protected enum values (["OPEN", "IN_PROGRESS", "DONE"]) - cannot be removed
- `user_values`: User-extensible values (e.g., ["BLOCKED", "WAITING"]) - can be added/removed
- `extensible`: true allows users to add new values via UI

**Task Instance Example:**
```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    parent_id,
    origin_node_id,
    properties
) VALUES (
    'uuid-task-123',
    'task',                 -- References schema by type
    'Implement Pure JSON schema',
    '2025-01-03',          -- Parent is the daily note (can be indented under other nodes)
    '2025-01-03',          -- Created on daily note page (immutable, for bulk fetch)
    '{
        "status": "IN_PROGRESS",
        "due_date": "2025-01-10",
        "started_at": "2025-01-03"
    }'
);
```

**User-Extended Task Example:**
```sql
-- User extended status enum with "BLOCKED" value
-- User added custom field "priority" (protection: user)
INSERT INTO nodes (
    id,
    node_type,
    content,
    parent_id,
    origin_node_id,
    properties
) VALUES (
    'uuid-task-456',
    'task',
    'Fix critical bug',
    '2025-01-03',
    '2025-01-03',
    '{
        "status": "BLOCKED",
        "due_date": "2025-01-05",
        "priority": "URGENT",
        "assignee": "person-uuid-789"
    }'
);
```

**Query with JSON Path Index:**
```sql
-- Find tasks with specific status (enum validation ensures valid values)
SELECT * FROM nodes
WHERE node_type = 'task'
  AND properties->>'$.status' = 'IN_PROGRESS'
  AND properties->>'$.due_date' IS NOT NULL;

#### Person Schema

```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    properties
) VALUES (
    'person',              -- Schema id = type name
    'schema',
    'Person',
    '{
        "is_core": true,
        "description": "Person/contact information",
        "fields": [
            {"name": "first_name", "type": "text", "indexed": true},
            {"name": "last_name", "type": "text", "indexed": true},
            {"name": "email", "type": "text", "indexed": true},
            {"name": "phone", "type": "text", "indexed": false},
            {"name": "notes", "type": "text", "indexed": false}
        ]
    }'
);
```

#### Date Schema

**Date nodes use deterministic IDs** (`YYYY-MM-DD`) and are created lazily when referenced.

```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    properties
) VALUES (
    'date',                -- Schema id = type name
    'schema',
    'Date',
    '{
        "is_core": true,
        "description": "Date node for daily notes and temporal references",
        "fields": [
            {"name": "timezone", "type": "text", "indexed": false},
            {"name": "is_holiday", "type": "boolean", "indexed": false}
        ]
    }'
);
```

**Date Instance Example:**
```sql
INSERT INTO nodes (
    id,                    -- "2025-01-03" (deterministic ID)
    node_type,            -- "date"
    content,              -- "2025-01-03"
    parent_id,            -- NULL (dates are root-level nodes)
    origin_node_id,       -- NULL (date pages are their own origin)
    properties,           -- Optional metadata
    embedding_vector      -- NULL (dates don't need embeddings)
) VALUES (
    '2025-01-03',
    'date',
    '2025-01-03',
    NULL,
    NULL,
    '{"timezone": "UTC", "is_holiday": false}',
    NULL
);
```

#### Project Schema

```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    properties
) VALUES (
    'project',
    'schema',
    'Project',
    '{
        "is_core": true,
        "description": "Project tracking with dates and status",
        "fields": [
            {"name": "status", "type": "text", "indexed": true},
            {"name": "start_date", "type": "date", "indexed": true},
            {"name": "end_date", "type": "date", "indexed": true},
            {"name": "owner", "type": "person", "indexed": true}
        ]
    }'
);
```

## User-Defined Schemas

### Dynamic Schema Creation (Pure JSON)

Users can create custom schemas without any table creation - schemas are just nodes:

**Example: Invoice Schema Creation**
```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    properties
) VALUES (
    'invoice',             -- Schema id = type name
    'schema',
    'Invoice',
    '{
        "is_core": false,
        "description": "Invoice tracking with amounts and customers",
        "fields": [
            {"name": "invoice_number", "type": "text", "indexed": true, "required": true},
            {"name": "amount", "type": "number", "indexed": true, "required": true},
            {"name": "customer", "type": "person", "indexed": true},
            {"name": "line_items", "type": "json", "indexed": false},
            {"name": "notes", "type": "text", "indexed": false}
        ]
    }'
);
```

**Invoice Instance (No New Table Required):**
```sql
INSERT INTO nodes (
    id,
    node_type,
    content,
    parent_id,
    origin_node_id,
    properties
) VALUES (
    'uuid-invoice-789',
    'invoice',             -- References schema
    'Invoice INV-2025-001',
    'project-uuid-123',    -- Parent is the project (can be indented under other nodes)
    'project-uuid-123',    -- Created on project page (immutable, for bulk fetch)
    '{
        "invoice_number": "INV-2025-001",
        "amount": 15000.00,
        "customer": "person-uuid-456",
        "line_items": [
            {"description": "Consulting", "amount": 10000},
            {"description": "Implementation", "amount": 5000}
        ],
        "notes": "Payment terms: Net 30"
    }'
);
```

**Auto-Reference Detection:**
- Field `"customer": "person"` → Automatically detected as reference (person schema exists)
- Field `"amount": "number"` → Primitive type
- Field `"line_items": "json"` → Primitive JSON type

## surrealdb Integration Patterns

### 1. Pure JSON Queries (All Entities)

```rust
use surrealdb::{Database, de};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Node {
    id: String,
    node_type: String,
    content: String,
    parent_id: Option<String>,
    origin_node_id: Option<String>,
    properties: serde_json::Value,
    embedding_vector: Option<Vec<u8>>,
}

impl NodeService {
    pub async fn get_tasks_by_priority(&self, priority: &str) -> Result<Vec<Node>> {
        let mut stmt = self.db.prepare(
            "SELECT * FROM nodes
             WHERE node_type = 'task'
               AND properties->>'$.priority' = ?1
               AND properties->>'$.status' != 'completed'
             ORDER BY properties->>'$.due_date' ASC"
        ).await?;

        let rows = stmt.query([priority]).await?;
        let tasks: Vec<Node> = de::from_rows(&rows)?;
        Ok(tasks)
    }
}
```

### 2. Schema Lookup and Validation

```rust
const PRIMITIVE_TYPES: &[&str] = &["text", "number", "boolean", "date", "json"];

impl SchemaService {
    // Get schema definition
    pub async fn get_schema(&self, schema_name: &str) -> Result<SchemaDefinition> {
        let mut stmt = self.db.prepare(
            "SELECT properties FROM nodes
             WHERE id = ?1 AND node_type = 'schema'"
        ).await?;

        let row = stmt.query_row([schema_name]).await?;
        let properties: SchemaDefinition = serde_json::from_str(&row.get::<String>(0)?)?;
        Ok(properties)
    }

    // Auto-detect field type
    pub async fn detect_field_type(&self, field_type: &str) -> Result<FieldType> {
        if PRIMITIVE_TYPES.contains(&field_type) {
            Ok(FieldType::Primitive(field_type.to_string()))
        } else {
            // Check if schema exists
            if self.schema_exists(field_type).await? {
                Ok(FieldType::Reference { schema_name: field_type.to_string() })
            } else {
                Err(anyhow!("Unknown type: {}", field_type))
            }
        }
    }

    async fn schema_exists(&self, schema_name: &str) -> Result<bool> {
        let mut stmt = self.db.prepare(
            "SELECT COUNT(*) FROM nodes WHERE id = ?1 AND node_type = 'schema'"
        ).await?;

        let count: i64 = stmt.query_row([schema_name]).await?.get(0)?;
        Ok(count > 0)
    }
}
```

### 3. Dynamic Index Management (Rule-Based)

```rust
pub struct IndexManager {
    db: Arc<Database>,
    index_registry: HashMap<String, IndexDefinition>,
}

impl IndexManager {
    // Rule: Create JSON path index when query frequency exceeds threshold
    pub async fn analyze_and_optimize(&self, query_stats: &QueryStatistics) -> Result<()> {
        for (field_path, stats) in query_stats.json_field_access.iter() {
            if stats.queries_per_day > 10 && !self.has_index(field_path) {
                self.create_json_path_index(field_path).await?;
            }
        }
        Ok(())
    }

    async fn create_json_path_index(&self, json_path: &str) -> Result<()> {
        // Example: "task.priority" -> CREATE INDEX idx_task_priority ON nodes((properties->>'$.priority')) WHERE node_type = 'task';
        let parts: Vec<&str> = json_path.split('.').collect();
        let node_type = parts[0];
        let field_name = parts[1];

        let index_name = format!("idx_{}_{}", node_type, field_name);
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON nodes((properties->>'$.{}')) WHERE node_type = '{}'",
            index_name, field_name, node_type
        );

        self.db.execute(&sql, ()).await?;
        Ok(())
    }

    // Rule: Remove unused indexes after 30 days
    pub async fn cleanup_unused_indexes(&self) -> Result<()> {
        for (field_path, stats) in self.index_registry.iter() {
            if stats.last_used_days_ago > 30 && stats.queries_per_month < 5 {
                self.drop_index(field_path).await?;
            }
        }
        Ok(())
    }
}
```

### 4. Database Initialization (No ALTER TABLE)

```rust
// Pure JSON schema means NO migrations to user machines
impl SurrealStore {
    // Initial database setup (one-time)
    pub async fn initialize_database(&self) -> Result<()> {
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                node_type TEXT NOT NULL,
                content TEXT NOT NULL,
                parent_id TEXT,
                origin_node_id TEXT,
                before_sibling_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                properties JSON NOT NULL DEFAULT '{}',
                embedding_vector BLOB,
                FOREIGN KEY (parent_id) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (origin_node_id) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (before_sibling_id) REFERENCES nodes(id) ON DELETE SET NULL
            )",
            ()
        ).await?;

        // Create core indexes (never changes)
        self.create_core_indexes().await?;

        // Seed core schemas
        self.seed_core_schemas().await?;

        Ok(())
    }

    // Seed core schemas (task, person, date, project, text)
    async fn seed_core_schemas(&self) -> Result<()> {
        // Insert core schema nodes
        for schema in CORE_SCHEMAS {
            self.db.execute(
                "INSERT OR IGNORE INTO nodes (id, node_type, content, properties)
                 VALUES (?1, 'schema', ?2, ?3)",
                (schema.id, schema.content, schema.properties.to_string())
            ).await?;
        }
        Ok(())
    }
}
```

## Vector Search Strategy

### Current Approach: In-Memory Similarity

```rust
impl NodeService {
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize
    ) -> Result<Vec<Node>, Error> {
        // Generate query embedding
        let query_embedding = self.nlp.generate_embedding(query).await?;

        // Fetch all nodes with embeddings
        let rows = sqlx::query!(
            "SELECT id, embedding_vector FROM nodes WHERE embedding_vector IS NOT NULL"
        ).fetch_all(&self.pool).await?;

        // Calculate similarities in memory
        let mut similarities = Vec::new();
        for row in rows {
            if let Some(embedding_blob) = row.embedding_vector {
                let embedding: Vec<f32> = bincode::deserialize(&embedding_blob)?;
                let similarity = cosine_similarity(&query_embedding, &embedding);
                similarities.push((row.id, similarity));
            }
        }

        // Sort and get top results
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let top_ids: Vec<String> = similarities
            .into_iter()
            .take(limit)
            .map(|(id, _)| id)
            .collect();

        // Fetch full nodes
        self.get_nodes_by_ids(&top_ids).await
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}
```

### Future: sqlite-vss Extension

For better performance with large datasets:

```sql
-- Install sqlite-vss extension
-- CREATE VIRTUAL TABLE node_embeddings USING vss0(embedding(384));

-- Vector search with native extension
SELECT nodes.*, node_embeddings.distance
FROM node_embeddings
JOIN nodes ON node_embeddings.rowid = nodes.rowid
WHERE vss_search(node_embeddings.embedding, vector_query)
LIMIT 10;
```

## Performance Considerations

### Indexing Strategy
- **Frequently queried fields**: Move to complementary tables with proper indexes
- **JSON fields**: Use JSON operators when available in SQLite
- **Full-text search**: Consider FTS5 for content search
- **Composite indexes**: For complex query patterns

### Query Optimization
- **Pagination**: LIMIT/OFFSET for large result sets
- **Prepared statements**: Reuse compiled queries
- **Connection pooling**: SQLx connection pools for concurrency
- **Transaction management**: Batch operations for consistency

### Storage Optimization
- **BLOB compression**: Compress embedding vectors if needed
- **JSON optimization**: Minimize JSON field size
- **Vacuum scheduling**: Periodic database cleanup
- **WAL mode**: Write-Ahead Logging for better concurrency

## Migration Strategy

> **📖 For detailed implementation procedures**, see the [Schema Management Implementation Guide](../development/schema-management-implementation-guide.md).

**Summary**: NodeSpace uses lazy migration with version tracking to handle schema evolution without ALTER TABLE operations. This preserves the Pure JSON architecture while enabling controlled schema evolution.

### From External Repositories
When migrating from separate `nodespace-*` repositories:

1. **Preserve data structures**: Keep existing interfaces where possible
2. **Gradual migration**: Move one component at a time
3. **Version compatibility**: Maintain API compatibility during transition
4. **Test coverage**: Comprehensive testing before switching

### Schema Evolution

**Complete Implementation**: [Issue #106 - Schema Management System](https://github.com/malibio/nodespace-core/issues/106)

NodeSpace uses a **lazy migration strategy** for safe schema evolution:

- **Version tracking**: Each node tracks `_schema_version` in properties field
- **On-access migration**: Nodes upgrade when read, not bulk operation (desktop-safe)
- **Migration registry**: Pure functions for schema transforms (v1→v2, v2→v3, etc.)
- **No ALTER TABLE**: Pure JSON schema eliminates SQL migrations on user machines
- **Protection levels**: core/user/system fields with enforcement at API level
- **Enum extensibility**: `core_values` (protected) + `user_values` (user-extensible)
- **Forward compatibility**: Schemas designed for additive changes via protection levels
- **Rollback capability**: Old schema versions supported indefinitely

See [Issue #106](https://github.com/malibio/nodespace-core/issues/106) for complete 7-phase implementation details including SchemaService, MigrationRegistry, MCP tools, and frontend wrappers.

This schema design provides the foundation for NodeSpace's flexible, AI-native knowledge management system while maintaining performance and extensibility.