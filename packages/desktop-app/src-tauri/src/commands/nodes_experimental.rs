//! Experimental LanceDB parallel commands for CRUD validation
//!
//! This module implements parallel CRUD operations running both Turso and LanceDB
//! simultaneously for comparison and validation. These commands are experimental
//! and enabled only via the EXPERIMENTAL_USE_LANCEDB environment variable.

use crate::datastore::lance::LanceDataStore;
use nodespace_core::{Node, NodeService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tauri::State;
use tokio::sync::RwLock;

/// Comparison result for side-by-side validation
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonResult {
    /// Time taken by Turso operation in milliseconds
    pub turso_time_ms: f64,
    /// Time taken by LanceDB operation in milliseconds
    pub lance_time_ms: f64,
    /// Whether results match between backends
    pub results_match: bool,
    /// Result from Turso (serialized as JSON)
    pub turso_result: serde_json::Value,
    /// Result from LanceDB (serialized as JSON)
    pub lance_result: serde_json::Value,
    /// List of differences found (if any)
    pub differences: Vec<String>,
}

/// Structured error for experimental commands
#[derive(Debug, Serialize)]
pub struct ExperimentalError {
    pub message: String,
    pub code: String,
}

impl From<nodespace_core::NodeServiceError> for ExperimentalError {
    fn from(err: nodespace_core::NodeServiceError) -> Self {
        ExperimentalError {
            message: err.to_string(),
            code: "NODE_SERVICE_ERROR".to_string(),
        }
    }
}

impl From<crate::datastore::lance::LanceDBError> for ExperimentalError {
    fn from(err: crate::datastore::lance::LanceDBError) -> Self {
        ExperimentalError {
            message: err.to_string(),
            code: "LANCEDB_ERROR".to_string(),
        }
    }
}

/// Experimental state wrapper holding optional LanceDB instance
pub struct ExperimentalState {
    pub lance_db: Option<Arc<RwLock<LanceDataStore>>>,
}

/// Create a node using LanceDB (experimental)
///
/// This bypasses Turso entirely and uses only LanceDB for storage.
/// Requires EXPERIMENTAL_USE_LANCEDB=true environment variable.
#[tauri::command]
pub async fn create_node_lance_experimental(
    experimental_state: State<'_, ExperimentalState>,
    node: Node,
) -> Result<String, ExperimentalError> {
    if let Some(lance_db) = &experimental_state.lance_db {
        let db = lance_db.read().await;
        let id = db
            .create_node(node)
            .await
            .map_err(ExperimentalError::from)?;
        Ok(id)
    } else {
        Err(ExperimentalError {
            message: "LanceDB not enabled. Set EXPERIMENTAL_USE_LANCEDB=true".to_string(),
            code: "LANCEDB_NOT_ENABLED".to_string(),
        })
    }
}

/// Read a node using LanceDB (experimental)
#[tauri::command]
pub async fn read_node_lance_experimental(
    experimental_state: State<'_, ExperimentalState>,
    id: String,
) -> Result<Option<Node>, ExperimentalError> {
    if let Some(lance_db) = &experimental_state.lance_db {
        let db = lance_db.read().await;
        let node = db.read_node(&id).await.map_err(ExperimentalError::from)?;
        Ok(node)
    } else {
        Err(ExperimentalError {
            message: "LanceDB not enabled".to_string(),
            code: "LANCEDB_NOT_ENABLED".to_string(),
        })
    }
}

/// Update a node using LanceDB (experimental)
#[tauri::command]
pub async fn update_node_lance_experimental(
    experimental_state: State<'_, ExperimentalState>,
    node: Node,
) -> Result<(), ExperimentalError> {
    if let Some(lance_db) = &experimental_state.lance_db {
        let db = lance_db.read().await;
        db.update_node(node)
            .await
            .map_err(ExperimentalError::from)?;
        Ok(())
    } else {
        Err(ExperimentalError {
            message: "LanceDB not enabled".to_string(),
            code: "LANCEDB_NOT_ENABLED".to_string(),
        })
    }
}

/// Delete a node using LanceDB (experimental)
#[tauri::command]
pub async fn delete_node_lance_experimental(
    experimental_state: State<'_, ExperimentalState>,
    id: String,
) -> Result<(), ExperimentalError> {
    if let Some(lance_db) = &experimental_state.lance_db {
        let db = lance_db.read().await;
        db.delete_node(&id).await.map_err(ExperimentalError::from)?;
        Ok(())
    } else {
        Err(ExperimentalError {
            message: "LanceDB not enabled".to_string(),
            code: "LANCEDB_NOT_ENABLED".to_string(),
        })
    }
}

/// Operation types for comparison
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Create,
    Read,
    Update,
    Delete,
}

