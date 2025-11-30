//! Type-Safe SchemaNode Wrapper
//!
//! Provides ergonomic, compile-time type-safe access to schema node properties
//! while maintaining the universal Node storage model.
//!
//! # Examples
//!
//! ```rust
//! use nodespace_core::models::{Node, SchemaNode};
//! use nodespace_core::models::schema::{SchemaField, SchemaProtectionLevel};
//! use serde_json::json;
//!
//! // Create from existing node
//! let node = Node::new(
//!     "schema".to_string(),
//!     "task".to_string(),  // content = schema name
//!     json!({
//!         "isCore": true,
//!         "version": 1,
//!         "description": "Task schema",
//!         "fields": []
//!     }),
//! );
//! let schema = SchemaNode::from_node(node).unwrap();
//!
//! // Type-safe property access
//! assert!(schema.is_core());
//! assert_eq!(schema.version(), 1);
//! ```

use crate::models::schema::{SchemaField, SchemaProtectionLevel};
use crate::models::{Node, ValidationError};
use serde::{Serialize, Serializer};

/// Type-safe wrapper for schema nodes
///
/// Provides ergonomic access to schema-specific properties while maintaining
/// the universal Node storage model underneath.
///
/// Schema nodes store entity type definitions (like "task", "person") and their
/// field configurations. Properties are stored flat in the spoke table (schema:<id>)
/// matching the TaskNode pattern.
///
/// When serialized (for Tauri/HTTP responses), outputs a flat structure with typed fields:
/// ```json
/// {
///   "id": "task",
///   "nodeType": "schema",
///   "isCore": true,
///   "version": 1,
///   "description": "Task schema",
///   "fields": [...]
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SchemaNode {
    node: Node,
}

/// Serialization output for SchemaNode - flat structure with typed fields
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaNodeSerialized<'a> {
    id: &'a str,
    node_type: &'a str,
    content: &'a str,
    created_at: &'a chrono::DateTime<chrono::Utc>,
    modified_at: &'a chrono::DateTime<chrono::Utc>,
    version: i64,
    // Schema-specific typed fields (not buried in properties)
    is_core: bool,
    schema_version: u32,
    description: String,
    fields: Vec<SchemaField>,
}

impl Serialize for SchemaNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let output = SchemaNodeSerialized {
            id: &self.node.id,
            node_type: &self.node.node_type,
            content: &self.node.content,
            created_at: &self.node.created_at,
            modified_at: &self.node.modified_at,
            version: self.node.version,
            // Extract typed fields from properties
            is_core: self.is_core(),
            schema_version: self.version(),
            description: self.description(),
            fields: self.fields(),
        };
        output.serialize(serializer)
    }
}

impl SchemaNode {
    /// Create a SchemaNode from an existing Node
    ///
    /// # Errors
    ///
    /// Returns `ValidationError::InvalidNodeType` if the node type is not "schema".
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nodespace_core::models::{Node, SchemaNode};
    /// use serde_json::json;
    ///
    /// let node = Node::new(
    ///     "schema".to_string(),
    ///     "task".to_string(),
    ///     json!({"isCore": true, "version": 1, "description": "", "fields": []})
    /// );
    /// let schema = SchemaNode::from_node(node).unwrap();
    /// ```
    pub fn from_node(node: Node) -> Result<Self, ValidationError> {
        if node.node_type != "schema" {
            return Err(ValidationError::InvalidNodeType(format!(
                "Expected 'schema', got '{}'",
                node.node_type
            )));
        }
        Ok(Self { node })
    }

    /// Get whether this is a core (built-in) schema
    ///
    /// Core schemas are shipped with NodeSpace and cannot be deleted.
    pub fn is_core(&self) -> bool {
        self.node
            .properties
            .get("isCore")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Set whether this is a core schema
    pub fn set_is_core(&mut self, is_core: bool) {
        if let Some(obj) = self.node.properties.as_object_mut() {
            obj.insert("isCore".to_string(), serde_json::json!(is_core));
        }
    }

    /// Get the schema version number
    ///
    /// Version increments on any schema change. Used for lazy migration.
    pub fn version(&self) -> u32 {
        self.node
            .properties
            .get("version")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1)
    }

