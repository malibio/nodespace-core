# SurrealDB Schema Design - Hub-and-Spoke with Record Links

## Overview

NodeSpace uses a **hub-and-spoke architecture** in SurrealDB that combines:
1. **Hub `node` table** - Universal metadata for all nodes
2. **Spoke tables** (`task`, `schema`) - Type-specific queryable data with bidirectional Record Links
3. **Graph relation tables** - `has_child` edges for hierarchy, `mentions` for references

**Note**: Per Issue #670, `date` nodes no longer have a spoke table - all date data is stored in the hub's `content` field.

**Key Pattern**: Record Links (not RELATE) for hub-spoke composition, RELATE edges for node-to-node relationships.

This design leverages SurrealDB's Record Links for composition and graph capabilities for relationships.

## Architecture Decisions

## Quick Reference: Key Design Shift

**IMPORTANT**: This document was updated to reflect the current graph relations architecture.

**Old approach (documented but not implemented):**
- Used `parent_id` fields on nodes table
- Hierarchy stored as denormalized data

**Current approach (implemented):**
- Uses `has_child` graph edges for hierarchy
- Clean separation between nodes and relationships
- See [Database Debugging Guide](../development/database-debugging-guide.md) for querying

---

## Architecture Decisions

### Why Graph Relations Instead of `parent_id` Fields?

Graph relations provide:
- **Cleaner semantics** - Relationships are first-class entities
- **Better performance** - Optimized by SurrealDB for graph traversal
- **Extensibility** - Add new relation types without schema changes
- **Bidirectional** - Parent→children and child→parent equally efficient

## Schema Definition

### Hub Table (Universal Metadata)

The `node` table serves as the universal hub storing metadata for ALL node types. Type-specific data lives in spoke tables, linked via Record Links.

```sql
-- Hub table: Universal metadata for all nodes
DEFINE TABLE node SCHEMAFULL;

-- Required universal fields
DEFINE FIELD id ON TABLE node TYPE string ASSERT $value != NONE;
DEFINE FIELD content ON TABLE node TYPE string DEFAULT "";
DEFINE FIELD nodeType ON TABLE node TYPE string ASSERT $value != NONE;
DEFINE FIELD data ON TABLE node TYPE option<record>;  -- Record Link to spoke (task, date, schema, or NULL)
DEFINE FIELD version ON TABLE node TYPE int DEFAULT 1 ASSERT $value >= 1;
DEFINE FIELD createdAt ON TABLE node TYPE datetime DEFAULT time::now();
DEFINE FIELD modifiedAt ON TABLE node TYPE datetime DEFAULT time::now();

-- Indexes for performance
DEFINE INDEX idx_node_type ON TABLE node COLUMNS nodeType;
DEFINE INDEX idx_node_modified ON TABLE node COLUMNS modifiedAt;

-- NO beforeSiblingId - structure lives in has_child edges!
-- NO parentId - hierarchy lives in has_child edges!
-- NO properties object - type-specific data lives in spoke tables!
```

**Why Record Link (`data` field)?**
- ✅ Faster than RELATE edges for 1-to-1 composition
- ✅ Direct field access: `node.data.status`
- ✅ NULL for simple nodes (text, header) - no spoke needed
- ✅ Cleaner than graph traversal for composition

### Spoke Tables (Type-Specific Data)

Type-specific queryable data stored in separate spoke tables with bidirectional links to hub.

