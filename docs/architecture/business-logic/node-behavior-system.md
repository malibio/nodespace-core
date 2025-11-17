# NodeSpace Node Behavior System

## Overview

The Node Behavior System is a trait-based architecture that provides type-specific functionality for different kinds of nodes in NodeSpace. It enables extensible behavior while maintaining a universal data storage format, supporting both built-in node types and future plugin development.

## Architectural Decision: Hybrid Approach

NodeSpace uses a **hybrid architecture** combining hardcoded behaviors with schema-driven extensions:

### Core Layer (Hardcoded Behaviors)
- **Purpose**: Protect critical properties that UI components depend on
- **Implementation**: Rust traits in `packages/core/src/behaviors/mod.rs`
- **Advantages**:
  - ✅ Compile-time type safety
  - ✅ Performance optimization (1000x faster validation)
  - ✅ UI stability guarantees (core fields cannot be deleted by users)
  - ✅ Git-versioned evolution with code review
- **Scope**: Built-in types (task, text, date, project, person) and their core properties

### Extension Layer (Schema-Driven)
- **Purpose**: User customization and plugin extensibility
- **Implementation**: Schema nodes with validation at runtime
- **Advantages**:
  - ✅ Runtime extensibility (no code deployment)
  - ✅ User-customizable properties
  - ✅ MCP/AI can modify schemas
  - ✅ Plugin/extension support
- **Scope**: User-defined properties and custom entity types

### Validation Hierarchy
```rust
// Step 1: Core behavior validation (PROTECTED)
if let Some(behavior) = registry.get(&node.node_type) {
    behavior.validate(node)?;  // Cannot be overridden
}

// Step 2: Schema validation (USER-EXTENSIBLE)
if let Some(schema) = schema_service.get_schema(&node.node_type)? {
    validate_against_schema(node, &schema)?;
}
```

### Property Ownership Model

| Property Type | Protection | Modifiable | Example |
|--------------|------------|------------|---------|
| **Core Properties** | `core` | ❌ No | task.status, task.priority |
| **User Extensions** | `user` | ✅ Yes | custom:estimatedHours, custom:tags |
| **Custom Types** | N/A | ✅ Yes | recipe, workout (no hardcoded behavior) |

### Schema Defaults During Type Conversion

When nodes are converted between types (via slash commands or pattern detection), the system automatically applies default property values from the target node type's schema:

#### How It Works

1. **Conversion Trigger**: User converts a node type using:
   - Slash commands (`/task`, `/header`, etc.)
   - Pattern detection (typing `[ ] ` converts to task)

2. **Default Extraction**: System extracts default values from schema:
   ```typescript
   // Example schema with defaults
   {
     id: 'task',
     fields: [
       { name: 'status', type: 'enum', default: 'todo' },
       { name: 'priority', type: 'enum', default: 'medium' },
       { name: 'estimatedHours', type: 'number' }  // No default
     ]
   }
   ```

3. **Property Merging**: Defaults are merged with existing properties:
   ```typescript
   // Before conversion (text node with custom data)
   properties: {
     someData: { customField: 'value' }
   }

   // After conversion to 'task'
   properties: {
     task: {
       status: 'todo',       // ← Applied from schema default
       priority: 'medium'    // ← Applied from schema default
     },
     someData: { customField: 'value' }  // ← Preserved
   }
   ```

#### Merge Behavior (Non-Destructive)

The system follows a **non-destructive merge strategy** to preserve user data:

- ✅ **Schema defaults are applied** for new properties
- ✅ **Existing user values are preserved** (never overwritten)
- ✅ **Optional fields without defaults are NOT created**
- ✅ **Other property namespaces are preserved**

```typescript
// Example: User has already set status to 'in-progress'
// Before conversion
properties: {
  task: {
    status: 'in-progress',  // User-set value
    customField: 'data'
  }
}

// After re-conversion to 'task'
properties: {
  task: {
    status: 'in-progress',  // ← PRESERVED (not overwritten)
    priority: 'medium',     // ← ADDED (from default)
    customField: 'data'     // ← PRESERVED
  }
}
```

#### Implementation Details

