# NodeStore Abstraction Layer - Architecture Design

## Overview

This document describes the NodeStore trait abstraction layer for Epic #461 (SurrealDB Migration - Phase 1). The abstraction enables parallel implementations of different database backends (Turso/libsql, SurrealDB) without changing business logic in NodeService.

## Executive Summary

**Goal**: Create a trait-based abstraction layer that decouples NodeService (business logic) from database implementation details.

**Scope**: Phase 1 only - Abstraction layer + TursoStore wrapper (Issues #462-#463)

**Timeline**: 2 weeks (14 days)

**Risk Level**: LOW - Proven refactoring pattern with clear validation criteria

**Success Criteria**:
- Zero test regressions (100% pass rate required)
- Performance overhead <5%
- Complete API coverage

## Architecture

### Current Architecture (Before Phase 1)

```
Tauri Commands (packages/desktop-app/src-tauri/src/commands/)
    ↓ (direct calls)
NodeService (packages/core/src/services/node_service.rs)
    ↓ (direct SQL via libsql)
DatabaseService (packages/core/src/db/database.rs)
    ↓
Turso/libsql embedded database
```

**Problem**: SQL queries scattered across NodeService, tightly coupled to libsql.

### Target Architecture (After Phase 1)

```
Tauri Commands
    ↓ (no changes - same API)
NodeService (business logic)
    ↓ (trait-based abstraction)
NodeStore Trait ← ← ← NEW ABSTRACTION LAYER
    ↓
TursoStore (wraps DatabaseService)
    ↓
DatabaseService (unchanged)
    ↓
Turso/libsql embedded database
```

**Benefits**:
- ✅ Clean separation: Business logic vs data access
- ✅ Enables parallel backend implementations
- ✅ Preserves existing functionality (zero breaking changes)
- ✅ Future-proof for SurrealDB integration (Phase 2)

### Why Abstraction at NodeService ↔ DatabaseService Boundary?

**Alternative**: Abstract at command layer (7 modules)
**Chosen**: Abstract at NodeService layer (1 interface)

**Rationale**:
1. **Minimizes code changes** - One abstraction point vs seven
2. **Preserves command layer** - Zero regression risk at API boundary
3. **Logical separation** - NodeService already encapsulates database operations
4. **Clear responsibilities** - NodeService = business logic, NodeStore = data access

## NodeStore Trait Design

### Complete Trait Interface

```rust
// File: packages/core/src/db/node_store.rs

use async_trait::async_trait;
use anyhow::Result;
use crate::models::{Node, NodeFilter, NodeUpdate, OrderBy};
use serde_json::Value;

/// Abstraction layer for node persistence operations
///
/// This trait enables parallel implementations of different database backends
/// (Turso/libsql, SurrealDB) without changing business logic in NodeService.
///
/// All methods are async to support both embedded (Turso) and network (SurrealDB) backends.
#[async_trait]
pub trait NodeStore: Send + Sync {
    //
    // CORE CRUD OPERATIONS
    //

    /// Create a new node in the database
    ///
    /// # Arguments
    /// * `node` - Node to create (ownership transferred)
    ///
    /// # Returns
    /// Created node with any generated fields (timestamps, etc.)
    ///
    /// # Ownership
    /// Uses move semantics (takes ownership, returns ownership)
    /// to avoid unnecessary cloning.
    async fn create_node(&self, node: Node) -> Result<Node>;

    /// Get node by ID
    ///
    /// # Returns
    /// Some(node) if found, None if not exists
    async fn get_node(&self, id: &str) -> Result<Option<Node>>;

    /// Update node properties
    ///
    /// # Arguments
    /// * `id` - Node ID to update
    /// * `update` - Fields to update (sparse update, only provided fields changed, ownership transferred)
    ///
    /// # Ownership
    /// Takes ownership of NodeUpdate to avoid cloning. Returns owned Node.
    async fn update_node(&self, id: &str, update: NodeUpdate) -> Result<Node>;

    /// Delete node and its children (cascading delete)
    ///
    /// # Cascade Delete Semantics
    ///
    /// **Behavior**: Recursively deletes the target node and all descendants in the node hierarchy.
    ///
    /// **Implementation Requirements**:
    ///
    /// 1. **Atomicity**: All deletes MUST occur in a single transaction (all-or-nothing).
    ///    - If any delete fails, the entire operation rolls back
    ///    - No partial deletions allowed
    ///
    /// 2. **Traversal Order**: Depth-first, leaf-to-root deletion order
    ///    - Prevents foreign key violations
    ///    - Children deleted before parents
    ///
    /// 3. **Mention Cleanup**: Delete all mention relationships where:
    ///    - `source_id` equals any deleted node ID, OR
    ///    - `target_id` equals any deleted node ID
    ///    - This prevents orphaned references
    ///
    /// 4. **Recursion Limit**: Implementations must support hierarchies up to 1,000 levels deep
    ///    - Exceeding 1,000 levels returns validation error (not a crash)
    ///    - Realistic limit for NodeSpace use cases
    ///
    /// 5. **Performance Requirements**:
    ///    - Use bulk DELETE queries where backend supports them (avoid N+1 queries)
    ///    - Target: Delete subtree of 10,000 nodes in <5 seconds
    ///    - Monitor stack depth to prevent stack overflow
    ///
    /// 6. **Error Handling**:
    ///    - Database constraint violations → roll back with clear error message
    ///    - Concurrent modification detected → conflict error, caller should retry
    ///    - Node not found → succeed silently (idempotent delete)
    ///
    /// # Test Cases
    /// - Single node deletion (no children): <10ms
    /// - Node with 1,000 descendants: <1 second
    /// - Node with 10,000 descendants: <5 seconds
    /// - Concurrent deletion + creation: One operation wins, other fails cleanly
    /// - Deletion during mention creation: Atomic (mention points to valid node or delete fails)
    async fn delete_node(&self, id: &str) -> Result<()>;

    //
    // QUERYING & FILTERING
    //

    /// Query nodes with filters, ordering, and pagination
    ///
    /// # Arguments
    /// * `filter` - Filter criteria (type, parent_id, properties, etc.)
    /// * `order_by` - Optional ordering (field + direction)
    /// * `limit` - Max results to return
    /// * `offset` - Number of results to skip (for pagination)
    async fn query_nodes(
        &self,
        filter: &NodeFilter,
        order_by: Option<OrderBy>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Node>>;

    /// Get all children of a parent node (ordered by position)
    ///
    /// # Arguments
    /// * `parent_id` - Parent node ID, or None for root nodes
    ///
    /// # Returns
    /// Children ordered by position field (ascending)
    async fn get_children(&self, parent_id: Option<&str>) -> Result<Vec<Node>>;

    /// Get nodes by container ID (for viewer pages)
    ///
    /// Container nodes group related nodes (e.g., DateNode contains tasks for that date)
    async fn get_nodes_by_container(&self, container_id: &str) -> Result<Vec<Node>>;

    /// Search nodes by content (full-text search)
    ///
    /// # Arguments
    /// * `query` - Search query string
    /// * `limit` - Max results to return
    async fn search_nodes_by_content(&self, query: &str, limit: Option<i64>) -> Result<Vec<Node>>;

    //
    // HIERARCHY OPERATIONS
    //

    /// Move node to new parent (update parent_id, preserve position)
    ///
    /// # Note
    /// Position is recalculated to place at end of new parent's children
    async fn move_node(&self, id: &str, new_parent_id: Option<&str>) -> Result<()>;

    /// Reorder node within siblings (update position)
    ///
    /// # Arguments
    /// * `new_position` - Float position for fractional indexing
    async fn reorder_node(&self, id: &str, new_position: f64) -> Result<()>;

    //
    // MENTION GRAPH OPERATIONS
    //

    /// Create mention relationship (source node mentions target node)
    async fn create_mention(&self, source_id: &str, target_id: &str) -> Result<()>;

    /// Delete mention relationship
    async fn delete_mention(&self, source_id: &str, target_id: &str) -> Result<()>;

    /// Get outgoing mentions from node (nodes this node mentions)
    ///
    /// # Returns
    /// List of target node IDs
    async fn get_outgoing_mentions(&self, node_id: &str) -> Result<Vec<String>>;

    /// Get incoming mentions to node (nodes that mention this node)
    ///
    /// # Returns
    /// List of source node IDs
    async fn get_incoming_mentions(&self, node_id: &str) -> Result<Vec<String>>;

    /// Get containers mentioning this node (for backlinks)
    ///
    /// Used to show "Where is this node referenced?" in UI
    async fn get_mentioning_containers(&self, node_id: &str) -> Result<Vec<Node>>;

    //
    // SCHEMA OPERATIONS (Dynamic Properties)
    //

    /// Get schema definition for node type
    ///
    /// # Returns
    /// JSON schema definition, or None if no schema defined
    async fn get_schema(&self, node_type: &str) -> Result<Option<Value>>;

    /// Update schema definition
    ///
    /// # Arguments
    /// * `schema` - JSON schema definition
    async fn update_schema(&self, node_type: &str, schema: &Value) -> Result<()>;

    //
    // EMBEDDING OPERATIONS (Vector Search)
    //

    /// Get nodes without embeddings (for background processing)
    ///
    /// Used by embedding service to find nodes needing vector generation
    async fn get_nodes_without_embeddings(&self, limit: Option<i64>) -> Result<Vec<Node>>;

    /// Update node embedding vector
    ///
    /// # Arguments
    /// * `embedding` - Binary-encoded embedding vector
    async fn update_embedding(&self, node_id: &str, embedding: &[u8]) -> Result<()>;

    /// Search nodes by embedding similarity (vector search)
    ///
    /// # Returns
    /// List of (node, similarity_score) tuples ordered by similarity (descending)
    async fn search_by_embedding(&self, embedding: &[u8], limit: i64) -> Result<Vec<(Node, f64)>>;

    //
    // BATCH OPERATIONS
    //

    /// Batch create nodes (optimized insertion)
    ///
    /// # Note
    /// Implementation should use bulk insert for performance
    ///
    /// # Ownership
    /// Takes ownership of Vec<Node> to avoid cloning
    async fn batch_create_nodes(&self, nodes: Vec<Node>) -> Result<Vec<Node>>;

    //
    // DATABASE LIFECYCLE
    //

    /// Close database connection and cleanup resources
    async fn close(&self) -> Result<()>;
}
```

### Method Categories

| Category | Methods | Purpose |
|----------|---------|---------|
| **Core CRUD** | 4 | Basic create, read, update, delete |
| **Querying** | 4 | Filtering, searching, pagination |
| **Hierarchy** | 2 | Parent/child relationships, ordering |
| **Mentions** | 5 | Node reference graph (backlinks) |
| **Schema** | 2 | Dynamic property definitions |
| **Embeddings** | 3 | Vector search (AI integration) |
| **Batch/Transaction** | 2 | Performance optimization |
| **Lifecycle** | 1 | Resource management |

**Total**: 22 trait methods covering complete NodeSpace database API

### NodeFilter Specification

The `NodeFilter` struct provides comprehensive filtering for node queries:

```rust
#[derive(Debug, Clone, Default)]
pub struct NodeFilter {
    /// Filter by node type (e.g., "text", "task", "date")
    pub node_type: Option<String>,

    /// Filter by parent node ID (immediate children only)
    pub parent_id: Option<String>,

    /// Filter by container ID (all descendants of a container)
    pub container_id: Option<String>,

    /// Filter by creation timestamp (nodes created after this time)
    pub created_after: Option<DateTime<Utc>>,

    /// Filter by update timestamp (nodes updated after this time)
    pub updated_after: Option<DateTime<Utc>>,

    /// Filter by property values (JSON path queries)
    /// Example: {"status": "completed", "priority": "high"}
    pub property_filters: Option<HashMap<String, serde_json::Value>>,

    /// Filter by content text search (case-insensitive substring match)
    pub content_contains: Option<String>,

    /// Limit number of results (for pagination)
    pub limit: Option<u32>,

    /// Offset for pagination (skip first N results)
    pub offset: Option<u32>,
}
```

**Query Semantics**:
- All filter fields are combined with AND logic (all conditions must match)
- `None` values are ignored (no filtering on that field)
- Empty filter returns all nodes
- Implementations must support efficient indexed queries where possible

**Example Usage**:
```rust
// Find all completed tasks updated in the last 7 days
let filter = NodeFilter {
    node_type: Some("task".to_string()),
    updated_after: Some(Utc::now() - Duration::days(7)),
    property_filters: Some(HashMap::from([
        ("status".to_string(), json!("completed")),
    ])),
    ..Default::default()
};
let nodes = store.get_nodes(filter).await?;
```

### Embedding Specification

NodeSpace uses vector embeddings for semantic search and AI-powered features.

**Embedding Format**:
- **Model**: BAAI/bge-small-en-v1.5 (current default)
- **Dimensions**: 384 floating-point values
- **Storage Format**: Binary-encoded `Vec<u8>` (IEEE 754 float32, little-endian)
- **Size**: 384 dimensions × 4 bytes = 1,536 bytes per embedding

**Similarity Metric**:
- **Method**: Cosine similarity
- **Range**: -1.0 (opposite) to 1.0 (identical)
- **Typical range**: 0.3 (unrelated) to 0.95 (highly similar)
- **Threshold for "related"**: ≥ 0.5 (configurable)

**Binary Encoding/Decoding**:
```rust
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

// Encode Vec<f32> to Vec<u8>
pub fn encode_embedding(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &value in embedding {
        bytes.write_f32::<LittleEndian>(value).unwrap();
    }
    bytes
}

// Decode Vec<u8> to Vec<f32>
pub fn decode_embedding(bytes: &[u8]) -> Result<Vec<f32>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut embedding = Vec::with_capacity(bytes.len() / 4);
    while cursor.position() < bytes.len() as u64 {
        embedding.push(cursor.read_f32::<LittleEndian>()?);
    }
    Ok(embedding)
}
```

**Database Storage**:
- **Turso**: BLOB column (`embedding BLOB`)
- **SurrealDB**: Bytes field (`embedding: bytes`)
- **Indexed**: Yes (for efficient vector search)

**Query Semantics**:
- `search_by_embedding()` returns nodes ordered by cosine similarity (descending)
- Results include both the node and its similarity score
- `limit` parameter controls max results (default: 10, max: 100)

**Example Usage**:
```rust
// Generate embedding for query text
let query_embedding = embedding_service.generate("How do I use tasks?").await?;
let encoded = encode_embedding(&query_embedding);

// Search for similar nodes
let results = store.search_by_embedding(&encoded, 10).await?;
for (node, score) in results {
    println!("Node: {}, Similarity: {:.2}", node.content, score);
}
```

**Performance Requirements**:
- Embedding search on 100K nodes: <100ms
- Batch embedding updates: 1,000 nodes/second
- Background embedding generation: Non-blocking (async worker)

### Design Decisions

#### 1. Async-First Design

**Decision**: All methods are async
**Rationale**:
- Turso (embedded) benefits from async for I/O operations
- SurrealDB (future) requires async for network operations
- Consistent API regardless of backend

#### 2. Property Storage

**Decision**: No `get_property`/`set_property` methods
**Rationale**:
- Properties stored in JSON field on Node struct
- Accessed via `update_node` with sparse updates
- Keeps API focused on node-level operations

#### 3. Transaction Support

**Decision**: No generic transaction method in Phase 1
**Rationale**:
- Phase 1 focuses on abstraction layer only
- Transaction API requires careful design (closure cannot be async)
- Deferred to Phase 2 (SurrealDB implementation) where requirements are clearer
- Current operations are mostly atomic (single node/mention operations)
- Batch operations provide basic multi-operation support

**Future Considerations** (Phase 2):
- May use explicit `begin_transaction()`/`commit()`/`rollback()` API
- Or callback-based API with proper async support
- Will evaluate based on SurrealDB transaction capabilities

#### 4. Error Handling

**Decision**: Use `anyhow::Result` for all operations
**Rationale**:
- Consistent error handling across backends
- Flexible: Each backend can add context to errors
- Matches existing NodeService error handling

## TursoStore Implementation

### Architecture

```rust
// File: packages/core/src/db/turso_store.rs

use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;
use crate::db::{DatabaseService, NodeStore};
use crate::models::{Node, NodeFilter, NodeUpdate, OrderBy};

/// Turso/libsql implementation of NodeStore trait
///
/// This is a thin wrapper around existing DatabaseService that implements
/// the NodeStore abstraction. It delegates all operations to DatabaseService
/// methods, preserving 100% backward compatibility.
pub struct TursoStore {
    db_service: Arc<DatabaseService>,
}

impl TursoStore {
    /// Create new TursoStore wrapping existing DatabaseService
    pub fn new(db_service: Arc<DatabaseService>) -> Self {
        Self { db_service }
    }
}

#[async_trait]
impl NodeStore for TursoStore {
    async fn create_node(&self, node: &Node) -> Result<Node> {
        // Delegate to DatabaseService (to be extracted from NodeService)
        self.db_service.db_create_node(node).await
    }

    async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        self.db_service.db_get_node(id).await
    }

    // ... implement all trait methods by delegating to DatabaseService
}
```

### Implementation Strategy

**CRITICAL**: Current architecture has SQL scattered across NodeService

**3-Step Refactoring Process**:

#### Step 1: Extract SQL to DatabaseService

**Goal**: Move raw SQL queries from NodeService → new DatabaseService methods

**Example**:
```rust
// BEFORE (Current NodeService - SQL in business logic)
impl NodeService {
    async fn create_node(&self, node: Node) -> Result<Node> {
        let conn = self.db.connect_with_timeout().await?;

        // ❌ Raw SQL directly in NodeService
        conn.execute(
            "INSERT INTO nodes (id, node_type, content, ...) VALUES (?, ?, ?, ...)",
            params![node.id, node.node_type, node.content, ...]
        ).await?;

        Ok(node)
    }
}

// AFTER Step 1 (DatabaseService has extracted SQL)
impl DatabaseService {
    pub async fn db_create_node(&self, node: &Node) -> Result<Node> {
        let conn = self.connect_with_timeout().await?;

        // ✅ SQL extracted to DatabaseService
        conn.execute(
            "INSERT INTO nodes (id, node_type, content, ...) VALUES (?, ?, ?, ...)",
            params![node.id, node.node_type, node.content, ...]
        ).await?;

        Ok(node.clone())
    }
}
```

**Process**:
1. Create new `db_*` method in DatabaseService
2. Move SQL query from NodeService → DatabaseService
3. Test: Run `bun run test:all` (must pass)
4. Commit: One method per commit
5. Repeat for all 23 methods

#### Step 2: Implement TursoStore

**Goal**: Create thin wrapper that delegates to DatabaseService

```rust
#[async_trait]
impl NodeStore for TursoStore {
    async fn create_node(&self, node: &Node) -> Result<Node> {
        // ✅ Simple delegation (no business logic)
        self.db_service.db_create_node(node).await
    }

    // ... 22 more delegations
}
```

**Process**:
1. Implement all 23 trait methods
2. Each method = 1 line delegation
3. Test: Integration tests for each method
4. Commit: One commit for complete implementation

#### Step 3: Refactor NodeService

**Goal**: Replace DatabaseService dependency with NodeStore trait

```rust
// BEFORE (Direct DatabaseService dependency)
impl NodeService {
    async fn create_node(&self, node: Node) -> Result<Node> {
        // Validation logic
        validate_node(&node)?;

        // Direct SQL call
        let conn = self.db.connect_with_timeout().await?;
        conn.execute("INSERT INTO ...").await?;

        Ok(node)
    }
}

// AFTER (NodeStore abstraction)
impl NodeService {
    // ✅ Accepts trait object instead of concrete type
    fn new(store: Arc<dyn NodeStore>) -> Self {
        Self { store }
    }

    async fn create_node(&self, node: Node) -> Result<Node> {
        // Validation logic (unchanged)
        validate_node(&node)?;

        // ✅ Delegate to abstraction
        self.store.create_node(&node).await
    }
}
```

**Process**:
1. Change NodeService constructor to accept `Arc<dyn NodeStore>`
2. Replace all DatabaseService calls with trait method calls
3. Test: ALL existing tests must pass (zero regressions)
4. Commit: One commit for complete refactoring

## Risk Assessment

### High-Risk Areas

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| **Test regressions** | CRITICAL | MEDIUM | Run `bun run test:all` after each method extraction |
| **Performance degradation** | HIGH | LOW | Benchmark before/after, trait dispatch overhead minimal (~1-2ns) |
| **Transaction semantics** | HIGH | MEDIUM | Preserve exact transaction boundaries, test concurrent operations |
| **SQL extraction errors** | HIGH | MEDIUM | Extract one method at a time, test each individually |
| **Breaking Tauri commands** | CRITICAL | LOW | Commands unchanged, only internal NodeService refactored |

### Mitigation Strategies

#### 1. Incremental Refactoring (Required)

**Process**:
- ✅ Extract ONE method at a time
- ✅ Run full test suite after each extraction
- ✅ Commit after each passing test suite
- ❌ Do NOT extract multiple methods without validation

#### 2. Test-Driven Migration

**Baseline**:
```bash
# BEFORE starting refactoring
bun run test:all > baseline_tests.txt
# Document: 753 passing (728 passed + 26 skipped)
```

**After each method**:
```bash
bun run test:all > current_tests.txt
diff baseline_tests.txt current_tests.txt
# Verify: NO new failures, identical pass/fail counts
```

#### 3. Performance Validation

**Benchmarks** (to be created):
```rust
// packages/core/benches/node_store_overhead.rs

// Measure trait dispatch overhead
#[bench]
fn bench_direct_call(b: &mut Bencher) {
    // Direct DatabaseService call
}

#[bench]
fn bench_trait_call(b: &mut Bencher) {
    // NodeStore trait call
}

// Target: <5% overhead
```

#### 4. Escape Hatch

**Feature flag** (safety mechanism):
```rust
// Cargo.toml
[features]
default = ["use-node-store-abstraction"]
use-node-store-abstraction = []

// Runtime selection
pub fn create_node_service(db: Arc<DatabaseService>) -> NodeService {
    #[cfg(feature = "use-node-store-abstraction")]
    {
        let store: Arc<dyn NodeStore> = Arc::new(TursoStore::new(db));
        NodeService::new(store)
    }

    #[cfg(not(feature = "use-node-store-abstraction"))]
    {
        NodeService::new_legacy(db) // Keep old implementation temporarily
    }
}
```

**Usage**: If abstraction causes issues, disable with:
```bash
cargo build --no-default-features
```

## Testing Strategy

### Test Coverage Requirements

#### 1. Unit Tests (TursoStore)

**Location**: `packages/core/src/db/turso_store_test.rs`

**Coverage**:
- ✅ Each trait method has dedicated test
- ✅ Error handling (invalid IDs, constraint violations)
- ✅ Edge cases (NULL values, empty results)

**Example**:
```rust
#[tokio::test]
async fn test_create_node() {
    let db = setup_test_db().await;
    let store = TursoStore::new(Arc::new(db));

    let node = Node::new("test-1", "text", "Test content");
    let created = store.create_node(&node).await.unwrap();

    assert_eq!(created.id, node.id);
    assert_eq!(created.content, node.content);
}
```

#### 2. Integration Tests (NodeService + TursoStore)

**Location**: Existing test files in `packages/core/src/services/`

**Requirements**:
- ✅ All existing NodeService tests pass unchanged
- ✅ New tests for abstraction layer interactions
- ✅ Test concurrent operations (race conditions)

#### 3. Performance Tests

**Location**: `packages/core/benches/node_store_overhead.rs`

**Benchmarks**:
- ✅ Direct call vs trait call overhead
- ✅ CRUD operations throughput
- ✅ Batch operations performance

**Success criteria**: <5% overhead from abstraction

#### 4. Regression Tests

**Process**:
```bash
# BEFORE refactoring
bun run test:all > baseline_tests.txt
# Document: 753 passing

# AFTER refactoring
bun run test:all > current_tests.txt

# Validation
diff baseline_tests.txt current_tests.txt
# Must show: ZERO new failures
```

## Implementation Timeline

### Week 1: Trait Design & SQL Extraction

**Days 1-2: NodeStore Trait Definition**
- ✅ Define complete trait in `packages/core/src/db/node_store.rs`
- ✅ Document trait with examples and rationale
- ✅ Review trait with stakeholders (GitHub issue discussion)
- **Deliverable**: Trait definition (compiles, no implementation yet)

**Days 3-7: Extract SQL to DatabaseService**
- ✅ Create new DatabaseService methods (one per day):
  - Day 3: `db_create_node`, `db_get_node`, `db_delete_node`
  - Day 4: `db_update_node`, `db_query_nodes`
  - Day 5: `db_get_children`, `db_move_node`, `db_reorder_node`
  - Day 6: Mention operations (5 methods)
  - Day 7: Embedding operations (3 methods), schema operations (2 methods)
- ✅ Test suite run after EACH method extraction
- **Deliverable**: DatabaseService with complete data access API

### Week 2: TursoStore Implementation & NodeService Refactoring

**Days 8-10: TursoStore Wrapper**
- ✅ Implement all trait methods (delegate to DatabaseService)
- ✅ Add error handling and edge case tests
- ✅ Integration tests for each trait method
- **Deliverable**: Complete TursoStore implementation

**Days 11-13: NodeService Refactoring**
- ✅ Replace DatabaseService with `Arc<dyn NodeStore>` dependency
- ✅ Update all method calls to use trait
- ✅ Verify ALL existing tests pass
- **Deliverable**: NodeService using abstraction layer

**Day 14: Testing & Documentation**
- ✅ Performance benchmarks (validate <5% overhead)
- ✅ Update architecture documentation
- ✅ Create PR with comprehensive description
- **Deliverable**: Ready for code review

**Total Timeline**: 14 days (2 weeks) for Phase 1

## Success Criteria

### Must-Haves (Blockers)

- ✅ **ZERO new test failures** - Any regression blocks merge
- ✅ **Performance within 5%** - Overhead must be minimal
- ✅ **Complete API coverage** - All NodeService operations abstracted
- ✅ **Comprehensive documentation** - Future maintainers understand design

### Nice-to-Haves (Not Blockers)

- ⚠️ Perfect trait design - Can evolve with SurrealDB implementation
- ⚠️ Performance benchmarks in CI/CD - Can add post-merge
- ⚠️ 100% test coverage - Existing coverage acceptable

## Future Phases

### Phase 2: SurrealDB Implementation (Epic #467)

**Prerequisites**: Phase 1 stable in production for 1 month

**Scope**:
- Implement `SurrealStore` using PoC code from Epic #460
- Add feature flag for backend selection (`--features surrealdb`)
- Parallel testing with both Turso and SurrealDB

### Phase 3: A/B Testing Framework (Epic #468)

**Prerequisites**: Phase 2 complete, both backends working

**Scope**:
- Runtime backend switching
- Performance comparison dashboard
- Automated regression testing across backends

### Phase 4: Gradual Rollout (Epic #469)

**Prerequisites**: Phase 3 complete, A/B testing validates equivalence

**Scope**:
- Percentage-based rollout (10% → 25% → 50% → 100%)
- User feedback collection
- Rollback mechanism if issues detected

## References

- Epic #461: SurrealDB Migration - Hybrid Architecture
- Epic #460: SurrealDB Feasibility PoC (completed, all GO criteria met)
- Issue #462: Create NodeStore Trait Abstraction Layer
- Issue #463: Implement NodeStore Trait for Turso Backend
- Senior Architect Analysis: See issue #461 comments

## Approval Process

This document represents the proposed architecture for Phase 1. Implementation will NOT begin until:

1. ✅ Architecture review by senior stakeholders
2. ✅ Trait design approval (method signatures, semantics)
3. ✅ Timeline and risk assessment validated
4. ✅ Planning PR approved/merged

**Status**: Awaiting review (Planning PR created)