```sql
-- Task spoke: Indexed fields for efficient queries
DEFINE TABLE task SCHEMAFULL;
DEFINE FIELD id ON TABLE task TYPE record ASSERT $value != NONE;
DEFINE FIELD node ON TABLE task TYPE record(node) ASSERT $value != NONE;  -- Reverse link to hub

-- Core task fields
-- Status uses lowercase canonical values: open, in_progress, done, cancelled (Issue #670)
DEFINE FIELD status ON TABLE task TYPE string DEFAULT 'open';
DEFINE FIELD priority ON TABLE task TYPE option<string>;
DEFINE FIELD due_date ON TABLE task TYPE option<datetime>;
DEFINE FIELD assignee ON TABLE task TYPE option<record>;

-- Indexes for efficient spoke queries
DEFINE INDEX idx_task_status ON TABLE task COLUMNS status;
DEFINE INDEX idx_task_priority ON TABLE task COLUMNS priority;
DEFINE INDEX idx_task_due_date ON TABLE task COLUMNS due_date;

-- User extensions allowed
DEFINE FIELD * ON TABLE task FLEXIBLE;

-- ============================================================================
-- NOTE: Date spoke table removed per Issue #670
-- Date nodes store all data in the hub's content field (no spoke table needed).
-- ============================================================================

-- Schema spoke: Type definitions
DEFINE TABLE schema SCHEMAFULL;
DEFINE FIELD id ON TABLE schema TYPE record ASSERT $value != NONE;
DEFINE FIELD node ON TABLE schema TYPE record(node) ASSERT $value != NONE;
DEFINE FIELD is_core ON TABLE schema TYPE bool DEFAULT false;
DEFINE FIELD version ON TABLE schema TYPE int DEFAULT 1;
DEFINE FIELD fields ON TABLE schema TYPE array DEFAULT [];
DEFINE FIELD * ON TABLE schema FLEXIBLE;
```

**Why Bidirectional Record Links?**
- ✅ **Hub → Spoke**: Fast access to type-specific data (`node.data.status`)
- ✅ **Spoke → Hub**: Efficient queries with context (`task.node.content`)
- ✅ **Indexed queries**: Query spokes directly with WHERE clauses
- ✅ **No subqueries**: Join hub context via reverse link

**Which Node Types Need Spokes?**
- ✅ `task`, `schema` - Have queryable structured data
- ❌ `text`, `header`, `code-block`, `date` - Just content (`data = null`)

**Note**: Per Issue #670, `date` nodes no longer have a spoke table - they store all data in the hub's `content` field.

### Graph Relations (Node-to-Node Relationships)

Relationships between nodes are managed via SurrealDB RELATE edges, not Record Links.

```sql
-- Has_child relation: Hierarchical relationships with fractional ordering
DEFINE TABLE has_child SCHEMAFULL TYPE RELATION IN node OUT node;

DEFINE FIELD order ON TABLE has_child TYPE float ASSERT $value != NONE;
DEFINE FIELD createdAt ON TABLE has_child TYPE datetime DEFAULT time::now();
DEFINE FIELD version ON TABLE has_child TYPE int DEFAULT 1;  -- OCC for concurrent edits

-- Indexes for efficient ordering queries
DEFINE INDEX idx_child_order ON TABLE has_child COLUMNS in, order;
DEFINE INDEX idx_unique_child ON TABLE has_child COLUMNS in, out UNIQUE;

-- ============================================================================

-- Mentions relation: Bidirectional references between nodes
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION IN node OUT node;

DEFINE FIELD createdAt ON TABLE mentions TYPE datetime DEFAULT time::now();
DEFINE FIELD context ON TABLE mentions TYPE string DEFAULT "";
DEFINE FIELD offset ON TABLE mentions TYPE int DEFAULT 0;

-- Indexes for efficient bidirectional queries
DEFINE INDEX idx_mentions_in ON TABLE mentions COLUMNS in;
DEFINE INDEX idx_mentions_out ON TABLE mentions COLUMNS out;
DEFINE INDEX idx_unique_mention ON TABLE mentions COLUMNS in, out UNIQUE;
```

**Why Graph Relations for Hierarchy/References?**

- ✅ **Cleaner semantics** - Relationships are explicit and first-class
- ✅ **Fractional ordering** - O(1) inserts between siblings
- ✅ **Bidirectional traversal** - Easy parent→children and child→parent queries
- ✅ **Extensible** - Add new relation types without schema changes
- ✅ **No denormalization** - Single source of truth for relationships

