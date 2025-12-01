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

## Current State Analysis

### What Works Today

NodeSpace has two existing patterns for relating nodes:

#### 1. Composition via Record Links (Spoke Table Fields)
- **Implementation**: `record` field type in `SchemaField`
- **Storage**: Property stored in spoke table
- **Example**: `task.assignee: option<record>`
- **Queries**: Fast direct access via spoke table
- **Use case**: Single unidirectional reference without metadata

#### 2. Structural Graph Edges (Hardcoded Relations)
- **Implementation**: Defined in `schema.surql`
- **Storage**: Dedicated edge tables with IN/OUT constraints
- **Examples**:
  - `has_child` - Hierarchy with ordering and OCC
  - `mentions` - Cross-references with context and offset
- **Queries**: Bidirectional graph traversal
- **Use case**: Core structural relationships with edge metadata

### What Doesn't Work

- **No automatic edge table generation** from schema definitions
- **No schema-driven bidirectional relationships**
- **No support for declaring relations with edge properties**
- **No migration path** from simple `record` to complex `relation`

## Architecture Decisions

### Decision 1: Composition vs. Relationship - Decision Tree

Use this decision tree to determine the appropriate pattern:

```
START: Does this reference need edge metadata (timestamps, context, properties)?
  ├─ NO → Does it need bidirectional queries?
  │   ├─ NO → Use COMPOSITION (record field in spoke table)
  │   └─ YES → Use RELATIONSHIP (graph edge with minimal metadata)
  └─ YES → Use RELATIONSHIP (graph edge with properties)

COMPOSITION PATTERN (Record Field):
  - 1-to-1 or 1-to-many from owner's perspective
  - Queries primarily in one direction
  - No edge metadata needed
  - Fast direct access required
  - Example: task.assignee → person

RELATIONSHIP PATTERN (Graph Edge):
  - Many-to-many relationships
  - Bidirectional query requirements
  - Edge has properties (assigned_at, role, context)
  - Structural relationship (hierarchy, mentions)
  - Example: person ←assigned_to→ task (with assigned_at, assigned_by)
```

### Decision 2: Relationships Stored Separately from Fields

**Recommendation**: Add `relationships` array to schema spoke structure, separate from `fields`

**Rationale:**
- Relationships create edge tables (different from spoke fields)
- Clearer separation of concerns
- Easier for NLP to parse and understand entity relationships
- Retrieval can use abstract graph traversal methods (no dynamic SQL building)

#### Option A: Enhanced `record` Type (Simple References)

Use existing `record` type for composition:

```json
{
  "name": "assignee",
  "type": "record",
  "targetType": "person",
  "indexed": true,
  "required": false,
  "description": "Person assigned to this task"
}
```

**Generated DDL:**
```sql
DEFINE FIELD assignee ON TABLE task TYPE option<record>;
DEFINE INDEX idx_task_assignee ON TABLE task COLUMNS assignee;
```

**Advantages:**
- ✅ Simple, fast queries: `SELECT * FROM task WHERE assignee = person:alice`
- ✅ Minimal overhead - just a field in the spoke table
- ✅ Works with existing `SchemaTableManager` code
- ✅ Natural for 1-to-1 ownership patterns

**Limitations:**
- ❌ No automatic bidirectional support
- ❌ No edge metadata
- ❌ Reverse queries require manual indexing

#### Option B: New `relation` Type (Graph Edges)

Add new `relation` field type for complex relationships:

```json
{
  "name": "assigned_to",
  "type": "relation",
  "targetType": "task",
  "direction": "out",
  "edgeTable": "person_assigned_to_task",
  "bidirectional": true,
  "edgeFields": [
    {
      "name": "assigned_at",
      "type": "datetime",
      "indexed": true,
      "required": true,
      "default": "time::now()"
    },
    {
      "name": "assigned_by",
      "type": "record",
      "targetType": "person",
      "indexed": false
    },
    {
      "name": "role",
      "type": "string",
      "indexed": true
    }
  ],
  "indexed": true
}
```

