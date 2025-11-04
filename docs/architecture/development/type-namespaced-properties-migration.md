# Type-Namespaced Properties Migration (Issue #397)

## Overview

As of November 2025, NodeSpace uses type-namespaced property structure to preserve properties during type conversions. This architectural change enables users to convert nodes between types (e.g., task → text → task) without losing type-specific property data.

## Problem Statement

**Old Behavior**: When a user converted a task node to a text node and back to a task node, all task-specific properties (status, priority, due date, etc.) were lost because the properties were stored in a flat structure at the root level.

```json
// Old flat format (deprecated)
{
  "id": "node-123",
  "node_type": "task",
  "properties": {
    "status": "IN_PROGRESS",
    "priority": 3,
    "due_date": "2025-01-15"
  }
}

// After converting to text and back to task:
{
  "id": "node-123",
  "node_type": "task",
  "properties": {} // ❌ All properties lost!
}
```

**New Behavior**: Properties are preserved under type-specific namespaces, allowing safe type conversions without data loss.

## Property Structure Changes

### Old Format (Deprecated)

```json
{
  "status": "IN_PROGRESS",
  "priority": 3,
  "due_date": "2025-01-15"
}
```

**Problems**:
- Properties lost on type conversion
- Single namespace forced single-type properties
- No way to preserve task data when temporarily converting to text

### New Format (Current)

```json
{
  "task": {
    "status": "IN_PROGRESS",
    "priority": 3,
    "due_date": "2025-01-15"
  }
}
```

**Benefits**:
- ✅ Properties preserved when converting task → text → task
- ✅ Multiple type properties can coexist in same node
- ✅ Cleaner separation of concerns
- ✅ Better schema validation isolation
- ✅ Future-proof for multi-type nodes

### Example: Multi-Type Properties

```json
{
  "task": {
    "status": "DONE",
    "priority": 1
  },
  "person": {
    "email": "user@example.com",
    "role": "contributor"
  }
}
```

This node could be both a task AND a person reference simultaneously (future feature).

## Implementation Details

### Backend (Rust)

**File**: `packages/core/src/behaviors/mod.rs`

The `TaskNodeBehavior` validation now supports both formats during transition:

```rust
// Try new nested format first, fall back to old flat format
let task_props = node.properties.get("task").or(Some(&node.properties));

// Validate properties in either format
if let Some(props) = task_props {
    if let Some(status) = props.get("status") {
        status.as_str().ok_or_else(|| {
            NodeValidationError::InvalidProperties("Status must be a string".to_string())
        })?;
    }
}
```

**Default Metadata** (new nodes):

```rust
fn default_metadata(&self) -> serde_json::Value {
    serde_json::json!({
        "task": {
            "status": "pending",
            "priority": 2,
            "due_date": null,
            "assignee_id": null
        }
    })
}
```

### Frontend (TypeScript/Svelte)

**File**: `packages/desktop-app/src/lib/components/property-forms/schema-property-form.svelte`

**Reading Properties** (backward compatible):

```typescript
function getPropertyValue(fieldName: string): unknown {
  if (!node) return undefined;

  // Try new nested format first: properties[nodeType][fieldName]
  const typeNamespace = node.properties?.[nodeType];
  if (typeNamespace && typeof typeNamespace === 'object' && fieldName in typeNamespace) {
    return (typeNamespace as Record<string, unknown>)[fieldName];
  }

  // Fall back to old flat format: properties[fieldName]
  return node.properties?.[fieldName];
}
```

**Writing Properties** (auto-migration):

```typescript
function updateProperty(fieldName: string, value: unknown) {
  const typeNamespace = node.properties?.[nodeType];
  const isOldFormat = !typeNamespace || typeof typeNamespace !== 'object';

  let migratedNamespace: Record<string, unknown> = {};

  if (isOldFormat) {
    // AUTO-MIGRATE: Convert all existing flat properties to nested format
    schema.fields.forEach((field) => {
      const oldValue = node.properties?.[field.name];
      if (oldValue !== undefined) {
        migratedNamespace[field.name] = oldValue;
      }
    });
  } else {
    migratedNamespace = { ...(typeNamespace as Record<string, unknown>) };
  }

  // Apply the update
  migratedNamespace[fieldName] = value;

  const updatedProperties = {
    ...node.properties,
    [nodeType]: migratedNamespace
  };

  // Remove old flat properties after migration
  if (isOldFormat) {
    schema.fields.forEach((field) => {
      delete updatedProperties[field.name];
    });
  }

  // Save to database
  sharedNodeStore.updateNode(nodeId, { properties: updatedProperties });
}
```

## Backward Compatibility

The system supports **both formats** during transition:

