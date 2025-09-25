# NodeSpace Business Logic Layer - Architecture Overview

## Overview

The NodeSpace Business Logic Layer provides the core data management, node operations, and service orchestration for the NodeSpace knowledge management system. It is designed as a Rust-based package system that can be shared across desktop, mobile, and web applications while maintaining clean separation of concerns.

## Architecture Principles

### 1. Package-Based Architecture
- **Hybrid approach**: Tightly coupled components together, separate independent concerns
- **Workspace management**: Single repository with multiple Cargo packages
- **Clear boundaries**: Business logic vs AI/ML services

### 2. Local-First Data Architecture
- **Embedded Turso database**: SQLite-compatible with native vector search
- **Offline-complete**: Full functionality without network dependency
- **Sync-ready**: Architecture prepared for multi-device synchronization

### 3. AI-Native Integration
- **Vector embeddings**: Every node automatically gets semantic embeddings
- **Intent classification**: Natural language to operations translation
- **MCP support**: AI agents can access the system programmatically

## Package Structure

```
nodespace-core/
├── Cargo.toml                   # Workspace root
├── packages/
│   ├── core/                    # Business logic + database layer
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs          # Public API exports
│   │   │   ├── models/         # Data structures (Node, Task, etc.)
│   │   │   ├── behaviors/      # Node type system
│   │   │   ├── services/       # Business services
│   │   │   ├── db/             # Database layer with SQLx
│   │   │   └── mcp/            # MCP stdio server
│   │   ├── migrations/         # SQL schema migrations
│   │   └── tests/              # Integration tests
│   ├── nlp-engine/             # AI/ML services (separate package)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── embeddings/     # FastEmbed integration
│   │   │   ├── intent/         # Intent classification
│   │   │   └── generation/     # llama.cpp text generation
│   │   ├── models/             # Model files (gitignored)
│   │   └── tests/
│   └── desktop-app/            # Existing Tauri application
│       ├── package.json
│       └── src-tauri/
│           ├── Cargo.toml
│           └── src/
│               └── commands.rs # Thin Tauri command layer
```

## Core Components

### 1. Universal Node Model

All content in NodeSpace is represented as nodes using a universal schema:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Node {
    pub id: String,                      // UUID (frontend-generated)
    pub node_type: String,               // "text", "task", "person", etc.
    pub content: String,                 // Primary content/text
    pub parent_id: Option<String>,       // Hierarchy parent
    pub root_id: String,                 // Root node reference
    pub before_sibling_id: Option<String>, // Single-pointer sibling ordering
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub metadata: serde_json::Value,     // Type-specific JSON properties
    pub embedding_vector: Option<Vec<u8>>, // AI embeddings (BLOB)
}
```

**Design Benefits:**
- **Single table storage**: All nodes use same database structure
- **Type flexibility**: New node types via `node_type` field and metadata
- **Hierarchy support**: Parent/child relationships with sibling ordering
- **AI-ready**: Built-in embedding storage for semantic search

### 2. Node Behavior System

Extensible trait-based system for node-type-specific logic:

```rust
pub trait NodeBehavior: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn validate(&self, node: &Node) -> Result<(), ValidationError>;
    fn can_have_children(&self) -> bool;
    fn supports_markdown(&self) -> bool;
    fn process_content(&self, content: &str) -> Result<String, ProcessingError>;
}

// Built-in implementations
pub struct TextNodeBehavior;
pub struct TaskNodeBehavior;
pub struct PersonNodeBehavior;
// ... etc
```

**Registry System:**
```rust
pub struct NodeBehaviorRegistry {
    behaviors: HashMap<String, Box<dyn NodeBehavior + Send + Sync>>,
}

