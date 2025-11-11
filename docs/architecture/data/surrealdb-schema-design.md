# SurrealDB Schema Design - Hybrid Architecture

## Overview

NodeSpace will use a **hybrid dual-table architecture** in SurrealDB that combines:
1. **Universal `nodes` table** - Common metadata, embeddings, hierarchy
2. **Type-specific tables** - Type-safe schemas per entity (`task`, `text`, `project`, etc.)

This design leverages SurrealDB's native Record IDs (`table:⟨uuid⟩`) while maintaining fast cross-type operations.

## Architecture Decisions

### Why Hybrid (Not Single Table or Pure Type Tables)?

**Single `nodes` table only (Turso approach):**
- ✅ Easy cross-type queries
- ❌ No type in ID (requires lookup: `SELECT type FROM node:⟨uuid⟩`)
- ❌ No type safety per entity
- ❌ Properties JSON blob (not SurrealDB-native)

**Pure type tables (no universal table):**
- ✅ Type in ID (`task:⟨uuid⟩`)
- ✅ Type safety per entity
- ❌ Slow vector search (must scan all tables)
- ❌ Complex hierarchy queries (JOINs across tables)
- ❌ Difficult to add common fields (must ALTER all tables)

**Hybrid (Universal + Type Tables):**
- ✅ Type in ID (`task:⟨uuid⟩`)
- ✅ Fast vector search (single `nodes` table scan)
- ✅ Type safety per entity (SurrealDB enforces schemas)
- ✅ Fast hierarchy queries (no JOINs needed)
- ✅ Dynamic entity creation at runtime
- ⚠️ Dual storage (acceptable overhead: ~1.7KB per node)

## Schema Definition

### Universal Nodes Table

Stores common metadata, embeddings, and hierarchy information for ALL node types.

```sql
-- Universal metadata + embeddings table
DEFINE TABLE nodes SCHEMAFULL;

-- Primary identifier (can be task:⟨uuid⟩, text:⟨uuid⟩, project:⟨uuid⟩, etc.)
DEFINE FIELD id ON nodes TYPE record;

-- Node type (redundant with table prefix but useful for queries)
DEFINE FIELD node_type ON nodes TYPE string;

-- Primary content/text
DEFINE FIELD content ON nodes TYPE string;

-- Hierarchy fields
DEFINE FIELD parent_id ON nodes TYPE option<record(nodes)>;
DEFINE FIELD container_node_id ON nodes TYPE option<record(nodes)>;
DEFINE FIELD before_sibling_id ON nodes TYPE option<record(nodes)>;

-- Vector embeddings (384-dimensional for BAAI/bge-small-en-v1.5)
DEFINE FIELD embedding_vector ON nodes TYPE option<array<float>>;
DEFINE FIELD embedding_stale ON nodes TYPE bool DEFAULT true;

-- Optimistic concurrency control
DEFINE FIELD version ON nodes TYPE int DEFAULT 1;

-- Timestamps
DEFINE FIELD created_at ON nodes TYPE datetime;
DEFINE FIELD modified_at ON nodes TYPE datetime;

-- Indexes for common queries
DEFINE INDEX idx_nodes_parent ON nodes FIELDS parent_id;
DEFINE INDEX idx_nodes_container ON nodes FIELDS container_node_id;
DEFINE INDEX idx_nodes_type ON nodes FIELDS node_type;
DEFINE INDEX idx_nodes_stale ON nodes FIELDS embedding_stale;
```

### Type-Specific Tables

Each node type gets its own table with type-specific fields and constraints.

#### Core Types (Built-in)

```sql
-- Task nodes
DEFINE TABLE task SCHEMAFULL;
DEFINE FIELD id ON task TYPE record;
DEFINE FIELD status ON task TYPE string
    ASSERT $value IN ['todo', 'in_progress', 'done'];
DEFINE FIELD priority ON task TYPE option<string>;
DEFINE FIELD due_date ON task TYPE option<datetime>;
DEFINE FIELD assignee ON task TYPE option<record>;

-- Text nodes (minimal type-specific data)
DEFINE TABLE text SCHEMAFULL;
DEFINE FIELD id ON text TYPE record;
-- Text nodes have no additional fields beyond what's in nodes table
-- But we create the table anyway for ID consistency

-- Date nodes
DEFINE TABLE date SCHEMAFULL;
DEFINE FIELD id ON date TYPE record;  -- Will be date:2025-01-03
DEFINE FIELD timezone ON date TYPE string DEFAULT 'UTC';
DEFINE FIELD is_holiday ON date TYPE bool DEFAULT false;

-- Header nodes
DEFINE TABLE header SCHEMAFULL;
DEFINE FIELD id ON header TYPE record;
DEFINE FIELD level ON header TYPE int ASSERT $value >= 1 AND $value <= 6;

-- Code block nodes
DEFINE TABLE code_block SCHEMAFULL;
DEFINE FIELD id ON code_block TYPE record;
DEFINE FIELD language ON code_block TYPE string;
DEFINE FIELD line_numbers ON code_block TYPE bool DEFAULT false;
```

