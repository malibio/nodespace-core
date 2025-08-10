// NodeSpace Core Library
pub mod node;
pub mod storage;
pub mod error;
pub mod utils;

use tauri::{Builder, Manager};
use std::sync::Arc;

pub use node::*;
pub use storage::*;
pub use error::*;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Node management commands
#[tauri::command]
async fn create_node(
    node_data: NodeCreateRequest,
    state: tauri::State<'_, AppState>,
) -> Result<NodeResponse, String> {
    let node_manager = &state.node_manager;
    match node_manager.create_node(node_data.into()).await {
        Ok(node) => Ok(node.into()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn get_node(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<NodeResponse>, String> {
    let node_manager = &state.node_manager;
    match node_manager.get_node(&id).await {
        Ok(node) => Ok(node.map(|n| n.into())),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn update_node(
    node_data: NodeUpdateRequest,
    state: tauri::State<'_, AppState>,
) -> Result<NodeResponse, String> {
    let node_manager = &state.node_manager;
    match node_manager.update_node(node_data.into()).await {
        Ok(node) => Ok(node.into()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn delete_node(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let node_manager = &state.node_manager;
    match node_manager.delete_node(&id).await {
        Ok(deleted) => Ok(deleted),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn search_nodes(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<NodeResponse>, String> {
    let node_manager = &state.node_manager;
    match node_manager.search_nodes(&query).await {
        Ok(nodes) => Ok(nodes.into_iter().map(|n| n.into()).collect()),
        Err(e) => Err(e.to_string()),
    }
}

// Application state
pub struct AppState {
    pub node_manager: Arc<NodeManager>,
}

impl AppState {
    pub fn new() -> Result<Self, NodeError> {
        let storage = Arc::new(MockDataStore::new());
        let node_manager = Arc::new(NodeManager::new(storage));
        
        Ok(AppState {
            node_manager,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_state = AppState::new()
                .map_err(|e| format!("Failed to initialize app state: {}", e))?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            create_node,
            get_node,
            update_node,
            delete_node,
            search_nodes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_creation() {
        let app_state = AppState::new();
        assert!(app_state.is_ok());
    }

    #[test]
    fn test_greet_command() {
        let result = greet("World");
        assert_eq!(result, "Hello, World! You've been greeted from Rust!");
    }
}