### Backend Validation
- ✅ Accepts `properties.status` (old flat format)
- ✅ Accepts `properties.task.status` (new nested format)
- ✅ Validates type correctness (string vs number)
- ✅ Schema system will validate enum values (future)

### Frontend Access
- ✅ `getPropertyValue()` reads both formats
- ✅ `updateProperty()` writes new nested format
- ✅ Auto-migrates on first edit (one-time conversion)
- ✅ Removes old flat properties after migration

### Migration Strategy

**Automatic Migration**: Properties auto-migrate on first edit

1. User edits a task property (e.g., changes status)
2. Frontend detects old flat format
3. All properties migrate to nested format atomically
4. Old flat properties are removed
5. Update saved to database

**No Manual Migration Required**: Old format continues to work indefinitely until first edit.

## Testing

### Backend Tests

**File**: `packages/core/src/behaviors/mod.rs`

```rust
#[test]
fn test_type_conversion_preserves_properties() {
    let behavior = TaskNodeBehavior;

    // Create task with nested properties
    let mut task_node = Node::new(
        "task".to_string(),
        "Important task".to_string(),
        None,
        json!({
            "task": {
                "status": "in_progress",
                "priority": 3,
                "due_date": "2025-01-15"
            }
        }),
    );

    // Convert to text
    task_node.node_type = "text".to_string();

    // Properties still exist (not deleted)
    assert_eq!(task_node.properties["task"]["status"], "in_progress");

    // Convert back to task
    task_node.node_type = "task".to_string();

    // Properties validate correctly
    assert!(behavior.validate(&task_node).is_ok());
    assert_eq!(task_node.properties["task"]["status"], "in_progress");
}
```

### Test Coverage

- ✅ Old flat format validation
- ✅ New nested format validation
- ✅ Type conversion property preservation
- ✅ Backward compatibility (both formats work)
- ✅ Auto-migration on first write
- ✅ Default metadata uses nested format

## For Developers

### Creating New Nodes

Always use the nested format for new nodes:

```typescript
// ❌ OLD (deprecated) - will be auto-migrated on first edit
const newNode = {
  node_type: 'task',
  properties: {
    status: 'OPEN',
    priority: 1
  }
};

// ✅ NEW (correct) - use type-namespaced format
const newNode = {
  node_type: 'task',
  properties: {
    task: {
      status: 'OPEN',
      priority: 1
    }
  }
};
```

### Adding New Node Types

When implementing a new node type, use type-namespaced properties from the start:

```rust
// In your node behavior's default_metadata()
fn default_metadata(&self) -> serde_json::Value {
    serde_json::json!({
        "my_type": {  // Use your node type as namespace
            "field1": "default_value",
            "field2": null
        }
    })
}
```

### Accessing Properties

Always use the `getPropertyValue()` helper function (or equivalent) to access properties:

```typescript
// ❌ BAD - only works with one format
const status = node.properties.status;

// ✅ GOOD - works with both formats (backward compatible)
const status = getPropertyValue('status');
```

## Database Schema Changes

No database schema changes were required. Properties are stored as JSON, so the structure change is transparent to the database:

```sql
-- No changes to this schema
CREATE TABLE nodes (
  id TEXT PRIMARY KEY,
  node_type TEXT NOT NULL,
  content TEXT NOT NULL,
  properties JSON NOT NULL DEFAULT '{}',  -- Stores both formats
  created_at TEXT NOT NULL,
  modified_at TEXT NOT NULL
);
```

## Future Enhancements

### Phase 2: Manual Migration Utility (Planned)

For users who want to migrate all nodes at once:

```bash
# Future command (not yet implemented)
bun run migrate:properties

# Output:
# Migrating 1,247 nodes from flat to nested format...
# ✓ Migrated 823 task nodes
# ✓ Migrated 156 person nodes
# ✓ Migrated 268 project nodes
# ✓ Skipped 0 nodes (already in new format)
# Migration complete!
```

### Phase 3: Remove Backward Compatibility (Future)

After 6-12 months and confirmation that all nodes are migrated:
1. Remove fallback logic from `getPropertyValue()`
2. Remove backward compatibility from backend validation
3. Simplify code to only support nested format
4. Update documentation to remove "deprecated" warnings

## Related Issues

- **Issue #397**: Type-Namespaced Properties (this feature)
- **Issue #392**: Schema System Enum Validation (related)
- **PR #399**: Implementation

## References

- [Storage Architecture](../data/storage-architecture.md)
- [How to Add New Node Type](how-to-add-new-node-type.md)
- [Schema Management Guide](schema-management-implementation-guide.md)

---

**Last Updated**: 2025-11-04
**Status**: ✅ Implemented and Merged
**Backward Compatibility**: Maintained (both formats supported)
