use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::helpers::default_version;
use crate::node::Node;

fn default_protection_level() -> SchemaProtectionLevel {
    SchemaProtectionLevel::User
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnumValue {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SchemaProtectionLevel {
    Core,
    User,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default = "default_protection_level")]
    pub protection: SchemaProtectionLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_values: Option<Vec<EnumValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_values: Option<Vec<EnumValue>>,
    #[serde(default)]
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
    pub item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<SchemaField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_fields: Option<Vec<SchemaField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EdgeField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipDirection {
    Out,
    In,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipCardinality {
    One,
    Many,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRelationship {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target_type: Option<String>,
    pub direction: RelationshipDirection,
    pub cardinality: RelationshipCardinality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_cardinality: Option<RelationshipCardinality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_fields: Option<Vec<EdgeField>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SchemaRelationship {
    /// Compute the edge table name for this relationship.
    ///
    /// Returns the explicit `edge_table` if set, otherwise computes it as
    /// `{source_type}_{name}_{target_type}` when `target_type` is set, or
    /// `{source_type}_{name}` when `target_type` is `None`.
    pub fn compute_edge_table_name(&self, source_type: &str) -> String {
        self.edge_table
            .clone()
            .unwrap_or_else(|| match &self.target_type {
                Some(target) => format!("{}_{}_{}", source_type, self.name, target),
                None => format!("{}_{}", source_type, self.name),
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaNode {
    pub id: String,
    pub content: String,
    #[serde(default = "default_version")]
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: Vec<SchemaField>,
    #[serde(default)]
    pub relationships: Vec<SchemaRelationship>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties_header_summary_template: Option<String>,
}

impl SchemaNode {
    pub fn from_node(node: Node) -> Result<Self, String> {
        if node.node_type != "schema" {
            return Err(format!("Expected 'schema', got '{}'", node.node_type));
        }

        let is_core = node
            .properties
            .get("isCore")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let schema_version = node
            .properties
            .get("schemaVersion")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        let description = node
            .properties
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let fields: Vec<SchemaField> = node
            .properties
            .get("fields")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let relationships: Vec<SchemaRelationship> = node
            .properties
            .get("relationships")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let title_template = node
            .properties
            .get("titleTemplate")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let properties_header_summary_template = node
            .properties
            .get("propertiesHeaderSummaryTemplate")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(Self {
            id: node.id,
            content: node.content,
            version: node.version,
            created_at: node.created_at,
            modified_at: node.modified_at,
            is_core,
            schema_version,
            description,
            fields,
            relationships,
            title_template,
            properties_header_summary_template,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_field() -> SchemaField {
        SchemaField {
            name: "status".to_string(),
            field_type: "enum".to_string(),
            protection: SchemaProtectionLevel::Core,
            core_values: Some(vec![
                EnumValue {
                    value: "open".to_string(),
                    label: "Open".to_string(),
                },
                EnumValue {
                    value: "done".to_string(),
                    label: "Done".to_string(),
                },
            ]),
            user_values: Some(vec![EnumValue {
                value: "blocked".to_string(),
                label: "Blocked".to_string(),
            }]),
            indexed: true,
            required: Some(true),
            extensible: Some(true),
            default: Some(json!("open")),
            description: Some("Task status".to_string()),
            item_type: None,
            fields: None,
            item_fields: None,
        }
    }

    #[test]
    fn test_schema_field_serialization() {
        let field = create_test_field();
        let json = serde_json::to_value(&field).unwrap();

        assert_eq!(json["name"], "status");
        assert_eq!(json["protection"], "core");
        // field_type serializes to "type" due to #[serde(rename = "type")]
        assert_eq!(json["type"], "enum");
        // core_values serializes to coreValues
        assert!(json["coreValues"].is_array());
        assert_eq!(json["indexed"], true);
    }

    #[test]
    fn test_schema_field_deserialization() {
        let json = json!({
            "name": "status",
            "type": "enum",
            "protection": "core",
            "coreValues": [
                { "value": "open", "label": "Open" },
                { "value": "done", "label": "Done" }
            ],
            "indexed": true
        });

        let field: SchemaField = serde_json::from_value(json).unwrap();
        assert_eq!(field.name, "status");
        assert_eq!(field.field_type, "enum");
        assert_eq!(field.protection, SchemaProtectionLevel::Core);
        assert!(field.indexed);

        let core_values = field.core_values.unwrap();
        assert_eq!(core_values.len(), 2);
        assert_eq!(core_values[0].value, "open");
        assert_eq!(core_values[0].label, "Open");
    }

    #[test]
    fn test_schema_field_rejects_snake_case_core_values() {
        // core_values is the Rust field name; the wire key is coreValues
        // (rename_all = "camelCase"). A payload using the snake_case name must
        // be rejected outright, not silently dropped as an unknown field.
        let json = json!({
            "name": "status",
            "type": "enum",
            "core_values": [
                { "value": "open", "label": "Open" },
                { "value": "done", "label": "Done" }
            ]
        });

        let err = serde_json::from_value::<SchemaField>(json).unwrap_err();
        assert!(
            err.to_string().contains("core_values"),
            "expected error naming the unknown field `core_values`, got: {}",
            err
        );
    }

    #[test]
    fn test_protection_level_serialization() {
        assert_eq!(
            serde_json::to_value(SchemaProtectionLevel::Core).unwrap(),
            "core"
        );
        assert_eq!(
            serde_json::to_value(SchemaProtectionLevel::User).unwrap(),
            "user"
        );
        assert_eq!(
            serde_json::to_value(SchemaProtectionLevel::System).unwrap(),
            "system"
        );
    }

    #[test]
    fn test_nested_field_serialization() {
        let address_field = SchemaField {
            name: "address".to_string(),
            field_type: "object".to_string(),
            protection: SchemaProtectionLevel::User,
            core_values: None,
            user_values: None,
            indexed: false,
            required: Some(false),
            extensible: None,
            default: None,
            description: Some("Address information".to_string()),
            item_type: None,
            fields: Some(vec![
                SchemaField {
                    name: "street".to_string(),
                    field_type: "string".to_string(),
                    protection: SchemaProtectionLevel::User,
                    core_values: None,
                    user_values: None,
                    indexed: false,
                    required: Some(false),
                    extensible: None,
                    default: None,
                    description: Some("Street address".to_string()),
                    item_type: None,
                    fields: None,
                    item_fields: None,
                },
                SchemaField {
                    name: "city".to_string(),
                    field_type: "string".to_string(),
                    protection: SchemaProtectionLevel::User,
                    core_values: None,
                    user_values: None,
                    indexed: true,
                    required: Some(false),
                    extensible: None,
                    default: None,
                    description: Some("City".to_string()),
                    item_type: None,
                    fields: None,
                    item_fields: None,
                },
            ]),
            item_fields: None,
        };

        let json = serde_json::to_value(&address_field).unwrap();
        assert_eq!(json["name"], "address");
        assert_eq!(json["type"], "object");
        assert_eq!(json["fields"][0]["name"], "street");
        assert_eq!(json["fields"][1]["name"], "city");
        assert_eq!(json["fields"][1]["indexed"], true);
    }

    #[test]
    fn test_nested_field_deserialization() {
        let json = json!({
            "name": "address",
            "type": "object",
            "protection": "user",
            "indexed": false,
            "fields": [
                {
                    "name": "city",
                    "type": "string",
                    "protection": "user",
                    "indexed": true
                }
            ]
        });

        let field: SchemaField = serde_json::from_value(json).unwrap();
        assert_eq!(field.name, "address");
        assert_eq!(field.field_type, "object");

        let nested_fields = field.fields.as_ref().unwrap();
        assert_eq!(nested_fields.len(), 1);
        assert_eq!(nested_fields[0].name, "city");
        assert!(nested_fields[0].indexed);
    }

    #[test]
    fn test_array_of_objects_serialization() {
        let contacts_field = SchemaField {
            name: "contacts".to_string(),
            field_type: "array".to_string(),
            protection: SchemaProtectionLevel::User,
            core_values: None,
            user_values: None,
            indexed: false,
            required: Some(false),
            extensible: None,
            default: None,
            description: Some("Contact list".to_string()),
            item_type: Some("object".to_string()),
            fields: None,
            item_fields: Some(vec![SchemaField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                protection: SchemaProtectionLevel::User,
                core_values: None,
                user_values: None,
                indexed: true,
                required: Some(false),
                extensible: None,
                default: None,
                description: Some("Email address".to_string()),
                item_type: None,
                fields: None,
                item_fields: None,
            }]),
        };

        let json = serde_json::to_value(&contacts_field).unwrap();
        assert_eq!(json["name"], "contacts");
        assert_eq!(json["type"], "array");
        // item_type serializes to itemType with camelCase
        assert_eq!(json["itemType"], "object");
        // item_fields serializes to itemFields with camelCase
        assert_eq!(json["itemFields"][0]["name"], "email");
        assert_eq!(json["itemFields"][0]["indexed"], true);
    }

    #[test]
    fn test_edge_field_serialization() {
        let field = EdgeField {
            name: "role".to_string(),
            field_type: "string".to_string(),
            indexed: Some(true),
            required: Some(false),
            default: Some(json!("member")),
            target_type: None,
            description: Some("Assignment role".to_string()),
        };

        let json = serde_json::to_value(&field).unwrap();
        assert_eq!(json["name"], "role");
        assert_eq!(json["type"], "string");
        assert_eq!(json["indexed"], true);
        assert_eq!(json["required"], false);
        assert_eq!(json["default"], "member");
        assert_eq!(json["description"], "Assignment role");
        // target_type should be absent (skip_serializing_if = None)
        assert!(json.get("targetType").is_none());
    }

    #[test]
    fn test_edge_field_deserialization() {
        let json = json!({
            "name": "billing_date",
            "type": "date",
            "required": true,
            "indexed": true
        });

        let field: EdgeField = serde_json::from_value(json).unwrap();
        assert_eq!(field.name, "billing_date");
        assert_eq!(field.field_type, "date");
        assert_eq!(field.required, Some(true));
        assert_eq!(field.indexed, Some(true));
        assert!(field.default.is_none());
        assert!(field.target_type.is_none());
        assert!(field.description.is_none());
    }

    #[test]
    fn test_edge_field_with_record_type() {
        let field = EdgeField {
            name: "approved_by".to_string(),
            field_type: "record".to_string(),
            indexed: Some(true),
            required: None,
            default: None,
            target_type: Some("person".to_string()),
            description: Some("Who approved this".to_string()),
        };

        let json = serde_json::to_value(&field).unwrap();
        assert_eq!(json["type"], "record");
        assert_eq!(json["targetType"], "person");
    }

    #[test]
    fn test_edge_field_minimal() {
        // Test minimal edge field (only required fields)
        let json = json!({
            "name": "simple",
            "type": "string"
        });

        let field: EdgeField = serde_json::from_value(json).unwrap();
        assert_eq!(field.name, "simple");
        assert_eq!(field.field_type, "string");
        assert!(field.indexed.is_none());
        assert!(field.required.is_none());
        assert!(field.default.is_none());
    }

    #[test]
    fn test_relationship_direction_serialization() {
        assert_eq!(
            serde_json::to_value(RelationshipDirection::Out).unwrap(),
            "out"
        );
        assert_eq!(
            serde_json::to_value(RelationshipDirection::In).unwrap(),
            "in"
        );
    }

    #[test]
    fn test_relationship_direction_deserialization() {
        let out: RelationshipDirection = serde_json::from_value(json!("out")).unwrap();
        assert_eq!(out, RelationshipDirection::Out);

        let r#in: RelationshipDirection = serde_json::from_value(json!("in")).unwrap();
        assert_eq!(r#in, RelationshipDirection::In);
    }

    #[test]
    fn test_relationship_cardinality_serialization() {
        assert_eq!(
            serde_json::to_value(RelationshipCardinality::One).unwrap(),
            "one"
        );
        assert_eq!(
            serde_json::to_value(RelationshipCardinality::Many).unwrap(),
            "many"
        );
    }

    #[test]
    fn test_relationship_cardinality_deserialization() {
        let one: RelationshipCardinality = serde_json::from_value(json!("one")).unwrap();
        assert_eq!(one, RelationshipCardinality::One);

        let many: RelationshipCardinality = serde_json::from_value(json!("many")).unwrap();
        assert_eq!(many, RelationshipCardinality::Many);
    }

    #[test]
    fn test_schema_relationship_serialization() {
        let relationship = SchemaRelationship {
            name: "billed_to".to_string(),
            target_type: Some("customer".to_string()),
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::One,
            required: Some(true),
            reverse_name: Some("invoices".to_string()),
            reverse_cardinality: Some(RelationshipCardinality::Many),
            edge_table: None,
            edge_fields: Some(vec![
                EdgeField {
                    name: "billing_date".to_string(),
                    field_type: "date".to_string(),
                    indexed: Some(true),
                    required: Some(true),
                    default: None,
                    target_type: None,
                    description: None,
                },
                EdgeField {
                    name: "payment_terms".to_string(),
                    field_type: "string".to_string(),
                    indexed: None,
                    required: None,
                    default: Some(json!("net-30")),
                    target_type: None,
                    description: None,
                },
            ]),
            description: Some("Customer this invoice is billed to".to_string()),
        };

        let json = serde_json::to_value(&relationship).unwrap();

        assert_eq!(json["name"], "billed_to");
        assert_eq!(json["targetType"], "customer");
        assert_eq!(json["direction"], "out");
        assert_eq!(json["cardinality"], "one");
        assert_eq!(json["required"], true);
        assert_eq!(json["reverseName"], "invoices");
        assert_eq!(json["reverseCardinality"], "many");
        assert!(json.get("edgeTable").is_none()); // Not set, should be absent
        assert_eq!(json["edgeFields"].as_array().unwrap().len(), 2);
        assert_eq!(json["edgeFields"][0]["name"], "billing_date");
        assert_eq!(json["edgeFields"][1]["default"], "net-30");
    }

    #[test]
    fn test_schema_relationship_deserialization() {
        let json = json!({
            "name": "assigned_to",
            "targetType": "person",
            "direction": "out",
            "cardinality": "many",
            "reverseName": "tasks",
            "reverseCardinality": "many",
            "edgeFields": [
                {
                    "name": "role",
                    "type": "string",
                    "indexed": true
                },
                {
                    "name": "assigned_at",
                    "type": "date",
                    "required": true
                }
            ]
        });

        let relationship: SchemaRelationship = serde_json::from_value(json).unwrap();

        assert_eq!(relationship.name, "assigned_to");
        assert_eq!(relationship.target_type, Some("person".to_string()));
        assert_eq!(relationship.direction, RelationshipDirection::Out);
        assert_eq!(relationship.cardinality, RelationshipCardinality::Many);
        assert_eq!(relationship.reverse_name, Some("tasks".to_string()));
        assert_eq!(
            relationship.reverse_cardinality,
            Some(RelationshipCardinality::Many)
        );
        assert!(relationship.required.is_none());
        assert!(relationship.edge_table.is_none());

        let edge_fields = relationship.edge_fields.unwrap();
        assert_eq!(edge_fields.len(), 2);
        assert_eq!(edge_fields[0].name, "role");
        assert_eq!(edge_fields[1].name, "assigned_at");
    }

    #[test]
    fn test_schema_relationship_minimal() {
        // Test minimal relationship (only required fields)
        let json = json!({
            "name": "parent_of",
            "targetType": "document",
            "direction": "out",
            "cardinality": "many"
        });

        let relationship: SchemaRelationship = serde_json::from_value(json).unwrap();

        assert_eq!(relationship.name, "parent_of");
        assert_eq!(relationship.target_type, Some("document".to_string()));
        assert_eq!(relationship.direction, RelationshipDirection::Out);
        assert_eq!(relationship.cardinality, RelationshipCardinality::Many);
        assert!(relationship.required.is_none());
        assert!(relationship.reverse_name.is_none());
        assert!(relationship.reverse_cardinality.is_none());
        assert!(relationship.edge_table.is_none());
        assert!(relationship.edge_fields.is_none());
        assert!(relationship.description.is_none());
    }

    #[test]
    fn test_schema_relationship_with_custom_edge_table() {
        let relationship = SchemaRelationship {
            name: "collaborates_with".to_string(),
            target_type: Some("person".to_string()),
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::Many,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: Some("collaborations".to_string()),
            edge_fields: None,
            description: None,
        };

        let json = serde_json::to_value(&relationship).unwrap();
        assert_eq!(json["edgeTable"], "collaborations");
    }

    #[test]
    fn test_compute_edge_table_name_auto() {
        let relationship = SchemaRelationship {
            name: "billed_to".to_string(),
            target_type: Some("customer".to_string()),
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::One,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: None,
            edge_fields: None,
            description: None,
        };

        let edge_table = relationship.compute_edge_table_name("invoice");
        assert_eq!(edge_table, "invoice_billed_to_customer");
    }

    #[test]
    fn test_compute_edge_table_name_explicit() {
        let relationship = SchemaRelationship {
            name: "assigned_to".to_string(),
            target_type: Some("person".to_string()),
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::Many,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: Some("assignments".to_string()),
            edge_fields: None,
            description: None,
        };

        let edge_table = relationship.compute_edge_table_name("task");
        assert_eq!(edge_table, "assignments"); // Uses explicit name, ignores source_type
    }

    #[test]
    fn test_schema_relationship_incoming_direction() {
        // Test "in" direction (less common but valid)
        let json = json!({
            "name": "owned_by",
            "targetType": "organization",
            "direction": "in",
            "cardinality": "one"
        });

        let relationship: SchemaRelationship = serde_json::from_value(json).unwrap();
        assert_eq!(relationship.direction, RelationshipDirection::In);
    }

    #[test]
    fn test_schema_relationship_untyped_deserialization() {
        // target_type absent → None (untyped/generic relationship)
        let json = json!({
            "name": "related",
            "direction": "out",
            "cardinality": "many"
        });

        let relationship: SchemaRelationship = serde_json::from_value(json).unwrap();
        assert_eq!(relationship.name, "related");
        assert!(relationship.target_type.is_none());
    }

    #[test]
    fn test_schema_relationship_untyped_serialization() {
        let relationship = SchemaRelationship {
            name: "related".to_string(),
            target_type: None,
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::Many,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: None,
            edge_fields: None,
            description: None,
        };

        let json = serde_json::to_value(&relationship).unwrap();
        assert_eq!(json["name"], "related");
        // targetType absent when None
        assert!(json.get("targetType").is_none());
    }

    #[test]
    fn test_compute_edge_table_name_none_target() {
        // When target_type is None, table name is {source}_{name}
        let relationship = SchemaRelationship {
            name: "related".to_string(),
            target_type: None,
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::Many,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: None,
            edge_fields: None,
            description: None,
        };

        let edge_table = relationship.compute_edge_table_name("note");
        assert_eq!(edge_table, "note_related");
    }

    #[test]
    fn test_compute_edge_table_name_some_target() {
        // When target_type is Some, table name is {source}_{name}_{target}
        let relationship = SchemaRelationship {
            name: "billed_to".to_string(),
            target_type: Some("customer".to_string()),
            direction: RelationshipDirection::Out,
            cardinality: RelationshipCardinality::One,
            required: None,
            reverse_name: None,
            reverse_cardinality: None,
            edge_table: None,
            edge_fields: None,
            description: None,
        };

        let edge_table = relationship.compute_edge_table_name("invoice");
        assert_eq!(edge_table, "invoice_billed_to_customer");
    }
}