- **Location**: `ReactiveNodeService.updateNodeType()`
- **Performance**: Synchronous operation using schema cache (no async delay)
- **Graceful Degradation**: If schema not found, conversion proceeds without defaults
- **Cache-Based**: Uses LRU cache in `SchemaService` for instant access

#### When Defaults Are Applied

| Conversion Method | Defaults Applied? | Notes |
|------------------|-------------------|-------|
| Slash Commands | ✅ Yes | `/task`, `/header`, etc. |
| Pattern Detection | ✅ Yes | `[ ] `, `# `, etc. |
| Programmatic API | ✅ Yes | `updateNodeType()` call |
| Initial Creation | ❌ No | Handled separately by node creation logic |

See [Schema Service Documentation](../../lib/services/schema-service.ts) for schema default configuration.

### Why This Approach?

The hybrid model provides:
1. **Stability**: Core UI components won't break from user modifications
2. **Performance**: Critical validation paths remain compiled and fast
3. **Type Safety**: Core properties have compile-time guarantees
4. **Extensibility**: Users can add properties and types without code changes
5. **Best of Both Worlds**: Reliability for core features + flexibility for customization

See [Schema Management Guide](../development/schema-management-implementation-guide.md) for schema-driven extensions.

## Core Architecture

### Separation of Data and Behavior

NodeSpace separates **data storage** from **type-specific behavior**:

- **Universal Data**: All nodes stored in same database structure
- **Type-Specific Behavior**: Implemented via traits and registered handlers

```rust
// Universal data structure (stored in database)
pub struct Node {
    pub id: String,
    pub node_type: String,          // Determines which behavior to use
    pub content: String,
    pub metadata: serde_json::Value, // Type-specific properties
    // ... other universal fields
}

// Type-specific behavior (not stored)
pub trait NodeBehavior: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn validate(&self, node: &Node) -> Result<(), ValidationError>;
    // ... other behaviors
}
```

## NodeBehavior Trait Definition

### Core Trait

```rust
use serde_json::Value;
use chrono::{DateTime, Utc};

pub trait NodeBehavior: Send + Sync {
    /// Unique identifier for this node type
    fn type_name(&self) -> &'static str;

    /// Validate node content and metadata
    fn validate(&self, node: &Node) -> Result<(), ValidationError>;

    /// Whether this node type can have children
    fn can_have_children(&self) -> bool;

    /// Whether this node type supports markdown formatting
    fn supports_markdown(&self) -> bool;

    /// Process/transform content before storage
    fn process_content(&self, content: &str) -> Result<String, ProcessingError> {
        Ok(content.to_string()) // Default: no processing
    }

    /// Extract references/mentions from content
    fn extract_mentions(&self, content: &str) -> Vec<String> {
        Vec::new() // Default: no mentions
    }

    /// Get schema definition for metadata validation
    fn get_metadata_schema(&self) -> Option<Value> {
        None // Default: no schema validation
    }

    /// Initialize default metadata for new nodes
    fn default_metadata(&self) -> Value {
        Value::Null
    }

    /// Handle node deletion (cleanup, cascade rules, etc.)
    fn on_delete(&self, node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        Ok(vec![]) // Default: no special cleanup
    }
}

#[derive(Debug)]
pub enum DeletionAction {
    DeleteChildren,           // Cascade delete all children
    OrphanChildren,          // Set children's parent_id to null
    MoveChildren(String),    // Move children to different parent
    DeleteMentions,          // Remove from mention tables
}

pub type ValidationError = String;
pub type ProcessingError = String;
```

## Built-in Node Behaviors

### 1. TextNodeBehavior

