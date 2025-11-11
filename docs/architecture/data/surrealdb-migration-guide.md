# SurrealDB Migration Implementation Guide - Phase 1

> **🚧 Implementation Status**: **Phase 1 - In Progress**
>
> This guide tracks the step-by-step implementation of Phase 1 (NodeStore abstraction). See [`surrealdb-migration-roadmap.md`](surrealdb-migration-roadmap.md) for the complete migration roadmap.
>
> **Phase 1 Progress**:
> - 🚧 Issue #462: NodeStore Trait - In Progress
> - 📋 Issue #463: TursoStore Implementation - Planned
> - 📋 Issue #464: SurrealDBStore Implementation - Planned
>
> **Completion Status**: Check boxes below track actual implementation progress

## Purpose

This guide provides step-by-step implementation instructions for Phase 1 of Epic #461: Creating the NodeStore abstraction layer and TursoStore wrapper.

**Target Audience**: Developers implementing the migration

**Prerequisites**:
- Architecture approved (`node-store-abstraction.md`)
- Roadmap reviewed (`surrealdb-migration-roadmap.md`)
- Development environment setup (Rust, Bun, tests passing)

## Implementation Checklist

### Pre-Implementation

- [ ] Architecture review complete
- [ ] Planning PR approved
- [ ] Test baseline documented (`bun run test:all`)
- [ ] Branch created: `feature/issue-461-surrealdb-hybrid-migration`
- [ ] Issue #461 assigned and status set to "In Progress"

### Week 1: Trait Design & SQL Extraction

- [ ] **Day 1-2**: Define NodeStore trait
  - [ ] Create `packages/core/src/db/node_store.rs`
  - [ ] Define trait with 22 methods (transaction method deferred to Phase 2)
  - [ ] Add comprehensive documentation
  - [ ] Compile check passes
  - [ ] Commit: "Define NodeStore trait abstraction layer (#462)"

