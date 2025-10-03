//! Node CRUD operation commands for Text, Task, and Date nodes

use nodespace_core::{Node, NodeService, NodeUpdate};
use tauri::State;

/// Allowed node types for initial E2E testing
const ALLOWED_NODE_TYPES: &[&str] = &["text", "task", "date"];

/// Validate that node type is supported
fn validate_node_type(node_type: &str) -> Result<(), String> {
    if !ALLOWED_NODE_TYPES.contains(&node_type) {
        return Err(format!(
            "Only text, task, and date nodes are supported. Got: {}",
            node_type
        ));
    }
    Ok(())
}

/// Create a new node (Text, Task, or Date only)
#[tauri::command]
pub async fn create_node(service: State<'_, NodeService>, node: Node) -> Result<String, String> {
    validate_node_type(&node.node_type)?;

    service
        .create_node(node)
        .await
        .map_err(|e| format!("Failed to create node: {}", e))
}

/// Get a node by ID
#[tauri::command]
pub async fn get_node(service: State<'_, NodeService>, id: String) -> Result<Option<Node>, String> {
    service
        .get_node(&id)
        .await
        .map_err(|e| format!("Failed to get node: {}", e))
}

/// Update an existing node
#[tauri::command]
pub async fn update_node(
    service: State<'_, NodeService>,
    id: String,
    update: NodeUpdate,
) -> Result<(), String> {
    service
        .update_node(&id, update)
        .await
        .map_err(|e| format!("Failed to update node: {}", e))
}

/// Delete a node by ID
#[tauri::command]
pub async fn delete_node(service: State<'_, NodeService>, id: String) -> Result<(), String> {
    service
        .delete_node(&id)
        .await
        .map_err(|e| format!("Failed to delete node: {}", e))
}

/// Get child nodes of a parent node
#[tauri::command]
pub async fn get_children(
    service: State<'_, NodeService>,
    parent_id: String,
) -> Result<Vec<Node>, String> {
    service
        .get_children(&parent_id)
        .await
        .map_err(|e| format!("Failed to get children: {}", e))
}
