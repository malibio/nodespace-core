# Schema Cache Population Strategy (Issue #704)

## Overview

SurrealStore maintains two in-memory caches derived from schema definitions:
- `types_with_spoke_tables`: Which node types need spoke tables
- `valid_node_types`: All valid node types (for SQL injection prevention)

These caches avoid querying the database on every CRUD operation.

## The Challenge

Schema seeding moved from SurrealStore (data layer) to NodeService (domain layer) in Issue #704. This created a timing issue:

```
SurrealStore::new()
  → initialize_schema() creates tables (node, schema, task, mentions, has_child)
  → build_schema_caches() queries schema table (EMPTY on first launch!)
  → Cache = {"schema"} only

NodeService::new()
  → seed_core_schemas() creates schema RECORDS
  → Cache is now STALE
```

## The Solution: Two-Phase Cache Population

### Scenario 1: First Launch (Fresh Database)

```rust
1. SurrealStore::new()
   → initialize_schema() creates infrastructure tables
   → build_schema_caches() queries schema table (empty)
   → Cache starts as {"schema"} only (hardcoded)

2. NodeService::new()
   → Detects no schemas exist
   → Seeds schema records:
      for schema in core_schemas {
          create_schema_node_atomic(schema);

          // ✅ Populate cache incrementally (no DB re-query!)
          add_to_schema_cache(schema.id, !schema.fields.is_empty());
      }
   → Cache now complete: {"schema", "task", "text", "date", ...}

3. All operations use fully populated cache ✅
```

### Scenario 2: Subsequent Launches (Existing Database)

```rust
1. SurrealStore::new()
   → initialize_schema() (tables already exist, no-op)
   → build_schema_caches() queries schema table (populated!)
   → Cache fully loaded: {"schema", "task", "text", "date", ...}

2. NodeService::new()
   → Detects schemas already exist
   → No seeding needed

3. All operations use fully populated cache ✅
```

## Key Benefits

1. **No re-fetching**: Schema data is already in memory during seeding
2. **Single query on subsequent launches**: One `SELECT` gets all schemas
3. **Optimal performance**: O(1) cache lookups instead of DB queries on every operation
4. **Scales well**: With hundreds of node types (user plugins), caching becomes more important

## Implementation

### SurrealStore Methods

```rust
// Queries database for existing schema records (subsequent launches)
async fn build_schema_caches() -> Result<(HashSet<String>, HashSet<String>)>

// Adds a type to caches during seeding (first launch)
pub(crate) fn add_to_schema_cache(&mut self, type_name: String, has_fields: bool)
```

### NodeService Logic

```rust
// NodeService::new() takes &mut Arc to enable cache updates
pub async fn new(store: &mut Arc<SurrealStore<C>>) -> Result<Self> {
    // Seed before cloning the Arc into Self
    Self::seed_core_schemas_if_needed(store).await?;

    // Now clone into Self
    let service = Self { store: Arc::clone(store), ... };
    Ok(service)
}

async fn seed_core_schemas_if_needed(store: &mut Arc<SurrealStore<C>>) -> Result<()> {
    // Check if already seeded
    if store.get_node("task").await?.is_some() {
        return Ok(());
    }

    for schema in core_schemas {
        // Create schema node + spoke table
        store.create_schema_node_atomic(node, ddl).await?;
    }

    // Get mutable access (we're the only Arc owner at this point)
    let store_mut = Arc::get_mut(store)?;
    for schema in core_schemas {
        store_mut.add_to_schema_cache(schema.id, !schema.fields.is_empty());
    }
}
```

## Why Cache at All?

With potentially hundreds of node types (especially with user plugins), querying the database on every `create_node()`, `update_node()`, `delete_node()` operation would be expensive:

- **Without cache**: ~100 node types × 1000 operations = 100,000 queries
- **With cache**: 1 query at startup + O(1) HashSet lookups

The caches are tiny (few KB) but queried constantly during all CRUD operations.

## Verification

### Manual Testing (Dev-Proxy Fresh DB)

1. Delete SurrealDB data: `rm -rf ~/.nodespace/dev.db`
2. Start SurrealDB: `bun run dev:db:persist`
3. Start dev-proxy: `bun run dev:proxy`
4. Verify schemas seeded: `curl http://localhost:3001/api/schemas/date`
5. Expected: 200 OK with date schema JSON (not 404)

### Automated Testing

The implementation is verified by tests in `packages/core/src/services/node_service.rs`:

1. **Fresh database seeding test**: `test_seeding_fresh_database` (line ~600)
   - Creates fresh SurrealStore with no schemas
   - Calls `NodeService::new()` which triggers seeding
   - Verifies all 7 core schemas created via `get_schema_node()`

2. **Idempotent seeding test**: `test_seeding_idempotent` (line ~630)
   - Calls `NodeService::new()` twice
   - Verifies schemas only created once (not duplicated)

3. **Cache population test**: Cache is verified indirectly through subsequent CRUD operations
   - After seeding, `has_spoke_table("task")` returns true
   - Node creation for "task" type works correctly with spoke table