#### Custom Types (Created at Runtime)

Users can create custom entity types dynamically:

```sql
-- Example: User creates "Project" entity type
DEFINE TABLE project SCHEMAFULL;
DEFINE FIELD id ON project TYPE record;
DEFINE FIELD budget ON project TYPE number;
DEFINE FIELD deadline ON project TYPE datetime;
DEFINE FIELD status ON project TYPE string
    ASSERT $value IN ['planning', 'active', 'completed', 'cancelled'];

-- Example: User creates "Invoice" entity type
DEFINE TABLE invoice SCHEMAFULL;
DEFINE FIELD id ON invoice TYPE record;
DEFINE FIELD amount ON invoice TYPE number;
DEFINE FIELD paid ON invoice TYPE bool DEFAULT false;
DEFINE FIELD due_date ON invoice TYPE datetime;
```

### Mention Relationships (Graph Edges)

```sql
-- Mention relationships (for [[links]] and @mentions)
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION
    FROM nodes TO nodes;
DEFINE FIELD container_id ON mentions TYPE record(nodes);
DEFINE FIELD created_at ON mentions TYPE datetime;

-- Example usage:
RELATE (task:⟨uuid1⟩)->mentions->(text:⟨uuid2⟩)
    CONTENT {
        container_id: date:2025-01-03,
        created_at: time::now()
    };
```

## Record ID Format

### SurrealDB Native Format

```
<table>:<identifier>
```

Examples:
- `task:⟨550e8400-e29b-41d4-a716-446655440000⟩`
- `text:⟨abc-123-def-456⟩`
- `date:2025-01-03` (deterministic ID for date nodes)
- `project:⟨client-generated-uuid⟩`

### Key Benefits

1. **Type Embedded in ID** - Parse before `:` to get type instantly
   ```rust
   let id = "task:550e8400-e29b-41d4-a716-446655440000";
   let node_type = id.split(':').next().unwrap(); // "task" - no DB query!
   ```

2. **Fire-and-Forget** - Client generates full Record ID
   ```rust
   let id = format!("task:{}", Uuid::new_v4());
   // Send to SurrealDB without waiting for ID assignment
   ```

3. **Polymorphic References** - Can reference any table
   ```sql
   DEFINE FIELD parent_id ON nodes TYPE option<record(nodes)>;
   -- Can be: task:⟨uuid⟩, text:⟨uuid⟩, date:2025-01-03, etc.
   ```

## CRUD Operations

### Create Node (Dual Insert)

When creating a node, insert into BOTH tables with the SAME ID:

```sql
-- Generate ID on client
LET $id = type::thing('task', rand::uuid());

-- Insert into universal table
CREATE nodes CONTENT {
    id: $id,
    node_type: 'task',
    content: 'Finish migration',
    parent_id: NONE,
    container_node_id: NONE,
    before_sibling_id: NONE,
    embedding_vector: NONE,
    embedding_stale: true,
    version: 1,
    created_at: time::now(),
    modified_at: time::now()
};

-- Insert into type-specific table (same ID!)
CREATE task CONTENT {
    id: $id,
    status: 'todo',
    priority: 'high',
    due_date: <datetime>'2025-12-31'
};
```

### Read Node

**Option 1: Fast metadata only (no type-specific fields)**
```sql
-- Single query to nodes table
SELECT * FROM $id;
-- Returns: content, parent_id, embedding_vector, etc.
```

**Option 2: Full node with type-specific fields**
```rust
// Parse type from ID
let table = id.split(':').next().unwrap(); // "task"

// Query both tables with JOIN
db.query(format!("
    SELECT
        nodes.*,
        {table}.*
    FROM nodes
    INNER JOIN {table} ON nodes.id = {table}.id
    WHERE nodes.id = $id
", table = table))
    .bind(("id", id))
    .await?
```

### Update Node

```sql
-- Update common fields
UPDATE $id SET
    content = 'Updated content',
    modified_at = time::now(),
    version += 1
WHERE version = $expected_version;

-- Update type-specific fields (if type is task)
UPDATE $id SET
    status = 'done',
    priority = 'low'
WHERE type::table($id) = 'task';
```

