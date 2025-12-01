# Schema Support for Relational Fields - Architecture Design

## Executive Summary

This document defines the architecture for supporting **runtime schema-driven relationships** in NodeSpace. Users can define custom node types with relationships via NLP/MCP, and NodeSpace automatically generates the necessary SurrealDB edge tables and provides query capabilities.

**Key Insight:** This is a **runtime schema engine** where:
- Users define schemas via NLP → MCP → SchemaNodes
- SchemaNodes drive DDL generation → SurrealDB tables/edges
- NLP queries → Parse schema metadata → Generate SurrealDB queries dynamically
- No compile-time code generation - all retrieval uses generic, schema-driven methods

**Example Use Cases:**
- `Invoice "is_billed_to" Customer` (with billing_date, payment_terms)
- `Task "is_assigned_to" Person` (with assigned_at, assigned_by, role)
- `Task "is_part_of" Project`

---

## Architecture Decisions

### Decision 1: Edge-Only Storage for Relationships

**Recommendation**: Relationships are stored **only in edge tables** - no spoke table fields.

**Rationale:**
- **Single source of truth** - The edge table IS the relationship
- **Consistency** - Both directions work the same way (query edge table)
- **No sync issues** - No need to keep spoke fields in sync with edges
- **Simpler mental model** - Relationships = edge tables, period

**How it works:**
```sql
-- Create relationship (edge only, no spoke field)
RELATE invoice:INV-001->billed_to->customer:acme CONTENT {
  billing_date: time::now(),
  payment_terms: 'net-30'
};

-- Query from invoice side
SELECT ->billed_to->customer.* FROM invoice:INV-001;

-- Query from customer side (same edge table, reverse direction)
SELECT <-billed_to<-invoice.* FROM customer:acme;
```

**No spoke field like `invoice.customer_id` is created.**

### Decision 2: Relationships Stored Separately from Fields

**Recommendation**: Add `relationships` array to schema spoke structure, separate from `fields`

**Rationale:**
- Relationships create edge tables (different from spoke fields)
- Clearer separation of concerns
- Easier for NLP to parse and understand entity relationships
- Retrieval uses abstract graph traversal methods

**Schema Structure:**
```json
{
  "id": "invoice",
  "nodeType": "schema",
  "content": "Invoice",
  "fields": [
    { "name": "amount", "type": "number", "required": true },
    { "name": "due_date", "type": "date" }
  ],
  "relationships": [
    {
      "name": "billed_to",
      "targetType": "customer",
      "direction": "out",
      "cardinality": "one",
      "required": true,
      "reverseName": "invoices",
      "reverseCardinality": "many",
      "edgeFields": [
        { "name": "billing_date", "type": "date", "required": true },
        { "name": "payment_terms", "type": "string" }
      ]
    }
  ]
}
```

### Decision 3: Computed Reverse Relationship Discovery

**Recommendation**: Do NOT mutate target schemas. Use computed metadata instead.

**Problem with auto-mutating target schemas:**
- Write amplification (updating one schema requires updating another)
- Transaction/locking complexity
- OCC version conflicts
- User confusion ("I never created that relationship!")

**Solution**: Query-time discovery of reverse relationships:

```rust
/// Discover all inbound relationships for a node type
pub async fn get_inbound_relationships(type_name: &str) -> Vec<RelationshipMetadata> {
    // Query ALL schemas for relationships targeting this type
    let query = "SELECT * FROM schema WHERE relationships[*].targetType = $type_name";
    // Return computed list of inbound relationships with reverse names
}
```

**Benefits:**
- No schema mutation cascades
- No version conflicts
- NLP can still discover relationships from either side
- Consistent with "schema-as-metadata" pattern

**Performance:** Cache inbound relationship index in memory, rebuild on schema changes.

### Decision 4: Forward-Looking Schema Changes (Grandfathering)

**Recommendation**: Schema changes apply to **new data only**. Existing data is grandfathered.

