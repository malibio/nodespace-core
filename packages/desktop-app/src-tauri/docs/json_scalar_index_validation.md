# JSON Scalar Index Validation Report (Sub-issue #454)

**Date**: 2025-11-10
**Epic**: #451 LanceDB Evaluation Phase 2
**Investigator**: Senior Architect Reviewer Agent

## Executive Summary

### Critical Finding: LanceDB Cannot Query JSON Strings with SQL

**Status**: 🔴 **MAJOR LIMITATION IDENTIFIED**

LanceDB's current implementation stores the `properties` field as a **JSON string** (Arrow `DataType::Utf8`), which **prevents SQL-based querying of nested properties**. This is a fundamental architectural constraint that significantly impacts NodeSpace's ability to filter nodes by dynamic properties.

### Key Findings

| Aspect | Status | Impact |
|--------|--------|--------|
| **JSON String Storage** | Current Implementation | ❌ Cannot query nested properties with SQL |
| **SQL json_extract() Support** | Not Available | ❌ DataFusion SQL doesn't have json_extract() |
| **Arrow Struct Storage** | Not Implemented | ✅ Would enable SQL queries on nested fields |
| **Dynamic Schema** | Required | NodeSpace needs flexible property schemas |

---

## Detailed Analysis

### 1. Current Implementation (Phase 1)

**What was built:**
```rust
// In packages/desktop-app/src-tauri/src/datastore/lance/store.rs
Field::new("properties", DataType::Utf8, true), // Nullable JSON string
```

**How properties are stored:**
- Properties are serialized to JSON string: `{"status": "done", "priority": 5}`
- Stored as plain text in Arrow Utf8 column
- No schema enforcement or type information preserved

**Consequences:**
1. ❌ **No SQL filtering**: Cannot write `WHERE properties.status = 'done'`
2. ❌ **No indexes**: Cannot create scalar indexes on nested properties
3. ❌ **Application-level filtering only**: Must load all nodes and filter in Rust code
4. ❌ **Performance impact**: Full table scans required for any property-based query

### 2. What We Tried to Test

**Attempted query syntax** (from test suite):
```rust
// Attempt 1: SQLite-style json_extract()
let filter = "properties != '' AND cast(json_extract(properties, '$.status') as text) = 'done'";

// Error returned:
// LanceError(IO): Error during planning: Invalid function 'json_extract'.
```

**Why it failed:**
- LanceDB uses **Apache DataFusion SQL**, not SQLite SQL
- DataFusion doesn't have `json_extract()` function for string columns
- JSON strings are opaque blobs to the SQL engine

### 3. LanceDB's Expected Pattern: Arrow Struct

**How LanceDB is designed to work:**

LanceDB/DataFusion SQL expects structured data as **Arrow Struct types**, not JSON strings:

```rust
// CORRECT approach for queryable nested data:
Field::new("properties", DataType::Struct(Fields::from(vec![
    Field::new("status", DataType::Utf8, true),
    Field::new("priority", DataType::Int64, true),
    Field::new("customer", DataType::Struct(Fields::from(vec![
        Field::new("name", DataType::Utf8, true),
        Field::new("address", DataType::Struct(Fields::from(vec![
            Field::new("street", DataType::Utf8, true),
            Field::new("zip", DataType::Utf8, true),
        ])), true),
    ])), true),
])), true)
```

**Query syntax with Struct:**
```rust
// Backtick syntax for nested fields (per LanceDB documentation)
let filter = "`properties`.`status` = 'done'";
let filter = "`properties`.`customer`.`address`.`zip` = '00042'";
```

### 4. The Dynamic Schema Problem

**NodeSpace's requirement:**
- Users can define custom node types (e.g., "customer", "project", "invoice")
- Each node type has different properties (flexible schema)
- Properties are defined via schema-as-nodes (Pure JSON approach)
- Schema can evolve without database migrations

**LanceDB Arrow Struct constraint:**
- Struct schema is **fixed at table creation time**
- Cannot add new fields dynamically without table schema migration
- All rows must conform to the same struct schema

**This is a fundamental mismatch!**

---

## Architectural Options Analysis

### Option 1: Keep JSON String Storage (Current)

**Pros:**
- ✅ Maximum flexibility - any property structure allowed
- ✅ No schema migrations required
- ✅ Easy to implement (already done)
- ✅ Aligns with NodeSpace's "Pure JSON" architecture