**Record Links vs RELATE - Decision Matrix:**
- **Use Record Link**: Composition (node "contains" task data, 1-to-1)
- **Use RELATE Edge**: Association (node references node, many-to-many)

### Type-Specific Tables

Each node type gets its own table with type-specific fields and constraints.

#### Core Types (Built-in)

**Design Decision**: Use **SCHEMAFULL + FLEXIBLE** for all built-in types
- Core fields strictly typed and validated (catches bugs early)
- FLEXIBLE allows user extensions without schema migrations
- Schema nodes track which fields are core vs. user-defined

```sql
-- Task nodes: Core behavior fields + user extensibility
DEFINE TABLE task SCHEMAFULL;
DEFINE FIELD id ON task TYPE record;

-- Core status field (app behavior depends on these values)
-- Validation enforced at application layer via schema nodes
DEFINE FIELD status ON task TYPE string;

-- Core optional fields
DEFINE FIELD priority ON task TYPE option<string>;
DEFINE FIELD due_date ON task TYPE option<datetime>;
DEFINE FIELD assignee ON task TYPE option<record>;

-- Allow arbitrary user-defined fields (custom_status, labels, etc.)
DEFINE FIELD * ON task FLEXIBLE;

-- Text nodes: Minimal core + maximum flexibility
DEFINE TABLE text SCHEMAFULL;
DEFINE FIELD id ON text TYPE record;
DEFINE FIELD * ON text FLEXIBLE;  -- Users can add any metadata

-- NOTE: Date nodes do NOT have a spoke table (Issue #670)
-- Date nodes store all data in the hub's content field.
-- ID format is YYYY-MM-DD (deterministic), content can be custom.

-- Header nodes: Core structure + user styling
DEFINE TABLE header SCHEMAFULL;
DEFINE FIELD id ON header TYPE record;
DEFINE FIELD level ON header TYPE int ASSERT $value >= 1 AND $value <= 6;
DEFINE FIELD * ON header FLEXIBLE;  -- Custom colors, icons, collapse state

-- Code block nodes: Core syntax + user configuration
DEFINE TABLE code_block SCHEMAFULL;
DEFINE FIELD id ON code_block TYPE record;
DEFINE FIELD language ON code_block TYPE string;
DEFINE FIELD line_numbers ON code_block TYPE bool DEFAULT false;
DEFINE FIELD * ON code_block FLEXIBLE;  -- Themes, run configs, annotations
```

**Why FLEXIBLE?**
- ✅ **Early error detection** - Core fields validated at write time
- ✅ **User customization** - Add custom fields without ALTER TABLE
- ✅ **Plugin support** - Plugins can extend nodes with prefixed fields
- ✅ **Workflow flexibility** - Users can add team-specific fields
- ✅ **No migrations** - Schema evolves naturally with usage

#### Schema Node Validation Pattern

**CRITICAL**: Validation is enforced at **application layer** via schema nodes, not database constraints.

**Schema Node (stored in universal `nodes` table):**
```sql
CREATE schema:task CONTENT {
  id: 'schema:task',
  node_type: 'schema',
  content: 'Task',
  properties: {
    version: 1,
    is_core: true,
    fields: [
      {
        name: 'status',
        type: 'enum',
        protection: 'core',  -- Field cannot be deleted
        core_values: ['open', 'in_progress', 'done', 'cancelled'],  -- App behavior (Issue #670)
        user_values: [],  -- User can extend via UI
        extensible: true,
        required: true,
        default: 'open'
      },
      {
        name: 'priority',
        type: 'enum',
        protection: 'user',  -- User can modify/delete
        core_values: ['urgent', 'high', 'medium', 'low'],
        user_values: [],
        extensible: true,
        required: false
      }
    ]
  }
};
```