This is consistent with how fields already work in NodeSpace (see [Schema Migration Behavior](#schema-migration-behavior) section).

| Change | Historical Data | New Data |
|--------|-----------------|----------|
| Add relationship | N/A (no edges exist) | Must provide if required |
| Remove relationship | Edges preserved (soft-delete) | Can't create new edges |
| Set `required: true` | Null allowed (grandfathered) | Must provide |
| Change cardinality `many→one` | Keep all existing (no enforcement) | Enforce on new |
| Add edge field | Null for historical | Required if marked required |
| Remove edge field | Data preserved (not exposed) | Not stored |

**Example - Making relationship required:**
```rust
// Setting required = true means:
// - Future invoice creation MUST include billed_to relationship
// - Existing invoices with no billed_to edge are fine (grandfathered)
// - If you UPDATE an existing invoice, you must now provide billed_to

schema.relationship("billed_to").set_required(true)
// No backfill needed. No default customer creation. Clean.
```

**Optional backfill for simple types:**
```rust
// For edge fields, user can optionally provide retroactive default
schema.relationship("assigned_to")
    .add_edge_field("role", type: "enum", required: true)
    // Historical edges: role = null (grandfathered by default)

// OR with explicit backfill (opt-in)
schema.relationship("assigned_to")
    .add_edge_field("role", type: "enum", required: true, retroactive_default: "collaborator")
    // Historical edges: role = "collaborator" (backfilled)
```

### Decision 5: Soft-Delete for Relationship Removal

**Recommendation**: When user removes a relationship from schema, preserve the edge table and data.

**Behavior:**
- **Schema node**: Relationship definition removed (source of truth for what's "active")
- **Edge table**: Kept in SurrealDB (data preserved)
- **Existing edges**: Preserved but not exposed in queries
- **Future operations**: Cannot create new edges for removed relationship

This mirrors how fields work - SurrealDB keeps the data (FLEXIBLE tables), but we stop reading/writing it.

**Why soft-delete?**
- No accidental data loss
- User can recover by re-adding relationship definition
- Consistent with existing field behavior
- "Permanent delete" can be a separate explicit operation if needed

### Decision 6: Atomic Schema + Relationship Creation

**Requirement**: Schema node, spoke table, AND edge tables created atomically in single transaction.

NodeSpace already supports this pattern for schema + spoke table creation (see `create_schema_node_atomic` in `surreal_store.rs`). We extend it to include edge table DDL.

```rust
// Atomic transaction includes:
// 1. Schema node in hub table
// 2. Schema spoke record
// 3. Spoke table DDL (for the type this schema defines)
// 4. Edge table DDL (for each relationship)

let mut transaction_parts = vec!["BEGIN TRANSACTION;".to_string()];

// DDL for spoke table
transaction_parts.extend(spoke_ddl_statements);

// DDL for edge tables (NEW)
for relationship in &relationships {
    transaction_parts.extend(generate_edge_table_ddl(type_name, relationship)?);
}

// Create hub node
transaction_parts.push(create_hub_node_sql);

// Create spoke record (includes relationships array)
transaction_parts.push(create_spoke_record_sql);

transaction_parts.push("COMMIT TRANSACTION;".to_string());
```

### Decision 7: Existing Structural Relations Remain Hardcoded

**Recommendation**: Keep `has_child` and `mentions` in `schema.surql` (infrastructure)

**Rationale:**
- Core to NodeSpace architecture - not user-extensible
- Performance critical - hierarchy traversal happens constantly
- Special semantics - `has_child.order` uses fractional indexing, custom OCC
- UI dependencies - components hardcode these relation names

User-defined relationships use the new schema-driven approach. Structural relations are infrastructure.

---

## Schema Migration Behavior

### How Field Changes Work Today

NodeSpace already implements a "soft-delete" pattern for fields:

| Operation | DDL Generated | Data Behavior |
|-----------|---------------|---------------|
| **Add field** | `DEFINE FIELD IF NOT EXISTS` | New field available |
| **Remove field** | No `REMOVE FIELD` generated | Data preserved (FLEXIBLE tables keep it) |
| **Modify field** | `DEFINE FIELD IF NOT EXISTS` | May or may not update |

**Key insight**: We don't explicitly remove fields from SurrealDB when user deletes them from schema. The schema node is the source of truth for what fields are "active", but SurrealDB preserves the data.

### Applying Same Pattern to Relationships

| Operation | DDL Generated | Data Behavior |
|-----------|---------------|---------------|
| **Add relationship** | `DEFINE TABLE IF NOT EXISTS` (edge table) | New edge table available |
| **Remove relationship** | No `REMOVE TABLE` generated | Edge table + edges preserved |
| **Modify relationship** | Regenerate edge table DDL | May add new fields/indexes |

**Consistency**: Fields and relationships follow the same migration pattern.

### Breaking Changes Require Decision Points

For changes that could affect data integrity, the API requires explicit decisions:

```rust
// Example: Changing cardinality from many to one
schema.update_relationship("managers", RelationshipUpdate {
    cardinality: Some("one".to_string()),
    // Decision required: what to do with nodes that have multiple edges?
    on_cardinality_violation: Some(CardinalityResolution::KeepNewest),
})?;

// Options for on_cardinality_violation:
// - KeepNewest: Keep most recent edge, soft-delete others
// - KeepOldest: Keep first edge, soft-delete others
// - Error: Reject the change if violations exist
// - AllowViolations: Change schema but don't enforce on existing data
```

---

## Data Structures

### SchemaRelationship Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRelationship {
    /// Relationship name (e.g., "billed_to", "assigned_to")
    pub name: String,

    /// Target node type (e.g., "customer", "person")
    pub target_type: String,

    /// Direction: "out" (this->target) or "in" (target->this)
    pub direction: String,

    /// Cardinality: "one" or "many"
    pub cardinality: String,

    /// Whether this relationship is required for new nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Suggested reverse relationship name (for NLP discovery, not schema mutation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_name: Option<String>,

    /// Reverse cardinality
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_cardinality: Option<String>,

    /// Auto-computed edge table name (can be overridden)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_table: Option<String>,

    /// Fields stored on the edge itself
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_fields: Option<Vec<EdgeField>>,

    /// Human-readable description (for NLP)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
```

### EdgeField Struct (Simplified)

Edge fields are simpler than schema fields - they're always on the edge table:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeField {
    /// Field name
    pub name: String,

    /// Field type: "string" | "number" | "boolean" | "date" | "record"
    pub field_type: String,

    /// Whether to create an index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed: Option<bool>,

    /// Whether field is required for new edges
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Default value for new edges
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Target type for record fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,

    /// Human-readable description (for NLP)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
```

**Why EdgeField is Simpler than SchemaField:**

1. **No Protection Levels** - Edge fields are always user-defined
   - Core relationships (`has_child`, `mentions`) remain in `schema.surql` as infrastructure
   - User-defined relationships are inherently `protection: "user"`
   - No need for `protection` field on edge fields

2. **No Nested Types** - Edge fields use primitives only
   - Prevents complex nested structures in relationship metadata
   - Keeps edge tables lightweight and performant
   - No `fields` or `item_fields` recursion needed

3. **No Enum Extension** - No `coreValues`/`userValues`/`extensible` fields
   - Edge fields can use simple string type with application-level validation
   - Simpler lifecycle than spoke table enum fields

4. **Tightly Coupled Lifecycle** - Edge fields live/die with relationship
   - Created when relationship created
   - Removed when relationship removed
   - No independent field evolution

---

## Edge Table DDL Generation

### Naming Convention

**Pattern**: `{source_type}_{relationship_name}_{target_type}`

**Examples**:
- `invoice_billed_to_customer`
- `task_assigned_to_person`
- `document_tagged_with_tag`

### Generated DDL Template

```sql
-- Edge table with RELATION type
DEFINE TABLE IF NOT EXISTS {edge_table_name} SCHEMAFULL TYPE RELATION IN {source_type} OUT {target_type};

-- Core tracking fields (always generated)
DEFINE FIELD IF NOT EXISTS created_at ON TABLE {edge_table_name} TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS version ON TABLE {edge_table_name} TYPE int DEFAULT 1;

-- User-defined edge fields
DEFINE FIELD IF NOT EXISTS {field.name} ON TABLE {edge_table_name} TYPE {mapped_type};

-- Core indexes (always generated)
DEFINE INDEX IF NOT EXISTS idx_{edge_table_name}_in ON TABLE {edge_table_name} COLUMNS in;
DEFINE INDEX IF NOT EXISTS idx_{edge_table_name}_out ON TABLE {edge_table_name} COLUMNS out;
DEFINE INDEX IF NOT EXISTS idx_{edge_table_name}_unique ON TABLE {edge_table_name} COLUMNS in, out UNIQUE;

-- User-defined indexes
DEFINE INDEX IF NOT EXISTS idx_{edge_table_name}_{field_name} ON TABLE {edge_table_name} COLUMNS {field_name};
```

---

## Query Patterns

### Creating Relationships

```sql
-- Create edge (no spoke field involved)
RELATE invoice:INV-001->billed_to->customer:acme CONTENT {
  billing_date: time::now(),
  payment_terms: 'net-30'
};
```

### Querying Forward (Source → Target)

```sql
-- Get customer for invoice
SELECT ->billed_to->customer.* FROM invoice:INV-001;

-- Get all tasks assigned to person
SELECT ->assigned_to->task.* FROM person:alice;
```

### Querying Reverse (Target → Source)

```sql
-- Get invoices for customer
SELECT <-billed_to<-invoice.* FROM customer:acme;

-- Get who is assigned to task
SELECT <-assigned_to<-person.* FROM task:daily-standup;
```

### Querying with Edge Properties

```sql
-- Assignments with edge metadata
SELECT in AS person, out AS task, assigned_at, role
FROM task_assigned_to_person
WHERE assigned_at > time::now() - 7d;

-- Tasks where Alice is owner
SELECT out.* FROM task_assigned_to_person
WHERE in = person:alice AND role = 'owner';
```

---

## Implementation Phases

### Phase 1: Data Model Extensions
- [ ] Add `SchemaRelationship` struct to `packages/core/src/models/schema.rs`
- [ ] Add `EdgeField` struct
- [ ] Update `packages/core/src/db/schema.surql` - add `relationships` field to schema table
- [ ] Add serialization/deserialization tests

### Phase 2: DDL Generation for Edge Tables
- [ ] Add `generate_relationship_ddl_statements()` to `SchemaTableManager`
- [ ] Implement edge table name computation
- [ ] Generate edge field DDL
- [ ] Generate bidirectional indexes
- [ ] Add tests for edge table DDL generation

### Phase 3: Schema CRUD API
- [ ] Extend `NodeService` to handle schemas with relationships
- [ ] Atomic transaction: schema node + spoke + edge tables
- [ ] Add relationship validation (target schema exists if required)
- [ ] Add tests for schema CRUD with relationships

### Phase 4: Relationship CRUD API
- [ ] Add `create_relationship(source_id, relationship_name, target_id, edge_data)`
- [ ] Add `delete_relationship(source_id, relationship_name, target_id)`
- [ ] Add `get_related_nodes(node_id, relationship_name, direction)`
- [ ] Add tests for relationship CRUD

### Phase 5: NLP Discovery API
- [ ] Add `get_schema_with_relationships(schema_id)` helper
- [ ] Add `get_inbound_relationships(node_type)` computed lookup
- [ ] Cache relationship metadata for fast NLP access
- [ ] Add documentation for NLP integration

### Phase 6: MCP Handler Integration
- [ ] Extend MCP schema handlers to support relationships
- [ ] Add relationship CRUD handlers
- [ ] Add integration tests

---

## Implementation Details

### Cardinality Enforcement

Cardinality constraints are enforced at **application level** when creating relationships:

**For `cardinality: "one"`:**
```rust
// When creating a relationship with cardinality="one"
pub async fn create_relationship(
    source_id: &str,
    relationship_name: &str,
    target_id: &str,
    edge_data: Value,
) -> Result<(), NodeServiceError> {
    // Get schema and find relationship definition
    let schema = self.get_schema_for_node(source_id).await?;
    let relationship = schema.find_relationship(relationship_name)?;

    // For cardinality="one", check if edge already exists
    if relationship.cardinality == "one" {
        let existing_edges = self.query_edges(source_id, relationship_name).await?;
        if !existing_edges.is_empty() {
            return Err(NodeServiceError::CardinalityViolation(format!(
                "Relationship '{}' has cardinality 'one' but edge already exists",
                relationship_name
            )));
        }
    }

    // Create edge using RELATE
    let edge_table = compute_edge_table_name(&schema.id, relationship);
    db.query(&format!(
        "RELATE {}->{}->{}",
        source_id, edge_table, target_id
    )).await?;
}
```

**Database-level uniqueness** is enforced via `UNIQUE` index on `(in, out)`:
```sql
DEFINE INDEX idx_{edge_table}_unique ON TABLE {edge_table} COLUMNS in, out UNIQUE;
```

This prevents duplicate edges (same source + target), but doesn't prevent multiple targets for `cardinality="one"`. Application logic handles cardinality.

### Relationship Name Validation

**Valid characters**: Alphanumeric, underscores, hyphens
- Pattern: `^[a-zA-Z][a-zA-Z0-9_-]*$`
- Must start with letter
- Case-sensitive (stored exactly as defined)

**Reserved names** (cannot be used):
- `has_child` - Structural hierarchy relation
- `mentions` - Structural reference relation
- `node` - Reserved for hub-spoke link
- `data` - Reserved for hub-spoke link

**Validation timing**: During schema node validation (in `SchemaNodeBehavior`)

```rust
fn validate_relationship_name(name: &str) -> Result<(), ValidationError> {
    // Check reserved names
    let reserved = ["has_child", "mentions", "node", "data"];
    if reserved.contains(&name) {
        return Err(ValidationError::ReservedName(name.to_string()));
    }

    // Check pattern
    let pattern = Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]*$").unwrap();
    if !pattern.is_match(name) {
        return Err(ValidationError::InvalidName(format!(
            "Relationship name '{}' must start with letter and contain only alphanumeric, underscores, hyphens",
            name
        )));
    }

    Ok(())
}
```

### Target Schema Validation Timing

**Validation happens at two points:**

1. **Schema Creation/Update** (Lenient for forward references):
   ```rust
   // When creating schema with relationships
   // - Do NOT require target schema to exist yet (allow forward references)
   // - Allows creating Invoice schema before Customer schema exists
   // - Validation deferred to relationship creation time
   ```

2. **Relationship Creation** (Strict):
   ```rust
   // When creating actual relationship edge
   pub async fn create_relationship(...) -> Result<(), NodeServiceError> {
       // 1. Validate source node exists
       let source = self.get_node(source_id).await?
           .ok_or(NodeServiceError::NodeNotFound)?;

       // 2. Validate target schema exists
       let target_schema_exists = self.schema_exists(&relationship.target_type).await?;
       if !target_schema_exists {
           return Err(NodeServiceError::TargetSchemaNotFound(
               relationship.target_type.clone()
           ));
       }

       // 3. Validate target node exists
       let target = self.get_node(target_id).await?
           .ok_or(NodeServiceError::NodeNotFound)?;

       // 4. Validate target node type matches
       if target.node_type != relationship.target_type {
           return Err(NodeServiceError::TargetTypeMismatch);
       }

       // All validations pass - create edge
   }
   ```

### Edge Table Name Collision Handling

**Pattern**: `{source_type}_{relationship_name}_{target_type}`

**Collision scenarios:**

1. **Same source, same name, same target** - No collision (same relationship redefined)
   ```
   invoice_billed_to_customer (defined by invoice schema)
   invoice_billed_to_customer (redefined by invoice schema update)
   → Same table, DDL uses IF NOT EXISTS
   ```

2. **Different source, same name, same target** - No collision (different edge tables)
   ```
   invoice_billed_to_customer (invoice → customer)
   quote_billed_to_customer   (quote → customer)
   → Different tables, different relationships
   ```

3. **Same source, different name, same target** - No collision (different relationships)
   ```
   invoice_billed_to_customer   (invoice.billed_to → customer)
   invoice_shipped_to_customer  (invoice.shipped_to → customer)
   → Different tables, different purposes
   ```

**Edge table names are unique by design** - the triplet `(source, relationship_name, target)` uniquely identifies each relationship.

**Validation during schema creation:**
```rust
fn validate_edge_table_uniqueness(
    source_type: &str,
    relationship: &SchemaRelationship,
) -> Result<(), NodeServiceError> {
    let edge_table = compute_edge_table_name(source_type, relationship);

    // Check if table already exists for DIFFERENT relationship
    // (Same relationship updating is OK - uses IF NOT EXISTS)
    let existing = self.get_edge_table_owner(&edge_table).await?;

    if let Some((existing_source, existing_rel_name)) = existing {
        if existing_source != source_type || existing_rel_name != relationship.name {
            return Err(NodeServiceError::EdgeTableCollision(format!(
                "Edge table '{}' already used by {}.{}",
                edge_table, existing_source, existing_rel_name
            )));
        }
    }

    Ok(())
}
```

### Inbound Relationship Caching Strategy

**Goal**: Fast NLP discovery of "what points to this node type" without scanning all schemas.

**Implementation:**

```rust
pub struct InboundRelationshipCache {
    /// Map: target_type → Vec<(source_type, relationship_name, metadata)>
    cache: Arc<RwLock<HashMap<String, Vec<InboundRelationship>>>>,

    /// Last cache refresh time
    last_refresh: Arc<RwLock<Instant>>,
}

#[derive(Clone)]
pub struct InboundRelationship {
    pub source_type: String,
    pub relationship_name: String,
    pub reverse_name: Option<String>,
    pub cardinality: String,
    pub edge_table: String,
}

impl InboundRelationshipCache {
    /// Get all relationships pointing TO a node type
    pub async fn get_inbound_relationships(
        &self,
        target_type: &str,
    ) -> Result<Vec<InboundRelationship>, Error> {
        // Check if cache is stale
        if self.needs_refresh().await {
            self.refresh_cache().await?;
        }

        // Return from cache (fast!)
        Ok(self.cache.read().await
            .get(target_type)
            .cloned()
            .unwrap_or_default())
    }

    async fn refresh_cache(&self) -> Result<(), Error> {
        // Query ALL schemas once
        let all_schemas: Vec<SchemaNode> = self.db
            .query("SELECT * FROM schema")
            .await?;

        // Build index of inbound relationships
        let mut index: HashMap<String, Vec<InboundRelationship>> = HashMap::new();

        for schema in all_schemas {
            for relationship in schema.relationships {
                let inbound = InboundRelationship {
                    source_type: schema.id.clone(),
                    relationship_name: relationship.name.clone(),
                    reverse_name: relationship.reverse_name.clone(),
                    cardinality: relationship.cardinality.clone(),
                    edge_table: relationship.edge_table
                        .unwrap_or_else(|| compute_edge_table_name(&schema.id, &relationship)),
                };

                index.entry(relationship.target_type.clone())
                    .or_default()
                    .push(inbound);
            }
        }

        // Atomic swap
        *self.cache.write().await = index;
        *self.last_refresh.write().await = Instant::now();

        Ok(())
    }

    fn needs_refresh(&self) -> bool {
        // Refresh if stale (>60s) OR if schema change event detected
        let stale = self.last_refresh.read().await.elapsed() > Duration::from_secs(60);
        let schema_changed = self.schema_change_flag.load(Ordering::Relaxed);

        stale || schema_changed
    }
}
```

**Cache Invalidation:**
- **Event-driven**: When schema node created/updated/deleted, set `schema_change_flag`
- **Time-based**: Max 60 seconds stale
- **On-demand**: First query after schema change triggers refresh

**Performance:**
- **Cache hit**: <1µs (HashMap lookup)
- **Cache refresh**: 50-200ms (loads all schemas once)
- **Memory overhead**: ~10KB per 100 schemas

---

## Related Documents

- [Schema Management Implementation Guide](../development/schema-management-implementation-guide.md) - How schemas work today
- [SurrealDB Schema Design](./surrealdb-schema-design.md) - Hub-spoke architecture
- [Node Behavior System](../business-logic/node-behavior-system.md) - Validation architecture

## Related Issues

- #703 - This architecture design
- #691 - Schema seeding and DDL generation foundation
- #690 - Schema simplification (removed SchemaDefinition)
