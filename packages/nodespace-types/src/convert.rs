use chrono::{DateTime, NaiveDate, Utc};
use std::str::FromStr;

use crate::ai_chat::{AiChatMessage, AiChatNode};
use crate::node::Node;
use crate::schema::SchemaNode;
use crate::task::{TaskNode, TaskPriority, TaskStatus};

fn normalize_date_field(s: &str) -> String {
    if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
        return s.to_string();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.format("%Y-%m-%d").to_string();
    }
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return dt.format("%Y-%m-%d").to_string();
    }
    s.to_string()
}

/// Convert a `Node` to its strongly-typed JSON representation for the frontend.
///
/// For typed nodes (`task`, `ai-chat`, `schema`), promotes type-specific
/// properties to top-level fields. For all other types, returns the generic
/// node shape. Adds a `nodespace://` URI field for rich client rendering.
///
/// This is the single canonical implementation used by all entry points
/// (Tauri commands, MCP, HTTP) and the SOLE authority for property flattening
/// and `nodespace://` URI production. Do NOT re-implement either in another layer
/// (e.g. a TypeScript-side flatten in the frontend converters) — the frontend
/// `nodeTo*` converters trust the flat, top-level shape this function guarantees.
/// The `wire_contract` tests below pin that shape.
pub fn node_to_typed_value(node: Node) -> Result<serde_json::Value, String> {
    let mut node = node;
    flatten_properties_for_api(&mut node);

    let node_id = node.id.clone();
    let mut value = match node.node_type.as_str() {
        "task" => task_node_to_value(node),
        "ai-chat" => ai_chat_node_to_value(node),
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

/// Convert a vec of nodes to their strongly-typed JSON representations.
pub fn nodes_to_typed_values(nodes: Vec<Node>) -> Result<Vec<serde_json::Value>, String> {
    nodes.into_iter().map(node_to_typed_value).collect()
}

/// Flatten namespaced properties for API response.
///
/// Storage format: `{ "task": { "status": "open" } }`
/// API format:     `{ "status": "open" }`
///
/// Dormant namespaces (from previous type changes) are not exposed.
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

fn task_node_to_value(node: Node) -> Result<serde_json::Value, String> {
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
        .map(normalize_date_field);

    let assignee = props
        .get("assignee")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let started_at = props
        .get("startedAt")
        .or_else(|| props.get("started_at"))
        .and_then(|v| v.as_str())
        .map(normalize_date_field);

    let completed_at = props
        .get("completedAt")
        .or_else(|| props.get("completed_at"))
        .and_then(|v| v.as_str())
        .map(normalize_date_field);

    let lifecycle_status = node.lifecycle_status.clone();
    let title = node.title.clone();
    let task = TaskNode {
        id: node.id,
        node_type: node.node_type,
        content: node.content,
        title,
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

fn ai_chat_node_to_value(node: Node) -> Result<serde_json::Value, String> {
    let props = &node.properties;

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

    let lifecycle_status = node.lifecycle_status.clone();
    let chat = AiChatNode {
        id: node.id,
        node_type: node.node_type,
        content: node.content,
        version: node.version,
        created_at: node.created_at,
        modified_at: node.modified_at,
        properties: node.properties,
        lifecycle_status,
        status,
        provider,
        model,
        messages,
    };

    serde_json::to_value(&chat).map_err(|e| format!("Failed to serialize ai-chat node: {}", e))
}

#[cfg(test)]
mod wire_contract {
    use super::*;
    use crate::node::Node;

    // These tests pin the wire contract that the frontend TS converters trust:
    // for typed nodes, type-specific fields are promoted to TOP-LEVEL and the
    // namespaced `properties.<type>` form is flattened away. If this contract
    // ever changes, the frontend nodeTo* converters must change in lockstep.

    #[test]
    fn task_promotes_fields_top_level_and_flattens_properties() {
        let node = Node::new(
            "task".to_string(),
            "Buy milk".to_string(),
            serde_json::json!({
                "task": { "status": "in_progress", "priority": "high", "custom:store": "Costco" }
            }),
        );
        let out = node_to_typed_value(node).unwrap();

        // Core fields promoted to top level.
        assert_eq!(out["status"], "in_progress");
        assert_eq!(out["priority"], "high");
        // `properties` is flattened: no `task` namespace survives.
        assert!(out["properties"].get("task").is_none());
        // User/custom fields remain in flat `properties`.
        assert_eq!(out["properties"]["custom:store"], "Costco");
        // URI is injected by the backend.
        assert!(out["uri"].as_str().unwrap().starts_with("nodespace://"));
    }

    #[test]
    fn ai_chat_promotes_fields_top_level_and_flattens_properties() {
        let node = Node::new(
            "ai-chat".to_string(),
            "Chat".to_string(),
            serde_json::json!({
                "ai-chat": {
                    "status": "active",
                    "provider": "ollama",
                    "messages": [{ "role": "user", "content": "hi" }]
                }
            }),
        );
        let out = node_to_typed_value(node).unwrap();

        assert_eq!(out["status"], "active");
        assert_eq!(out["provider"], "ollama");
        assert_eq!(out["messages"][0]["content"], "hi");
        assert!(out["properties"].get("ai-chat").is_none());
        assert!(out["uri"].as_str().unwrap().starts_with("nodespace://"));
    }

    #[test]
    fn schema_promotes_fields_top_level_and_injects_uri() {
        let node = Node::new(
            "schema".to_string(),
            "Task schema".to_string(),
            serde_json::json!({
                "isCore": true,
                "schemaVersion": 2,
                "description": "Task type",
                "fields": []
            }),
        );
        let out = node_to_typed_value(node).unwrap();

        // Schema fields are promoted to the top level by SchemaNode.
        assert_eq!(out["isCore"], true);
        assert_eq!(out["schemaVersion"], 2);
        assert_eq!(out["description"], "Task type");
        assert!(out["uri"].as_str().unwrap().starts_with("nodespace://"));
    }
}