**Application Validation (Rust):**
```rust
// Validate task status against schema
pub fn validate_task(task: &Node, schema: &SchemaNode) -> Result<()> {
    let status = task.get_property("status")?;
    let field = schema.get_field("status")?;

    // All valid options: core + user
    let all_valid = field.core_values
        .iter()
        .chain(field.user_values.iter())
        .collect::<Vec<_>>();

    if !all_valid.contains(&status) {
        return Err(anyhow::anyhow!(
            "Invalid status '{}'. Valid: {:?}",
            status, all_valid
        ));
    }

    Ok(())
}

// App behavior only depends on core values
pub fn render_task_icon(status: &str) -> Icon {
    match status {
        "todo" => Icon::Circle,
        "in_progress" => Icon::Spinner,
        "done" => Icon::Check,
        _ => Icon::Custom  // User-defined statuses get generic icon
    }
}
```

**User Extends Enum Values:**
```javascript
// User adds "on-hold" status via UI
await nodeService.extendEnumField('schema:task', 'status', 'on-hold');

// Schema node updated:
// user_values: ['on-hold']
// No ALTER TABLE needed - FLEXIBLE field accepts it!
```

**Why Application-Layer Validation?**
- ✅ **No ALTER TABLE** - Instant schema updates
- ✅ **Consistent enforcement** - Same validation in backend and frontend
- ✅ **Core protection** - `core_values` never removed, app behavior stable
- ✅ **User flexibility** - `user_values` can be added/removed freely
- ✅ **Gradual evolution** - Promote useful user fields to core over time

**Related Documentation**: See [`/docs/architecture/development/schema-management-implementation-guide.md`](../development/schema-management-implementation-guide.md) for complete schema management patterns.

#### Custom Types (Created at Runtime)

Users can create custom entity types dynamically via UI:

```sql
-- User creates "Project" entity type (fully SCHEMALESS)
DEFINE TABLE project SCHEMALESS;

-- User defines fields through schema node
CREATE schema:project CONTENT {
  id: 'schema:project',
  node_type: 'schema',
  content: 'Project',
  properties: {
    version: 1,
    is_core: false,  -- User-defined type
    fields: [
      {
        name: 'budget',
        type: 'number',
        required: true
      },
      {
        name: 'deadline',
        type: 'datetime',
        required: false
      },
      {
        name: 'status',
        type: 'enum',
        core_values: [],  -- No core values (user-defined)
        user_values: ['planning', 'active', 'completed', 'cancelled'],
        extensible: true
      }
    ]
  }
};

-- Create project instances
CREATE project:abc-123 CONTENT {
  id: 'project:abc-123',
  budget: 50000.00,
  deadline: <datetime>'2025-12-31',
  status: 'active',
  custom_field: 'anything'  -- SCHEMALESS allows arbitrary fields
};
```

### Mention Relationships (Graph Edges)

```sql
-- Mention relationships (for [[links]] and @mentions)
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD createdAt ON mentions TYPE datetime DEFAULT time::now();
DEFINE FIELD context ON mentions TYPE string DEFAULT "";
DEFINE FIELD offset ON mentions TYPE int DEFAULT 0;

-- Indexes for efficient bidirectional queries
DEFINE INDEX idx_mentions_in ON mentions COLUMNS in;
DEFINE INDEX idx_mentions_out ON mentions COLUMNS out;
DEFINE INDEX idx_unique_mention ON mentions COLUMNS in, out UNIQUE;

-- Example usage:
RELATE node:⟨uuid1⟩->mentions->node:⟨uuid2⟩
    CONTENT {
        context: "Check this out @NodeB...",
        createdAt: time::now()
    };
```

#### Graph Traversal Patterns for Mentions

**CRITICAL**: Always use SurrealDB's native graph traversal syntax (`->` and `<-`) instead of manually querying the edge table.

**✅ CORRECT - Use Graph Traversal:**
```rust
// Forward lookup: "Who did I mention?"
let query = "SELECT ->mentions->node.id AS mentioned_ids FROM node:⟨source_uuid⟩;";

// Reverse lookup: "Who mentioned me?" (backlinks)
let query = "SELECT <-mentions<-node.id AS mentioned_by_ids FROM node:⟨target_uuid⟩;";

// Rich retrieval with edge metadata
let query = "
    SELECT
        <-mentions.createdAt AS mentioned_at,
        <-mentions.context AS snippet,
        <-mentions<-node.id AS source_ids,
        <-mentions<-node.content AS source_content
    FROM node:⟨target_uuid⟩;
";
```

