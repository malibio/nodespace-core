# SurrealDB Deserialization Patterns and Expert Insights

This document captures expert guidance on properly handling Rust enums and complex data structures in SurrealDB, learned during Issue #562 resolution.

## Problem: "invalid type: enum, expected any valid JSON value"

### Root Cause
The error occurs **only when deserializing to generic `serde_json::Value`**. The SurrealDB SDK works perfectly when targeting strongly typed structs.

```rust
// ❌ This fails with enum deserialization errors
let result: Vec<serde_json::Value> = db.select("schema_def").await?;

// ✅ This works perfectly
let result: Vec<SchemaDefinition> = db.select("schema_def").await?;
```

Why? `serde_json::Value` has no concept of Rust enums (only JSON primitives: string, number, bool, null, array, object). When the SDK tries to pass a Rust enum variant to serde_json::Value, it fails because Value's Deserializer explicitly rejects visit_enum calls.

## Correct Pattern: Strong Typing Deserialization

### Write Side (Storage)
Use normal Serde serialization with proper enum attributes:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionLevel {
    Core,
    User,
    System,
}

#[derive(Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub protection: ProtectionLevel,  // Stores as "core", "user", "system"
}

#[derive(Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub fields: Vec<SchemaField>,
}

// Store natively (don't stringify!)
let doc = SchemaDefinition { fields: vec![...] };
db.create("schema_def").content(&doc).await?;
```

Data stored in SurrealDB:
```json
{
  "fields": [
    { "name": "id", "protection": "core" },
    { "name": "bio", "protection": "user" }
  ]
}
```

### Read Side (Retrieval)
**Always deserialize to the typed struct:**

```rust
// ✅ CORRECT - Struct knows how to handle enums
let schemas: Vec<SchemaDefinition> = db.select("schema_def").await?;

// Now access your data with type safety:
for schema in schemas {
    for field in schema.fields {
        match field.protection {
            ProtectionLevel::Core => { /* ... */ },
            ProtectionLevel::User => { /* ... */ },
            ProtectionLevel::System => { /* ... */ },
        }
    }
}
```

## Fallback Patterns (Only if Generic Data Required)

### Option 1: Use SurrealDB's Native Value Type
If you need generic/dynamic data that can hold Rust-specific types:

```rust
use surrealdb::sql::Value;

let results: Vec<Value> = db.select("schema_def").await?;
// Value supports Record IDs, Datetimes, Geometries, and custom types
// Can convert to JSON string if needed: value.to_string()
```

### Option 2: Force JSON Conversion at Query Time
If you absolutely need `serde_json::Value` (e.g., for frontend APIs):

```rust
// Cast the data to JSON in the query
let sql = "SELECT <json> fields FROM schema_def";
let results: Vec<serde_json::Value> = db.query(sql).await?.take(0)?;
// Now fields are forced to pure JSON strings, no enums
```

## SurrealDB Field Type Definitions

For storing arrays of complex objects with enums:

```sql
-- Define the array container
DEFINE FIELD fields ON TABLE schema_def TYPE array<object>;

-- Define nested field constraints
DEFINE FIELD fields.*.name ON TABLE schema_def TYPE string;
DEFINE FIELD fields.*.field_type ON TABLE schema_def TYPE string;

-- Use String Literals for enum enforcement (SurrealDB v2.0+)
-- This acts exactly like a Rust enum, rejecting any string not in the list
DEFINE FIELD fields.*.protection ON TABLE schema_def TYPE "core" | "user" | "system";
```

**Why not FLEXIBLE?** Use FLEXIBLE only if you want to allow data that violates the schema. For strict type enforcement, use standard (non-FLEXIBLE) mode.

## Related SurrealDB Issues

### Issue #4921: Record Link Deserialization
- **Status**: Resolved in SDK v2.2.0
- **Solution**: Use `surrealdb::sql::Thing` instead of `serde_json::Value` for record links
- **Example**:
  ```rust
  // ✅ Correct
  pub struct Node {
      pub id: Thing,
      pub data: Thing,
  }

  // ❌ Wrong - can't deserialize Thing to serde_json::Value
  pub data: serde_json::Value,
  ```

### Issue #5794: Enum Deserialization in Binary Protocol
- **Status**: Handled correctly when using strong typing
- **Key**: The SDK binary protocol preserves type information correctly; the issue is only at the serde boundary when targeting generic Value types

## Transaction Atomicity for Multi-Step Operations

When performing multiple writes that must be atomic (CREATE hub, CREATE spoke, UPDATE hub):

```rust
let mut response = db.query(r#"
    BEGIN TRANSACTION;

    -- 1. Create Hub Record
    CREATE ONLY node:invoice CONTENT {
        created_at: time::now(),
        node_type: "schema"
    };

    -- 2. Create Spoke Record (CRITICAL: Must commit before SELECT)
    CREATE ONLY type::thing('schema', 'invoice') CONTENT {
        is_core: false,
        version: 1,
        fields: [...]
    };

    -- 3. Update Hub Record with Link
    UPDATE node:invoice SET data = type::thing('schema', 'invoice');

    -- 4. Final commit guarantees all writes are persisted
    COMMIT TRANSACTION;

    -- Optional: Return created record to verify
    SELECT * FROM type::thing('schema', 'invoice');
"#).await?;

// Consume response to ensure execution
let final_record: Option<Value> = response.take(0)?;
```

## Datetime Serialization

**Issue**: SurrealDB expects native datetime types, not RFC3339 strings

```rust
use chrono::Utc;

// ❌ Don't: Send RFC3339 strings
.bind(("created_at", created_at.to_rfc3339()))

// ✅ Do: Send Unix timestamps and convert in query
.bind(("created_at", created_at.timestamp()))
// In query: time::from::unix($timestamp)
```

## Key Principles

1. **Strong Typing is Not Optional**: Always deserialize to typed structs when enum fields are involved
2. **serde_json::Value is for Simple JSON**: Only use for basic structures without Rust-specific types
3. **Trust the SDK**: When properly typed, the SDK handles deserialization correctly
4. **Document Data Formats**: Clarify in code comments whether data is stored as string or structured
5. **Test with Real Types**: Don't test deserialization with generic Value; use actual structs
6. **Use Native Types**: Let SurrealDB store data in its native format, not as stringified JSON

## NodeSpace Application

In NodeSpace, this pattern applies to:
- **Schema definitions**: Store as `array<object>` with proper enum serialization
- **Node properties**: Deserialize using typed `Node` and `SchemaDefinition` structs
- **Markdown export**: Queryable arrays enable proper hierarchy traversal
- **MCP handlers**: Use typed deserialization for schema operations

## References

- SurrealDB Rust SDK: https://github.com/surrealdb/surrealdb.rs
- Serde Documentation: https://serde.rs/
- Issue #562: Fix SurrealDB schema serialization
- Commit 956ed95: Remove stringification workaround for native array storage
