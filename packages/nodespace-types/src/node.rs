use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::helpers::default_lifecycle_status;

fn default_version() -> i64 {
    1
}
fn is_active_lifecycle(s: &str) -> bool {
    s == "active"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeReference {
    pub id: String,
    pub title: Option<String>,
    pub node_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl Node {
    pub fn new(node_type: String, content: String, properties: serde_json::Value) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            node_type,
            content,
            version: 1,
            created_at: now,
            modified_at: now,
            properties,
            mentions: Vec::new(),
            mentioned_in: Vec::new(),
            title: None,
            lifecycle_status: "active".to_string(),
        }
    }

    pub fn new_with_id(
        id: String,
        node_type: String,
        content: String,
        properties: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            node_type,
            content,
            version: 1,
            created_at: now,
            modified_at: now,
            properties,
            mentions: Vec::new(),
            mentioned_in: Vec::new(),
            title: None,
            lifecycle_status: "active".to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.id.is_empty() {
            return Err(ValidationError::MissingField("id".to_string()));
        }
        if self.node_type.is_empty() {
            return Err(ValidationError::MissingField("node_type".to_string()));
        }
        if !self.properties.is_object() {
            return Err(ValidationError::InvalidProperties(
                "properties must be a JSON object".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.modified_at = Utc::now();
    }

    pub fn set_properties(&mut self, properties: serde_json::Value) {
        self.properties = properties;
        self.modified_at = Utc::now();
    }

    pub fn merge_properties(&mut self, updates: serde_json::Value) {
        if let (Some(existing), Some(new)) = (self.properties.as_object_mut(), updates.as_object())
        {
            for (key, value) in new {
                existing.insert(key.clone(), value.clone());
            }
            self.modified_at = Utc::now();
        }
    }
}

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

impl NodeQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn by_id(id: String) -> Self {
        Self {
            id: Some(id),
            ..Default::default()
        }
    }

    pub fn mentioned_by(node_id: String) -> Self {
        Self {
            mentioned_by: Some(node_id),
            ..Default::default()
        }
    }

    pub fn content_contains(search: String) -> Self {
        Self {
            content_contains: Some(search),
            ..Default::default()
        }
    }

    pub fn by_type(node_type: String) -> Self {
        Self {
            node_type: Some(node_type),
            ..Default::default()
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

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

impl NodeUpdate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = Some(properties);
        self
    }

    pub fn with_node_type(mut self, node_type: String) -> Self {
        self.node_type = Some(node_type);
        self
    }

    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_lifecycle_status(mut self, lifecycle_status: String) -> Self {
        self.lifecycle_status = Some(lifecycle_status);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.node_type.is_none()
            && self.content.is_none()
            && self.properties.is_none()
            && self.title.is_none()
            && self.lifecycle_status.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResult {
    pub existed: bool,
    pub deleted_count: u64,
}

impl DeleteResult {
    pub fn existed() -> Self {
        Self {
            existed: true,
            deleted_count: 1,
        }
    }

    pub fn not_found() -> Self {
        Self {
            existed: false,
            deleted_count: 0,
        }
    }
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Invalid node type: {0}")]
    InvalidNodeType(String),
    #[error("Invalid node ID format: {0}")]
    InvalidId(String),
    #[error("Invalid parent reference: {0}")]
    InvalidParent(String),
    #[error("Invalid root reference: {0}")]
    InvalidRoot(String),
    #[error("Properties validation failed: {0}")]
    InvalidProperties(String),
}