**❌ WRONG - Manual Edge Table Queries:**
```rust
// Don't do this - inefficient and verbose
let query = "SELECT out FROM mentions WHERE in = $node_thing;";
let query = "SELECT in FROM mentions WHERE out = $node_thing;";
```

**Why Graph Traversal is Superior:**
- ✅ **Native optimization** - SurrealDB optimizes graph operations internally
- ✅ **Cleaner syntax** - Expresses intent clearly
- ✅ **Bidirectional for free** - `<-` operator gives backlinks without maintaining separate lists
- ✅ **Edge metadata access** - Can access both edge properties and destination node properties
- ✅ **Composable** - Easy to chain multiple traversals

**Architecture Rule**: Always RELATE between hub nodes (`node` table), never between spoke tables:
```rust
// ✅ CORRECT - Hub-to-hub edges
RELATE node:A->mentions->node:B

// ❌ WRONG - Spoke-to-spoke edges break when types change
RELATE task:A->mentions->person:B
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
    root_id: NONE,
    embedding_vector: NONE,
    embedding_stale: true,
    version: 1,
    created_at: time::now(),
    modified_at: time::now()
};
-- Note: Sibling ordering is on has_child edge `order` field (Issue #614)

-- Insert into type-specific table (same ID!)
CREATE task CONTENT {
    id: $id,
    status: 'open',
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
-- Get all children of a node (any type) - ordered by edge order field
SELECT out.* FROM has_child
WHERE in = $parent_id
ORDER BY order ASC;

-- Get all descendants (recursive)
SELECT * FROM nodes
WHERE parent_id INSIDE (
    SELECT id FROM nodes WHERE id = $root_id
    UNION
    SELECT id FROM nodes WHERE parent_id = $root_id
    -- ... recursive traversal
);
```
-- Note: Sibling ordering uses fractional `order` field on has_child edges (Issue #614)

**Why Fast:**
- No JOINs needed (all hierarchy in one table)
- Indexed on parent_id and root_id
- Polymorphic references work across types

### 3. Type-Specific Queries

```sql
-- Get all open tasks
SELECT
    nodes.*,
    task.*
FROM nodes
INNER JOIN task ON nodes.id = task.id
WHERE task.status = 'open'
ORDER BY task.due_date ASC;

-- Get all high-priority tasks in a root
SELECT
    nodes.*,
    task.*
FROM nodes
INNER JOIN task ON nodes.id = task.id
WHERE nodes.root_id = $root_id
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
SELECT ->mentions->node.* FROM $node_id;

-- Get all nodes that mention this node (incoming/backlinks)
SELECT <-mentions<-node.* FROM $node_id;

-- Get just the IDs of mentioned nodes
SELECT ->mentions->node.id AS mentioned_ids FROM $node_id;

-- Get just the IDs of nodes that mention this one
SELECT <-mentions<-node.id AS mentioned_by_ids FROM $node_id;

-- Rich retrieval with context (for "References" UI section)
SELECT
    <-mentions.createdAt AS when_mentioned,
    <-mentions.context AS snippet,
    <-mentions<-node.id AS source_id,
    <-mentions<-node.content AS source_content,
    <-mentions<-node.nodeType AS source_type
FROM $node_id;
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
    version INTEGER DEFAULT 1,
    properties TEXT, -- JSON blob
    embedding_vector BLOB,
    embedding_stale INTEGER DEFAULT 1,
    created_at TEXT,
    modified_at TEXT
);
-- Note: before_sibling_id removed - ordering is on has_child edges (Issue #614)
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
    turso: &SurrealStore,
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
                root_id: $root_id,
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

## Common Pitfalls and Solutions

### Issue: "invalid type: enum, expected any valid JSON value"

**Symptom**: Deserialization errors when querying spoke tables or hub nodes, even when no `TYPE enum` fields are defined.

**Root Cause**: The `id` field (Record ID) is a **Thing type** that cannot deserialize to generic JSON. When you SELECT records and try to deserialize to `serde_json::Value` or `HashMap<String, serde_json::Value>`, the `id` field causes this cryptic error.

**Solution**: Always OMIT Thing-typed fields when deserializing to generic JSON:

```rust
// ❌ WRONG - includes id field (Thing type)
let query = format!("SELECT * FROM schema:`{}`;", id);
let records: Vec<HashMap<String, Value>> = response.take(0)?;
// Error: Db(Serialization("invalid type: enum, expected any valid JSON value"))

// ✅ CORRECT - omit id and other Thing-typed fields
let query = format!("SELECT * OMIT id, node FROM schema:`{}`;", id);
let records: Vec<HashMap<String, Value>> = response.take(0)?;
// Success!
```

**Thing-typed fields to always OMIT:**
- `id` - The record identifier (automatically a Thing)
- `node` - Reverse link to hub (defined as `TYPE option<record>`)
- Any field defined as `TYPE record` or `TYPE option<record>`

### Issue: Empty Properties When Querying Spoke Tables

**Symptom**: Hub node exists, spoke records exist, but `node.properties` is empty after calling `get_node()`.

**Root Cause**: The spoke table query is failing to deserialize due to Thing-typed fields being included in the SELECT.

**Solution**: Update `get_node()` to use `OMIT id, node` pattern:

```rust
// Get properties from spoke table
let props_query = format!("SELECT * OMIT id, node FROM {}:`{}`;", node_type, id);
```

### Issue: Never Redefine the `id` Field

**Problem**: Defining `id` as a field in your schema conflicts with SurrealDB's automatic Record ID:

```sql
-- ❌ WRONG - conflicts with native Record ID
DEFINE FIELD id ON TABLE node TYPE string ASSERT $value != NONE;

-- ✅ CORRECT - let SurrealDB manage id automatically
-- (No id field definition needed!)
```

**Why**: The `id` is automatically the primary key (Thing type). Redefining it causes type conflicts and deserialization errors.

**Extracting the ID**: Use `record::id()` function to get the identifier portion as a string:

```sql
SELECT record::id(id) AS raw_id FROM node:invoice;
-- Returns: "invoice" (string)
```

### Query Syntax: table:id vs type::thing()

Both syntaxes work for accessing records by ID:

```rust
// Option 1: Direct table:id syntax (simpler for literals)
let query = format!("SELECT * FROM node:`{}`;", id);

// Option 2: type::thing() function (better for parameterized queries)
let query = "SELECT * FROM type::thing('node', $id);";
```

**When to use which:**
- **Use `table:id`**: When the table name and ID are known at query construction time
- **Use `type::thing()`**: When table/ID come from variables or need runtime construction
- **Backtick-quote IDs**: IDs with special characters (hyphens, spaces) need backticks: ``node:`code-block` ``

**Note**: Both are functionally equivalent - choose based on readability and context.

## References

- [SurrealDB Record IDs](https://surrealdb.com/docs/surrealql/datamodel/ids)
- [SurrealDB Graph Relations](https://surrealdb.com/docs/surrealql/statements/relate)
- [SurrealDB Schema Definition](https://surrealdb.com/docs/surrealql/statements/define/table)
- NodeSpace Phase 1: `/docs/architecture/data/node-store-abstraction.md`
- NodeSpace Migration Guide: `/docs/architecture/data/surrealdb-migration-guide.md`
- NodeSpace Deserialization Patterns: `/docs/architecture/data/surrealdb-deserialization-patterns.md`

---

**Status**: Architecture design approved for Phase 2 implementation
**Last Updated**: 2025-11-21 (Added troubleshooting section from Issue #562)
**Related Issues**: #461 (Epic), #462 (Phase 1), #463 (Phase 2 - TBD), #562 (Schema Creation)