```rust
pub struct TextNodeBehavior;

impl NodeBehavior for TextNodeBehavior {
    fn type_name(&self) -> &'static str {
        "text"
    }

    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        if node.content.trim().is_empty() {
            return Err("Text nodes must have non-empty content".to_string());
        }

        // Validate markdown if enabled
        if let Some(markdown_enabled) = node.metadata.get("markdown_enabled") {
            if markdown_enabled.as_bool().unwrap_or(false) {
                self.validate_markdown(&node.content)?;
            }
        }

        Ok(())
    }

    fn can_have_children(&self) -> bool {
        true // Text nodes can contain other nodes
    }

    fn supports_markdown(&self) -> bool {
        true
    }

    fn process_content(&self, content: &str) -> Result<String, ProcessingError> {
        // Process markdown, extract mentions, etc.
        Ok(content.to_string())
    }

    fn extract_mentions(&self, content: &str) -> Vec<String> {
        // Find @node-references or [[node-links]]
        let mut mentions = Vec::new();

        // Pattern: @node:node-id
        for cap in regex::Regex::new(r"@node:([a-zA-Z0-9-]+)").unwrap().captures_iter(content) {
            mentions.push(cap[1].to_string());
        }

        // Pattern: [[node-id]]
        for cap in regex::Regex::new(r"\[\[([a-zA-Z0-9-]+)\]\]").unwrap().captures_iter(content) {
            mentions.push(cap[1].to_string());
        }

        mentions
    }

    fn default_metadata(&self) -> Value {
        json!({
            "markdown_enabled": true,
            "auto_save": true,
            "word_wrap": true
        })
    }

    fn on_delete(&self, _node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        Ok(vec![
            DeletionAction::DeleteMentions, // Clean up mention references
        ])
    }
}

impl TextNodeBehavior {
    fn validate_markdown(&self, content: &str) -> Result<(), ValidationError> {
        // Basic markdown validation
        // Could use pulldown-cmark or similar for validation
        Ok(())
    }
}
```

### 2. TaskNodeBehavior

```rust
pub struct TaskNodeBehavior;

impl NodeBehavior for TaskNodeBehavior {
    fn type_name(&self) -> &'static str {
        "task"
    }

    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        // Validate required fields
        if node.content.trim().is_empty() {
            return Err("Task nodes must have a description".to_string());
        }

        // NOTE: In the actual implementation (Issue #392), enum value validation
        // is handled by the schema system, not behaviors. Behaviors only validate TYPES.
        // This example is kept for historical reference.

        // Validate metadata structure (TYPE checking only - values checked by schema)
        if let Some(status) = node.metadata.get("status") {
            if !status.is_string() && !status.is_null() {
                return Err("Status must be a string".to_string());
            }
            // Value validation (OPEN, IN_PROGRESS, DONE, BLOCKED) handled by schema
        }

        if let Some(due_date) = node.metadata.get("due_date") {
            if !due_date.is_string() && !due_date.is_null() {
                return Err("Due date must be a valid ISO date string".to_string());
            }
        }

        if let Some(priority) = node.metadata.get("priority") {
            if !priority.is_string() && !priority.is_null() {
                return Err("Priority must be a string".to_string());
            }
            // Value validation (LOW, MEDIUM, HIGH) handled by schema
        }

        Ok(())
    }

    fn can_have_children(&self) -> bool {
        true // Tasks can have subtasks
    }

    fn supports_markdown(&self) -> bool {
        false // Tasks are usually single-line descriptions
    }

    fn default_metadata(&self) -> Value {
        json!({
            "task": {  // Namespaced under "task" (Issue #397)
                "status": "OPEN",  // Schema-defined enum values are UPPERCASE
                "priority": "MEDIUM",  // Priority is string enum (LOW, MEDIUM, HIGH)
                "due_date": null,
                "assignee_id": null
            }
        })
    }

    fn on_delete(&self, _node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        Ok(vec![
            DeletionAction::DeleteMentions,
            DeletionAction::OrphanChildren, // Subtasks become orphaned, don't auto-delete
        ])
    }
}
```

### 3. PersonNodeBehavior

