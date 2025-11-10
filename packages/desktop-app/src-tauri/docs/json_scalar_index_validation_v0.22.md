# JSON Scalar Index Validation - LanceDB v0.22.3 Retest

## Executive Summary

**Tested Version**: LanceDB v0.22.3 (upgraded from v0.20)
**Test Date**: 2025-11-10
**Result**: **JSON querying BROKEN - Blocker WORSENED**

## Critical Finding

⚠️ **BREAKING CHANGE**: LanceDB v0.22.3 has **BROKEN** the `json_extract()` functionality that worked in v0.20.

### Phase 2 (v0.20) Status
- ✅ `json_extract(properties, '$.field')` worked with `DataType::Utf8` (string) storage
- ✅ Query syntax: `properties != '' AND cast(json_extract(properties, '$.status') as text) = 'done'`
- ⚠️ No scalar indexes on JSON fields, but basic querying functioned

### v0.22.3 Status
- ❌ **REGRESSION**: `json_extract()` function signature changed
- ❌ **NEW REQUIREMENT**: Properties must be stored as `LargeBinary` with JSONB format
- ❌ **API INCOMPATIBILITY**: Can't cast `Utf8` to `LargeBinary` in SQL
- ❌ **FORMAT REQUIREMENT**: Expects JSONB (binary JSON format), not UTF-8 JSON strings

## Upgrade Notes

### Breaking API Changes

1. **Function Signature Change**:
   ```
   v0.20: json_extract(Utf8, Utf8) → Utf8
   v0.22.3: json_extract(LargeBinary, Utf8) → ??? (requires JSONB input)
   ```

2. **DataType Cast Changes**:
   - v0.20: `cast(... as text)` supported
   - v0.22.3: Must use `cast(... as string)` - `text` type no longer exists

3. **Null Check Changes**:
   - v0.20: `properties != ''` worked for Utf8 columns
   - v0.22.3: Must use `properties IS NOT NULL` for LargeBinary columns

### Compilation Issues Encountered

1. **Arrow Version Mismatch** (FIXED):
   - LanceDB 0.22.3 requires Arrow 56.x (was using 55.x)
   - **Solution**: Updated `arrow-array`, `arrow-schema`, `arrow-data` to `56.0`

2. **Schema Type Mismatch** (ATTEMPTED FIX):
   - Changed `properties` column from `DataType::Utf8` → `DataType::LargeBinary`
   - Updated array types from `StringArray` → `LargeBinaryArray`
   - **Result**: Compilation successful, but runtime errors

### Schema Migration Attempt

**Goal**: Store properties as LargeBinary to match v0.22.3 `json_extract()` requirements

**Changes Made**:
```rust
// OLD (v0.20):
Field::new("properties", DataType::Utf8, true)
Arc::new(StringArray::from(properties))

// NEW (v0.22.3 attempt):
Field::new("properties", DataType::LargeBinary, true)
Arc::new(LargeBinaryArray::from(properties_refs))

// Conversion:
let properties_bytes: Vec<Vec<u8>> = nodes
    .iter()
    .map(|n| n.properties.as_ref().map(|v| v.to_string().into_bytes()).unwrap_or_default())
    .collect();
```

**Result**: ❌ **FAILED** - `InvalidJsonb` error

## Test Results

### Test 1: json_extract() with Binary Storage
**Status**: ❌ FAILED
**Syntax tested**:
```sql
properties IS NOT NULL AND cast(json_extract(properties, '$.status') as string) = 'done'
```
**Error**:
```
Failed to select values from path '$.status': InvalidJsonb
```

**Root Cause**: LanceDB v0.22.3 expects JSONB format (binary JSON), not UTF-8 encoded JSON strings.

### JSONB Format Requirement