**Generated DDL:**
```sql
-- Edge table definition
DEFINE TABLE person_assigned_to_task SCHEMAFULL TYPE RELATION IN person OUT task;

-- Edge properties
DEFINE FIELD assigned_at ON TABLE person_assigned_to_task TYPE datetime DEFAULT time::now();
DEFINE FIELD assigned_by ON TABLE person_assigned_to_task TYPE option<record>;
DEFINE FIELD role ON TABLE person_assigned_to_task TYPE option<string>;

-- Indexes for bidirectional queries
DEFINE INDEX idx_assigned_to_in ON TABLE person_assigned_to_task COLUMNS in;
DEFINE INDEX idx_assigned_to_out ON TABLE person_assigned_to_task COLUMNS out;
DEFINE INDEX idx_assigned_to_unique ON TABLE person_assigned_to_task COLUMNS in, out UNIQUE;
DEFINE INDEX idx_assigned_to_assigned_at ON TABLE person_assigned_to_task COLUMNS assigned_at;
DEFINE INDEX idx_assigned_to_role ON TABLE person_assigned_to_task COLUMNS role;
```

**Advantages:**
- ✅ Bidirectional queries: "tasks assigned to Alice" AND "who is assigned to this task"
- ✅ Edge metadata: assigned_at, assigned_by, role
- ✅ Many-to-many relationships naturally supported
- ✅ Queryable edge properties: "assignments created last week"

**Limitations:**
- ❌ More complex DDL generation
- ❌ Requires edge table cleanup on deletion
- ❌ Slightly slower than direct record field access

### Decision 3: Atomic Schema + Relationship Creation

**Requirement**: When a custom node schema is created/updated, both the spoke table AND edge tables must be created atomically in a single transaction.

**Mandatory Relationships**: If a relationship is marked as `required: true`, validation ensures:
- Target node type schema exists
- Edge must exist when creating/updating nodes of this type

**Example:**
```json
{
  "id": "invoice",
  "relationships": [
    {
      "name": "billed_to",
      "targetType": "customer",
      "required": true,  // Cannot create invoice without customer
      "cardinality": "one"
    }
  ]
}
```

### Decision 4: Bidirectional Relationship Auto-Sync

**Recommendation**: Auto-add reverse relationship metadata to target schema

When `invoice` defines `billed_to -> customer`, automatically update `customer` schema to include reverse relationship metadata:

```json
{
  "id": "customer",
  "relationships": [
    {
      "name": "invoices",
      "targetType": "invoice",
      "direction": "in",
      "cardinality": "many",
      "reverseOf": "billed_to",  // Links back to source relationship
      "autoGenerated": true       // Flag for UI/NLP
    }
  ]
}
```

**Benefits:**
- ✅ NLP can discover relationships from either side
- ✅ Schema introspection shows complete graph
- ✅ No extra storage cost (just metadata)
- ✅ Queries work in both directions

### Decision 5: Edge Table Generation Strategy

For relationships, generate edge tables automatically:

#### Naming Convention

**Pattern**: `{source_type}_{field_name}_{target_type}`

**Examples**:
- `person_assigned_to_task`
- `invoice_has_line_item_line_item`
- `document_authored_by_person`

**Validation Rules**:
- All lowercase with underscores
- Must be unique across all schemas
- Alphanumeric characters and underscores only

#### DDL Structure Template

```sql
-- Edge table (RELATION type enforces graph semantics)
DEFINE TABLE {edge_table_name} SCHEMAFULL TYPE RELATION IN {source_type} OUT {target_type};

-- Core tracking fields (always generated)
DEFINE FIELD created_at ON TABLE {edge_table_name} TYPE datetime DEFAULT time::now();
DEFINE FIELD version ON TABLE {edge_table_name} TYPE int DEFAULT 1;

-- User-defined edge fields (from edgeFields array)
{for each edge_field}
  DEFINE FIELD {field.name} ON TABLE {edge_table_name} TYPE {mapped_type};
{end for}

-- Core indexes (always generated)
DEFINE INDEX idx_{edge_table_name}_in ON TABLE {edge_table_name} COLUMNS in;
DEFINE INDEX idx_{edge_table_name}_out ON TABLE {edge_table_name} COLUMNS out;
DEFINE INDEX idx_{edge_table_name}_unique ON TABLE {edge_table_name} COLUMNS in, out UNIQUE;

-- User-defined indexes (from edgeFields with indexed: true)
{for each indexed edge_field}
  DEFINE INDEX idx_{edge_table_name}_{field_name} ON TABLE {edge_table_name} COLUMNS {field_name};
{end for}
```