```rust
pub struct PersonNodeBehavior;

impl NodeBehavior for PersonNodeBehavior {
    fn type_name(&self) -> &'static str {
        "person"
    }

    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        // Person nodes are usually computed content: "FirstName LastName"
        let first_name = node.metadata.get("first_name")
            .and_then(|v| v.as_str())
            .ok_or("Person nodes must have first_name in metadata")?;

        let last_name = node.metadata.get("last_name")
            .and_then(|v| v.as_str())
            .ok_or("Person nodes must have last_name in metadata")?;

        // Validate email format if present
        if let Some(email) = node.metadata.get("email") {
            if let Some(email_str) = email.as_str() {
                if !email_str.contains('@') || !email_str.contains('.') {
                    return Err("Invalid email format".to_string());
                }
            }
        }

        // Content should match computed format
        let expected_content = format!("{} {}", first_name, last_name);
        if node.content != expected_content {
            return Err(format!(
                "Person node content should be computed as '{}'",
                expected_content
            ));
        }

        Ok(())
    }

    fn can_have_children(&self) -> bool {
        false // People are usually leaf nodes
    }

    fn supports_markdown(&self) -> bool {
        false
    }

    fn process_content(&self, _content: &str) -> Result<String, ProcessingError> {
        // Content is computed from metadata, ignore input
        Ok("".to_string()) // Will be set by compute_content
    }

    fn default_metadata(&self) -> Value {
        json!({
            "first_name": "",
            "last_name": "",
            "email": null,
            "phone": null,
            "organization": null,
            "role": null
        })
    }

    fn on_delete(&self, _node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        // Persons might be referenced by other nodes (tasks, projects)
        // Don't auto-delete references, but clean up mentions
        Ok(vec![
            DeletionAction::DeleteMentions,
        ])
    }
}

impl PersonNodeBehavior {
    /// Compute display content from metadata
    pub fn compute_content(&self, metadata: &Value) -> Result<String, ProcessingError> {
        let first_name = metadata.get("first_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let last_name = metadata.get("last_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if first_name.is_empty() && last_name.is_empty() {
            return Err("Person must have at least first or last name".to_string());
        }

        Ok(format!("{} {}", first_name, last_name).trim().to_string())
    }
}
```

### 4. ProjectNodeBehavior

```rust
pub struct ProjectNodeBehavior;

impl NodeBehavior for ProjectNodeBehavior {
    fn type_name(&self) -> &'static str {
        "project"
    }

    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        if node.content.trim().is_empty() {
            return Err("Project nodes must have a name/title".to_string());
        }

        // NOTE: This is a hypothetical example. In actual implementation (Issue #392),
        // enum value validation is handled by schemas, not behaviors.
        // Validate status TYPE only
        if let Some(status) = node.metadata.get("status") {
            if !status.is_string() && !status.is_null() {
                return Err("Status must be a string".to_string());
            }
            // Value validation would be handled by project schema
        }

        // Validate date logic
        if let (Some(start), Some(end)) = (
            node.metadata.get("start_date").and_then(|v| v.as_str()),
            node.metadata.get("end_date").and_then(|v| v.as_str())
        ) {
            // Basic date comparison (could use chrono for proper validation)
            if start > end {
                return Err("Project start date must be before end date".to_string());
            }
        }

        Ok(())
    }

    fn can_have_children(&self) -> bool {
        true // Projects contain tasks, milestones, etc.
    }

    fn supports_markdown(&self) -> bool {
        true // Project descriptions can be rich
    }

    fn default_metadata(&self) -> Value {
        json!({
            "status": "planning",
            "start_date": null,
            "end_date": null,
            "owner_id": null,
            "team_members": [],
            "budget": null,
            "priority": 2
        })
    }

    fn on_delete(&self, _node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        Ok(vec![
            DeletionAction::DeleteMentions,
            DeletionAction::OrphanChildren, // Project tasks become orphaned
        ])
    }
}
```

### 5. DateNodeBehavior