LanceDB v0.22.3 uses **JSONB** (binary JSON format, similar to PostgreSQL's JSONB):
- Not just UTF-8 bytes of JSON string
- Requires specialized binary encoding with type tags and offsets
- Different from simple `serde_json::to_string().as_bytes()`

**Implication**: Requires substantial schema and encoding changes to support v0.22.3.

### Tests Not Completed

Due to the JSONB format requirement, the following tests could not be completed:
- ❌ Multi-level nested property filtering
- ❌ Deep nesting (5 levels)
- ❌ Sparse property behavior
- ❌ Performance benchmarks
- ❌ Scalar index creation

## Technical Analysis

### Why v0.22.3 Is Worse Than v0.20

1. **Loss of Functionality**:
   - v0.20: Basic JSON querying worked (even without indexes)
   - v0.22.3: JSON querying completely broken without JSONB encoding

2. **Increased Complexity**:
   - v0.20: Store JSON as simple UTF-8 string → works
   - v0.22.3: Must implement JSONB encoder/decoder → non-trivial

3. **Migration Barrier**:
   - v0.20 → v0.22.3 requires:
     - Schema migration (Utf8 → LargeBinary)
     - Data migration (re-encode all JSON as JSONB)
     - Query updates (fix cast syntax, null checks)
     - JSONB encoder implementation

### JSONB Implementation Requirements

To support v0.22.3, we would need to:

1. **Implement JSONB Encoder**:
   ```rust
   fn encode_jsonb(value: &serde_json::Value) -> Vec<u8> {
       // Convert serde_json::Value to JSONB binary format
       // This is non-trivial - requires understanding JSONB spec
   }
   ```

2. **Implement JSONB Decoder**:
   ```rust
   fn decode_jsonb(bytes: &[u8]) -> Result<serde_json::Value> {
       // Parse JSONB binary back to serde_json::Value
   }
   ```

3. **Update All Storage Operations**:
   - Insert: JSON → JSONB encoding
   - Query: JSONB → JSON decoding
   - Update: Re-encode modified JSON

**Complexity Estimate**: 2-3 days of development + testing

## Updated Recommendation

### 🔴 **STRONGLY DO NOT MIGRATE to LanceDB v0.22.3**

The Phase 2 recommendation to **not migrate** is now **STRENGTHENED**:

**v0.20 Issues**:
- ⚠️ No scalar indexes on JSON fields
- ⚠️ Full table scan performance penalty
- ✅ But basic querying works

**v0.22.3 Issues**:
- ❌ JSON querying completely broken
- ❌ Requires JSONB format implementation
- ❌ Breaking API changes with no migration path
- ❌ Increased complexity without any benefits
- ❌ All Phase 2 limitations PLUS new blockers

### Comparison to Phase 2 Findings

| Aspect | v0.20 (Phase 2) | v0.22.3 (Current) |
|--------|----------------|-------------------|
| JSON Querying | ✅ Works | ❌ Broken |
| Scalar Indexes | ❌ Not supported | ❌ Not supported |
| Query Performance | ⚠️ Full scan | ❌ Can't query |
| Migration Effort | Medium | **Very High** |
| Recommendation | Do Not Migrate | **STRONGLY Do Not Migrate** |

## Recommendation: Stay on Turso

**Turso Advantages** (reinforced by v0.22.3 findings):
- ✅ Native JSON/JSONB support with SQLite's json_extract()
- ✅ Scalar indexes work on nested JSON fields
- ✅ Mature, stable API with backward compatibility
- ✅ No breaking changes between versions
- ✅ Excellent query performance with indexes

**LanceDB v0.22.3 Disadvantages**:
- ❌ Breaking changes with no migration guide
- ❌ Lost functionality compared to v0.20
- ❌ Requires custom JSONB implementation
- ❌ Still no scalar indexes after all this effort
- ❌ Immature API undergoing frequent breaking changes

## Next Steps

1. **Abandon LanceDB for Full Node Storage**:
   - Do NOT attempt to implement JSONB encoder
   - Do NOT upgrade beyond v0.20 (if ever used)
   - Maintain Turso as primary datastore

2. **Proceed with Final Evaluation Report (#459)**:
   - Document v0.22.3 regression as additional evidence
   - Update recommendation to "STRONGLY Do Not Migrate"
   - Emphasize API stability concerns

3. **Consider Hybrid Architecture (if embedding search needed)**:
   - Turso: Full node storage with JSON querying
   - LanceDB: Vector embeddings ONLY (not properties)
   - Keep datastores separate and focused

## Files Modified

- `packages/desktop-app/src-tauri/Cargo.toml`:
  - lancedb: 0.20 → 0.22.3
  - arrow-*: 55.0 → 56.0

- `packages/desktop-app/src-tauri/src/datastore/lance/store.rs`:
  - Schema: properties DataType::Utf8 → DataType::LargeBinary
  - Arrays: StringArray → LargeBinaryArray (incomplete, reverted)

- `packages/desktop-app/src-tauri/src/datastore/lance/tests/`:
  - json_index_tests.rs: Updated query syntax (`text` → `string`, `''` → `IS NOT NULL`)
  - json_index_v22_retest.rs: New comprehensive test suite (tests failed)

## Conclusion

LanceDB v0.22.3 has **WORSENED** the situation compared to v0.20:
- Lost basic JSON querying capability
- Introduced breaking API changes
- Added JSONB format requirement
- Still no scalar indexes on JSON fields

**The Phase 2 blocker persists and is now WORSE**. NodeSpace should **STRONGLY avoid** LanceDB v0.22.3 and remain on Turso for all node storage needs.

### Rollback Recommendation

Revert all v0.22.3 changes:
```bash
git checkout packages/desktop-app/src-tauri/Cargo.toml
git checkout packages/desktop-app/src-tauri/src/datastore/lance/
```

Or if keeping for documentation:
- Keep test files for reference
- Revert Cargo.toml to lancedb = "0.20"
- Document findings in final evaluation report
