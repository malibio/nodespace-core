# NodeSpace Database Schema Design

## Overview

NodeSpace uses a **hybrid database schema** approach with Turso (SQLite-compatible) that combines:
- **Universal core table**: All nodes stored in single table for flexibility
- **Complementary tables**: Indexed fields for performance-critical queries
- **Dynamic tables**: Runtime-created tables for user-defined entities

## Core Schema Architecture

### Universal Node Table

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,                    -- UUID from frontend (or date:YYYY-MM-DD)
    node_type TEXT NOT NULL,                -- "text", "task", "person", etc.
    content TEXT NOT NULL,                  -- Primary content/text
    parent_id TEXT,                         -- Hierarchy parent (nullable for roots)
    root_id TEXT NOT NULL,                  -- Root node reference
    before_sibling_id TEXT,                 -- Single-pointer sibling ordering
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    metadata JSON,                          -- Type-specific properties
    embedding_vector BLOB,                  -- Vector embeddings (serialized)

    FOREIGN KEY (parent_id) REFERENCES nodes(id),
    FOREIGN KEY (root_id) REFERENCES nodes(id)
);

-- Indexes for performance
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_nodes_parent ON nodes(parent_id);
CREATE INDEX idx_nodes_root ON nodes(root_id);
CREATE INDEX idx_nodes_modified ON nodes(modified_at);
CREATE INDEX idx_nodes_content_search ON nodes(content); -- For text search
```

**Design Rationale:**
- **Single table**: All nodes share same structure, enabling polymorphic queries
- **JSON metadata**: Flexible storage for type-specific properties
- **BLOB embeddings**: Vector search capability with manual similarity computation
- **Hierarchy support**: Parent-child relationships with root tracking

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

## Complementary Tables (Performance Layer)

### Tasks Table

```sql
CREATE TABLE tasks (
    node_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,                   -- Extensible: "pending", "in-progress", "completed", etc.
    due_date DATE,                          -- ISO date format
    priority INTEGER,                       -- 1=low, 2=medium, 3=high, 4=urgent

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_tasks_due_date ON tasks(due_date);
CREATE INDEX idx_tasks_priority ON tasks(priority);
CREATE INDEX idx_tasks_status_due ON tasks(status, due_date); -- Compound for overdue tasks
```

**Query Examples:**
```sql
-- Find overdue incomplete tasks
SELECT n.content, t.due_date, t.priority
FROM nodes n
JOIN tasks t ON n.id = t.node_id
WHERE t.status != 'completed' AND t.due_date < DATE('now');

-- High priority tasks due this week
SELECT n.content, t.due_date
FROM nodes n
JOIN tasks t ON n.id = t.node_id
WHERE t.priority >= 3
  AND t.due_date BETWEEN DATE('now') AND DATE('now', '+7 days');
```

### Persons Table

```sql
CREATE TABLE persons (
    node_id TEXT PRIMARY KEY,
    first_name TEXT,
    last_name TEXT,
    email TEXT UNIQUE,                      -- Unique constraint for lookups

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Indexes for lookups
CREATE INDEX idx_persons_email ON persons(email);
CREATE INDEX idx_persons_name ON persons(last_name, first_name);
CREATE UNIQUE INDEX idx_persons_email_unique ON persons(email) WHERE email IS NOT NULL;
```

### Projects Table

```sql
CREATE TABLE projects (
    node_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,                   -- "planning", "active", "completed", "archived"
    start_date DATE,
    end_date DATE,

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Indexes for project queries
CREATE INDEX idx_projects_status ON projects(status);
CREATE INDEX idx_projects_dates ON projects(start_date, end_date);
CREATE INDEX idx_projects_active ON projects(status, start_date) WHERE status = 'active';
```

### Dates Table

```sql
CREATE TABLE dates (
    node_id TEXT PRIMARY KEY,               -- Format: "date:2024-01-15"
    date DATE NOT NULL,                     -- YYYY-MM-DD
    recurring TEXT,                         -- "daily", "weekly", "monthly", "yearly"

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Indexes for date operations
CREATE INDEX idx_dates_date ON dates(date);
CREATE INDEX idx_dates_recurring ON dates(recurring);
CREATE INDEX idx_dates_date_range ON dates(date DESC); -- For date range queries
```

**Special Date Node Handling:**
- **Deterministic IDs**: `date:2024-01-15` format
- **Lazy creation**: Only created when referenced by other nodes
- **Automatic reference**: Any date mention creates/links to date node

## Dynamic Entity Tables (User-Defined)

### Schema Management

```sql
-- Store user-defined entity schemas
CREATE TABLE entity_schemas (
    entity_type TEXT PRIMARY KEY,           -- "invoice", "contract", etc.
    schema_definition JSON NOT NULL,        -- Complete schema as JSON
    version TEXT NOT NULL,                  -- Schema version for migrations
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

### Runtime Table Creation

When users define custom entities, tables are created dynamically:

**Example: Invoice Entity Schema**
```json
{
  "entity_type": "invoice",
  "fields": [
    {"name": "invoice_number", "type": "TEXT", "indexed": true, "required": true},
    {"name": "amount", "type": REAL", "indexed": true, "required": true},
    {"name": "customer_id", "type": "TEXT", "indexed": true},
    {"name": "line_items", "type": "JSON", "indexed": false},
    {"name": "notes", "type": "TEXT", "indexed": false}
  ]
}
```

**Generated Table:**
```sql
CREATE TABLE invoice (
    node_id TEXT PRIMARY KEY,
    invoice_number TEXT NOT NULL,           -- Indexed field
    amount REAL NOT NULL,                   -- Indexed field
    customer_id TEXT,                       -- Indexed field
    metadata JSON,                          -- Non-indexed fields stored here

    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Generated indexes
CREATE INDEX idx_invoice_number ON invoice(invoice_number);
CREATE INDEX idx_invoice_amount ON invoice(amount);
CREATE INDEX idx_invoice_customer ON invoice(customer_id);
```

## SQLx Integration Patterns

### 1. Compile-Time Checked Queries (Core Tables)

```rust
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, FromRow)]
struct TaskRow {
    node_id: String,
    status: String,
    due_date: Option<NaiveDate>,
    priority: Option<i32>,
}

// Compile-time verified SQL
impl TaskService {
    pub async fn get_overdue_tasks(&self) -> Result<Vec<TaskRow>, sqlx::Error> {
        sqlx::query_as!(
            TaskRow,
            r#"
            SELECT node_id, status, due_date, priority
            FROM tasks
            WHERE status != 'completed' AND due_date < DATE('now')
            ORDER BY due_date ASC, priority DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
    }
}
```

### 2. Dynamic Queries (User-Defined Tables)

```rust
impl DynamicEntityService {
    pub async fn query_entity_with_filters(
        &self,
        entity_type: &str,
        filters: &[(&str, &str, serde_json::Value)]
    ) -> Result<Vec<serde_json::Value>, Error> {
        // Build query dynamically
        let mut sql = format!("SELECT * FROM {}", entity_type);
        let mut conditions = Vec::new();
        let mut bindings = Vec::new();

        for (field, operator, value) in filters {
            conditions.push(format!("{} {} ?", field, operator));
            bindings.push(value.clone());
        }

        if !conditions.is_empty() {
            sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
        }

        // Execute dynamic query
        let mut query = sqlx::query(&sql);
        for binding in bindings {
            query = match binding {
                serde_json::Value::String(s) => query.bind(s),
                serde_json::Value::Number(n) => query.bind(n.as_f64().unwrap()),
                _ => query.bind(binding.to_string()),
            };
        }

        let rows = query.fetch_all(&self.pool).await?;

        // Convert rows to JSON
        let results = rows.into_iter()
            .map(|row| self.row_to_json(row, entity_type))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }
}
```

### 3. Migration Management

```rust
// Database migrations using sqlx-migrate
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

impl DatabaseService {
    pub async fn run_migrations(&self) -> Result<(), Error> {
        MIGRATOR.run(&self.pool).await.map_err(Into::into)
    }

    pub async fn create_custom_entity_table(
        &self,
        schema: &EntitySchema
    ) -> Result<(), Error> {
        let sql = self.generate_create_table_sql(schema);
        sqlx::query(&sql).execute(&self.pool).await?;

        // Create indexes for indexed fields
        for field in &schema.fields {
            if field.indexed {
                let index_sql = format!(
                    "CREATE INDEX idx_{}_{} ON {}({})",
                    schema.entity_type, field.name,
                    schema.entity_type, field.name
                );
                sqlx::query(&index_sql).execute(&self.pool).await?;
            }
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

### From External Repositories
When migrating from separate `nodespace-*` repositories:

1. **Preserve data structures**: Keep existing interfaces where possible
2. **Gradual migration**: Move one component at a time
3. **Version compatibility**: Maintain API compatibility during transition
4. **Test coverage**: Comprehensive testing before switching

### Schema Evolution
- **Version tracking**: Track schema versions in metadata
- **Forward compatibility**: Design for additive changes
- **Migration scripts**: Automated schema updates
- **Rollback capability**: Safe downgrade procedures

This schema design provides the foundation for NodeSpace's flexible, AI-native knowledge management system while maintaining performance and extensibility.