### Delete Node (Cascade)

```sql
-- Delete from type table
DELETE FROM $id;

-- Delete from universal table
DELETE FROM nodes WHERE id = $id;

-- Delete mention relationships (automatic with graph edges)
DELETE mentions WHERE in = $id OR out = $id;
```

## Common Query Patterns

### 1. Vector Search (Fast Single-Table Scan)

```sql
-- Search across ALL node types using embeddings
SELECT id, content, node_type,
       vector::similarity::cosine(embedding_vector, $query_vector) AS similarity
FROM nodes
WHERE embedding_vector IS NOT NONE
ORDER BY similarity DESC
LIMIT 10;
```

**Why Fast:**
- Single table scan (no JOINs)
- 384-dimensional vectors indexed efficiently
- Cosine similarity computed on GPU (if available)

### 2. Hierarchy Queries (Type-Agnostic)

```sql
-- Get all children of a node (any type)
SELECT * FROM nodes
WHERE parent_id = $parent_id
ORDER BY before_sibling_id;

-- Get all descendants (recursive)
SELECT * FROM nodes
WHERE parent_id INSIDE (
    SELECT id FROM nodes WHERE id = $root_id
    UNION
    SELECT id FROM nodes WHERE parent_id = $root_id
    -- ... recursive traversal
);

-- Get sibling chain (linked list ordering)
SELECT * FROM nodes
WHERE parent_id = $parent_id
START WITH before_sibling_id = NONE
FOLLOW before_sibling_id;
```

**Why Fast:**
- No JOINs needed (all hierarchy in one table)
- Indexed on parent_id and container_node_id
- Polymorphic references work across types

### 3. Type-Specific Queries

```sql
-- Get all TODO tasks
SELECT
    nodes.*,
    task.*
FROM nodes
INNER JOIN task ON nodes.id = task.id
WHERE task.status = 'todo'
ORDER BY task.due_date ASC;

-- Get all high-priority tasks in a container
SELECT
    nodes.*,
    task.*
FROM nodes
INNER JOIN task ON nodes.id = task.id
WHERE nodes.container_node_id = $container_id
  AND task.priority = 'high';
```

### 4. Cross-Type Searches

```sql
-- Search by content across all types
SELECT * FROM nodes
WHERE content @@ $search_query
ORDER BY modified_at DESC
LIMIT 20;

-- Get all nodes modified in last 7 days
SELECT * FROM nodes
WHERE modified_at > time::now() - 7d
ORDER BY modified_at DESC;
```

### 5. Mention Graph Queries

```sql
-- Get all nodes this node mentions (outgoing)
SELECT ->mentions->nodes.* FROM $node_id;

-- Get all nodes that mention this node (incoming)
SELECT <-mentions<-nodes.* FROM $node_id;

-- Get containers where this node is mentioned (backlinks)
SELECT DISTINCT container_id FROM mentions
WHERE out = $node_id;
```

## Dynamic Schema Creation

Users can create custom entity types at runtime:

```rust
// User creates "Project" type in UI
async fn create_custom_type(
    db: &Surreal<Client>,
    type_name: &str,
    fields: Vec<FieldDefinition>
) -> Result<()> {
    // 1. Define the table
    db.query(format!("DEFINE TABLE {} SCHEMAFULL", type_name)).await?;
    db.query(format!("DEFINE FIELD id ON {} TYPE record", type_name)).await?;

    // 2. Define custom fields
    for field in fields {
        let constraint = match field.field_type {
            FieldType::String => "TYPE string",
            FieldType::Number => "TYPE number",
            FieldType::DateTime => "TYPE datetime",
            FieldType::Bool => "TYPE bool",
            FieldType::Enum(values) => {
                format!("TYPE string ASSERT $value IN {:?}", values)
            }
        };

        db.query(format!(
            "DEFINE FIELD {} ON {} {}",
            field.name, type_name, constraint
        )).await?;
    }

    // 3. Create schema node in nodes table for UI metadata
    db.query("
        CREATE nodes CONTENT {
            id: type::thing('schema', $type_name),
            node_type: 'schema',
            content: $type_name,
            properties: $fields_metadata
        }
    ")
    .bind(("type_name", type_name))
    .bind(("fields_metadata", fields))
    .await?;

    Ok(())
}
```

## Migration from Turso

### Data Transformation

**Turso Schema (Current):**
```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    content TEXT NOT NULL,
    parent_id TEXT,
    container_node_id TEXT,
    before_sibling_id TEXT,
    version INTEGER DEFAULT 1,
    properties TEXT, -- JSON blob
    embedding_vector BLOB,
    embedding_stale INTEGER DEFAULT 1,
    created_at TEXT,
    modified_at TEXT
);
```

