# Schema Management Implementation Guide

**Version**: 1.0
**Last Updated**: 2025-01-02
**Status**: Implementation Ready
**Related Issue**: [#106 - Implement Schema Management Service](https://github.com/malibio/nodespace-core/issues/106)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Architecture Overview](#architecture-overview)
3. [7-Phase Implementation Plan](#7-phase-implementation-plan)
4. [Sub-Agent Delegation Guide](#sub-agent-delegation-guide)
5. [Commit Message Templates](#commit-message-templates)
6. [Common Pitfalls & Solutions](#common-pitfalls--solutions)
7. [Testing Strategy](#testing-strategy)
8. [References](#references)

---

## Executive Summary

### Purpose

This guide provides detailed, step-by-step instructions for implementing NodeSpace's schema management system with lazy migration support. It's designed to be followed by junior engineers or AI agents working incrementally across multiple sessions.

### What This Guide Covers

- **Schema versioning infrastructure** - Track which schema version validated each node
- **SchemaService** - CRUD operations with protection level enforcement
- **Migration registry** - Transform functions for schema evolution
- **Lazy migration** - Automatic node upgrades on access
- **MCP integration** - Both generic and specialized schema tools
- **Frontend wrapper** - TypeScript service for UI (future)

### When to Use This Guide

- Implementing schema management for the first time (#106)
- Adding migration capabilities to NodeSpace
- Modifying schema evolution behavior
- Understanding schema protection enforcement

### Key Architectural Principles

✅ **Pure JSON Architecture** - No ALTER TABLE, no SQL migrations
✅ **Lazy Migration** - Nodes upgrade on first access after schema change
✅ **Version Tracking** - All nodes track their schema version
✅ **Protection Enforcement** - Core fields cannot be broken
✅ **Desktop-Safe** - No bulk operations, distributed load

---

## Architecture Overview

### Schema-as-Node Pattern

NodeSpace stores schemas as regular nodes with `node_type = 'schema'`:

```json
{
  "id": "task",              // Convention: id = type_name
  "node_type": "schema",
  "content": "Task",
  "properties": {
    "is_core": true,
    "version": 2,            // Schema version (increments on changes)
    "description": "Task tracking schema",
    "fields": [
      {
        "name": "status",
        "type": "enum",
        "protection": "core",        // Cannot delete/modify
        "core_values": ["OPEN", "IN_PROGRESS", "DONE"],
        "user_values": ["BLOCKED"],  // User can add/remove
        "extensible": true,
        "indexed": true,
        "required": true,
        "default": "OPEN"
      }
    ]
  }
}
```

### Node Instance Versioning

Each node instance tracks which schema version validated it:

```json
{
  "id": "task-123",
  "node_type": "task",
  "content": "Implement schema migration",
  "properties": {
    "_schema_version": 2,    // Tracks schema version
    "status": "IN_PROGRESS",
    "due_date": "2025-01-15"
  }
}
```

### Lazy Migration Flow

```
User/Agent requests node
        ↓
NodeService.get_node(id)
        ↓
Check: node._schema_version < schema.version?
        ↓ YES
Apply migration transforms (v1→v2, v2→v3, etc.)
        ↓
Update node with new properties + version
        ↓
Persist to database
        ↓
Return migrated node
```

### Protection Levels

| Level | Modifiable | Deletable | Use Case |
|-------|-----------|-----------|----------|
| `core` | ❌ No | ❌ No | Foundation fields UI depends on |
| `user` | ✅ Yes | ✅ Yes | User-added customizations |
| `system` | ❌ No | ❌ No | Auto-managed internal fields |

### Why Lazy Migration?

**Desktop Application Reality**:
- 10,000+ user installations
- Cannot coordinate bulk migrations
- No downtime windows
- Catastrophic failure = lost user data

**Lazy Migration Benefits**:
- ✅ No bulk operations (desktop-safe)
- ✅ Distributed load (users migrate as they access)
- ✅ Backward compatible (old nodes keep working)
- ✅ Testable (each migration is pure function)
- ✅ Rollback-friendly (keep old parsers indefinitely)

---

## 7-Phase Implementation Plan

### Phase Overview

| Phase | Component | Complexity | Estimated Time | Sub-Agent |
|-------|-----------|------------|----------------|-----------|
| 1 | Version Tracking | Low | 2-4 hours | General Purpose |
| 2 | SchemaService Core | Medium | 4-6 hours | Senior Architect |
| 3 | Migration Registry | Medium | 3-5 hours | Senior Architect |
| 4 | Lazy Integration | High | 4-6 hours | Senior Architect |
| 5 | MCP Generic Validation | Low | 2-3 hours | AI/ML Engineer |
| 6 | MCP Specialized Tools | Medium | 3-4 hours | AI/ML Engineer |
| 7 | Frontend Wrapper | Low | 2-3 hours | Frontend Architect |

**Total Estimated Time**: 20-31 hours across 7 incremental commits

---

## Phase 1: Version Tracking Infrastructure

### Goal

Add `_schema_version` field to all node instances to track which schema version validated them.

### Files to Modify

- `packages/core/src/services/node_service.rs` (lines ~150-200, ~300-350)
- `packages/core/src/models/node.rs` (if type changes needed)

### Implementation Steps

#### Step 1.1: Add Version to Node Creation

**Location**: `node_service.rs` in `create_node()` method

```rust
// In create_node() method
pub async fn create_node(&self, params: CreateNodeParams) -> Result<Node, ServiceError> {
    // ... existing validation ...

    // NEW: Add schema version tracking
    let mut properties = params.properties.unwrap_or_else(|| json!({}));

    // Get current schema version for this node type
    if let Some(schema) = self.get_schema_for_type(&params.node_type).await? {
        if let Some(obj) = properties.as_object_mut() {
            obj.insert(
                "_schema_version".to_string(),
                json!(schema.version)
            );
        }
    } else {
        // No schema defined - default to version 1
        if let Some(obj) = properties.as_object_mut() {
            obj.insert("_schema_version".to_string(), json!(1));
        }
    }

    // ... rest of create logic ...
}
```

#### Step 1.2: Add Schema Lookup Helper

```rust
// Add new helper method to NodeService
impl NodeService {
    /// Get schema definition for a given node type
    async fn get_schema_for_type(&self, node_type: &str) -> Result<Option<SchemaDefinition>, ServiceError> {
        // Schema lookup convention: id = type_name, node_type = "schema"
        let schema_node = self.get_node(node_type).await?;

        if let Some(node) = schema_node {
            if node.node_type == "schema" {
                let schema: SchemaDefinition = serde_json::from_value(node.properties)?;
                return Ok(Some(schema));
            }
        }

        Ok(None)
    }
}
```

#### Step 1.3: Backfill Existing Nodes

**Location**: `node_service.rs` in `get_node()` method

```rust
// In get_node() method - add backfill logic
pub async fn get_node(&self, id: &str) -> Result<Option<Node>, ServiceError> {
    let mut node = self.db.get_node(id).await?;

    if let Some(ref mut node) = node {
        // NEW: Backfill nodes without version
        if let Some(props) = node.properties.as_object() {
            if !props.contains_key("_schema_version") {
                // No version - backfill with v1
                if let Some(obj) = node.properties.as_object_mut() {
                    obj.insert("_schema_version".to_string(), json!(1));

                    // Persist the backfilled version
                    self.db.update_node(node).await?;
                }
            }
        }
    }

    Ok(node)
}
```

### Testing Requirements

**Unit Tests** (`tests/unit/node_service_test.rs`):

```rust
#[tokio::test]
async fn test_new_nodes_get_schema_version() {
    let service = setup_test_service().await;

    // Create a node
    let node = service.create_node(CreateNodeParams {
        node_type: "task".to_string(),
        content: "Test task".to_string(),
        properties: Some(json!({
            "status": "OPEN"
        })),
        ..Default::default()
    }).await.unwrap();

    // Verify _schema_version was added
    assert!(node.properties["_schema_version"].is_number());
    assert_eq!(node.properties["_schema_version"], 1); // Current task schema version
}

#[tokio::test]
async fn test_existing_nodes_backfilled_with_version() {
    let service = setup_test_service().await;

    // Manually create node without version (simulating old data)
    let node_id = service.db.execute(
        "INSERT INTO nodes (id, node_type, content, properties) VALUES (?, ?, ?, ?)",
        params!["test-node", "task", "Old task", json!({"status": "OPEN"})]
    ).await.unwrap();

    // Fetch the node (should trigger backfill)
    let node = service.get_node("test-node").await.unwrap().unwrap();

    // Verify version was backfilled
    assert_eq!(node.properties["_schema_version"], 1);
}
```

### Verification Steps

```bash
# 1. Run tests
bun run test:db -- node_service_test

# 2. Manual verification (in dev console or test script)
# Create a new task
let task = await createNode({
  nodeType: "task",
  content: "Test",
  properties: { status: "OPEN" }
});

console.log(task.properties._schema_version); // Should output: 1

# 3. Check existing nodes get backfilled
let existingTask = await getNode("some-existing-task-id");
console.log(existingTask.properties._schema_version); // Should output: 1
```

### Exit Criteria

- [x] All new nodes automatically get `_schema_version` field
- [x] Existing nodes get backfilled with version 1 on first access
- [x] Version number matches current schema version
- [x] Tests pass
- [x] No breaking changes to existing functionality

### Common Errors

**Error**: "Cannot insert _schema_version into non-object properties"
- **Cause**: `properties` is null or not a JSON object
- **Fix**: Initialize as empty object before inserting version

**Error**: "Schema not found for node type"
- **Cause**: Schema node doesn't exist yet
- **Fix**: Default to version 1 if schema lookup fails

---

## Phase 2: SchemaService Core Operations

### Goal

Create `SchemaService` with CRUD operations and protection level enforcement.

### Files to Create

- `packages/core/src/services/schema_service.rs` (new file)
- `packages/core/src/services/mod.rs` (add schema_service export)
- `packages/core/src/models/schema.rs` (new file - schema types)

### Implementation Steps

#### Step 2.1: Define Schema Types

**Location**: `packages/core/src/models/schema.rs` (new file)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionLevel {
    Core,   // Cannot modify/delete
    User,   // Fully modifiable
    System, // Auto-managed, read-only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub protection: ProtectionLevel,

    // Enum-specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_values: Option<Vec<String>>,

    pub indexed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>, // For array types
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub is_core: bool,
    pub version: u32,
    pub description: String,
    pub fields: Vec<SchemaField>,
}

impl SchemaDefinition {
    /// Get all enum values (core + user)
    pub fn get_enum_values(&self, field_name: &str) -> Option<Vec<String>> {
        let field = self.fields.iter().find(|f| f.name == field_name)?;

        let mut values = Vec::new();
        if let Some(core_vals) = &field.core_values {
            values.extend(core_vals.clone());
        }
        if let Some(user_vals) = &field.user_values {
            values.extend(user_vals.clone());
        }

        Some(values)
    }

    /// Check if a field can be deleted
    pub fn can_delete_field(&self, field_name: &str) -> bool {
        if let Some(field) = self.fields.iter().find(|f| f.name == field_name) {
            field.protection == ProtectionLevel::User
        } else {
            false
        }
    }
}
```

#### Step 2.2: Create SchemaService

**Location**: `packages/core/src/services/schema_service.rs` (new file)

```rust
use crate::models::node::Node;
use crate::models::schema::{SchemaDefinition, SchemaField, ProtectionLevel};
use crate::services::node_service::NodeService;
use anyhow::{anyhow, Result};
use serde_json::json;
use std::sync::Arc;

pub struct SchemaService {
    node_service: Arc<NodeService>,
}

impl SchemaService {
    pub fn new(node_service: Arc<NodeService>) -> Self {
        Self { node_service }
    }

    /// Get schema definition by schema ID (convention: id = type_name)
    pub async fn get_schema(&self, schema_id: &str) -> Result<SchemaDefinition> {
        let node = self.node_service
            .get_node(schema_id)
            .await?
            .ok_or_else(|| anyhow!("Schema '{}' not found", schema_id))?;

        if node.node_type != "schema" {
            return Err(anyhow!("Node '{}' is not a schema (type: {})", schema_id, node.node_type));
        }

        let schema: SchemaDefinition = serde_json::from_value(node.properties)?;
        Ok(schema)
    }

    /// Add a new field to a schema (only user-protected fields allowed)
    pub async fn add_field(&self, schema_id: &str, field: SchemaField) -> Result<()> {
        // Protection: Only user fields can be added
        if field.protection != ProtectionLevel::User {
            return Err(anyhow!(
                "Can only add user-protected fields. Field '{}' has protection: {:?}",
                field.name, field.protection
            ));
        }

        let mut schema = self.get_schema(schema_id).await?;

        // Check if field already exists
        if schema.fields.iter().any(|f| f.name == field.name) {
            return Err(anyhow!("Field '{}' already exists in schema '{}'", field.name, schema_id));
        }

        // Add field and bump version
        schema.fields.push(field);
        schema.version += 1;

        // Update schema node
        self.update_schema(schema_id, schema).await?;

        Ok(())
    }

    /// Remove a field from schema (only user-protected fields can be removed)
    pub async fn remove_field(&self, schema_id: &str, field_name: &str) -> Result<()> {
        let mut schema = self.get_schema(schema_id).await?;

        // Find the field
        let field_idx = schema.fields.iter()
            .position(|f| f.name == field_name)
            .ok_or_else(|| anyhow!("Field '{}' not found in schema '{}'", field_name, schema_id))?;

        let field = &schema.fields[field_idx];

        // Protection: Only user fields can be removed
        if field.protection != ProtectionLevel::User {
            return Err(anyhow!(
                "Cannot remove field '{}' with protection level {:?}. Only user fields can be removed.",
                field_name, field.protection
            ));
        }

        // Remove field and bump version
        schema.fields.remove(field_idx);
        schema.version += 1;

        // Update schema node
        self.update_schema(schema_id, schema).await?;

        Ok(())
    }

    /// Extend an enum field with a new value (adds to user_values only)
    pub async fn extend_enum_field(&self, schema_id: &str, field_name: &str, new_value: String) -> Result<()> {
        let mut schema = self.get_schema(schema_id).await?;

        // Find the field
        let field = schema.fields.iter_mut()
            .find(|f| f.name == field_name)
            .ok_or_else(|| anyhow!("Field '{}' not found in schema '{}'", field_name, schema_id))?;

        // Verify it's an enum field
        if field.field_type != "enum" {
            return Err(anyhow!("Field '{}' is not an enum (type: {})", field_name, field.field_type));
        }

        // Verify it's extensible
        if !field.extensible.unwrap_or(false) {
            return Err(anyhow!("Enum field '{}' is not extensible", field_name));
        }

        // Check if value already exists (in core or user values)
        let all_values = schema.get_enum_values(field_name).unwrap_or_default();
        if all_values.contains(&new_value) {
            return Err(anyhow!("Value '{}' already exists in enum '{}'", new_value, field_name));
        }

        // Add to user_values
        field.user_values.get_or_insert_with(Vec::new).push(new_value);
        schema.version += 1;

        // Update schema node
        self.update_schema(schema_id, schema).await?;

        Ok(())
    }

    /// Remove a value from enum field (only from user_values, cannot remove core_values)
    pub async fn remove_enum_value(&self, schema_id: &str, field_name: &str, value: &str) -> Result<()> {
        let mut schema = self.get_schema(schema_id).await?;

        // Find the field
        let field = schema.fields.iter_mut()
            .find(|f| f.name == field_name)
            .ok_or_else(|| anyhow!("Field '{}' not found in schema '{}'", field_name, schema_id))?;

        // Check if value is in core_values (cannot remove)
        if let Some(core_vals) = &field.core_values {
            if core_vals.contains(&value.to_string()) {
                return Err(anyhow!(
                    "Cannot remove core value '{}' from enum '{}'. Only user values can be removed.",
                    value, field_name
                ));
            }
        }

        // Remove from user_values
        if let Some(user_vals) = &mut field.user_values {
            if let Some(idx) = user_vals.iter().position(|v| v == value) {
                user_vals.remove(idx);
                schema.version += 1;

                // Update schema node
                self.update_schema(schema_id, schema).await?;
                return Ok(());
            }
        }

        Err(anyhow!("Value '{}' not found in user values of enum '{}'", value, field_name))
    }

    /// Update schema node with new definition
    async fn update_schema(&self, schema_id: &str, schema: SchemaDefinition) -> Result<()> {
        let properties = serde_json::to_value(&schema)?;

        self.node_service.update_node(UpdateNodeParams {
            id: schema_id.to_string(),
            properties: Some(properties),
            ..Default::default()
        }).await?;

        Ok(())
    }
}
```

### Testing Requirements

**Unit Tests** (`tests/unit/schema_service_test.rs`):

```rust
#[tokio::test]
async fn test_add_user_field_succeeds() {
    let service = setup_test_schema_service().await;

    let new_field = SchemaField {
        name: "priority".to_string(),
        field_type: "enum".to_string(),
        protection: ProtectionLevel::User,
        core_values: Some(vec!["LOW".to_string(), "MEDIUM".to_string(), "HIGH".to_string()]),
        user_values: Some(vec![]),
        indexed: true,
        required: Some(false),
        extensible: Some(true),
        default: Some(json!("MEDIUM")),
        description: Some("Task priority level".to_string()),
        item_type: None,
    };

    service.add_field("task", new_field).await.unwrap();

    // Verify field was added
    let schema = service.get_schema("task").await.unwrap();
    assert!(schema.fields.iter().any(|f| f.name == "priority"));
    assert_eq!(schema.version, 2); // Version should be incremented
}

#[tokio::test]
async fn test_add_core_field_fails() {
    let service = setup_test_schema_service().await;

    let core_field = SchemaField {
        name: "forbidden".to_string(),
        field_type: "text".to_string(),
        protection: ProtectionLevel::Core,
        // ... other fields
    };

    let result = service.add_field("task", core_field).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("only add user-protected fields"));
}

#[tokio::test]
async fn test_remove_core_field_fails() {
    let service = setup_test_schema_service().await;

    let result = service.remove_field("task", "status").await; // status is core
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot remove field"));
}

#[tokio::test]
async fn test_extend_enum_succeeds() {
    let service = setup_test_schema_service().await;

    service.extend_enum_field("task", "status", "BLOCKED".to_string()).await.unwrap();

    let schema = service.get_schema("task").await.unwrap();
    let status_field = schema.fields.iter().find(|f| f.name == "status").unwrap();
    assert!(status_field.user_values.as_ref().unwrap().contains(&"BLOCKED".to_string()));
}

#[tokio::test]
async fn test_remove_core_enum_value_fails() {
    let service = setup_test_schema_service().await;

    let result = service.remove_enum_value("task", "status", "OPEN").await; // OPEN is core
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot remove core value"));
}
```

### Exit Criteria

- [x] SchemaService can get/add/remove fields
- [x] Protection levels enforced (cannot modify core/system fields)
- [x] Enum values can be extended (user_values only)
- [x] Core enum values cannot be removed
- [x] Schema version increments on changes
- [x] All tests pass

---

## Phase 3: Migration Registry System

### Goal

Create infrastructure for defining and applying schema migration transforms.

### Files to Create

- `packages/core/src/services/migration_registry.rs` (new file)
- `packages/core/src/services/migrations/task.rs` (example migrations)

### Implementation Steps

#### Step 3.1: Create Migration Registry

**Location**: `packages/core/src/services/migration_registry.rs` (new file)

```rust
use crate::models::node::Node;
use anyhow::Result;
use std::collections::HashMap;

/// A single schema migration transform
pub struct SchemaMigration {
    pub from_version: u32,
    pub to_version: u32,
    pub node_type: String,
    pub transform: Box<dyn Fn(&mut Node) -> Result<()> + Send + Sync>,
}

/// Registry of all schema migrations
pub struct MigrationRegistry {
    migrations: HashMap<String, Vec<SchemaMigration>>,
}

impl MigrationRegistry {
    pub fn new() -> Self {
        Self {
            migrations: HashMap::new(),
        }
    }

    /// Register a migration for a specific node type
    pub fn register_migration(&mut self, migration: SchemaMigration) {
        self.migrations
            .entry(migration.node_type.clone())
            .or_insert_with(Vec::new)
            .push(migration);
    }

    /// Apply all necessary migrations to bring node to target version
    pub fn migrate_node(&self, node: &mut Node, target_version: u32) -> Result<()> {
        let current_version = self.get_node_version(node);

        // Node already at or beyond target version
        if current_version >= target_version {
            return Ok(());
        }

        // Get migrations for this node type
        let migrations = match self.migrations.get(&node.node_type) {
            Some(m) => m,
            None => return Ok(()), // No migrations defined - allow through
        };

        // Apply migrations in sequence
        for migration in migrations {
            if migration.from_version >= current_version &&
               migration.to_version <= target_version {
                (migration.transform)(node)?;
            }
        }

        // Update node's schema version
        if let Some(obj) = node.properties.as_object_mut() {
            obj.insert("_schema_version".to_string(), json!(target_version));
        }

        Ok(())
    }

    /// Check if a node needs migration
    pub fn needs_migration(&self, node: &Node, schema_version: u32) -> bool {
        self.get_node_version(node) < schema_version
    }

    /// Get node's current schema version
    fn get_node_version(&self, node: &Node) -> u32 {
        node.properties.get("_schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32
    }
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

#### Step 3.2: Create Example Migration

**Location**: `packages/core/src/services/migrations/task.rs` (new file)

```rust
use crate::models::node::Node;
use crate::services::migration_registry::SchemaMigration;
use anyhow::Result;
use serde_json::json;

/// Example: Task schema v1 → v2
/// Adds default "priority" field to existing tasks
pub fn task_v1_to_v2() -> SchemaMigration {
    SchemaMigration {
        from_version: 1,
        to_version: 2,
        node_type: "task".to_string(),
        transform: Box::new(|node: &mut Node| {
            // Add priority field if it doesn't exist
            if let Some(obj) = node.properties.as_object_mut() {
                if !obj.contains_key("priority") {
                    obj.insert("priority".to_string(), json!("MEDIUM"));
                }
            }
            Ok(())
        }),
    }
}

/// Example: Task schema v2 → v3
/// Rename "assignee" → "assigned_to" (breaking change)
pub fn task_v2_to_v3() -> SchemaMigration {
    SchemaMigration {
        from_version: 2,
        to_version: 3,
        node_type: "task".to_string(),
        transform: Box::new(|node: &mut Node| {
            if let Some(obj) = node.properties.as_object_mut() {
                if let Some(assignee) = obj.remove("assignee") {
                    obj.insert("assigned_to".to_string(), assignee);
                }
            }
            Ok(())
        }),
    }
}

/// Register all task migrations
pub fn register_task_migrations(registry: &mut MigrationRegistry) {
    registry.register_migration(task_v1_to_v2());
    registry.register_migration(task_v2_to_v3());
}
```

#### Step 3.3: Initialize Registry

**Location**: `packages/core/src/services/mod.rs`

```rust
use crate::services::migration_registry::MigrationRegistry;
use crate::services::migrations::task;

/// Initialize migration registry with all registered migrations
pub fn init_migration_registry() -> MigrationRegistry {
    let mut registry = MigrationRegistry::new();

    // Register migrations for each node type
    task::register_task_migrations(&mut registry);
    // person::register_person_migrations(&mut registry);
    // date::register_date_migrations(&mut registry);
    // ... other types

    registry
}
```

### Testing Requirements

```rust
#[tokio::test]
async fn test_migration_v1_to_v2_adds_priority() {
    let mut registry = MigrationRegistry::new();
    registry.register_migration(task_v1_to_v2());

    let mut node = Node {
        id: "task-1".to_string(),
        node_type: "task".to_string(),
        content: "Old task".to_string(),
        properties: json!({
            "_schema_version": 1,
            "status": "OPEN"
        }),
        ..Default::default()
    };

    registry.migrate_node(&mut node, 2).unwrap();

    assert_eq!(node.properties["priority"], "MEDIUM");
    assert_eq!(node.properties["_schema_version"], 2);
}

#[tokio::test]
async fn test_migration_chain_v1_to_v3() {
    let mut registry = MigrationRegistry::new();
    registry.register_migration(task_v1_to_v2());
    registry.register_migration(task_v2_to_v3());

    let mut node = Node {
        id: "task-1".to_string(),
        node_type: "task".to_string(),
        properties: json!({
            "_schema_version": 1,
            "status": "OPEN",
            "assignee": "john@example.com"
        }),
        ..Default::default()
    };

    registry.migrate_node(&mut node, 3).unwrap();

    // v1→v2: added priority
    assert_eq!(node.properties["priority"], "MEDIUM");
    // v2→v3: renamed assignee→assigned_to
    assert_eq!(node.properties["assigned_to"], "john@example.com");
    assert!(node.properties.get("assignee").is_none());
    assert_eq!(node.properties["_schema_version"], 3);
}
```

### Exit Criteria

- [x] MigrationRegistry can register migrations
- [x] Migrations apply in correct sequence
- [x] Node version updates after migration
- [x] Migration chains work (v1→v2→v3)
- [x] Tests pass

---

## Phase 4: Lazy Migration Integration

### Goal

Hook migration logic into NodeService to automatically upgrade nodes on access.

### Files to Modify

- `packages/core/src/services/node_service.rs`
- Add migration registry to service initialization

### Implementation Steps

#### Step 4.1: Add Migration Registry to NodeService

```rust
pub struct NodeService {
    db: Arc<Database>,
    schema_service: Arc<SchemaService>,
    migration_registry: Arc<MigrationRegistry>,
}

impl NodeService {
    pub fn new(
        db: Arc<Database>,
        schema_service: Arc<SchemaService>,
        migration_registry: Arc<MigrationRegistry>,
    ) -> Self {
        Self {
            db,
            schema_service,
            migration_registry,
        }
    }

    // ... existing methods
}
```

#### Step 4.2: Add Lazy Migration to get_node()

```rust
pub async fn get_node(&self, id: &str) -> Result<Option<Node>, ServiceError> {
    let mut node = self.db.get_node(id).await?;

    if let Some(ref mut node) = node {
        // Backfill version if missing
        if let Some(props) = node.properties.as_object() {
            if !props.contains_key("_schema_version") {
                if let Some(obj) = node.properties.as_object_mut() {
                    obj.insert("_schema_version".to_string(), json!(1));
                }
            }
        }

        // NEW: Lazy migration
        if let Ok(schema) = self.schema_service.get_schema(&node.node_type).await {
            if self.migration_registry.needs_migration(node, schema.version) {
                // Apply migrations
                self.migration_registry.migrate_node(node, schema.version)?;

                // Persist migrated node
                self.db.update_node(node).await?;
            }
        }
    }

    Ok(node)
}
```

#### Step 4.3: Add Lazy Migration to query_nodes()

```rust
pub async fn query_nodes(&self, params: QueryNodeParams) -> Result<Vec<Node>, ServiceError> {
    let mut nodes = self.db.query_nodes(params).await?;

    // NEW: Lazy migrate each node
    for node in &mut nodes {
        if let Ok(schema) = self.schema_service.get_schema(&node.node_type).await {
            if self.migration_registry.needs_migration(node, schema.version) {
                self.migration_registry.migrate_node(node, schema.version)?;
                self.db.update_node(node).await?;
            }
        }
    }

    Ok(nodes)
}
```

### Testing Requirements

```rust
#[tokio::test]
async fn test_lazy_migration_on_get_node() {
    let service = setup_test_service_with_migrations().await;

    // Create old node with v1 schema
    let node_id = create_test_node_v1("task", json!({
        "_schema_version": 1,
        "status": "OPEN"
    })).await;

    // Update schema to v2 (adds priority field)
    update_task_schema_to_v2().await;

    // Fetch node - should trigger lazy migration
    let node = service.get_node(&node_id).await.unwrap().unwrap();

    // Verify migration was applied
    assert_eq!(node.properties["_schema_version"], 2);
    assert_eq!(node.properties["priority"], "MEDIUM");
}

#[tokio::test]
async fn test_no_migration_when_already_current() {
    let service = setup_test_service_with_migrations().await;

    // Create node with current schema version
    let node = create_test_node("task", json!({
        "_schema_version": 2,
        "status": "OPEN",
        "priority": "HIGH"
    })).await;

    let original_priority = node.properties["priority"].clone();

    // Fetch node
    let fetched = service.get_node(&node.id).await.unwrap().unwrap();

    // Verify no migration occurred (priority unchanged)
    assert_eq!(fetched.properties["priority"], original_priority);
}
```

### Exit Criteria

- [x] get_node() applies lazy migration
- [x] query_nodes() applies lazy migration to all results
- [x] Migrations only applied when needed (version check)
- [x] Migrated nodes persisted to database
- [x] Tests pass
- [x] No performance degradation on current-version nodes

---

## Phase 5: MCP Generic Validation

### Goal

Add schema validation to the existing `update_node` MCP handler to protect schemas from invalid modifications.

### Files to Modify

- `packages/core/src/mcp/handlers/nodes.rs`

### Implementation Steps

#### Step 5.1: Add Schema Validation to update_node

```rust
// In handlers/nodes.rs
pub async fn update_node(
    state: Arc<AppState>,
    params: UpdateNodeParams,
) -> Result<Node, McpError> {
    // Get existing node
    let existing_node = state.node_service
        .get_node(&params.id)
        .await?
        .ok_or_else(|| McpError::NotFound(format!("Node '{}' not found", params.id)))?;

    // NEW: Special handling for schema nodes
    if existing_node.node_type == "schema" {
        validate_schema_update(&existing_node, &params, &state.schema_service).await?;
    }

    // Proceed with update
    let updated_node = state.node_service.update_node(params).await?;

    // Emit update event
    emit_node_update_event(&state.tauri_handle, &updated_node).await;

    Ok(updated_node)
}

/// Validate schema modifications
async fn validate_schema_update(
    old_node: &Node,
    params: &UpdateNodeParams,
    schema_service: &SchemaService,
) -> Result<(), McpError> {
    // Parse old and new schemas
    let old_schema: SchemaDefinition = serde_json::from_value(old_node.properties.clone())?;
    let new_schema: SchemaDefinition = serde_json::from_value(
        params.properties.clone().unwrap_or(old_node.properties.clone())
    )?;

    // Protection: Check core fields haven't been deleted or modified
    for old_field in &old_schema.fields {
        if old_field.protection == ProtectionLevel::Core ||
           old_field.protection == ProtectionLevel::System {
            // Find in new schema
            let new_field = new_schema.fields.iter()
                .find(|f| f.name == old_field.name)
                .ok_or_else(|| McpError::ValidationError(
                    format!("Cannot delete {:?} field '{}'", old_field.protection, old_field.name)
                ))?;

            // Verify protection level unchanged
            if new_field.protection != old_field.protection {
                return Err(McpError::ValidationError(
                    format!("Cannot change protection level of field '{}'", old_field.name)
                ));
            }

            // Verify type unchanged
            if new_field.field_type != old_field.field_type {
                return Err(McpError::ValidationError(
                    format!("Cannot change type of {:?} field '{}'", old_field.protection, old_field.name)
                ));
            }

            // For enums, verify core_values unchanged
            if old_field.field_type == "enum" {
                if new_field.core_values != old_field.core_values {
                    return Err(McpError::ValidationError(
                        format!("Cannot modify core_values of enum field '{}'", old_field.name)
                    ));
                }
            }
        }
    }

    Ok(())
}
```

### Testing Requirements

```rust
#[tokio::test]
async fn test_mcp_update_node_rejects_core_field_deletion() {
    let state = setup_test_mcp_state().await;

    // Try to delete core field via update_node
    let result = update_node(
        state,
        UpdateNodeParams {
            id: "task".to_string(),
            properties: Some(json!({
                "is_core": true,
                "version": 1,
                "fields": [] // Deleted all fields including core ones
            })),
            ..Default::default()
        }
    ).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot delete"));
}

#[tokio::test]
async fn test_mcp_update_node_allows_user_field_changes() {
    let state = setup_test_mcp_state().await;

    // Add user field via SchemaService first
    state.schema_service.add_field("task", SchemaField {
        name: "priority".to_string(),
        protection: ProtectionLevel::User,
        // ...
    }).await.unwrap();

    // Modify user field via update_node - should succeed
    let result = update_node(
        state,
        UpdateNodeParams {
            id: "task".to_string(),
            properties: Some(json!({
                "fields": [
                    // ... core fields unchanged
                    {
                        "name": "priority",
                        "type": "enum",
                        "protection": "user",
                        "core_values": ["LOW", "MEDIUM", "HIGH", "URGENT"], // Extended
                        // ...
                    }
                ]
            })),
            ..Default::default()
        }
    ).await;

    assert!(result.is_ok());
}
```

### MCP Tool Description Update

Update the `update_node` tool description to document schema protection:

```rust
Tool {
    name: "update_node",
    description: r#"Update an existing node's properties.

Special behavior for schema nodes (node_type="schema"):
- Cannot delete or modify fields with protection="core" or "system"
- Cannot change protection levels of existing fields
- Cannot modify core_values of enum fields
- Can add/modify fields with protection="user"
- Use specialized schema tools (add_schema_field, etc.) for better UX

Protection will be enforced server-side with clear error messages."#,
    // ... parameters
}
```

### Exit Criteria

- [x] update_node validates schema modifications
- [x] Core field deletion rejected with clear error
- [x] Core field type changes rejected
- [x] Core enum values protected
- [x] User field modifications allowed
- [x] Tool description updated
- [x] Tests pass

---

## Phase 6: MCP Specialized Schema Tools

### Goal

Add specialized MCP tools for schema operations with better UX than generic update_node.

### Files to Create/Modify

- `packages/core/src/mcp/handlers/schema.rs` (new file)
- `packages/core/src/mcp/tools.rs` (register new tools)

### Implementation Steps

#### Step 6.1: Create Schema MCP Handlers

**Location**: `packages/core/src/mcp/handlers/schema.rs` (new file)

```rust
use crate::mcp::error::McpError;
use crate::mcp::state::AppState;
use crate::models::schema::{SchemaField, ProtectionLevel};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AddSchemaFieldParams {
    pub schema_id: String,
    pub field: SchemaField,
}

#[derive(Debug, Serialize)]
pub struct AddSchemaFieldResult {
    pub success: bool,
    pub new_version: u32,
}

pub async fn add_schema_field(
    state: Arc<AppState>,
    params: AddSchemaFieldParams,
) -> Result<AddSchemaFieldResult, McpError> {
    state.schema_service
        .add_field(&params.schema_id, params.field)
        .await
        .map_err(|e| McpError::ValidationError(e.to_string()))?;

    let schema = state.schema_service.get_schema(&params.schema_id).await?;

    Ok(AddSchemaFieldResult {
        success: true,
        new_version: schema.version,
    })
}

#[derive(Debug, Deserialize)]
pub struct RemoveSchemaFieldParams {
    pub schema_id: String,
    pub field_name: String,
}

pub async fn remove_schema_field(
    state: Arc<AppState>,
    params: RemoveSchemaFieldParams,
) -> Result<AddSchemaFieldResult, McpError> {
    state.schema_service
        .remove_field(&params.schema_id, &params.field_name)
        .await
        .map_err(|e| McpError::ValidationError(e.to_string()))?;

    let schema = state.schema_service.get_schema(&params.schema_id).await?;

    Ok(AddSchemaFieldResult {
        success: true,
        new_version: schema.version,
    })
}

#[derive(Debug, Deserialize)]
pub struct ExtendSchemaEnumParams {
    pub schema_id: String,
    pub field_name: String,
    pub new_value: String,
}

pub async fn extend_schema_enum(
    state: Arc<AppState>,
    params: ExtendSchemaEnumParams,
) -> Result<AddSchemaFieldResult, McpError> {
    state.schema_service
        .extend_enum_field(&params.schema_id, &params.field_name, params.new_value)
        .await
        .map_err(|e| McpError::ValidationError(e.to_string()))?;

    let schema = state.schema_service.get_schema(&params.schema_id).await?;

    Ok(AddSchemaFieldResult {
        success: true,
        new_version: schema.version,
    })
}

#[derive(Debug, Deserialize)]
pub struct RemoveSchemaEnumValueParams {
    pub schema_id: String,
    pub field_name: String,
    pub value: String,
}

pub async fn remove_schema_enum_value(
    state: Arc<AppState>,
    params: RemoveSchemaEnumValueParams,
) -> Result<AddSchemaFieldResult, McpError> {
    state.schema_service
        .remove_enum_value(&params.schema_id, &params.field_name, &params.value)
        .await
        .map_err(|e| McpError::ValidationError(e.to_string()))?;

    let schema = state.schema_service.get_schema(&params.schema_id).await?;

    Ok(AddSchemaFieldResult {
        success: true,
        new_version: schema.version,
    })
}

#[derive(Debug, Deserialize)]
pub struct GetSchemaParams {
    pub schema_id: String,
}

pub async fn get_schema_definition(
    state: Arc<AppState>,
    params: GetSchemaParams,
) -> Result<SchemaDefinition, McpError> {
    state.schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|e| McpError::NotFound(e.to_string()))
}
```

#### Step 6.2: Register Tools

**Location**: `packages/core/src/mcp/tools.rs`

```rust
pub fn get_schema_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "add_schema_field".to_string(),
            description: r#"Add a new field to a schema.

Only fields with protection='user' can be added. Core and system fields cannot be added via this tool (they are defined in schema migrations).

The schema version will be automatically incremented.

Protection levels:
- core: Cannot be modified or deleted (UI components depend on these)
- user: Fully modifiable and deletable by users
- system: Auto-managed, read-only

Example:
{
  "schema_id": "task",
  "field": {
    "name": "priority",
    "type": "enum",
    "protection": "user",
    "core_values": ["LOW", "MEDIUM", "HIGH"],
    "user_values": [],
    "indexed": true,
    "required": false,
    "extensible": true,
    "default": "MEDIUM",
    "description": "Task priority level"
  }
}"#.to_string(),
            inputSchema: json!({
                "type": "object",
                "properties": {
                    "schema_id": {
                        "type": "string",
                        "description": "Schema ID (convention: type name, e.g., 'task')"
                    },
                    "field": {
                        "type": "object",
                        "description": "Field definition to add"
                    }
                },
                "required": ["schema_id", "field"]
            }),
        },
        Tool {
            name: "remove_schema_field".to_string(),
            description: r#"Remove a field from a schema.

Only fields with protection='user' can be removed. Attempting to remove core or system fields will fail with a clear error message.

The schema version will be automatically incremented."#.to_string(),
            inputSchema: json!({
                "type": "object",
                "properties": {
                    "schema_id": {
                        "type": "string",
                        "description": "Schema ID"
                    },
                    "field_name": {
                        "type": "string",
                        "description": "Name of field to remove"
                    }
                },
                "required": ["schema_id", "field_name"]
            }),
        },
        Tool {
            name: "extend_schema_enum".to_string(),
            description: r#"Add a new value to an enum field.

Only adds to user_values (not core_values). The enum field must have extensible=true.

Example: Extend task status enum with "BLOCKED" value.

The schema version will be automatically incremented."#.to_string(),
            inputSchema: json!({
                "type": "object",
                "properties": {
                    "schema_id": {
                        "type": "string",
                        "description": "Schema ID"
                    },
                    "field_name": {
                        "type": "string",
                        "description": "Enum field name"
                    },
                    "new_value": {
                        "type": "string",
                        "description": "Value to add to user_values"
                    }
                },
                "required": ["schema_id", "field_name", "new_value"]
            }),
        },
        Tool {
            name: "remove_schema_enum_value".to_string(),
            description: r#"Remove a value from an enum field's user_values.

Cannot remove values from core_values (will fail with error).

The schema version will be automatically incremented."#.to_string(),
            inputSchema: json!({
                "type": "object",
                "properties": {
                    "schema_id": {
                        "type": "string",
                        "description": "Schema ID"
                    },
                    "field_name": {
                        "type": "string",
                        "description": "Enum field name"
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to remove from user_values"
                    }
                },
                "required": ["schema_id", "field_name", "value"]
            }),
        },
        Tool {
            name: "get_schema_definition".to_string(),
            description: r#"Get the complete schema definition for a node type.

Returns the schema with all fields, protection levels, versions, etc.

Example: Get task schema to see what fields are available and their types."#.to_string(),
            inputSchema: json!({
                "type": "object",
                "properties": {
                    "schema_id": {
                        "type": "string",
                        "description": "Schema ID (type name)"
                    }
                },
                "required": ["schema_id"]
            }),
        },
    ]
}
```

### Testing Requirements

```rust
#[tokio::test]
async fn test_mcp_add_schema_field() {
    let state = setup_mcp_state().await;

    let result = add_schema_field(
        state.clone(),
        AddSchemaFieldParams {
            schema_id: "task".to_string(),
            field: SchemaField {
                name: "priority".to_string(),
                field_type: "enum".to_string(),
                protection: ProtectionLevel::User,
                core_values: Some(vec!["LOW".into(), "MEDIUM".into(), "HIGH".into()]),
                user_values: Some(vec![]),
                indexed: true,
                required: Some(false),
                extensible: Some(true),
                default: Some(json!("MEDIUM")),
                description: Some("Priority level".to_string()),
                item_type: None,
            },
        }
    ).await.unwrap();

    assert!(result.success);
    assert_eq!(result.new_version, 2);
}

#[tokio::test]
async fn test_mcp_extend_schema_enum() {
    let state = setup_mcp_state().await;

    let result = extend_schema_enum(
        state,
        ExtendSchemaEnumParams {
            schema_id: "task".to_string(),
            field_name: "status".to_string(),
            new_value: "BLOCKED".to_string(),
        }
    ).await.unwrap();

    assert!(result.success);
}
```

### Exit Criteria

- [x] All 5 specialized schema tools implemented
- [x] Tools registered in MCP registry
- [x] Tool descriptions include protection rules
- [x] Error messages are clear and actionable
- [x] Tests pass
- [x] MCP integration tests verify tools work end-to-end

---

## Phase 7: Frontend SchemaService Wrapper

### Goal

Create TypeScript wrapper for SchemaService to enable future UI development.

### Files to Create

- `packages/desktop-app/src/lib/services/schemaService.ts` (new file)

### Implementation Steps

#### Step 7.1: Create TypeScript Types

```typescript
// packages/desktop-app/src/lib/types/schema.ts
export type ProtectionLevel = 'core' | 'user' | 'system';

export interface SchemaField {
  name: string;
  type: string;
  protection: ProtectionLevel;

  // Enum-specific
  core_values?: string[];
  user_values?: string[];

  indexed: boolean;
  required?: boolean;
  extensible?: boolean;
  default?: unknown;
  description?: string;
  item_type?: string; // For array types
}

export interface SchemaDefinition {
  is_core: boolean;
  version: number;
  description: string;
  fields: SchemaField[];
}
```

#### Step 7.2: Create SchemaService

```typescript
// packages/desktop-app/src/lib/services/schemaService.ts
import { invoke } from '@tauri-apps/api/core';
import type { SchemaDefinition, SchemaField } from '$lib/types/schema';

export class SchemaService {
  /**
   * Get schema definition by schema ID
   */
  async getSchema(schemaId: string): Promise<SchemaDefinition> {
    return invoke('get_schema_definition', { schemaId });
  }

  /**
   * Add a new field to a schema (only user-protected fields)
   */
  async addField(schemaId: string, field: SchemaField): Promise<void> {
    return invoke('add_schema_field', { schemaId, field });
  }

  /**
   * Remove a field from schema (only user-protected fields)
   */
  async removeField(schemaId: string, fieldName: string): Promise<void> {
    return invoke('remove_schema_field', { schemaId, fieldName });
  }

  /**
   * Extend an enum field with a new value (adds to user_values)
   */
  async extendEnumField(schemaId: string, fieldName: string, newValue: string): Promise<void> {
    return invoke('extend_schema_enum', { schemaId, fieldName, newValue });
  }

  /**
   * Remove a value from enum field (only from user_values)
   */
  async removeEnumValue(schemaId: string, fieldName: string, value: string): Promise<void> {
    return invoke('remove_schema_enum_value', { schemaId, fieldName, value });
  }

  /**
   * Get all enum values (core + user)
   */
  getEnumValues(field: SchemaField): string[] {
    const values: string[] = [];
    if (field.core_values) values.push(...field.core_values);
    if (field.user_values) values.push(...field.user_values);
    return values;
  }

  /**
   * Check if a field can be deleted
   */
  canDeleteField(field: SchemaField): boolean {
    return field.protection === 'user';
  }

  /**
   * Check if an enum value can be removed
   */
  canRemoveEnumValue(field: SchemaField, value: string): boolean {
    return field.user_values?.includes(value) ?? false;
  }
}

// Export singleton instance
export const schemaService = new SchemaService();
```

### Testing Requirements

```typescript
// packages/desktop-app/src/tests/unit/schemaService.test.ts
import { describe, it, expect, vi } from 'vitest';
import { schemaService } from '$lib/services/schemaService';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core');

describe('SchemaService', () => {
  it('should get schema definition', async () => {
    const mockSchema = {
      is_core: true,
      version: 1,
      description: 'Task schema',
      fields: []
    };

    vi.mocked(invoke).mockResolvedValueOnce(mockSchema);

    const schema = await schemaService.getSchema('task');

    expect(invoke).toHaveBeenCalledWith('get_schema_definition', { schemaId: 'task' });
    expect(schema).toEqual(mockSchema);
  });

  it('should add field', async () => {
    const field = {
      name: 'priority',
      type: 'enum',
      protection: 'user' as const,
      core_values: ['LOW', 'MEDIUM', 'HIGH'],
      user_values: [],
      indexed: true,
    };

    await schemaService.addField('task', field);

    expect(invoke).toHaveBeenCalledWith('add_schema_field', { schemaId: 'task', field });
  });

  it('should get enum values (core + user)', () => {
    const field = {
      name: 'status',
      type: 'enum',
      protection: 'core' as const,
      core_values: ['OPEN', 'DONE'],
      user_values: ['BLOCKED', 'WAITING'],
      indexed: true,
    };

    const values = schemaService.getEnumValues(field);

    expect(values).toEqual(['OPEN', 'DONE', 'BLOCKED', 'WAITING']);
  });

  it('should check if field can be deleted', () => {
    expect(schemaService.canDeleteField({ protection: 'user', ... })).toBe(true);
    expect(schemaService.canDeleteField({ protection: 'core', ... })).toBe(false);
    expect(schemaService.canDeleteField({ protection: 'system', ... })).toBe(false);
  });
});
```

### Exit Criteria

- [x] TypeScript SchemaService created
- [x] All methods mirror Rust API
- [x] Helper methods for common operations
- [x] Type definitions complete
- [x] Tests pass
- [x] Ready for future UI (#195)

---

## Sub-Agent Delegation Guide

### When to Use Sub-Agents

Use specialized sub-agents for complex phases:

| Phase | Recommended Sub-Agent | Rationale |
|-------|---------------------|-----------|
| 1 - Version Tracking | `general-purpose` | Straightforward NodeService modification |
| 2 - SchemaService | `senior-architect-reviewer` | Complex service with business logic |
| 3 - Migration Registry | `senior-architect-reviewer` | Critical architecture component |
| 4 - Lazy Integration | `senior-architect-reviewer` | Complex integration, performance critical |
| 5 - MCP Validation | `ai-ml-engineer` | MCP expertise required |
| 6 - MCP Tools | `ai-ml-engineer` | MCP tool design and registration |
| 7 - Frontend | `frontend-architect` | TypeScript/Svelte expertise |

### Sub-Agent Commissioning Template

```
CRITICAL SUB-AGENT INSTRUCTIONS:
- Main agent has completed setup (no startup sequence needed)
- You are implementing Phase N/7 of issue #106
- Focus ONLY on [specific deliverable]
- DO NOT commit changes - return control to main agent when done
- DO NOT run project management commands (gh:status, gh:pr, etc.)
- Continue established patterns from previous phases

CONTEXT FROM PREVIOUS PHASE:
[Key decisions, file locations, patterns established]

YOUR DELIVERABLE:
Phase N: [Phase Name]
- Create/modify: [specific files]
- Implement: [specific functions/features]
- Test: [specific test cases]
- Document: [inline comments, function docs]

REFERENCE IMPLEMENTATION:
- Previous similar code: [file:line reference]
- Architecture docs: [link to relevant docs]
- Patterns to follow: [established conventions]

EXIT CRITERIA:
- [Specific checklist from phase spec]
- Code compiles without errors
- Tests pass
- No TODO comments left in code

Return control to main agent with summary of what was implemented.
```

### Example: Phase 2 Delegation

```
CRITICAL SUB-AGENT INSTRUCTIONS:
- Main agent has completed setup (no startup sequence needed)
- You are implementing Phase 2/7 of issue #106
- Focus ONLY on creating SchemaService with CRUD operations
- DO NOT commit changes - return control to main agent when done
- DO NOT run project management commands

CONTEXT FROM PREVIOUS PHASE:
Phase 1 added _schema_version to all nodes. Version tracking works.
- NodeService now tracks versions in node.properties._schema_version
- get_node() backfills version 1 for old nodes
- Helper method: get_schema_for_type() added to NodeService

YOUR DELIVERABLE:
Phase 2: SchemaService Core Operations
- Create: packages/core/src/models/schema.rs (types)
- Create: packages/core/src/services/schema_service.rs
- Implement:
  - get_schema(schema_id) -> SchemaDefinition
  - add_field(schema_id, field) - user protection only
  - remove_field(schema_id, field_name) - user protection only
  - extend_enum_field(schema_id, field_name, value)
  - remove_enum_value(schema_id, field_name, value)
- Test: tests/unit/schema_service_test.rs
- Protection enforcement: core/system fields cannot be modified

REFERENCE IMPLEMENTATION:
- NodeService pattern: packages/core/src/services/node_service.rs
- Protection levels documented in: docs/architecture/business-logic/database-schema.md
- Error handling pattern: use anyhow::Result with descriptive messages

EXIT CRITERIA:
- [x] SchemaService compiles
- [x] All 5 methods implemented with protection checks
- [x] Unit tests pass (add/remove fields, enum extension, protection)
- [x] Error messages are clear and actionable
- [x] Version increments on schema changes

Return control with summary of implementation and any design decisions made.
```

---

## Commit Message Templates

### Standard Template

```
<type>: <description> (#106 - Phase N/7)

## Summary
[1-2 sentences: what this commit accomplishes]

## Changes
- [File 1]: [what changed]
- [File 2]: [what changed]
- [Tests]: [test coverage added]

## Architecture Decisions
[Important choices made and why]

## Testing
- [x] [Test scenario 1 - passing]
- [x] [Test scenario 2 - passing]
- [ ] [Known limitations for future work]

## Verification Steps
Manual verification commands:
```bash
# Example verification
bun run test:db -- schema_service_test
```

## Next Steps for AI Agent
**Phase N+1: [Next Phase Name]**

Context needed:
- [What the next agent should read first]
- [Key patterns established in this phase]
- [Gotchas or important details]

Implementation tasks:
- [Specific file to create/modify with path]
- [Key functions/features to implement]
- [Integration points with this phase]

See Phase N+1 section in implementation guide for detailed spec.

## Related
- Issue: #106 (Schema Management)
- Depends on: Phase N-1
- Blocks: Phase N+1
- Related: #195 (UI - deferred), #191, #193

🤖 Generated with Claude Code
Co-Authored-By: Claude <noreply@anthropic.com>
```

### Example: Phase 1 Commit Message

```
feat: Add schema version tracking to node instances (#106 - Phase 1/7)

## Summary
Implements version tracking infrastructure for schema evolution. All nodes now track which schema version validated them via _schema_version field.

## Changes
- packages/core/src/services/node_service.rs:
  - Added _schema_version to create_node() (lines 180-195)
  - Added get_schema_for_type() helper (lines 450-465)
  - Added backfill logic in get_node() (lines 250-262)
- tests/unit/node_service_test.rs:
  - test_new_nodes_get_schema_version()
  - test_existing_nodes_backfilled_with_version()

## Architecture Decisions
- Default to version 1 if schema lookup fails (graceful degradation)
- Backfill on read (lazy), not bulk operation (desktop-safe)
- Store in properties JSON (Pure JSON architecture preserved)

## Testing
- [x] New nodes automatically get current schema version
- [x] Existing nodes backfilled with v1 on first access
- [x] Version persists correctly in database
- [x] No impact on non-schema nodes

## Verification Steps
```bash
# Run version tracking tests
bun run test:db -- node_service_test

# Manual verification
node -e "
const task = await createNode({ nodeType: 'task', content: 'Test' });
console.log('Version:', task.properties._schema_version);
"
```

## Next Steps for AI Agent
**Phase 2: SchemaService Core Operations**

Context needed:
- Version tracking now working (_schema_version in all nodes)
- Helper method get_schema_for_type() available in NodeService
- Backfill happens automatically on read

Implementation tasks:
- Create packages/core/src/models/schema.rs (types)
  - Define ProtectionLevel enum (core/user/system)
  - Define SchemaField and SchemaDefinition structs
- Create packages/core/src/services/schema_service.rs
  - Implement get_schema(schema_id)
  - Implement add_field() with user-only protection
  - Implement remove_field() with protection checks
  - Implement extend_enum_field() and remove_enum_value()
  - All methods must increment schema.version
- Create tests/unit/schema_service_test.rs
  - Test protection enforcement
  - Test version incrementing
  - Test enum value management

See Phase 2 section in /docs/architecture/development/schema-management-implementation-guide.md for complete spec with code examples.

## Related
- Issue: #106 (Schema Management Service)
- Blocks: Phase 2 (SchemaService)
- Related: #195 (UI), #191 (Task Schema), #193 (Schema-driven UI)

🤖 Generated with Claude Code
Co-Authored-By: Claude <noreply@anthropic.com>
```

---

## Common Pitfalls & Solutions

### Pitfall 1: Forgetting to Increment Schema Version

**Problem**: Schema changes don't increment version, causing lazy migration to not trigger.

**Solution**: Every schema modification method must increment `schema.version`:

```rust
// ❌ BAD - Forgot to increment
schema.fields.push(new_field);
self.update_schema(schema_id, schema).await?;

// ✅ GOOD - Always increment
schema.fields.push(new_field);
schema.version += 1; // Don't forget!
self.update_schema(schema_id, schema).await?;
```

### Pitfall 2: Migration Applied Multiple Times

**Problem**: Migration runs every time node is accessed instead of once.

**Solution**: Check version before applying migration:

```rust
// ✅ GOOD - Check first
if self.migration_registry.needs_migration(node, schema.version) {
    self.migration_registry.migrate_node(node, schema.version)?;
    self.db.update_node(node).await?;
}
```

### Pitfall 3: Circular Dependency (NodeService ↔ SchemaService)

**Problem**: NodeService needs SchemaService, SchemaService needs NodeService.

**Solution**: SchemaService wraps NodeService (one direction only):

```rust
// ✅ GOOD - SchemaService wraps NodeService
pub struct SchemaService {
    node_service: Arc<NodeService>, // SchemaService depends on NodeService
}

// NodeService doesn't know about SchemaService directly
// It uses migration_registry which is dependency-injected
```

### Pitfall 4: Not Persisting Migrated Nodes

**Problem**: Migration transforms applied but not saved to database.

**Solution**: Always persist after migration:

```rust
// ✅ GOOD - Persist after migration
if needs_migration {
    self.migration_registry.migrate_node(node, target_version)?;
    self.db.update_node(node).await?; // Don't forget!
}
```

### Pitfall 5: Breaking Core Field References in UI

**Problem**: Renaming/deleting core field that UI components depend on.

**Solution**: Protection levels prevent this. If you must rename a core field:

1. Add new field with new name (additive)
2. Create migration to copy old→new
3. Update all UI components to use new field
4. After safe migration period, can deprecate old field
5. Never delete old field (keep for backward compatibility)

### Pitfall 6: Not Testing Protection Enforcement

**Problem**: Tests verify happy path but don't test protection rules.

**Solution**: Always test negative cases:

```rust
#[tokio::test]
async fn test_cannot_delete_core_field() {
    let result = schema_service.remove_field("task", "status").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot remove"));
}
```

### Pitfall 7: Migration Order Matters

**Problem**: Migrations applied in wrong order cause data corruption.

**Solution**: Migrations registered in version order:

```rust
// ✅ GOOD - Register in order
registry.register_migration(task_v1_to_v2()); // Apply first
registry.register_migration(task_v2_to_v3()); // Apply second
registry.register_migration(task_v3_to_v4()); // Apply third
```

### Pitfall 8: Forgetting to Update MCP Tool Descriptions

**Problem**: Tool descriptions don't reflect protection rules, agents use tools incorrectly.

**Solution**: Keep descriptions up-to-date with validation logic:

```rust
Tool {
    name: "add_schema_field",
    description: r#"Add a new field to a schema.

Only fields with protection='user' can be added.  // ← Make this clear!
Core and system fields cannot be added via this tool."#,
    // ...
}
```

---

## Testing Strategy

### Test Pyramid

```
                    /\
                   /  \
                  / E2E \          - Full MCP integration tests
                 /______\           - UI automation (Phase 7)
                /        \
               / Integration\       - SchemaService + NodeService
              /____________\        - Migration registry integration
             /              \
            /   Unit Tests   \      - Individual functions
           /__________________\     - Protection enforcement
                                    - Enum validation
```

### Test Coverage Requirements

**Phase 1: Version Tracking**
- [x] New nodes get version
- [x] Existing nodes backfilled
- [x] Version matches schema version
- [x] No version for nodes without schemas

**Phase 2: SchemaService**
- [x] get_schema() returns valid schema
- [x] add_field() with user protection succeeds
- [x] add_field() with core protection fails
- [x] remove_field() for user field succeeds
- [x] remove_field() for core field fails
- [x] extend_enum_field() adds to user_values
- [x] remove_enum_value() from core_values fails
- [x] remove_enum_value() from user_values succeeds
- [x] Version increments on all changes

**Phase 3: Migration Registry**
- [x] register_migration() stores migration
- [x] migrate_node() applies single migration
- [x] migrate_node() applies migration chain (v1→v2→v3)
- [x] migrate_node() skips if already current
- [x] needs_migration() detects version mismatch
- [x] Migration updates _schema_version

**Phase 4: Lazy Integration**
- [x] get_node() triggers migration when needed
- [x] get_node() skips migration when current
- [x] query_nodes() migrates all results
- [x] Migrated nodes persisted to database
- [x] No duplicate migrations on repeated access

**Phase 5: MCP Validation**
- [x] update_node validates schema changes
- [x] Rejects core field deletion
- [x] Rejects core field type changes
- [x] Rejects core_values modification
- [x] Allows user field changes
- [x] Clear error messages

**Phase 6: MCP Tools**
- [x] add_schema_field works via MCP
- [x] remove_schema_field enforces protection
- [x] extend_schema_enum adds user values
- [x] remove_schema_enum_value protects core values
- [x] get_schema_definition returns complete schema

**Phase 7: Frontend**
- [x] SchemaService methods call Tauri correctly
- [x] Helper methods work (getEnumValues, canDeleteField)
- [x] Type definitions match Rust types

### Integration Test Example

```rust
#[tokio::test]
async fn test_full_schema_evolution_flow() {
    let state = setup_test_state().await;

    // 1. Create task with v1 schema
    let task = state.node_service.create_node(CreateNodeParams {
        node_type: "task".to_string(),
        content: "Test task".to_string(),
        properties: Some(json!({
            "status": "OPEN"
        })),
        ..Default::default()
    }).await.unwrap();

    assert_eq!(task.properties["_schema_version"], 1);

    // 2. Extend schema to v2 (add priority field)
    state.schema_service.add_field("task", SchemaField {
        name: "priority".to_string(),
        field_type: "enum".to_string(),
        protection: ProtectionLevel::User,
        core_values: Some(vec!["LOW".into(), "MEDIUM".into(), "HIGH".into()]),
        user_values: Some(vec![]),
        indexed: true,
        required: Some(false),
        default: Some(json!("MEDIUM")),
        ..Default::default()
    }).await.unwrap();

    let schema = state.schema_service.get_schema("task").await.unwrap();
    assert_eq!(schema.version, 2);

    // 3. Register migration v1→v2
    state.migration_registry.register_migration(SchemaMigration {
        from_version: 1,
        to_version: 2,
        node_type: "task".to_string(),
        transform: Box::new(|node| {
            if let Some(obj) = node.properties.as_object_mut() {
                if !obj.contains_key("priority") {
                    obj.insert("priority".to_string(), json!("MEDIUM"));
                }
            }
            Ok(())
        }),
    });

    // 4. Fetch old task (should trigger lazy migration)
    let migrated_task = state.node_service.get_node(&task.id).await.unwrap().unwrap();

    assert_eq!(migrated_task.properties["_schema_version"], 2);
    assert_eq!(migrated_task.properties["priority"], "MEDIUM");

    // 5. Verify migration persisted
    let refetched = state.node_service.get_node(&task.id).await.unwrap().unwrap();
    assert_eq!(refetched.properties["_schema_version"], 2);
    assert_eq!(refetched.properties["priority"], "MEDIUM");
}
```

---

## References

### Architecture Documentation

- [Database Schema](../business-logic/database-schema.md) - Pure JSON schema architecture
- [Storage Architecture](../data/storage-architecture.md) - Local-first storage patterns
- [Validation System](../components/validation-system.md) - Validation framework
- [Data Layer ADR](../decisions/data-layer-architecture.md) - Data architecture decisions

### Issue References

- [#106](https://github.com/malibio/nodespace-core/issues/106) - Schema Management Service (primary)
- [#195](https://github.com/malibio/nodespace-core/issues/195) - SchemaNodeViewer UI (deferred)
- [#191](https://github.com/malibio/nodespace-core/issues/191) - Update Task Schema
- [#193](https://github.com/malibio/nodespace-core/issues/193) - Schema-Driven Property UI

### Related Guides

- [How to Add New Node Type](./how-to-add-new-node-type.md) - Node type creation
- [Testing Guide](./testing-guide.md) - Testing strategies
- [Issue Workflow](./process/issue-workflow.md) - Development workflow

### Code Patterns

- `packages/core/src/services/node_service.rs` - Service pattern example
- `packages/core/src/models/node.rs` - Model definitions
- `packages/core/src/mcp/handlers/nodes.rs` - MCP handler pattern
- `packages/desktop-app/src/lib/services/tauri-node-service.ts` - Frontend service pattern

---

## Appendix: Quick Reference

### Protection Level Quick Reference

| Level | Add? | Modify? | Delete? | Use Case |
|-------|------|---------|---------|----------|
| `core` | ❌ | ❌ | ❌ | Foundation fields (UI depends on) |
| `user` | ✅ | ✅ | ✅ | User customizations |
| `system` | ❌ | ❌ | ❌ | Auto-managed internal fields |

### Enum Value Quick Reference

| Value Type | Add? | Remove? | Modify? | Protected? |
|------------|------|---------|---------|------------|
| `core_values` | ❌ | ❌ | ❌ | ✅ Yes |
| `user_values` | ✅ | ✅ | ✅ | ❌ No |

### Migration Type Quick Reference

| Change Type | Breaking? | Migration Needed? | Example |
|-------------|-----------|-------------------|---------|
| Add optional field | ❌ No | ❌ No | Add "priority" field |
| Add required field | ⚠️ Maybe | ✅ Yes | Add "owner" (need default) |
| Rename field | ✅ Yes | ✅ Yes | "assignee" → "assigned_to" |
| Change field type | ✅ Yes | ✅ Yes | text → enum |
| Delete field | ✅ Yes | ⚠️ Maybe | Deprecated field cleanup |
| Extend enum | ❌ No | ❌ No | Add "BLOCKED" to status |

### File Locations Quick Reference

```
packages/core/src/
├── models/
│   ├── node.rs              # Node model
│   └── schema.rs            # Schema types (Phase 2)
├── services/
│   ├── node_service.rs      # Version tracking (Phase 1), Lazy migration (Phase 4)
│   ├── schema_service.rs    # Schema CRUD (Phase 2)
│   ├── migration_registry.rs # Migration system (Phase 3)
│   └── migrations/
│       └── task.rs          # Task migrations (Phase 3)
├── mcp/
│   ├── handlers/
│   │   ├── nodes.rs         # Generic validation (Phase 5)
│   │   └── schema.rs        # Specialized tools (Phase 6)
│   └── tools.rs             # Tool registration (Phase 6)

packages/desktop-app/src/lib/
├── types/
│   └── schema.ts            # TypeScript types (Phase 7)
└── services/
    └── schemaService.ts     # Frontend service (Phase 7)

tests/
├── unit/
│   ├── node_service_test.rs
│   ├── schema_service_test.rs
│   └── migration_registry_test.rs
└── integration/
    └── schema_evolution_test.rs
```

---

**End of Implementation Guide**

For questions or issues during implementation, refer to:
- This guide's specific phase sections
- Architecture documentation (links above)
- Existing code patterns in referenced files
- Issue #106 for requirements clarification