// Usage in services
let behavior = registry.get("task").unwrap();
behavior.validate(&node)?;
```

### 3. Database Layer (SQLx + Turso)

#### Hybrid Schema Approach

**Core Universal Table:**
```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    content TEXT NOT NULL,
    parent_id TEXT,
    root_id TEXT NOT NULL,
    before_sibling_id TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    metadata JSON,
    embedding_vector BLOB
);
```

**Complementary Tables for Performance:**
```sql
-- Fast queries on frequently-accessed fields
CREATE TABLE tasks (
    node_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    due_date DATE,
    priority INTEGER,
    FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE persons (
    node_id TEXT PRIMARY KEY,
    first_name TEXT,
    last_name TEXT,
    email TEXT UNIQUE,
    FOREIGN KEY (node_id) REFERENCES nodes(id)
);
```

**Relation Table for Mentions:**
```sql
-- Bidirectional fast queries for backlinks
CREATE TABLE node_mentions (
    node_id TEXT,
    mentions_node_id TEXT,
    PRIMARY KEY (node_id, mentions_node_id)
);
```

#### Three-Tier Query Approach

1. **Compile-time checked queries** (core tables):
```rust
let task = sqlx::query_as!(
    Task,
    "SELECT * FROM tasks WHERE status = ? AND due_date < ?",
    status, due_date
).fetch_one(&pool).await?;
```

2. **Dynamic queries** (user-defined entities):
```rust
let custom_entity = sqlx::query(&format!(
    "SELECT * FROM {} WHERE node_id = ?",
    user_table_name
)).bind(node_id).fetch_one(&pool).await?;
```

3. **Schema management** (runtime table creation):
```rust
pub async fn create_entity_table(&self, schema: &EntitySchema) -> Result<(), Error> {
    let sql = self.generate_create_table_sql(schema);
    sqlx::query(&sql).execute(&self.pool).await?;
    Ok(())
}
```

### 4. Service Layer

**NodeService - Core Operations:**
```rust
pub struct NodeService {
    pool: SqlitePool,
    behaviors: Arc<NodeBehaviorRegistry>,
    nlp: Arc<NLPService>,
}

impl NodeService {
    // Basic CRUD
    pub async fn create_node(&self, node: Node) -> Result<String, Error>;
    pub async fn get_node(&self, id: &str) -> Result<Option<Node>, Error>;
    pub async fn update_node(&self, id: &str, update: NodeUpdate) -> Result<(), Error>;
    pub async fn delete_node(&self, id: &str) -> Result<(), Error>;

    // Hierarchy operations
    pub async fn get_children(&self, parent_id: &str) -> Result<Vec<Node>, Error>;
    pub async fn move_node(&self, node_id: &str, new_parent: Option<&str>) -> Result<(), Error>;

    // Search and AI
    pub async fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<Node>, Error>;
}
```

### 5. NLP Engine Integration

**Separate Package for AI/ML:**
```rust
// packages/nlp-engine/src/lib.rs
pub struct NodeSpaceNLP {
    embeddings: EmbeddingService,
    intent_classifier: IntentClassifier,
    text_generator: TextGenerator,
}

impl NodeSpaceNLP {
    pub async fn generate_embedding(&self, content: &str) -> Result<Vec<f32>, Error>;
    pub async fn classify_intent(&self, text: &str) -> Result<Intent, Error>;
    pub async fn generate_text(&self, prompt: &str) -> Result<String, Error>;
}
```

**Integration Points:**
- **Automatic embeddings**: Generated on node create/update
- **Intent classification**: Natural language commands → API operations
- **Content generation**: AI-assisted content creation

## Multi-Platform Integration

### 1. Tauri Desktop App
```rust
// Thin command layer in packages/desktop-app/src-tauri/src/commands.rs
#[tauri::command]
pub async fn create_node(
    service: State<'_, NodeService>,
    node: Node
) -> Result<String, String> {
    service.create_node(node).await.map_err(|e| e.to_string())
}
```

### 2. MCP Support for AI Agents
```rust
// Dual-mode binary: Desktop app OR MCP server
fn main() {
    match args.get(1) {
        Some("--mcp-stdio") => run_mcp_stdio_server(),
        _ => run_desktop_app(),
    }
}

// MCP communicates via JSON over stdio
fn run_mcp_stdio_server() {
    let service = create_node_service();

    for line in stdin.lines() {
        let request: MCPRequest = serde_json::from_str(&line)?;
        let response = handle_mcp_request(&service, request).await;
        println!("{}", serde_json::to_string(&response)?);
    }
}
```

### 3. Reactive State Synchronization

**Problem**: Updates from MCP clients need to reflect in desktop UI

**Solution**: Shared state via Tauri's event system
```rust
// Any update (from UI or MCP) triggers event
service.update_node(id, update).await?;
app_handle.emit_all("node-changed", NodeChangeEvent { id, update })?;

// Frontend listens and updates Svelte stores
listen('node-changed', (event) => {
    nodeStore.update(nodes => {
        // Reactive update triggers UI re-render
        nodes[index] = event.payload;
        return nodes;
    });
});
```

## Implementation Phases

### Phase 1: Core Package Setup
1. Initialize Cargo workspace with packages/core
2. Define Node model and data structures
3. Implement NodeBehavior trait system
4. Set up database connection with SQLx
5. Create migration system

### Phase 2: Node Types & Services
1. Implement built-in node behaviors
2. Create NodeService with CRUD operations
3. Add hierarchy management
4. Implement mentions relation table
5. Add schema service for dynamic entities

### Phase 3: NLP Engine Package
1. Initialize packages/nlp-engine
2. Migrate embedding functionality
3. Add intent classification
4. Integrate llama.cpp for text generation

### Phase 4: Integration
1. Create Tauri command layer
2. Implement reactive state sync
3. Add MCP stdio server support
4. Integration tests and documentation

## Key Design Decisions

### Why Hybrid Package Structure?
- **Database and business logic are tightly coupled**: NodeService directly uses SQLx queries
- **NLP is genuinely different**: Heavy dependencies, could be optional
- **Start simple, split later**: Can extract packages as needed

### Why SQLx over ORM?
- **Compile-time SQL verification**: Catches errors at build time
- **Performance**: Direct SQL without abstraction overhead
- **Flexibility**: Dynamic queries for user-defined entities

### Why Embedded Turso?
- **Local-first**: Works offline with sync capability
- **SQLite compatibility**: Mature ecosystem, familiar SQL
- **Vector search**: Native support for embeddings
- **Desktop optimized**: No network overhead

### Why stdio for MCP?
- **Standard protocol**: MCP defines stdio as primary transport
- **No network setup**: Direct process communication
- **Security**: No ports to expose
- **Simplicity**: JSON over text streams

## Success Criteria

- [ ] Core package compiles and passes tests
- [ ] All built-in node types work correctly
- [ ] Database operations perform well with Turso
- [ ] Vector embeddings generate and search properly
- [ ] Tauri commands integrate seamlessly
- [ ] MCP server responds correctly to AI agents
- [ ] UI updates reactively from all sources
- [ ] Schema can be extended at runtime

## Future Considerations

### Performance Optimizations
- **Large dataset handling**: Pagination and streaming
- **Vector search scaling**: Consider sqlite-vss extension
- **Caching strategies**: Query result caching with invalidation

### Extensibility
- **Plugin system**: External node type packages
- **Workflow engine**: Rule-based automation
- **Multi-tenant**: User workspaces and permissions

This architecture provides a solid foundation for NodeSpace's AI-native knowledge management while maintaining simplicity and performance.