```rust
pub struct DateNodeBehavior;

impl NodeBehavior for DateNodeBehavior {
    fn type_name(&self) -> &'static str {
        "date"
    }

    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        // Date nodes have special ID format: YYYY-MM-DD (no prefix)
        // Use regex for robust validation
        use regex::Regex;
        use chrono::NaiveDate;

        lazy_static::lazy_static! {
            static ref DATE_PATTERN: Regex = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
        }

        if !DATE_PATTERN.is_match(&node.id) {
            return Err("Date nodes must have ID format 'YYYY-MM-DD'".to_string());
        }

        // Validate that it's an actual valid date
        NaiveDate::parse_from_str(&node.id, "%Y-%m-%d")
            .map_err(|_| "Invalid date format".to_string())?;

        // Content should match the date ID
        if node.content != node.id {
            return Err("Date node content should match the date ID".to_string());
        }

        Ok(())
    }

    fn can_have_children(&self) -> bool {
        true // Dates can contain events, tasks, etc.
    }

    fn supports_markdown(&self) -> bool {
        false // Dates are simple identifiers
    }

    fn default_metadata(&self) -> Value {
        json!({
            "is_holiday": false,
            "timezone": "UTC"
        })
    }

    fn on_delete(&self, _node: &Node) -> Result<Vec<DeletionAction>, ProcessingError> {
        // Date nodes are usually referenced, not deleted
        // But if deleted, orphan children (don't cascade delete events)
        Ok(vec![
            DeletionAction::DeleteMentions,
            DeletionAction::OrphanChildren,
        ])
    }
}

impl DateNodeBehavior {
    /// Create date node with deterministic ID
    pub fn create_date_node(date: &str) -> Result<Node, ProcessingError> {
        use chrono::NaiveDate;

        // Validate date format (YYYY-MM-DD)
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| "Date must be in YYYY-MM-DD format".to_string())?;

        let now = chrono::Utc::now();
        Ok(Node {
            id: date.to_string(),              // Just the date, no prefix
            node_type: "date".to_string(),
            content: date.to_string(),         // Content matches ID
            parent_id: None,
            root_id: date.to_string(),         // Self-referencing root
            before_sibling_id: None,
            created_at: now,
            modified_at: now,
            metadata: json!({
                "is_holiday": false,
                "timezone": "UTC"
            }),
            embedding_vector: None,            // Dates don't need embeddings
        })
    }
}
```

## Behavior Registry System

### Registry Implementation

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct NodeBehaviorRegistry {
    behaviors: HashMap<String, Arc<dyn NodeBehavior>>,
}

impl NodeBehaviorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            behaviors: HashMap::new(),
        };

        // Register built-in types
        registry.register("text", Arc::new(TextNodeBehavior));
        registry.register("task", Arc::new(TaskNodeBehavior));
        registry.register("person", Arc::new(PersonNodeBehavior));
        registry.register("project", Arc::new(ProjectNodeBehavior));
        registry.register("date", Arc::new(DateNodeBehavior));

        registry
    }

    pub fn register<T>(&mut self, node_type: &str, behavior: Arc<T>)
    where
        T: NodeBehavior + 'static,
    {
        self.behaviors.insert(node_type.to_string(), behavior);
    }

    pub fn get(&self, node_type: &str) -> Option<Arc<dyn NodeBehavior>> {
        self.behaviors.get(node_type).cloned()
    }

    pub fn get_all_types(&self) -> Vec<String> {
        self.behaviors.keys().cloned().collect()
    }

    pub fn validate_node(&self, node: &Node) -> Result<(), ValidationError> {
        if let Some(behavior) = self.get(&node.node_type) {
            behavior.validate(node)
        } else {
            Err(format!("Unknown node type: {}", node.node_type))
        }
    }
}
```

### Service Integration

```rust
use std::sync::Arc;

pub struct NodeService {
    pool: sqlx::SqlitePool,
    behaviors: Arc<NodeBehaviorRegistry>,
    nlp: Arc<NLPService>,
}

impl NodeService {
    pub fn new(
        pool: sqlx::SqlitePool,
        behaviors: Arc<NodeBehaviorRegistry>,
        nlp: Arc<NLPService>,
    ) -> Self {
        Self { pool, behaviors, nlp }
    }

    pub async fn create_node(&self, mut node: Node) -> Result<String, Error> {
        // Get behavior for validation
        let behavior = self.behaviors.get(&node.node_type)
            .ok_or_else(|| Error::UnknownNodeType(node.node_type.clone()))?;

        // Process content if needed
        node.content = behavior.process_content(&node.content)?;

        // Validate node
        behavior.validate(&node)?;

        // Extract mentions
        let mentions = behavior.extract_mentions(&node.content);

        // Generate embedding if content changed
        let embedding = if !node.content.is_empty() {
            Some(self.nlp.generate_embedding(&node.content).await?)
        } else {
            None
        };

        // Store in database
        self.store_node_with_embedding(&node, embedding, &mentions).await?;

        Ok(node.id)
    }

