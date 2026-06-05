use chrono::DateTime;
use std::str::FromStr;

use crate::ai_chat::{AiChatMessage, AiChatNode};
use crate::node::Node;
use crate::schema::SchemaNode;
use crate::task::{TaskNode, TaskPriority, TaskStatus};

/// Convert a `Node` to its strongly-typed JSON representation for the frontend.
///
/// For typed nodes (`task`, `ai-chat`, `schema`), promotes type-specific
/// properties to top-level fields. For all other types, returns the generic
/// node shape. Adds a `nodespace://` URI field for rich client rendering.
///
/// This is the single canonical implementation used by all entry points
/// (Tauri commands, MCP, HTTP).
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
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

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
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let completed_at = props
        .get("completedAt")
        .or_else(|| props.get("completed_at"))
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

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
