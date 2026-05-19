//! Local type mirrors for Tauri command layer.
//!
//! These structs replicate the shapes used by `packages/core` that the commands need
//! for deserialization (inputs from Svelte) and serialization (outputs to Svelte).
//! Defining them here severs the direct `nodespace_core` dependency from the command
//! files while keeping the wire format identical.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeReference {
    pub id: String,
    pub title: Option<String>,
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub node_type: String,
    pub content: String,
    #[serde(default = "default_version")]
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub properties: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentioned_in: Vec<NodeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(
        default = "default_lifecycle_status",
        skip_serializing_if = "is_active_lifecycle"
    )]
    pub lifecycle_status: String,
}

fn default_version() -> i64 {
    1
}

fn default_lifecycle_status() -> String {
    "active".to_string()
}

fn is_active_lifecycle(s: &str) -> bool {
    s == "active"
}

// ---------------------------------------------------------------------------
// NodeQuery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentioned_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// NodeUpdate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_status: Option<String>,
}

// ---------------------------------------------------------------------------
// DeleteResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResult {
    pub existed: bool,
}

// ---------------------------------------------------------------------------
// TaskStatus / TaskPriority / TaskNodeUpdate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Open,
    InProgress,
    Done,
    Cancelled,
    User(String),
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "open" => Self::Open,
            "in_progress" => Self::InProgress,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            other => Self::User(other.to_string()),
        })
    }
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::User(s) => s.as_str(),
        }
    }
}

impl Serialize for TaskStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskPriority {
    Low,
    #[default]
    Medium,
    High,
    User(String),
}

impl FromStr for TaskPriority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            other => Self::User(other.to_string()),
        })
    }
}

impl TaskPriority {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::User(s) => s.as_str(),
        }
    }
}

impl Serialize for TaskPriority {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskPriority {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::from_str(&s).unwrap())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskNodeUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Option<TaskPriority>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub started_at: Option<Option<DateTime<Utc>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "flexible_date::deserialize_with_null"
    )]
    pub completed_at: Option<Option<DateTime<Utc>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

mod flexible_date {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer};

    pub fn deserialize_with_null<'de, D>(
        deserializer: D,
    ) -> Result<Option<Option<DateTime<Utc>>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<Option<String>> = Option::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(None) => Ok(Some(None)),
            Some(Some(s)) => {
                let dt = DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .or_else(|_| {
                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
                    })
                    .map_err(serde::de::Error::custom)?;
                Ok(Some(Some(dt)))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SchemaNode and related types
// ---------------------------------------------------------------------------

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

fn default_protection_level() -> SchemaProtectionLevel {
    SchemaProtectionLevel::User
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipDirection {
    Out,
    In,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipCardinality {
    One,
    Many,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn default_schema_version() -> u32 {
    1
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
            .map(|s| s.to_string())
            .unwrap_or_default();

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

// ---------------------------------------------------------------------------
// node_to_typed_value — converts Node → strongly-typed JSON for the frontend
// ---------------------------------------------------------------------------

pub fn node_to_typed_value(node: Node) -> Result<serde_json::Value, String> {
    let mut node = node;
    flatten_properties_for_api(&mut node);

    let node_id = node.id.clone();
    let mut value = match node.node_type.as_str() {
        "task" => task_node_to_value(node),
        "schema" => SchemaNode::from_node(node).and_then(|s| {
            serde_json::to_value(s).map_err(|e| format!("Failed to serialize schema: {}", e))
        }),
        _ => serde_json::to_value(node).map_err(|e| format!("Failed to serialize node: {}", e)),
    }?;

    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "uri".to_string(),
            serde_json::Value::String(format!("nodespace://{}", node_id)),
        );
    }

    Ok(value)
}

pub fn nodes_to_typed_values(nodes: Vec<Node>) -> Result<Vec<serde_json::Value>, String> {
    nodes.into_iter().map(node_to_typed_value).collect()
}

fn flatten_properties_for_api(node: &mut Node) {
    let node_type = node.node_type.clone();

    let Some(props_obj) = node.properties.as_object() else {
        return;
    };

    if let Some(type_namespace) = props_obj.get(&node_type) {
        if let Some(type_props) = type_namespace.as_object() {
            let flat: serde_json::Map<String, serde_json::Value> = type_props
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            node.properties = serde_json::Value::Object(flat);
            return;
        }
    }

    let flat: serde_json::Map<String, serde_json::Value> = props_obj
        .iter()
        .filter(|(k, v)| !v.is_object() && !k.starts_with('_'))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    node.properties = serde_json::Value::Object(flat);
}

/// Local mirror of core's TaskNode — produces the flat top-level field shape
/// the frontend TypeScript interface expects:
/// { id, nodeType, content, version, ..., status, priority, dueDate, ... }
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskNode {
    id: String,
    #[serde(rename = "nodeType")]
    node_type: String,
    content: String,
    #[serde(default = "default_version")]
    version: i64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    properties: serde_json::Value,
    #[serde(default, skip_serializing_if = "is_active_lifecycle")]
    lifecycle_status: String,
    // Task-specific fields at top level (mirrors core TaskNode layout)
    status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<TaskPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<DateTime<Utc>>,
}

fn task_node_to_value(node: Node) -> Result<serde_json::Value, String> {
    // Properties are already flattened by flatten_properties_for_api before this call.
    let props = &node.properties;

    let status: TaskStatus = props
        .get("status")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();

    let priority = props
        .get("priority")
        .and_then(|v| v.as_str())
        .map(|s| TaskPriority::from_str(s).unwrap_or_default());

    let due_date = props
        .get("dueDate")
        .or_else(|| props.get("due_date"))
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let assignee = props
        .get("assignee")
        .or_else(|| props.get("assignee_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let started_at = props
        .get("startedAt")
        .or_else(|| props.get("started_at"))
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let completed_at = props
        .get("completedAt")
        .or_else(|| props.get("completed_at"))
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let lifecycle_status = node.lifecycle_status.clone();
    let task = TaskNode {
        id: node.id,
        node_type: node.node_type,
        content: node.content,
        version: node.version,
        created_at: node.created_at,
        modified_at: node.modified_at,
        properties: node.properties,
        lifecycle_status,
        status,
        priority,
        due_date,
        assignee,
        started_at,
        completed_at,
    };

    serde_json::to_value(&task).map_err(|e| format!("Failed to serialize task node: {}", e))
}