    pub async fn update_node(&self, id: &str, update: NodeUpdate) -> Result<(), Error> {
        let mut node = self.get_node(id).await?
            .ok_or_else(|| Error::NodeNotFound(id.to_string()))?;

        let behavior = self.behaviors.get(&node.node_type)
            .ok_or_else(|| Error::UnknownNodeType(node.node_type.clone()))?;

        // Apply updates
        if let Some(content) = update.content {
            node.content = behavior.process_content(&content)?;
        }
        if let Some(metadata) = update.metadata {
            node.metadata = metadata;
        }
        node.modified_at = chrono::Utc::now();

        // Validate updated node
        behavior.validate(&node)?;

        // Update mentions and embeddings if content changed
        let mentions = behavior.extract_mentions(&node.content);
        let embedding = if update.content.is_some() {
            Some(self.nlp.generate_embedding(&node.content).await?)
        } else {
            None
        };

        self.update_node_with_embedding(&node, embedding, &mentions).await?;

        Ok(())
    }

    pub async fn delete_node(&self, id: &str) -> Result<(), Error> {
        let node = self.get_node(id).await?
            .ok_or_else(|| Error::NodeNotFound(id.to_string()))?;

        let behavior = self.behaviors.get(&node.node_type)
            .ok_or_else(|| Error::UnknownNodeType(node.node_type.clone()))?;

        // Execute deletion actions
        let actions = behavior.on_delete(&node)?;
        for action in actions {
            self.execute_deletion_action(id, action).await?;
        }

        // Delete the node itself
        sqlx::query!("DELETE FROM nodes WHERE id = ?", id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
```

## Extension Points for Plugins

### Plugin Registration

```rust
// Future plugin system
pub trait NodePlugin {
    fn behavior(&self) -> Arc<dyn NodeBehavior>;
    fn migrations(&self) -> Vec<String>;
    fn dependencies(&self) -> Vec<String>;
}

impl NodeBehaviorRegistry {
    pub fn register_plugin(&mut self, plugin: Arc<dyn NodePlugin>) -> Result<(), Error> {
        let behavior = plugin.behavior();
        let type_name = behavior.type_name();

        // Check dependencies
        for dep in plugin.dependencies() {
            if !self.behaviors.contains_key(&dep) {
                return Err(Error::MissingDependency(dep));
            }
        }

        // Register the behavior
        self.behaviors.insert(type_name.to_string(), behavior);

        Ok(())
    }
}
```

### Custom Validation Rules

Future enhancement could support user-defined validation rules:

```rust
pub trait ValidationRule: Send + Sync {
    fn validate(&self, node: &Node) -> Result<(), ValidationError>;
    fn applies_to(&self, node_type: &str) -> bool;
}

// Example: Business rule validation
pub struct TaskDeadlineRule;

impl ValidationRule for TaskDeadlineRule {
    fn validate(&self, node: &Node) -> Result<(), ValidationError> {
        if let Some(due_date) = node.metadata.get("due_date") {
            // Business rule: Tasks can't be due more than 1 year in the future
            // Implementation...
        }
        Ok(())
    }

    fn applies_to(&self, node_type: &str) -> bool {
        node_type == "task"
    }
}
```

## Testing Strategy

### Unit Tests for Behaviors

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_node_validation() {
        let behavior = TextNodeBehavior;

        let valid_node = Node {
            id: "text-1".to_string(),
            node_type: "text".to_string(),
            content: "Hello world".to_string(),
            metadata: json!({"markdown_enabled": true}),
            // ... other fields
        };

        assert!(behavior.validate(&valid_node).is_ok());

        let invalid_node = Node {
            content: "".to_string(), // Empty content should fail
            ..valid_node
        };

        assert!(behavior.validate(&invalid_node).is_err());
    }

    #[test]
    fn test_mention_extraction() {
        let behavior = TextNodeBehavior;
        let content = "See @node:task-123 and [[person-456]] for details";

        let mentions = behavior.extract_mentions(content);
        assert_eq!(mentions, vec!["task-123", "person-456"]);
    }

    #[test]
    fn test_task_status_validation() {
        let behavior = TaskNodeBehavior;

        let invalid_status = Node {
            node_type: "task".to_string(),
            content: "Do something".to_string(),
            metadata: json!({"status": "invalid-status"}),
            // ... other fields
        };

        assert!(behavior.validate(&invalid_status).is_err());
    }
}
```

This behavior system provides a flexible, extensible foundation for NodeSpace's node type system while maintaining clean separation between data storage and business logic.