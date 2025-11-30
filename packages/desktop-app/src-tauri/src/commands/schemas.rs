//! Schema read commands for retrieving entity schemas
//!
//! As of Issue #690, schema operations use NodeService with strongly-typed SchemaNode.
//! Schema mutation operations (add_field, remove_field, etc.) were removed
//! as they weren't used by any UI components.
//!
//! This module provides read-only schema commands:
//! - `get_all_schemas` - List all schema nodes
//! - `get_schema_definition` - Get a specific schema by ID

use nodespace_core::services::NodeServiceError;
use nodespace_core::{Node, NodeQuery, NodeService};
use serde::Serialize;
use tauri::State;

/// Structured error type for Tauri commands
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub message: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl From<NodeServiceError> for CommandError {
    fn from(err: NodeServiceError) -> Self {
        CommandError {
            message: format!("Schema operation failed: {}", err),
            code: "SCHEMA_SERVICE_ERROR".to_string(),
            details: Some(err.to_string()),
        }
    }
}

/// Get all schema nodes
///
/// Retrieves all schema nodes (both core and custom) for plugin auto-registration.
/// Uses NodeService.query_nodes_simple() (Issue #690 - uses strongly-typed SchemaNode).
///
/// # Returns
/// * `Ok(Vec<Node>)` - Array of schema nodes (properties contain schema data)
/// * `Err(CommandError)` - Error if retrieval fails
#[tauri::command]
pub async fn get_all_schemas(
    service: State<'_, NodeService>,
) -> Result<Vec<Node>, CommandError> {
    // Query all schema nodes
    let query = NodeQuery {
        node_type: Some("schema".to_string()),
        ..Default::default()
    };
    let schema_nodes = service.query_nodes_simple(query).await.map_err(CommandError::from)?;

    Ok(schema_nodes)
}

/// Get schema by ID using strongly-typed SchemaNode
///
/// Retrieves the complete schema including all fields, protection levels,
/// and metadata. Uses NodeService.get_schema_node() for type-safe access.
///
/// # Arguments
/// * `schema_id` - ID of the schema to retrieve (e.g., "task", "person")
///
/// # Returns
/// * `Ok(Node)` - Schema node (properties contain isCore, version, fields, etc.)
/// * `Err(CommandError)` - Error if schema not found
#[tauri::command]
pub async fn get_schema_definition(
    service: State<'_, NodeService>,
    schema_id: String,
) -> Result<Node, CommandError> {
    let schema_node = service
        .get_schema_node(&schema_id)
        .await
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError {
            message: format!("Schema '{}' not found", schema_id),
            code: "SCHEMA_NOT_FOUND".to_string(),
            details: None,
        })?;

    // Return the underlying Node (properties contain schema data)
    Ok(schema_node.into_node())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_error_serialization() {
        let err = CommandError {
            message: "Test error".to_string(),
            code: "TEST_ERROR".to_string(),
            details: Some("Debug info".to_string()),
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Test error"));
        assert!(json.contains("TEST_ERROR"));
        assert!(json.contains("Debug info"));
    }

    #[test]
    fn test_command_error_without_details() {
        let err = CommandError {
            message: "Simple error".to_string(),
            code: "SIMPLE".to_string(),
            details: None,
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Simple error"));
        assert!(!json.contains("details"));
    }
}