**Cons:**
- ❌ **No SQL filtering on properties** - must load all nodes to memory
- ❌ **No scalar indexes** - cannot accelerate property queries
- ❌ **Poor query performance** - full table scans for every property filter
- ❌ **Application-level complexity** - must implement filtering in Rust
- ❌ **Scalability concerns** - becomes impractical with 100k+ nodes

**Verdict**: ⚠️ **Acceptable for PoC, problematic at scale**

### Option 2: Migrate to Arrow Struct (Fixed Schema)

**Pros:**
- ✅ SQL filtering on nested properties
- ✅ Scalar indexes possible
- ✅ Excellent query performance
- ✅ Leverages LanceDB's full capabilities

**Cons:**
- ❌ **Breaks dynamic schema requirement** - requires schema defined upfront
- ❌ **Complex schema migrations** - adding custom node types requires table restructuring
- ❌ **All-or-nothing type safety** - cannot mix property schemas in same table
- ❌ **Conflicts with Pure JSON architecture** - NodeSpace's core design principle

**Verdict**: ❌ **Incompatible with NodeSpace's architecture**

### Option 3: Hybrid Approach - Separate Tables per Node Type

**Implementation:**
```rust
// Create separate LanceDB tables for each node type
universal_nodes_text      // Minimal common schema
universal_nodes_task      // Task-specific struct properties
universal_nodes_customer  // Customer-specific struct properties
```

**Pros:**
- ✅ SQL filtering within each node type
- ✅ Scalar indexes per node type
- ✅ Schema flexibility (new node type = new table)

**Cons:**
- ❌ **Cross-type queries difficult** - cannot JOIN across tables efficiently
- ❌ **Schema versioning complexity** - each table evolves independently
- ❌ **Table proliferation** - 50 custom node types = 50 tables
- ❌ **Migration overhead** - new node types require table creation

**Verdict**: ⚠️ **Viable but adds significant complexity**

### Option 4: Top-Level Struct with JSON Fallback

**Implementation:**
```rust
Field::new("properties", DataType::Struct(Fields::from(vec![
    // Core common properties as struct fields (queryable)
    Field::new("status", DataType::Utf8, true),
    Field::new("priority", DataType::Int64, true),
    Field::new("due_date", DataType::Utf8, true),

    // Everything else in JSON fallback (not queryable)
    Field::new("custom_properties", DataType::Utf8, true), // JSON string
])), true)
```

**Pros:**
- ✅ SQL filtering on common properties
- ✅ Indexes on frequently-queried fields
- ✅ Flexible schema for custom properties
- ✅ Gradual migration path (start with common fields)

**Cons:**
- ❌ **Hybrid complexity** - two storage patterns
- ❌ **Custom properties still not queryable** - partial solution
- ❌ **Schema drift risk** - which properties go in struct vs JSON?

**Verdict**: ⚠️ **Pragmatic compromise, but complexity cost**

---

## Performance Impact Analysis

### Current Turso Implementation

Turso stores properties as JSON string with SQLite's json_extract():

```sql
-- Turso query (works)
SELECT * FROM nodes WHERE json_extract(properties, '$.status') = 'done'
```

**Performance characteristics:**
- SQLite's `json_extract()` parses JSON on-the-fly (runtime cost)
- No indexes on JSON properties (full table scan)
- Acceptable for 10k-100k rows with SSD storage
- Becomes slow at 100k+ rows

### LanceDB with JSON String (Current Implementation)

```rust
// LanceDB approach (doesn't work with SQL)
// Must load all nodes and filter in application code
let all_nodes = store.query_nodes("").await?;
let filtered: Vec<_> = all_nodes.into_iter()
    .filter(|n| n.properties["status"] == "done")
    .collect();
```

**Performance characteristics:**
- **Must load entire dataset into memory** (high memory usage)
- Application-level filtering (CPU-intensive with serde deserialization)
- Linear time complexity O(n) for every query
- **Worse than Turso** because no database-level filtering at all

**Conclusion**: ❌ **Current LanceDB implementation is SLOWER than Turso for property queries**

### LanceDB with Arrow Struct (Hypothetical)

```rust
// If we used Arrow Struct (hypothetical)
let filter = "`properties`.`status` = 'done'";
let results = store.query_nodes(filter).await?;
```