#### Generation Timing

**When to generate edge tables:**

1. **Schema Creation**: Edge table generated atomically with schema node and spoke table
2. **Schema Update**: Edge table updated when relation field added/modified
3. **Field Deletion**: Edge table removed when relation field deleted (with confirmation)

**Atomic Transaction Pattern:**

```rust
// When creating/updating schema with relation fields
db.query("BEGIN TRANSACTION;").await?;

// 1. Update schema node in hub table
db.query("UPDATE node:task SET ...").await?;

// 2. Update spoke table for schema
db.query("UPDATE schema:task SET fields = ...").await?;

// 3. Generate/update edge tables for relation fields
for relation_field in relation_fields {
    let ddl = generate_edge_table_ddl(type_name, relation_field)?;
    for statement in ddl {
        db.query(&statement).await?;
    }
}

db.query("COMMIT TRANSACTION;").await?;
```

#### Cleanup Strategy

**When relation field is deleted:**

1. **Confirm with user** (data loss warning)
2. **Delete edge table**: `REMOVE TABLE {edge_table_name};`
3. **All edges deleted** automatically by SurrealDB
4. **No orphaned data** (foreign key constraints prevent dangling references)

**When node is deleted:**

- **Option 1 (Default)**: Cascade delete edges
  ```sql
  -- Automatically handled by SurrealDB for RELATION tables
  DELETE node:task-123;  -- Also deletes all has_child and mentions edges
  ```

- **Option 2 (Configurable)**: Keep orphaned edges (useful for audit trails)
  ```json
  {
    "type": "relation",
    "onSourceDelete": "keep_orphaned",
    "onTargetDelete": "cascade"
  }
  ```

### Decision 4: Existing Structural Relations

**Recommendation**: Keep `has_child` and `mentions` as structural (defined in `schema.surql`)

#### Rationale

1. **Core to NodeSpace architecture** - These are not user-extensible
2. **Performance critical** - Hierarchy traversal happens constantly
3. **Special semantics** - `has_child.order` uses fractional indexing, custom OCC
4. **UI dependencies** - Components hardcode these relation names

#### Future Consideration

If users request custom hierarchy types (e.g., "alternative view", "timeline order"):
- Create new schema-driven relation: `custom_hierarchy`
- Keep `has_child` as primary structural relation
- UI can optionally render custom hierarchies

### Decision 5: Schema Field Type Extensions

Extend `SchemaField` to support `relation` type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,  // "string" | "number" | "record" | "relation"
    pub protection: SchemaProtectionLevel,

    // ... existing fields ...

    // NEW: Relation-specific fields
    /// Target node type for record and relation fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,

    /// Direction for relation fields: "in" | "out" | "both"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,

    /// Generated edge table name (auto-computed if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_table: Option<String>,

    /// Whether this relation supports bidirectional queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidirectional: Option<bool>,

    /// Properties stored on the edge itself (relation type only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_fields: Option<Vec<SchemaField>>,

    /// Cascade behavior on deletion: "cascade" | "keep_orphaned"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<String>,
}
```

## Implementation Approach

### Phase 1: Documentation & Design ✅ (This Document)

- [x] Document composition vs. relationship decision tree
- [x] Define when to use `record` vs. `relation`
- [x] Specify edge table naming conventions
- [x] Design `relation` field schema structure

### Phase 2: Schema Type Extension (Future)

**Not implemented yet - requires separate issue**

- [ ] Extend `SchemaField` with relation-specific fields
- [ ] Add validation for relation field definitions
- [ ] Support `targetType` for type safety on `record` fields
- [ ] Implement edge table name generation logic

### Phase 3: DDL Generation (Future)

**Not implemented yet - requires separate issue**

- [ ] Extend `SchemaTableManager.generate_ddl_statements()` to handle `relation` type
- [ ] Generate edge table DDL from relation field definitions
- [ ] Generate edge field DDL from `edgeFields` array
- [ ] Generate bidirectional indexes automatically

### Phase 4: CRUD Operations (Future)

**Not implemented yet - requires separate issue**

- [ ] Extend `NodeService` to create/delete edge records
- [ ] Support querying relations in both directions
- [ ] Implement cascade delete behavior
- [ ] Add transaction support for atomic edge + node updates

### Phase 5: Migration Path (Future)

**Not implemented yet - requires separate issue**

- [ ] Document how to migrate `record` → `relation`
- [ ] Provide data migration utilities
- [ ] Support gradual rollout for large datasets

## Use Case Examples

### Example 1: Task Assignment (Start with `record`, migrate to `relation` if needed)

**Initial Simple Approach (Composition):**

```json
{
  "id": "task",
  "fields": [
    {
      "name": "assignee",
      "type": "record",
      "targetType": "person",
      "indexed": true,
      "required": false
    }
  ]
}
```

**Queries:**
```sql
-- Get task's assignee (fast, direct)
SELECT *, assignee.* FROM task:daily-standup;