/// Compare backends by running the same operation on both
///
/// This is the primary validation command - it runs the same CRUD operation
/// on both Turso and LanceDB, measures performance, and compares results.
#[tauri::command]
pub async fn compare_backends(
    node_service: State<'_, NodeService>,
    experimental_state: State<'_, ExperimentalState>,
    operation: Operation,
    node_data: Option<Node>,
    node_id: Option<String>,
) -> Result<ComparisonResult, ExperimentalError> {
    if experimental_state.lance_db.is_none() {
        return Err(ExperimentalError {
            message: "LanceDB not enabled for comparison".to_string(),
            code: "LANCEDB_NOT_ENABLED".to_string(),
        });
    }

    let lance_db = experimental_state.lance_db.as_ref().unwrap();

    // Execute on Turso
    let turso_start = Instant::now();
    let turso_result = match operation {
        Operation::Create => {
            let node = node_data
                .as_ref()
                .ok_or_else(|| ExperimentalError {
                    message: "Node data required for create".to_string(),
                    code: "MISSING_NODE_DATA".to_string(),
                })?
                .clone();
            let id = node_service
                .create_node(node)
                .await
                .map_err(ExperimentalError::from)?;
            serde_json::json!({ "id": id })
        }
        Operation::Read => {
            let id = node_id.as_ref().ok_or_else(|| ExperimentalError {
                message: "Node ID required for read".to_string(),
                code: "MISSING_NODE_ID".to_string(),
            })?;
            let node = node_service
                .get_node(id)
                .await
                .map_err(ExperimentalError::from)?;
            serde_json::to_value(&node).unwrap_or(serde_json::Value::Null)
        }
        Operation::Update => {
            let node = node_data
                .as_ref()
                .ok_or_else(|| ExperimentalError {
                    message: "Node data required for update".to_string(),
                    code: "MISSING_NODE_DATA".to_string(),
                })?
                .clone();
            // Convert Node to NodeUpdate
            // NodeUpdate uses Option<Option<T>> for nullable fields
            let update = nodespace_core::NodeUpdate {
                node_type: Some(node.node_type.clone()),
                content: Some(node.content.clone()),
                parent_id: Some(node.parent_id.clone()),
                container_node_id: Some(node.container_node_id.clone()),
                before_sibling_id: Some(node.before_sibling_id.clone()),
                properties: Some(node.properties.clone()),
                embedding_vector: Some(node.embedding_vector.clone()),
            };
            node_service
                .update_node(&node.id, update)
                .await
                .map_err(ExperimentalError::from)?;
            serde_json::json!({ "updated": true })
        }
        Operation::Delete => {
            let id = node_id.as_ref().ok_or_else(|| ExperimentalError {
                message: "Node ID required for delete".to_string(),
                code: "MISSING_NODE_ID".to_string(),
            })?;
            node_service
                .delete_node(id)
                .await
                .map_err(ExperimentalError::from)?;
            serde_json::json!({ "deleted": true })
        }
    };
    let turso_duration = turso_start.elapsed();

    // Execute on LanceDB
    let lance_start = Instant::now();
    let lance_result = {
        let db = lance_db.read().await;
        match operation {
            Operation::Create => {
                let node = node_data.unwrap();
                let id = db
                    .create_node(node)
                    .await
                    .map_err(ExperimentalError::from)?;
                serde_json::json!({ "id": id })
            }
            Operation::Read => {
                let id = node_id.as_ref().unwrap();
                let node = db.read_node(id).await.map_err(ExperimentalError::from)?;
                serde_json::to_value(&node).unwrap_or(serde_json::Value::Null)
            }
            Operation::Update => {
                let node = node_data.unwrap();
                db.update_node(node)
                    .await
                    .map_err(ExperimentalError::from)?;
                serde_json::json!({ "updated": true })
            }
            Operation::Delete => {
                let id = node_id.as_ref().unwrap();
                db.delete_node(id).await.map_err(ExperimentalError::from)?;
                serde_json::json!({ "deleted": true })
            }
        }
    };
    let lance_duration = lance_start.elapsed();

    // Compare results
    let results_match = turso_result == lance_result;
    let differences = if !results_match {
        vec![format!(
            "Results differ: Turso={}, Lance={}",
            turso_result, lance_result
        )]
    } else {
        vec![]
    };

    Ok(ComparisonResult {
        turso_time_ms: turso_duration.as_secs_f64() * 1000.0,
        lance_time_ms: lance_duration.as_secs_f64() * 1000.0,
        results_match,
        turso_result,
        lance_result,
        differences,
    })
}