**Performance characteristics:**
- Native Arrow columnar filtering (very fast)
- Scalar indexes available (sub-millisecond queries)
- Pushdown predicates (only load matching rows)
- **Significantly faster than Turso** at scale

**Conclusion**: ✅ **Would be faster than Turso, but requires fixed schema**

---

## Test Results

### Tests Executed

1. ✅ **Dataset Creation** - Successfully created 100 test nodes with nested metadata
2. ✅ **LanceDB Insertion** - All nodes stored correctly as JSON strings
3. ❌ **Single-level filtering** - `json_extract()` not supported
4. ❌ **Multi-level filtering** - Cannot query nested properties
5. ❌ **Deep nesting (5 levels)** - Same limitation
6. ❌ **Sparse properties** - Same limitation
7. ⏸️  **Performance benchmark** - Skipped due to query limitation
8. 📋 **Documentation** - Findings captured in this report

### Error Messages

```
Error: LanceDB operation failed: Query failed: lance error: LanceError(IO):
Error during planning: Invalid function 'json_extract'.
Did you mean 'union_extract'?
```

**Root cause**: DataFusion SQL doesn't have `json_extract()` for Utf8 columns

---

## Recommendations

### Immediate Actions for Epic #451

1. **Document this limitation in evaluation report** (#459)
   - LanceDB JSON string storage prevents property-based SQL filtering
   - Current implementation is SLOWER than Turso for dynamic properties
   - Migration would require fundamental schema architecture changes

2. **Test alternative query patterns**
   - Investigate if Arrow Struct conversion is feasible for common properties
   - Measure application-level filtering performance vs Turso
   - Benchmark vector search without property filters (LanceDB's strength)

3. **Revise migration recommendation**
   - If NodeSpace prioritizes dynamic schema: **STAY WITH TURSO**
   - If NodeSpace accepts fixed schema: **LanceDB becomes viable**
   - Consider hybrid: Turso for metadata, LanceDB for embeddings only

### Long-term Architectural Considerations

**Scenario A: Keep Dynamic Schema (NodeSpace's current design)**
- ✅ **Recommendation**: Stay with Turso/SQLite
- Turso's `json_extract()` support is critical for property filtering
- LanceDB would require application-level filtering (worse performance)

**Scenario B: Adopt Fixed Schema per Node Type**
- ✅ **Recommendation**: Consider LanceDB with separate tables
- Enables SQL filtering and scalar indexes
- Requires abandoning Pure JSON architecture
- Large refactoring effort

**Scenario C: Hybrid Architecture**
- ✅ **Recommendation**: Turso for graph, LanceDB for embeddings
- Store node metadata and relationships in Turso (dynamic schema, JSON support)
- Store only embeddings in LanceDB (vector search strength)
- Use Turso's `json_extract()` for property filtering
- Use LanceDB's IVF-PQ index for semantic search
- **Best of both worlds**

---

## Next Steps

1. **Complete sub-issue #455**: Test filtered/partial index support
   - Likely to find similar limitations due to JSON string storage
   - Document findings for evaluation report

2. **Complete sub-issue #456**: Performance benchmark framework
   - Measure application-level filtering performance
   - Compare against Turso's `json_extract()` baseline
   - Quantify the performance gap

3. **Complete sub-issue #457**: Feature parity validation
   - Document all query capabilities Turso has that LanceDB lacks
   - Focus on JSON querying as critical gap

4. **Compile evaluation report (#459)**
   - Present these findings with clear migration risks
   - Recommend Hybrid Architecture or Stay with Turso

---

## Conclusion

**🔴 CRITICAL FINDING**: LanceDB's JSON string storage for `properties` prevents SQL-based filtering on dynamic properties, which is a core requirement for NodeSpace's Pure JSON architecture.

**Migration Risk**: HIGH
**Recommendation**: Do NOT migrate to LanceDB for full node storage. Consider Hybrid Architecture or Stay with Turso.

**Key Insight**: LanceDB excels at vector search with Arrow columnar data, but **struggles with dynamic schema and nested JSON queries** that SQLite/Turso handles well. The two databases have fundamentally different design philosophies.

---

**Report prepared by**: Senior Architect Reviewer Agent
**Epic**: #451 Phase 2 - Validation & Benchmarking
**Date**: 2025-11-10