    /// Set the schema version number
    pub fn set_version(&mut self, version: u32) {
        if let Some(obj) = self.node.properties.as_object_mut() {
            obj.insert("version".to_string(), serde_json::json!(version));
        }
    }

    /// Increment the schema version
    pub fn increment_version(&mut self) {
        self.set_version(self.version() + 1);
    }

    /// Get the schema description
    pub fn description(&self) -> String {
        self.node
            .properties
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    /// Set the schema description
    pub fn set_description(&mut self, description: String) {
        if let Some(obj) = self.node.properties.as_object_mut() {
            obj.insert("description".to_string(), serde_json::json!(description));
        }
    }

    /// Get the schema fields
    ///
    /// Returns the array of field definitions, or empty vec if parsing fails.
    pub fn fields(&self) -> Vec<SchemaField> {
        self.node
            .properties
            .get("fields")
            .and_then(|v| serde_json::from_value::<Vec<SchemaField>>(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Set the schema fields
    pub fn set_fields(&mut self, fields: Vec<SchemaField>) {
        if let Some(obj) = self.node.properties.as_object_mut() {
            if let Ok(fields_json) = serde_json::to_value(&fields) {
                obj.insert("fields".to_string(), fields_json);
            }
        }
    }

    /// Get a field by name
    pub fn get_field(&self, name: &str) -> Option<SchemaField> {
        self.fields().into_iter().find(|f| f.name == name)
    }

    /// Add a field to the schema
    ///
    /// Returns error if field with same name already exists.
    pub fn add_field(&mut self, field: SchemaField) -> Result<(), String> {
        let mut fields = self.fields();
        if fields.iter().any(|f| f.name == field.name) {
            return Err(format!("Field '{}' already exists", field.name));
        }
        fields.push(field);
        self.set_fields(fields);
        Ok(())
    }

    /// Remove a field by name
    ///
    /// Returns the removed field, or None if not found.
    pub fn remove_field(&mut self, name: &str) -> Option<SchemaField> {
        let mut fields = self.fields();
        let idx = fields.iter().position(|f| f.name == name)?;
        let removed = fields.remove(idx);
        self.set_fields(fields);
        Some(removed)
    }

    /// Get all valid values for an enum field (core + user values combined)
    ///
    /// Returns `None` if the field doesn't exist or isn't an enum.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let schema = SchemaNode::from_node(node)?;
    /// let status_values = schema.get_enum_values("status");
    /// // Returns: Some(["open", "in_progress", "done", "blocked"])
    /// ```
    pub fn get_enum_values(&self, field_name: &str) -> Option<Vec<String>> {
        let field = self.get_field(field_name)?;

        // Only return values for enum fields
        if field.field_type != "enum" {
            return None;
        }

        let mut values = Vec::new();
        if let Some(core_vals) = &field.core_values {
            values.extend(core_vals.clone());
        }
        if let Some(user_vals) = &field.user_values {
            values.extend(user_vals.clone());
        }

        Some(values)
    }

    /// Check if a field can be deleted based on its protection level
    ///
    /// Only `User` protected fields can be deleted.
    pub fn can_delete_field(&self, field_name: &str) -> bool {
        if let Some(field) = self.get_field(field_name) {
            field.protection == SchemaProtectionLevel::User
        } else {
            false
        }
    }

    /// Check if a field can be modified based on its protection level
    ///
    /// Only `User` protected fields can be modified (type changes, etc.).
    /// Core/System fields are immutable.
    pub fn can_modify_field(&self, field_name: &str) -> bool {
        if let Some(field) = self.get_field(field_name) {
            field.protection == SchemaProtectionLevel::User
        } else {
            false
        }
    }

    /// Get the schema ID (same as node ID, e.g., "task", "person")
    pub fn schema_id(&self) -> &str {
        &self.node.id
    }

    /// Get the schema name (same as node content)
    pub fn name(&self) -> &str {
        &self.node.content
    }

    /// Get a reference to the underlying Node
    pub fn as_node(&self) -> &Node {
        &self.node
    }

    /// Get a mutable reference to the underlying Node
    pub fn as_node_mut(&mut self) -> &mut Node {
        &mut self.node
    }

    /// Convert back to universal Node (consumes wrapper)
    pub fn into_node(self) -> Node {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schema::SchemaProtectionLevel;
    use serde_json::json;

    fn create_test_schema_node() -> Node {
        Node::new(
            "schema".to_string(),
            "task".to_string(),
            json!({
                "isCore": true,
                "version": 2,
                "description": "Task tracking schema",
                "fields": [
                    {
                        "name": "status",
                        "type": "enum",
                        "protection": "core",
                        "coreValues": ["open", "done"],
                        "indexed": true
                    }
                ]
            }),
        )
    }

    #[test]
    fn test_from_node_valid() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();
        assert_eq!(schema.schema_id(), schema.as_node().id);
        assert_eq!(schema.name(), "task");
    }

    #[test]
    fn test_from_node_invalid_type() {
        let node = Node::new("task".to_string(), "Test".to_string(), json!({}));
        let result = SchemaNode::from_node(node);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_core() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();
        assert!(schema.is_core());
    }

    #[test]
    fn test_version() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();
        assert_eq!(schema.version(), 2);
    }

    #[test]
    fn test_increment_version() {
        let node = create_test_schema_node();
        let mut schema = SchemaNode::from_node(node).unwrap();
        assert_eq!(schema.version(), 2);
        schema.increment_version();
        assert_eq!(schema.version(), 3);
    }

    #[test]
    fn test_fields() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();
        let fields = schema.fields();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "status");
        assert_eq!(fields[0].field_type, "enum");
    }

    #[test]
    fn test_get_field() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();

        let field = schema.get_field("status").unwrap();
        assert_eq!(field.name, "status");

        assert!(schema.get_field("nonexistent").is_none());
    }

