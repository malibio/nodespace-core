# SurrealDB-Only Architecture

**Status**: Current Architecture (as of November 2025)
**Migration**: Completed in PR #485 (Issue #470)

## Overview

NodeSpace uses **SurrealDB embedded** as its single, direct database backend with zero abstraction layers. This represents a major architectural simplification from the previous hybrid abstraction approach.

## Architecture Principles

### 1. Direct Access - Zero Abstraction Overhead

**Before (Hybrid Migration Phase)**:
```rust
Application → NodeStore Trait → SurrealStore Wrapper → SurrealStore → surrealdb
              (abstraction)      (wrapper)          (service layer)
```

**After (Current - Simplified)**:
```rust
Application → SurrealStore → SurrealDB Embedded
              (direct)       (database)
```

**Benefits**:
- ✅ Eliminated 2 layers of indirection
- ✅ Zero trait dispatch overhead
- ✅ Direct access to SurrealDB-specific features
- ✅ -8,860 lines of code (87% simplification)

### 2. Single Backend - No Database Switching

**Decision**: SurrealDB is the **only** database backend. No abstraction for switching databases.

**Rationale**:
- Pre-launch (zero users) = perfect time for breaking changes
- No migration path needed
- Simpler to maintain one backend
- Direct access to SurrealDB features (graph relations, versioning, RBAC)
- Performance: No trait dispatch, no wrapper overhead

### 3. Embedded-Only (Desktop Application)

**Current**: RocksDB storage engine for desktop-only embedded usage

```rust
SurrealStore::new(PathBuf::from("./data/nodespace.db")).await?
```

**Storage**: `kv-rocksdb` engine for local filesystem persistence

**Future** (Post-Launch): May add SurrealDB Cloud sync, but embedded remains primary

## Database Schema Design

### Universal `nodes` Table (SCHEMALESS)

```sql
DEFINE TABLE nodes SCHEMALESS;

DEFINE FIELD id ON nodes TYPE string;
DEFINE FIELD node_type ON nodes TYPE string;
DEFINE FIELD content ON nodes TYPE string;
DEFINE FIELD parent_id ON nodes TYPE option<string>;
DEFINE FIELD container_node_id ON nodes TYPE option<string>;
DEFINE FIELD properties ON nodes FLEXIBLE;  -- Pure JSON, no fixed schema
DEFINE FIELD version ON nodes TYPE int DEFAULT 1;
DEFINE FIELD created_at ON nodes TYPE datetime;
DEFINE FIELD updated_at ON nodes TYPE datetime;

-- Sibling chain fields
DEFINE FIELD prev_sibling_id ON nodes TYPE option<string>;
DEFINE FIELD next_sibling_id ON nodes TYPE option<string>;
```

**Key Design Decisions**:
- **SCHEMALESS**: Properties field accepts any JSON structure
- **Pure JSON**: No type-specific tables, all in `properties`
- **Sibling Chain**: Doubly-linked list for ordering
- **Version Field**: Optimistic concurrency control

### Graph Relations (Mentions Table)

```sql
DEFINE TABLE mentions TYPE RELATION;

-- mentions relation connects nodes (from -> to)
RELATE nodes:$mentioning_node_id->mentions->nodes:$mentioned_node_id;
```

**Benefits of Graph Relations**:
- Native traversal queries (no JOINs)
- Bidirectional lookup (outgoing/incoming)
- Type-safe relation records

## Core Operations

### Direct SurrealStore Methods

All operations go directly through `SurrealStore` with no abstraction:

```rust
pub struct SurrealStore {
    db: Surreal<Db>,
}

impl SurrealStore {
    // CRUD Operations
    pub async fn create_node(&self, node: Node) -> Result<Node>;
    pub async fn get_node(&self, id: &str) -> Result<Option<Node>>;
    pub async fn update_node(&self, id: &str, update: NodeUpdate) -> Result<Node>;
    pub async fn delete_node(&self, id: &str) -> Result<DeleteResult>;

    // Hierarchy Operations
    pub async fn get_children(&self, parent_id: Option<&str>) -> Result<Vec<Node>>;
    pub async fn move_node(&self, node_id: &str, new_parent_id: Option<&str>) -> Result<()>;

    // Graph Relations
    pub async fn create_mention(&self, from: &str, to: &str) -> Result<()>;
    pub async fn get_outgoing_mentions(&self, node_id: &str) -> Result<Vec<String>>;
    pub async fn get_incoming_mentions(&self, node_id: &str) -> Result<Vec<String>>;

    // Query Operations
    pub async fn query_nodes(&self, query: NodeQuery) -> Result<Vec<Node>>;
}
```

**No trait, no abstraction, no wrappers** - direct method calls.

## Service Layer Integration

### NodeService (Rust)

```rust
pub struct NodeService {
    store: Arc<SurrealStore>,  // Direct reference, not trait
    behaviors: Arc<NodeBehaviorRegistry>,
    migration_registry: Arc<MigrationRegistry>,
}

impl NodeService {
    pub async fn create_node(&self, node: Node) -> Result<Node> {
        // Apply behavior defaults
        let node = self.apply_behavior_defaults(node)?;

        // Validate against schema
        self.validate_node(&node)?;

        // Direct store call (no abstraction)
        self.store.create_node(node).await
    }
}
```

**Key Points**:
- `store` is concrete type `Arc<SurrealStore>`, not `Arc<dyn NodeStore>`
- No dynamic dispatch
- Direct method calls with zero overhead

