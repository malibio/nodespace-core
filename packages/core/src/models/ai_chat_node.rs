//! Strongly-Typed AiChatNode
//!
//! Provides compile-time type safety for `ai-chat` nodes, replacing the fragile
//! raw `serde_json::Value` path lookups that the daemon previously used to read
//! and write chat messages, status, model, and provider.
//!
//! Mirrors the [`TaskNode`](crate::models::TaskNode) pattern: data is stored in
//! the node's `properties` under a type namespace (`properties["ai-chat"]`), and
//! `from_node` / `into_node` convert between the universal [`Node`] and this
//! strongly-typed struct.
//!
//! # Status values
//!
//! `status` is intentionally a plain `String`, not an enum: the daemon writes
//! `"processing"` / `"idle"` while the frontend interface declares
//! `active | processing | archived`. These are not yet reconciled, so the struct
//! captures the value verbatim rather than forcing a lossy mapping.

use crate::models::{Node, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The node type discriminator for ai-chat nodes.
pub const AI_CHAT_NODE_TYPE: &str = "ai-chat";

/// The properties namespace key under which ai-chat data is stored.
const AI_CHAT_NAMESPACE: &str = "ai-chat";

fn default_version() -> i64 {
    1
}

/// A single message in an ai-chat conversation.
///
/// Mirrors the frontend `AiChatMessage` TypeScript interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessage {
    /// Sender role: `"user"`, `"assistant"`, or `"system"`.
    pub role: String,

    /// Message text.
    pub content: String,

    /// When the message was created (RFC3339), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,

    /// Model chain-of-thought reasoning toward the answer, when captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Strongly-typed view of an `ai-chat` node.
///
/// Construct from a generic [`Node`] with [`AiChatNode::from_node`] and convert
/// back with [`AiChatNode::into_node`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatNode {
    /// Unique identifier.
    pub id: String,

    /// Node type (always `"ai-chat"`).
    #[serde(rename = "nodeType")]
    pub node_type: String,

    /// Title/label of the chat (the node's primary content).
    pub content: String,

    /// Optimistic concurrency control version.
    #[serde(default = "default_version")]
    pub version: i64,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp.
    pub modified_at: DateTime<Utc>,

    /// Conversation status (e.g. `"processing"`, `"idle"`). Plain string —
    /// see the module docs for why this is not an enum.
    #[serde(default)]
    pub status: String,

    /// Inference provider (e.g. `"native"`, `"ollama"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model identifier used for inference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Ordered conversation history.
    #[serde(default)]
    pub messages: Vec<AiChatMessage>,
}

impl AiChatNode {
    /// Build an [`AiChatNode`] from a generic [`Node`].
    ///
    /// Reads ai-chat fields from the `properties["ai-chat"]` namespace, falling
    /// back to flat `properties` when the namespace is absent (e.g. after
    /// `flatten_properties_for_api` has promoted them). Mirrors the dual
    /// nested/flat handling in [`TaskNode::from_node`](crate::models::TaskNode).
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidNodeType`] when the node is not an
    /// ai-chat node.
    pub fn from_node(node: Node) -> Result<Self, ValidationError> {
        if node.node_type != AI_CHAT_NODE_TYPE {
            return Err(ValidationError::InvalidNodeType(format!(
                "Expected '{AI_CHAT_NODE_TYPE}', got '{}'",
                node.node_type
            )));
        }

        // Prefer the nested namespace; fall back to flat properties.
        let props = node
            .properties
            .get(AI_CHAT_NAMESPACE)
            .filter(|v| v.is_object())
            .unwrap_or(&node.properties);

        let status = props
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let provider = props
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let model = props
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let messages = props
            .get("messages")
            .cloned()
            .map(|v| serde_json::from_value::<Vec<AiChatMessage>>(v).unwrap_or_default())
            .unwrap_or_default();

        Ok(Self {
            id: node.id,
            node_type: AI_CHAT_NODE_TYPE.to_string(),
            content: node.content,
            version: node.version,
            created_at: node.created_at,
            modified_at: node.modified_at,
            status,
            provider,
            model,
            messages,
        })
    }

    /// Serialize the ai-chat fields as the namespace value
    /// (`{ "status": ..., "provider": ..., "model": ..., "messages": [...] }`).
    ///
    /// Use this to splice the typed fields back into an existing `properties`
    /// object without disturbing sibling namespaces — the read-modify-write
    /// pattern the daemon relies on.
    ///
    /// `AiChatNode` is the authoritative model for the `"ai-chat"` namespace:
    /// this rebuilds the namespace value entirely from the struct's fields, so
    /// every key stored under `"ai-chat"` MUST be represented as a field here.
    /// An unmodeled key would be dropped on the next write — when adding a field
    /// to the stored shape, add it to the struct too. (Sibling namespaces
    /// outside `"ai-chat"` are untouched; only this namespace is rebuilt.)
    pub fn to_properties_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("status".to_string(), serde_json::json!(self.status));
        if let Some(provider) = &self.provider {
            map.insert("provider".to_string(), serde_json::json!(provider));
        }
        if let Some(model) = &self.model {
            map.insert("model".to_string(), serde_json::json!(model));
        }
        map.insert(
            "messages".to_string(),
            serde_json::to_value(&self.messages).unwrap_or(serde_json::Value::Array(vec![])),
        );
        serde_json::Value::Object(map)
    }

    /// Convert back into a universal [`Node`], storing ai-chat fields under the
    /// `properties["ai-chat"]` namespace.
    ///
    /// Note: this produces a fresh `properties` object containing only the
    /// ai-chat namespace. When you need to preserve sibling namespaces, prefer
    /// merging [`AiChatNode::to_properties_value`] into the original
    /// `node.properties` instead.
    pub fn into_node(self) -> Node {
        let mut properties = serde_json::Map::new();
        properties.insert(AI_CHAT_NAMESPACE.to_string(), self.to_properties_value());

        Node {
            id: self.id,
            node_type: AI_CHAT_NODE_TYPE.to_string(),
            content: self.content,
            version: self.version,
            created_at: self.created_at,
            modified_at: self.modified_at,
            properties: serde_json::Value::Object(properties),
            mentions: Vec::new(),
            mentioned_in: Vec::new(),
            title: None, // ai-chat nodes have no indexed title
            lifecycle_status: "active".to_string(),
        }
    }

    /// Build a temporary [`Node`] without consuming `self`.
    pub fn as_node(&self) -> Node {
        self.clone().into_node()
    }
}