-- Find tasks assigned to Alice (indexed)
SELECT * FROM task WHERE assignee = person:alice;

-- Find who Alice is assigned to (reverse query - slower, requires full scan)
SELECT * FROM task WHERE assignee = person:alice;  -- Same as above
```

**When to Migrate to `relation`:**
- Need to track "assigned_at" timestamp
- Need to track "assigned_by" (who made the assignment)
- Need efficient "all assignments for this person" queries
- Support multiple assignees per task

**Migrated Complex Approach (Relationship):**

```json
{
  "id": "person",
  "fields": [
    {
      "name": "assigned_to",
      "type": "relation",
      "targetType": "task",
      "direction": "out",
      "bidirectional": true,
      "edgeFields": [
        {
          "name": "assigned_at",
          "type": "datetime",
          "default": "time::now()",
          "indexed": true
        },
        {
          "name": "assigned_by",
          "type": "record",
          "targetType": "person"
        },
        {
          "name": "role",
          "type": "enum",
          "coreValues": [
            {"value": "owner", "label": "Owner"},
            {"value": "collaborator", "label": "Collaborator"}
          ]
        }
      ]
    }
  ]
}
```

**Queries:**
```sql
-- All tasks assigned to Alice (bidirectional, fast)
SELECT ->assigned_to->task.* FROM person:alice;

-- Who is assigned to this task (reverse direction, fast)
SELECT <-assigned_to<-person.* FROM task:daily-standup;

-- Assignments created last week with edge metadata
SELECT in, out, assigned_at, assigned_by, role
FROM person_assigned_to_task
WHERE assigned_at > time::now() - 7d;

-- Tasks where Alice is the owner (query edge property)
SELECT out.* FROM person_assigned_to_task
WHERE in = person:alice AND role = 'owner';
```

### Example 2: Invoice Line Items (Composition is sufficient)

**Schema:**

```json
{
  "id": "line_item",
  "fields": [
    {
      "name": "invoice",
      "type": "record",
      "targetType": "invoice",
      "indexed": true,
      "required": true
    },
    {
      "name": "product",
      "type": "record",
      "targetType": "product",
      "indexed": true
    },
    {
      "name": "quantity",
      "type": "number",
      "required": true
    },
    {
      "name": "price",
      "type": "number",
      "required": true
    }
  ]
}
```

**Why Composition?**
- Line items "belong to" invoices (ownership)
- Queries are primarily "get line items for invoice" (one direction)
- No need for "invoices containing this product" reverse queries
- Quantity and price are properties of the line item itself, not the relationship

**Queries:**
```sql
-- Get all line items for an invoice (fast, indexed)
SELECT * FROM line_item WHERE invoice = invoice:INV-001;

-- Total invoice amount (aggregate over owned items)
SELECT sum(quantity * price) FROM line_item WHERE invoice = invoice:INV-001;