## Performance Characteristics

### Benchmarks (from SurrealDB PoC #460)

- **Startup Time**: <100ms (PoC: 52ms)
- **100K Nodes Query**: <200ms (PoC: 104ms)
- **Deep Pagination**: <50ms (PoC: 8.3ms)
- **Complex Queries**: <300ms avg (PoC: 211ms)

### Test Suite Performance

- **455 Rust Tests**: ~1.67 seconds (9.46ms average per test)
- **OCC Performance Overhead**: <15ms (adjusted for larger suite)
- **Zero Performance Regressions**: From migration

## Migration History

### Timeline

1. **Issue #460** (Oct 2025): SurrealDB PoC - Performance validated
2. **Issue #461** (Oct-Nov 2025): Epic - Initial SurrealDB migration with abstraction
3. **Issue #470** (Nov 2025): Remove abstraction layer completely
4. **PR #485** (Nov 13, 2025): **Migration completed** ✅

### What Was Removed

**Deleted Files** (~8,860 lines):
- `database.rs` - SurrealStore (2,661 lines)
- `node_store.rs` - direct SurrealStore access (768 lines)
- `turso_store.rs` - SurrealStore wrapper (805 lines)
- `ab_testing.rs` - A/B testing framework (261 lines)
- `ab_tests.rs` - A/B integration tests (568 lines)
- `metrics.rs` - Metrics collection (454 lines)

**Net Result**: -7,393 lines after adding SurrealStore implementation

### Migration Results

✅ **455/455 tests passing** (100% pass rate)
✅ **Zero regressions**
✅ **Massive simplification** (-87% code)
✅ **Direct database access** (no abstraction overhead)
✅ **Eliminated 2 layers of indirection**

## Advanced Features

### Current Status

**Working (90% of functionality)**:
- ✅ Core CRUD operations
- ✅ Hierarchy management
- ✅ Mentions and graph relations
- ✅ Schema management
- ✅ Basic querying
- ✅ Property updates
- ✅ Optimistic concurrency (basic)

**Temporarily Disabled (10% - Issue #481)**:
- ⏳ Embedding generation and staleness tracking
- ⏳ Advanced version-checked updates
- ⏳ Complex multi-table joins
- ⏳ Semantic search

### Future Enhancements

**Planned** (Post-Issue #481):
- SurrealDB's built-in version history
- RBAC for collaborative sync
- Advanced graph traversal queries
- Real-time subscriptions
- Vector search for embeddings

## Development Workflow

### Database Initialization

```rust
// Desktop app initialization
let db_path = app_data_dir.join("nodespace.db");
let store = Arc::new(SurrealStore::new(db_path).await?);

// NodeService initialization
let node_service = NodeService::new(store.clone())?;

// Schema service initialization
let schema_service = SchemaService::new(store.clone())?;
```

**Simple, direct, no abstraction.**

### Testing

```rust
#[tokio::test]
async fn test_create_node() {
    // Create in-memory SurrealDB
    let store = Arc::new(SurrealStore::new(":memory:".into()).await.unwrap());
    let service = NodeService::new(store).unwrap();

    // Direct test, no mocking needed
    let node = service.create_node(test_node()).await.unwrap();
    assert_eq!(node.content, "Test");
}
```

**Benefits**: No mocking abstractions, test real code paths.

## Architectural Decisions

### Why No Abstraction?

**Decision**: Remove direct SurrealStore access abstraction entirely

**Rationale**:
1. **Zero Users**: Pre-launch, no migration path needed
2. **Never Switch**: Won't revert to Turso or switch to another DB
3. **SurrealDB Features**: Need direct access (graph relations, versioning, RBAC)
4. **Simplicity**: One backend = simpler code, easier to reason about
5. **Performance**: No trait dispatch overhead, direct method calls

### Why SurrealDB?

**Decision**: SurrealDB as the only backend (not Turso, not SQLite)

**Rationale**:
1. **Built-in Versioning**: Native version history support
2. **Graph Relations**: RELATE for mentions/backlinks (no JOINs)
3. **RBAC**: Built-in role-based access for future collaboration
4. **Performance**: Benchmarks validated (<200ms for 100K nodes)
5. **Modern**: Active development, strong Rust ecosystem

### Why Direct Access?

**Decision**: NodeService uses `Arc<SurrealStore>` directly (not `Arc<dyn NodeStore>`)

**Rationale**:
1. **Zero Overhead**: No dynamic dispatch, direct method calls
2. **Type Safety**: Compile-time method resolution
3. **SurrealDB Features**: Access to all SurrealDB-specific APIs
4. **Maintainability**: Fewer abstractions = easier to understand and modify

## Related Documentation

- [SurrealDB Schema Design](./surrealdb-schema-design.md) - Detailed schema documentation
- [System Overview](../core/system-overview.md) - Overall architecture
- [Technology Stack](../core/technology-stack.md) - Technology choices

## Archived Documentation

Historical migration docs available in [`docs/architecture/archived/`](../archived/):
- direct database access documentation
- Migration guides and roadmaps
- Turso performance analysis
- A/B testing infrastructure

## References

- **Issue #470**: Remove direct database access, use SurrealDB directly
- **Issue #461**: Epic - SurrealDB Migration (original)
- **Issue #460**: SurrealDB PoC (performance validation)
- **PR #485**: Complete SurrealDB Migration (merged Nov 13, 2025)
- **Issue #481**: Advanced features migration (in progress)
- **Issue #486**: Documentation cleanup (this work)