- [ ] **Day 3**: Extract CRUD operations (4 methods)
  - [ ] `db_create_node()`
  - [ ] `db_get_node()`
  - [ ] `db_update_node()`
  - [ ] `db_delete_node()`
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Extract CRUD operations to DatabaseService (#462)"

- [ ] **Day 4**: Extract query operations (4 methods)
  - [ ] `db_query_nodes()`
  - [ ] `db_get_children()`
  - [ ] `db_get_nodes_by_container()`
  - [ ] `db_search_nodes_by_content()`
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Extract query operations to DatabaseService (#462)"

- [ ] **Day 5**: Extract hierarchy operations (2 methods)
  - [ ] `db_move_node()`
  - [ ] `db_reorder_node()`
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Extract hierarchy operations to DatabaseService (#462)"

- [ ] **Day 6**: Extract mention operations (5 methods)
  - [ ] `db_create_mention()`
  - [ ] `db_delete_mention()`
  - [ ] `db_get_outgoing_mentions()`
  - [ ] `db_get_incoming_mentions()`
  - [ ] `db_get_mentioning_containers()`
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Extract mention operations to DatabaseService (#462)"

- [ ] **Day 7**: Extract schema & embedding operations (5 methods)
  - [ ] `db_get_schema()`
  - [ ] `db_update_schema()`
  - [ ] `db_get_nodes_without_embeddings()`
  - [ ] `db_update_embedding()`
  - [ ] `db_search_by_embedding()`
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Extract schema and embedding operations to DatabaseService (#462)"

### Week 2: TursoStore Implementation & NodeService Refactoring

- [ ] **Day 8-9**: Implement TursoStore wrapper
  - [ ] Create `packages/core/src/db/turso_store.rs`
  - [ ] Implement all 23 trait methods (delegation)
  - [ ] Add constructor and helper methods
  - [ ] Test: Unit tests for each method
  - [ ] Commit: "Implement TursoStore wrapper (#463)"

- [ ] **Day 10**: Add TursoStore integration tests
  - [ ] Create `packages/core/src/db/turso_store_test.rs`
  - [ ] Test each trait method (success cases)
  - [ ] Test error handling (edge cases)
  - [ ] Test concurrent operations
  - [ ] Test: `bun run test:all` passes
  - [ ] Commit: "Add TursoStore integration tests (#463)"

- [ ] **Day 11-12**: Refactor NodeService
  - [ ] Change constructor to accept `Arc<dyn NodeStore>`
  - [ ] Replace DatabaseService calls with trait calls
  - [ ] Update initialization code
  - [ ] Test: `bun run test:all` passes (100% requirement)
  - [ ] Commit: "Refactor NodeService to use NodeStore abstraction (#463)"

- [ ] **Day 13**: Add performance benchmarks
  - [ ] Create `packages/core/benches/node_store_overhead.rs`
  - [ ] Benchmark direct vs trait calls
  - [ ] Benchmark CRUD operations
  - [ ] Benchmark batch operations
  - [ ] Validate: <5% overhead
  - [ ] Commit: "Add node_store_overhead benchmarks (#462)"

- [ ] **Day 14**: Documentation & PR
  - [ ] Update architecture docs
  - [ ] Add code examples
  - [ ] Create PR description
  - [ ] Request review
  - [ ] Address feedback

### Post-Implementation

- [ ] Code review approved
- [ ] All tests passing (100%)
- [ ] Performance benchmarks within limits (<5% overhead)
- [ ] Merged to main
- [ ] Deployed to production
- [ ] Monitor for 1 month (stability validation)
- [ ] Decision Point 1: Proceed to Phase 2?

## Detailed Implementation Steps

### Step 1: Create NodeStore Trait

**File**: `packages/core/src/db/node_store.rs`

```rust
use async_trait::async_trait;
use anyhow::Result;
use crate::models::{Node, NodeFilter, NodeUpdate, OrderBy};
use serde_json::Value;

#[async_trait]
pub trait NodeStore: Send + Sync {
    // CRUD operations
    async fn create_node(&self, node: &Node) -> Result<Node>;
    async fn get_node(&self, id: &str) -> Result<Option<Node>>;
    async fn update_node(&self, id: &str, update: &NodeUpdate) -> Result<Node>;
    async fn delete_node(&self, id: &str) -> Result<()>;

    // Querying
    async fn query_nodes(
        &self,
        filter: &NodeFilter,
        order_by: Option<OrderBy>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Node>>;
    async fn get_children(&self, parent_id: Option<&str>) -> Result<Vec<Node>>;
    async fn get_nodes_by_container(&self, container_id: &str) -> Result<Vec<Node>>;
    async fn search_nodes_by_content(&self, query: &str, limit: Option<i64>) -> Result<Vec<Node>>;

    // Hierarchy
    async fn move_node(&self, id: &str, new_parent_id: Option<&str>) -> Result<()>;
    async fn reorder_node(&self, id: &str, new_position: f64) -> Result<()>;

    // Mentions
    async fn create_mention(&self, source_id: &str, target_id: &str) -> Result<()>;
    async fn delete_mention(&self, source_id: &str, target_id: &str) -> Result<()>;
    async fn get_outgoing_mentions(&self, node_id: &str) -> Result<Vec<String>>;
    async fn get_incoming_mentions(&self, node_id: &str) -> Result<Vec<String>>;
    async fn get_mentioning_containers(&self, node_id: &str) -> Result<Vec<Node>>;

    // Schema
    async fn get_schema(&self, node_type: &str) -> Result<Option<Value>>;
    async fn update_schema(&self, node_type: &str, schema: &Value) -> Result<()>;

    // Embeddings
    async fn get_nodes_without_embeddings(&self, limit: Option<i64>) -> Result<Vec<Node>>;
    async fn update_embedding(&self, node_id: &str, embedding: &[u8]) -> Result<()>;
    async fn search_by_embedding(&self, embedding: &[u8], limit: i64) -> Result<Vec<(Node, f64)>>;

    // Batch/Transaction
    async fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&dyn NodeStore) -> Result<T> + Send,
        T: Send;
    async fn batch_create_nodes(&self, nodes: &[Node]) -> Result<Vec<Node>>;

    // Lifecycle
    async fn close(&self) -> Result<()>;
}
```

**Register module**: Add to `packages/core/src/db/mod.rs`:
```rust
pub mod node_store;
pub use node_store::NodeStore;
```

**Test**: `cargo check --package nodespace-core`

### Step 2: Extract SQL - Example (create_node)

**Current code** (NodeService):
```rust
impl NodeService {
    pub async fn create_node(&self, node: Node) -> Result<Node> {
        let conn = self.db.connect_with_timeout().await?;

        conn.execute(
            "INSERT INTO nodes (id, node_type, content, parent_id, position, properties, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &node.id,
                &node.node_type,
                &node.content,
                &node.parent_id,
                node.position,
                &node.properties,
                &node.created_at,
                &node.updated_at,
            ]
        ).await?;

        Ok(node)
    }
}
```

**Step 2.1**: Extract to DatabaseService:
```rust
// In packages/core/src/db/database.rs
impl DatabaseService {
    pub async fn db_create_node(&self, node: &Node) -> Result<Node> {
        let conn = self.connect_with_timeout().await?;

        conn.execute(
            "INSERT INTO nodes (id, node_type, content, parent_id, position, properties, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &node.id,
                &node.node_type,
                &node.content,
                &node.parent_id,
                node.position,
                &node.properties,
                &node.created_at,
                &node.updated_at,
            ]
        ).await?;

        Ok(node.clone())
    }
}
```

**Step 2.2**: Update NodeService (temporary - still using DatabaseService):
```rust
impl NodeService {
    pub async fn create_node(&self, node: Node) -> Result<Node> {
        // Validation logic (unchanged)
        validate_node(&node)?;

        // Delegate to DatabaseService
        self.db.db_create_node(&node).await
    }
}
```

**Step 2.3**: Test:
```bash
bun run test:all
# Must pass with ZERO new failures
```

**Step 2.4**: Commit:
```bash
git add .
git commit -m "Extract create_node SQL to DatabaseService (#462)"
```

**Repeat** for all 22 methods (one commit per day).

### Step 3: Implement TursoStore

**File**: `packages/core/src/db/turso_store.rs`

```rust
use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;
use crate::db::{DatabaseService, NodeStore};
use crate::models::{Node, NodeFilter, NodeUpdate, OrderBy};
use serde_json::Value;

/// Turso/libsql implementation of NodeStore trait
pub struct TursoStore {
    db_service: Arc<DatabaseService>,
}

impl TursoStore {
    pub fn new(db_service: Arc<DatabaseService>) -> Self {
        Self { db_service }
    }
}

#[async_trait]
impl NodeStore for TursoStore {
    async fn create_node(&self, node: &Node) -> Result<Node> {
        self.db_service.db_create_node(node).await
    }

    async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        self.db_service.db_get_node(id).await
    }

    async fn update_node(&self, id: &str, update: &NodeUpdate) -> Result<Node> {
        self.db_service.db_update_node(id, update).await
    }

    async fn delete_node(&self, id: &str) -> Result<()> {
        self.db_service.db_delete_node(id).await
    }

    // ... implement remaining 19 methods
}
```

**Register module**: Add to `packages/core/src/db/mod.rs`:
```rust
pub mod turso_store;
pub use turso_store::TursoStore;
```

**Test**: `cargo check --package nodespace-core`

### Step 4: Add TursoStore Tests

**File**: `packages/core/src/db/turso_store_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DatabaseService, TursoStore};
    use crate::models::Node;

    async fn setup() -> Arc<TursoStore> {
        let db = DatabaseService::new_in_memory().await.unwrap();
        Arc::new(TursoStore::new(Arc::new(db)))
    }

    #[tokio::test]
    async fn test_create_node() {
        let store = setup().await;

        let node = Node::new("test-1", "text", "Test content");
        let created = store.create_node(&node).await.unwrap();

        assert_eq!(created.id, node.id);
        assert_eq!(created.content, node.content);
    }

    #[tokio::test]
    async fn test_get_node() {
        let store = setup().await;

        let node = Node::new("test-1", "text", "Test content");
        store.create_node(&node).await.unwrap();

        let retrieved = store.get_node("test-1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "test-1");
    }

    #[tokio::test]
    async fn test_get_node_not_found() {
        let store = setup().await;

        let result = store.get_node("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    // Add tests for all 22 methods
    // Focus on: success cases, error cases, edge cases
}
```

**Test**: `cargo test --package nodespace-core db::turso_store_test`

### Step 5: Refactor NodeService

**File**: `packages/core/src/services/node_service.rs`

**Before**:
```rust
pub struct NodeService {
    db: Arc<DatabaseService>,
}

impl NodeService {
    pub fn new(db: Arc<DatabaseService>) -> Self {
        Self { db }
    }

    pub async fn create_node(&self, node: Node) -> Result<Node> {
        self.db.db_create_node(&node).await
    }
}
```

**After**:
```rust
pub struct NodeService {
    store: Arc<dyn NodeStore>,
}

impl NodeService {
    pub fn new(store: Arc<dyn NodeStore>) -> Self {
        Self { store }
    }

    pub async fn create_node(&self, node: Node) -> Result<Node> {
        // Validation logic (unchanged)
        validate_node(&node)?;

        // Delegate to abstraction layer
        self.store.create_node(&node).await
    }

    // Update all methods to use self.store instead of self.db
}
```

**Update initialization** (where NodeService is constructed):
```rust
// BEFORE
let db = Arc::new(DatabaseService::new().await?);
let node_service = NodeService::new(db);

// AFTER
let db = Arc::new(DatabaseService::new().await?);
let store: Arc<dyn NodeStore> = Arc::new(TursoStore::new(db));
let node_service = NodeService::new(store);
```

**Test**: `bun run test:all` (must pass 100%)

### Step 6: Add Performance Benchmarks

**File**: `packages/core/benches/node_store_overhead.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nodespace_core::db::{DatabaseService, NodeStore, TursoStore};
use nodespace_core::models::Node;
use std::sync::Arc;

fn setup() -> (Arc<DatabaseService>, Arc<dyn NodeStore>) {
    let db = Arc::new(DatabaseService::new_in_memory().unwrap());
    let store: Arc<dyn NodeStore> = Arc::new(TursoStore::new(db.clone()));
    (db, store)
}

fn benchmark_create_direct(c: &mut Criterion) {
    let (db, _) = setup();

    c.bench_function("create_node_direct", |b| {
        b.iter(|| {
            let node = Node::new("test", "text", "content");
            db.db_create_node(black_box(&node))
        })
    });
}

fn benchmark_create_trait(c: &mut Criterion) {
    let (_, store) = setup();

    c.bench_function("create_node_trait", |b| {
        b.iter(|| {
            let node = Node::new("test", "text", "content");
            store.create_node(black_box(&node))
        })
    });
}

criterion_group!(benches, benchmark_create_direct, benchmark_create_trait);
criterion_main!(benches);
```

**Run benchmarks**:
```bash
cargo bench --bench node_store_overhead
```

**Validate**: Trait call should be <5% slower than direct call (typically <2ns difference).

## SQL Audit Process

Before merging Phase 1, conduct a comprehensive SQL audit to ensure all database operations are properly abstracted.

### Audit Checklist

**1. Global SQL Search**
```bash
# Search for raw SQL in NodeService
rg "SELECT|INSERT|UPDATE|DELETE" packages/core/src/services/node_service.rs

# Should return ZERO matches (all SQL extracted to DatabaseService)
```

**2. DatabaseService Method Coverage**
```bash
# List all DatabaseService methods
rg "pub async fn db_" packages/core/src/services/database_service.rs

# Verify: 22 methods match NodeStore trait methods
```

**3. NodeService Dependency Audit**
```bash
# Search for any remaining direct database calls
rg "self\.database\." packages/core/src/services/node_service.rs

# Should only see: self.database.method_name() (no raw SQL)
```

**4. Test Coverage Verification**
```bash
# Check for SQL in test files
rg "SELECT|INSERT|UPDATE|DELETE" packages/core/src/services/node_service_test.rs

# Should return ZERO matches (tests use trait methods)
```

### SQL Migration Patterns

**Pattern 1: Simple CRUD Operations**

Before:
```rust
// In NodeService (❌ Direct SQL)
let node = sqlx::query_as!(
    Node,
    "SELECT * FROM nodes WHERE id = ?",
    id
)
.fetch_one(&self.database.pool)
.await?;
```

After:
```rust
// In DatabaseService (✅ Extracted)
pub async fn db_get_node(&self, id: &str) -> Result<Option<Node>> {
    let node = sqlx::query_as!(
        Node,
        "SELECT * FROM nodes WHERE id = ?",
        id
    )
    .fetch_optional(&self.pool)
    .await?;
    Ok(node)
}

// In NodeService (✅ Uses abstraction)
let node = self.database.db_get_node(id).await?
    .ok_or_else(|| anyhow!("Node not found"))?;
```

**Pattern 2: Complex Queries with JOINs**

Before:
```rust
// In NodeService (❌ Complex SQL inline)
let containers = sqlx::query_as!(
    Node,
    r#"
    SELECT DISTINCT n.*
    FROM nodes n
    JOIN mentions m ON n.id = m.container_id
    WHERE m.target_id = ?
    "#,
    node_id
)
.fetch_all(&self.database.pool)
.await?;
```

After:
```rust
// In DatabaseService (✅ Extracted and documented)
/// Get containers mentioning this node (for backlinks)
pub async fn db_get_mentioning_containers(&self, node_id: &str) -> Result<Vec<Node>> {
    let containers = sqlx::query_as!(
        Node,
        r#"
        SELECT DISTINCT n.*
        FROM nodes n
        JOIN mentions m ON n.id = m.container_id
        WHERE m.target_id = ?
        "#,
        node_id
    )
    .fetch_all(&self.pool)
    .await?;
    Ok(containers)
}

// In NodeService (✅ Uses abstraction)
let containers = self.database.db_get_mentioning_containers(node_id).await?;
```

**Pattern 3: Transactions (Deferred to Phase 2)**

Before:
```rust
// In NodeService (❌ Direct transaction)
let mut tx = self.database.pool.begin().await?;
sqlx::query!("INSERT INTO nodes ...").execute(&mut tx).await?;
sqlx::query!("INSERT INTO mentions ...").execute(&mut tx).await?;
tx.commit().await?;
```

After (Phase 1):
```rust
// In NodeService (✅ Sequential operations - acceptable for Phase 1)
self.database.db_create_node(node).await?;
self.database.db_create_mention(source_id, target_id).await?;

// Note: Not atomic in Phase 1, but acceptable risk for early development
// Phase 2 will add proper transaction support
```

### Audit Report Template

After completing the audit, document findings:

```markdown
## SQL Audit Report - Epic #461 Phase 1

**Date**: [YYYY-MM-DD]
**Auditor**: [Name/Agent]

### Summary
- ✅ All CRUD SQL extracted to DatabaseService
- ✅ All query SQL extracted to DatabaseService
- ✅ NodeService has ZERO direct SQL
- ✅ 22/22 trait methods implemented
- ⚠️  3 methods use sequential operations (not atomic) - acceptable for Phase 1

### Findings
1. **NodeService SQL**: 0 matches (PASS)
2. **DatabaseService methods**: 22 methods (PASS)
3. **Test SQL**: 0 matches (PASS)
4. **Edge cases**: All error paths use trait methods (PASS)

### Recommendations
- Phase 1 ready to merge
- Phase 2 should add transaction support for atomic multi-operation sequences
- Monitor production for any edge cases requiring transactions
```

### Continuous Audit (During Implementation)

Run this command after each commit:
```bash
# Quick SQL audit
rg "SELECT|INSERT|UPDATE|DELETE" packages/core/src/services/node_service.rs || echo "✅ No raw SQL in NodeService"
```

If any SQL found, extract it immediately before continuing.

## Testing Strategy

### Continuous Testing

**After EACH commit**:
```bash
# Run full test suite
bun run test:all

# Verify: ZERO new failures
diff baseline_tests.txt current_tests.txt
```

### Test Categories

**Unit Tests** (TursoStore):
- Each trait method has dedicated test
- Error handling (invalid IDs, constraints)
- Edge cases (NULL values, empty results)

**Integration Tests** (NodeService + TursoStore):
- All existing NodeService tests pass
- New abstraction layer tests
- Concurrent operations

**Performance Tests**:
- Trait dispatch overhead (<5%)
- CRUD throughput
- Batch operation performance

## Common Pitfalls

### Pitfall 1: Extracting Multiple Methods at Once

**Problem**: Extract 5 methods, tests fail, hard to debug which extraction caused issue

**Solution**: Extract ONE method at a time, test after each

### Pitfall 2: Forgetting to Test

**Problem**: Extract methods without running tests, discover regressions later

**Solution**: Run `bun run test:all` after EVERY commit

### Pitfall 3: Incomplete SQL Extraction

**Problem**: Miss some SQL in NodeService, abstraction incomplete

**Solution**: Use grep to find all SQL queries:
```bash
grep -r "execute\|query\|prepare" packages/core/src/services/
```

### Pitfall 4: Breaking Transaction Semantics

**Problem**: Refactoring changes transaction boundaries, causes data inconsistency

**Solution**: Preserve exact transaction scope, test concurrent operations

### Pitfall 5: Poor Error Messages

**Problem**: Abstraction layer errors don't provide context, hard to debug

**Solution**: Add context with `anyhow`:
```rust
self.db_service.db_create_node(node)
    .await
    .context(format!("Failed to create node: {}", node.id))?
```

## Troubleshooting

### Issue: Tests Fail After Extraction

**Symptoms**: Tests that passed before extraction now fail

**Diagnosis**:
1. Check SQL syntax (copy-paste errors)
2. Verify parameter order (params array matches query)
3. Check return type (Node vs Option<Node>)

**Solution**: Revert last commit, extract method more carefully

### Issue: Performance Regression

**Symptoms**: Benchmarks show >5% overhead

**Diagnosis**:
1. Check if trait object allocation excessive
2. Verify async overhead minimal
3. Profile with `cargo flamegraph`

**Solution**: Optimize hot paths, consider inlining

### Issue: Compilation Errors

**Symptoms**: Trait method signatures don't match implementation

**Diagnosis**:
1. Check async_trait macro used
2. Verify lifetimes match
3. Check Send + Sync bounds

**Solution**: Fix trait definition or implementation

## Next Steps After Phase 1

### Stability Period (1 month)

**Monitoring**:
- Error rates (should be identical to pre-abstraction)
- Performance metrics (latency, throughput)
- User feedback (any issues?)

**Success Criteria**:
- Zero critical issues
- Performance stable
- No user complaints

### Decision Point 1: Proceed to Phase 2?

**Evaluate**:
- ✅ Abstraction layer stable?
- ✅ Performance acceptable?
- ✅ Ready for SurrealDB implementation?

**Outcomes**:
- **YES**: Create Epic #467, begin Phase 2
- **NO**: Iterate on abstraction layer, defer SurrealDB

## References

- Architecture: `/docs/architecture/data/node-store-abstraction.md`
- Roadmap: `/docs/architecture/data/surrealdb-migration-roadmap.md`
- Epic #461: SurrealDB Migration - Phase 1
- Issue #462: NodeStore Trait Abstraction Layer
- Issue #463: TursoStore Wrapper Implementation