-- Line items for a specific product (possible but secondary use case)
SELECT * FROM line_item WHERE product = product:widget-pro;
```

### Example 3: Document Author (Simple Composition)

**Schema:**

```json
{
  "id": "document",
  "fields": [
    {
      "name": "author",
      "type": "record",
      "targetType": "person",
      "indexed": true,
      "required": true
    },
    {
      "name": "created_at",
      "type": "datetime",
      "default": "time::now()"
    }
  ]
}
```

**Why Not `relation`?**
- Single author per document (1-to-1 from document perspective)
- Primary query: "who authored this document" (stored directly)
- Secondary query: "documents by this person" (indexed, fast enough)
- No edge metadata needed (created_at is document property)

**Queries:**
```sql
-- Get document author (direct access)
SELECT *, author.* FROM document:proposal;

-- All documents by Alice (indexed, efficient)
SELECT * FROM document WHERE author = person:alice;
```

## Summary and Recommendations

### Current State (Issue #703)

**This architecture document establishes:**

✅ **Clear decision tree** for composition vs. relationship
✅ **Hybrid approach** keeping both `record` and defining new `relation` type
✅ **Edge table generation strategy** with naming conventions and DDL templates
✅ **Existing relations remain structural** (`has_child`, `mentions` stay in schema.surql)
✅ **Migration path** from simple `record` to complex `relation`

### Future Implementation (Separate Issues Required)

The following work is **NOT included in Issue #703**:

- ⏳ **Schema field type extensions** (add `targetType`, `direction`, `edgeFields` to `SchemaField`)
- ⏳ **DDL generation for edge tables** (extend `SchemaTableManager`)
- ⏳ **CRUD operations for relations** (extend `NodeService`)
- ⏳ **Migration utilities** (tools to convert `record` → `relation`)

### When to Use Each Pattern

| Use Case | Pattern | Field Type | Example |
|----------|---------|------------|---------|
| Single owner reference | Composition | `record` | document.author |
| Owned collection items | Composition | `record` | line_item.invoice |
| Bidirectional queries | Relationship | `relation` | person ↔ task |
| Edge metadata needed | Relationship | `relation` | assigned_at, role |
| Many-to-many | Relationship | `relation` | tags ↔ documents |
| Structural relations | Hardcoded | N/A | has_child, mentions |

## Acceptance Criteria

- [x] Architecture decision documented: composition vs. relationship
- [x] Schema field type defined for relational fields (both `record` and `relation`)
- [x] DDL generation strategy for edge tables specified
- [x] Decision on existing structural relations (keep in schema.surql)
- [x] Documentation with examples and decision tree
- [ ] Implementation deferred to future issues (not part of #703)

## Related Issues

- #691 - Schema seeding and DDL generation foundation
- #690 - Schema simplification (removed SchemaDefinition)
- #670 - Date node spoke table removal
- #614 - Sibling ordering via has_child edges

## Appendix: SurrealDB Record Link vs RELATE Reference

### Record Links (For Composition)

```sql
-- Hub points to spoke
DEFINE FIELD data ON TABLE node TYPE option<record>;

-- Spoke points back to hub
DEFINE FIELD node ON TABLE task TYPE option<record>;

-- Create with record links
CREATE node:task-1 CONTENT {
  nodeType: 'task',
  data: task:task-1  -- Composition: node "contains" task data
};

CREATE task:task-1 CONTENT {
  node: node:task-1,  -- Reverse link
  status: 'open'
};

-- Query with record links (direct access)
SELECT *, data.status FROM node:task-1;
```

### RELATE Edges (For Relationships)

```sql
-- Edge table with IN/OUT constraints
DEFINE TABLE has_child SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD order ON TABLE has_child TYPE float;

-- Create with RELATE
RELATE node:parent->has_child->node:child CONTENT {
  order: 1.0,
  version: 1
};

-- Query with graph traversal
SELECT ->has_child->node.* FROM node:parent;  -- Children
SELECT <-has_child<-node.* FROM node:child;   -- Parent
```

**Key Difference:**
- **Record Link**: Direct field pointer (composition, ownership)
- **RELATE Edge**: Explicit relationship entity (association, graph)