**SurrealDB Schema (Target):**
```sql
-- Universal table
nodes { id, node_type, content, parent_id, ... }

-- Type tables
task { id, status, priority, due_date, ... }
text { id }
project { id, budget, deadline, ... }
```

### Migration Script

```rust
async fn migrate_turso_to_surrealdb(
    turso: &DatabaseService,
    surreal: &Surreal<Client>
) -> Result<()> {
    // 1. Fetch all nodes from Turso
    let nodes = turso.query("SELECT * FROM nodes").await?;

    for node in nodes {
        // 2. Parse properties JSON
        let properties: HashMap<String, Value> =
            serde_json::from_str(&node.properties)?;

        // 3. Generate SurrealDB Record ID
        let surreal_id = format!("{}:{}", node.node_type, node.id);

        // 4. Insert into universal nodes table
        surreal.query("
            CREATE nodes CONTENT {
                id: type::thing($table, $uuid),
                node_type: $node_type,
                content: $content,
                parent_id: $parent_id,
                container_node_id: $container_id,
                embedding_vector: $embedding_vector,
                embedding_stale: $embedding_stale,
                created_at: $created_at,
                modified_at: $modified_at
            }
        ")
        .bind(("table", &node.node_type))
        .bind(("uuid", &node.id))
        // ... bind other fields
        .await?;

        // 5. Insert into type-specific table
        if !properties.is_empty() {
            surreal.query(format!("
                CREATE {} CONTENT {{
                    id: type::thing($table, $uuid),
                    {}
                }}
            ", node.node_type, properties_to_fields(&properties)))
            .bind(("table", &node.node_type))
            .bind(("uuid", &node.id))
            // ... bind property fields
            .await?;
        }
    }

    Ok(())
}
```

## Performance Characteristics

### Storage Overhead

**Per Node:**
- `nodes` table: ~200 bytes metadata + 1,536 bytes embedding = ~1.7 KB
- Type table: ~50-200 bytes (type-specific fields)
- **Total**: ~1.9 KB per node

**For 1 Million Nodes:**
- `nodes` table: ~1.7 GB
- Type tables combined: ~200 MB
- **Total**: ~1.9 GB (acceptable for modern storage)

### Query Performance

| Operation | Table(s) | Complexity | Performance |
|-----------|----------|------------|-------------|
| Vector search | `nodes` only | O(n) with index | <100ms for 100K nodes |
| Get node by ID | `nodes` or `nodes+type` | O(1) | <1ms |
| Hierarchy query | `nodes` only | O(children) | <10ms for 1000 children |
| Type-specific filter | `nodes+type` JOIN | O(n) with index | <50ms for 10K nodes |
| Cross-type search | `nodes` only | O(n) with FTS | <100ms for 100K nodes |
| Mention graph | `mentions` edges | O(mentions) | <10ms for 100 mentions |

### Why This Is Fast

1. **Right table for the job** - Query only what you need
2. **No unnecessary JOINs** - Hierarchy/embeddings don't need type tables
3. **Indexed efficiently** - SurrealDB indexes both tables independently
4. **Vector operations** - GPU-accelerated when available
5. **Graph traversals** - Native graph database operations

## Future Optimizations

### Potential Improvements

1. **Lazy type-table fetching** - Only fetch type fields when explicitly requested
2. **Embedding index** - Dedicated vector index for faster similarity search
3. **Materialized views** - Pre-computed common JOINs (nodes+task, etc.)
4. **Caching layer** - Redis cache for hot nodes (LRU eviction)
5. **Sharding** - Partition nodes table by container_id for scale

### Scalability Targets

- **Nodes**: 10M+ nodes per database
- **Embeddings**: Sub-100ms search on 1M nodes
- **Writes**: 10K+ nodes/second (bulk insert)
- **Queries**: <10ms for 99th percentile (indexed queries)

## References

- [SurrealDB Record IDs](https://surrealdb.com/docs/surrealql/datamodel/ids)
- [SurrealDB Graph Relations](https://surrealdb.com/docs/surrealql/statements/relate)
- [SurrealDB Schema Definition](https://surrealdb.com/docs/surrealql/statements/define/table)
- NodeSpace Phase 1: `/docs/architecture/data/node-store-abstraction.md`
- NodeSpace Migration Guide: `/docs/architecture/data/surrealdb-migration-guide.md`

---

**Status**: Architecture design approved for Phase 2 implementation
**Last Updated**: 2025-01-11
**Related Issues**: #461 (Epic), #462 (Phase 1), #463 (Phase 2 - TBD)