    #[test]
    fn test_add_field() {
        let node = create_test_schema_node();
        let mut schema = SchemaNode::from_node(node).unwrap();

        let new_field = SchemaField {
            name: "priority".to_string(),
            field_type: "number".to_string(),
            protection: SchemaProtectionLevel::User,
            core_values: None,
            user_values: None,
            indexed: false,
            required: Some(false),
            extensible: None,
            default: Some(json!(0)),
            description: Some("Priority level".to_string()),
            item_type: None,
            fields: None,
            item_fields: None,
        };

        schema.add_field(new_field).unwrap();
        assert_eq!(schema.fields().len(), 2);
        assert!(schema.get_field("priority").is_some());
    }

    #[test]
    fn test_add_duplicate_field_fails() {
        let node = create_test_schema_node();
        let mut schema = SchemaNode::from_node(node).unwrap();

        let duplicate = SchemaField {
            name: "status".to_string(), // Already exists
            field_type: "string".to_string(),
            protection: SchemaProtectionLevel::User,
            core_values: None,
            user_values: None,
            indexed: false,
            required: None,
            extensible: None,
            default: None,
            description: None,
            item_type: None,
            fields: None,
            item_fields: None,
        };

        let result = schema.add_field(duplicate);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_field() {
        let node = create_test_schema_node();
        let mut schema = SchemaNode::from_node(node).unwrap();

        let removed = schema.remove_field("status").unwrap();
        assert_eq!(removed.name, "status");
        assert_eq!(schema.fields().len(), 0);
    }

    #[test]
    fn test_get_enum_values() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();

        let values = schema.get_enum_values("status").unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&"open".to_string()));
        assert!(values.contains(&"done".to_string()));

        // Non-enum field should return None
        assert!(schema.get_enum_values("nonexistent").is_none());
    }

    #[test]
    fn test_can_delete_field() {
        let node = create_test_schema_node();
        let schema = SchemaNode::from_node(node).unwrap();

        // Core field cannot be deleted
        assert!(!schema.can_delete_field("status"));
        // Non-existent field returns false
        assert!(!schema.can_delete_field("nonexistent"));
    }

    #[test]
    fn test_into_node() {
        let node = create_test_schema_node();
        let original_id = node.id.clone();
        let schema = SchemaNode::from_node(node).unwrap();
        let node_back = schema.into_node();
        assert_eq!(node_back.id, original_id);
        assert_eq!(node_back.node_type, "schema");
    }
}
