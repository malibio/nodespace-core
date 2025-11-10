# Filtered/Partial Index Support Findings (Sub-issue #455)

**Date**: 2025-11-10
**Epic**: #451 LanceDB Evaluation Phase 2
**Status**: 📋 Documented - No Implementation Required

## Executive Summary

**Status**: ⏭️ **SKIPPED - BLOCKED BY SUB-ISSUE #454**

Due to the critical finding in sub-issue #454 (JSON string storage prevents SQL filtering), testing filtered/partial indexes is not applicable. **You cannot create indexes on data you cannot query**.

## Analysis

### What Are Filtered/Partial Indexes?

Filtered indexes (also called partial indexes) allow creating indexes on subsets of data:

```sql
-- Example: SQLite partial index
CREATE INDEX idx_done_tasks ON nodes(properties)
WHERE json_extract(properties, '$.status') = 'done'
AND node_type = 'task';
```

**Benefits:**
- Smaller index size (only indexes relevant rows)
- Faster index maintenance
- Better query performance for filtered queries

### LanceDB Filtered Index Capabilities

**Research Finding**: LanceDB 0.20 scalar index API does NOT support filtered/partial indexes.

**Current API:**
```rust
// LanceDB create_index() signature
table.create_index(
    &["column_name"],
    Index::BTree(Default::default())
)
```

**No WHERE clause parameter** - indexes are always created on the full column.

### Why This Doesn't Matter for NodeSpace

Given the findings from sub-issue #454:

1. **Properties are stored as JSON strings** - cannot be queried with SQL
2. **No SQL filtering on nested properties** - cannot create indexes anyway
3. **Application-level filtering required** - indexes wouldn't help

**Conclusion**: Filtered indexes are a non-issue because **property-based indexes aren't possible at all** with the current JSON string storage approach.

## Hypothetical: If Properties Were Arrow Struct

If NodeSpace converted `properties` to Arrow Struct (which we do NOT recommend):

```rust
// Hypothetical struct-based properties
Field::new("properties", DataType::Struct(Fields::from(vec![
    Field::new("status", DataType::Utf8, true),
    Field::new("priority", DataType::Int64, true),
])), true)
```

**Could we create filtered indexes?**
- ❌ **NO** - LanceDB API doesn't support WHERE clause in `create_index()`
- Workaround: Create separate tables per node type (each table is effectively a "filtered" dataset)

**Example workaround:**
```rust
// Instead of one table with filtered index:
universal_nodes (with partial index WHERE node_type = 'task')

// Use separate tables:
universal_nodes_task  (only task nodes - no index filter needed)
universal_nodes_text  (only text nodes)
```

## Recommendations

1. **For Epic #451 Evaluation**:
   - Document that filtered indexes are not supported in LanceDB 0.20
   - Note this is a minor limitation compared to the JSON querying issue
   - If NodeSpace needs filtered indexes: **separate tables per node type** is the workaround

2. **For Production Decision**:
   - This limitation is **low priority** compared to #454 findings
   - Focus migration decision on dynamic schema support (sub-issue #454)
   - Filtered indexes are a "nice-to-have", not a blocker

---

**Report prepared by**: Senior Architect Reviewer Agent
**Sub-issue**: #455
**Blocked by**: #454 (JSON string storage limitation)
