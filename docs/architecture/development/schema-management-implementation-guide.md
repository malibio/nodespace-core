# Schema Management Implementation Guide

**Version**: 2.0
**Last Updated**: 2025-12-01
**Status**: Implemented (Issue #690)

---

## Overview

NodeSpace stores schemas as regular nodes with `node_type = 'schema'`. Schema management uses the same generic CRUD operations as all other node types, with validation enforced through the `SchemaNodeBehavior` trait.

### Key Architecture Principles

- **Schema-as-Node**: Schemas are stored as regular nodes (no separate schema service)
- **Generic CRUD**: Use standard `create_node`, `update_node`, `delete_node` operations
- **Behavior-Based Validation**: `SchemaNodeBehavior` validates schema structure
- **Atomic DDL Sync**: Schema changes automatically sync corresponding SurrealDB table definitions
- **Protection Levels**: Core/user/system fields with appropriate access controls

---

## Schema Node Structure

```json
{
  "id": "task",
  "nodeType": "schema",
  "content": "Task",
  "isCore": true,
  "schemaVersion": 1,
  "description": "Task tracking schema",
  "fields": [
    {
      "name": "status",
      "type": "enum",
      "protection": "core",
      "coreValues": [
        { "value": "open", "label": "Open" },
        { "value": "in_progress", "label": "In Progress" },
        { "value": "done", "label": "Done" }
      ],
      "userValues": [],
      "extensible": true,
      "indexed": true,
      "required": true,
      "default": "open"
    }
  ]
}
```

### EnumValue Structure

Enum options use a `{ value, label }` structure for human-readable display:

```rust
pub struct EnumValue {
    pub value: String,  // Stored in database (e.g., "in_progress")
    pub label: String,  // Display label (e.g., "In Progress")
}
```

This provides three levels of description:
1. **Schema level**: `description` field on the schema
2. **Field level**: `description` field on each field
3. **Enum option level**: `label` field on each EnumValue

---

## Protection Levels

| Level | Modifiable | Deletable | Use Case |
|-------|-----------|-----------|----------|
| `core` | No | No | Foundation fields UI depends on |
| `user` | Yes | Yes | User-added customizations |
| `system` | No | No | Auto-managed internal fields |

### Distinguishing User vs System Definitions

The `is_core` flag indicates whether a schema, field, or enum value was defined by the system or the user:

- **`is_core: true`**: System-defined (shipped with NodeSpace)
- **`is_core: false`**: User-defined (created by user or MCP client)

**No namespace prefixes are required** for user properties. Anything not defined by the core system is inherently user-defined.

---

## Validation (SchemaNodeBehavior)

Schema validation is handled by `SchemaNodeBehavior` in `packages/core/src/behaviors/mod.rs`:

### Validation Rules

1. **Non-empty content**: Schema must have a name
2. **Unique field names**: No duplicate field names allowed
3. **Valid field name characters**: Alphanumeric and underscores only
4. **Enum fields require values**: Must have at least one value in `coreValues` or `userValues`
5. **Nested field validation**: Object and array fields validate their nested structure

```rust
impl NodeBehavior for SchemaNodeBehavior {
    fn validate(&self, node: &Node) -> Result<(), NodeValidationError> {
        // 1. Basic validation - non-empty content
        if is_empty_or_whitespace(&node.content) {
            return Err(NodeValidationError::MissingField(
                "Schema nodes must have content (schema name)".to_string(),
            ));
        }

        // 2. Parse and validate fields
        let schema = SchemaNode::from_node(node.clone())?;
        let fields = schema.fields();

        // 3. Check field uniqueness
        let mut seen_names = HashSet::new();
        for field in &fields {
            if !seen_names.insert(&field.name) {
                return Err(NodeValidationError::DuplicateField(field.name.clone()));
            }
            validate_schema_field(field)?;
        }

        Ok(())
    }
}
```

---

## Schema Operations via Generic CRUD

### Creating a Schema

```rust
// Via MCP or NodeService
let schema_node = create_node(CreateNodeParams {
    node_type: "schema".to_string(),
    id: Some("project".to_string()),  // Convention: id = type name
    content: "Project".to_string(),
    properties: json!({
        "isCore": false,
        "version": 1,
        "description": "Project management",
        "fields": [
            {
                "name": "status",
                "type": "enum",
                "protection": "user",
                "coreValues": [
                    { "value": "planning", "label": "Planning" },
                    { "value": "active", "label": "Active" }
                ],
                "indexed": true
            }
        ]
    }),
});
```

### Updating a Schema (Adding Fields)

```rust
// Fetch existing schema
let schema = get_node("task")?;
let mut props = schema.properties.clone();

// Add new field to fields array
let fields = props["fields"].as_array_mut().unwrap();
fields.push(json!({
    "name": "priority",
    "type": "enum",
    "protection": "user",
    "coreValues": [
        { "value": "low", "label": "Low" },
        { "value": "medium", "label": "Medium" },
        { "value": "high", "label": "High" }
    ],
    "default": "medium"
}));

// Increment version
props["version"] = json!(props["version"].as_u64().unwrap() + 1);

// Update via generic handler
update_node(UpdateNodeParams {
    id: "task".to_string(),
    properties: Some(props),
    ..Default::default()
});
```

### Extending Enum Values

Users can add custom values to extensible enum fields via `userValues`:

```rust
// Add "blocked" status to task schema
let field = schema.get_field("status").unwrap();
let mut user_values = field.user_values.clone().unwrap_or_default();
user_values.push(EnumValue {
    value: "blocked".to_string(),
    label: "Blocked".to_string(),
});

// Update field and save schema...
```

---

## DDL Synchronization (SchemaTableManager)

When schema nodes are created or updated, `SchemaTableManager` automatically generates corresponding SurrealDB table definitions:

```rust
pub struct SchemaTableManager {
    db: Arc<SurrealStore>,
}

impl SchemaTableManager {
    /// Generate DDL statements for a schema
    pub fn generate_ddl_statements(&self, schema: &SchemaNode) -> Vec<String> {
        let mut statements = Vec::new();
        let table_name = schema.schema_id();

        for field in schema.fields() {
            match field.field_type.as_str() {
                "enum" => {
                    // Generate ASSERT for enum validation
                    let values = self.get_all_enum_values(&field);
                    statements.push(format!(
                        "DEFINE FIELD {} ON {} ASSERT $value IN [{}]",
                        field.name, table_name,
                        values.iter().map(|v| format!("'{}'", v.value)).join(", ")
                    ));
                }
                // ... other field types
            }
        }

        statements
    }
}
```

---

## Type-Safe Wrappers

### SchemaNode (Rust)

`packages/core/src/models/schema_node.rs` provides type-safe access to schema properties:

```rust
let schema = SchemaNode::from_node(node)?;

// Type-safe property access
println!("Schema: {}", schema.name());
println!("Version: {}", schema.version());
println!("Core: {}", schema.is_core());

// Field operations
let status_field = schema.get_field("status");
let enum_values = schema.get_enum_values("status");  // Returns Vec<EnumValue>
let value_strings = schema.get_enum_value_strings("status");  // Returns Vec<String>

// Protection checks
if schema.can_delete_field("priority") {
    schema.remove_field("priority");
}
```

### SchemaNode TypeScript Interface

`packages/desktop-app/src/lib/types/schema-node.ts`:

```typescript
export interface EnumValue {
  value: string;
  label: string;
}

export interface SchemaField {
  name: string;
  type: string;
  protection: ProtectionLevel;
  coreValues?: EnumValue[];
  userValues?: EnumValue[];
  indexed: boolean;
  required?: boolean;
  extensible?: boolean;
  default?: unknown;
  description?: string;
}

export interface SchemaNode {
  id: string;
  nodeType: 'schema';
  content: string;
  isCore: boolean;
  schemaVersion: number;
  description: string;
  fields: SchemaField[];
}
```

---

## MCP Integration

Schema nodes are managed through the standard MCP node handlers:

- `create_node` - Create new schemas
- `update_node` - Modify schemas (with validation)
- `delete_node` - Delete user schemas (core schemas protected)
- `get_node` - Retrieve schema definitions
- `query_nodes` - List schemas (filter by `nodeType: "schema"`)

### Natural Language Schema Creation

The `natural_language_schema` MCP resource allows AI clients to create schemas from descriptions:

```json
{
  "description": "Create a schema for tracking recipes with name, ingredients, and cooking time"
}
```

This generates appropriate field types, labels, and defaults automatically.

---

## Testing

### Unit Tests

- `packages/core/src/models/schema_node.rs` - SchemaNode wrapper tests
- `packages/core/src/behaviors/mod.rs` - SchemaNodeBehavior validation tests

### Key Test Cases

1. **Field uniqueness validation**
2. **Enum requires values validation**
3. **Nested field validation**
4. **Protection level enforcement**
5. **EnumValue label handling**

---

## Migration from Previous Architecture

Issue #690 simplified the schema architecture:

| Before | After |
|--------|-------|
| `SchemaService` | Deleted - use generic CRUD |
| `SchemaDefinition` struct | Deleted - use `SchemaNode` wrapper |
| Namespace prefix enforcement | Removed - use `is_core` flag |
| Separate schema MCP handlers | Deleted - use generic handlers |
| `Vec<String>` for enum values | `Vec<EnumValue>` with labels |

---

## References

- [Node Behavior System](../business-logic/node-behavior-system.md) - Validation architecture
- [Schema Types](../../../packages/core/src/models/schema.rs) - Rust type definitions
- [SchemaNode Wrapper](../../../packages/core/src/models/schema_node.rs) - Type-safe access
- [SchemaNodeBehavior](../../../packages/core/src/behaviors/mod.rs) - Validation implementation
