//! Operations Layer
//!
//! Shared orchestration logic that both MCP handlers and local agent tools call.
//! Each function accepts typed inputs, coordinates service calls (collection resolution,
//! OCC auto-fetch, lifecycle management, search post-filtering), and returns typed outputs.

pub mod collection_ops;
pub mod context_ops;
pub mod node_ops;
pub mod query_ops;
pub mod rel_ops;
#[cfg(feature = "nlp")]
pub mod search_ops;
#[cfg(feature = "nlp")]
pub mod skill_ops;
pub mod skill_updater;

use crate::services::NodeServiceError;

/// Shared error type for operations layer
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("Not found: {id}")]
    NotFound { id: String },

    #[error("Already exists: {id}")]
    AlreadyExists { id: String },

    #[error("Version conflict on {node_id}: expected {expected}, got {actual}")]
    VersionConflict {
        node_id: String,
        expected: i64,
        actual: i64,
        current_node: Option<serde_json::Value>,
    },

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<NodeServiceError> for OpsError {
    fn from(err: NodeServiceError) -> Self {
        match err {
            NodeServiceError::NodeNotFound { id } => OpsError::NotFound { id },
            NodeServiceError::VersionConflict {
                node_id,
                expected_version,
                actual_version,
            } => OpsError::VersionConflict {
                node_id,
                expected: expected_version,
                actual: actual_version,
                // task_node path: get_node() is not called before this conversion, so
                // current_node is unavailable here. The frontend falls back to
                // resyncNodeFromServer for task-node conflicts.
                current_node: None,
            },
            NodeServiceError::ValidationFailed(e) => OpsError::ValidationFailed(e.to_string()),
            NodeServiceError::InvalidParent { parent_id } => {
                OpsError::ValidationFailed(format!("Invalid parent: {}", parent_id))
            }
            NodeServiceError::InvalidRoot { root_node_id } => {
                OpsError::ValidationFailed(format!("Invalid root: {}", root_node_id))
            }
            NodeServiceError::CircularReference { context } => {
                OpsError::ValidationFailed(format!("Circular reference: {}", context))
            }
            NodeServiceError::HierarchyViolation(msg) => {
                OpsError::ValidationFailed(format!("Hierarchy violation: {}", msg))
            }
            NodeServiceError::NotAContainer {
                parent_id,
                node_type,
            } => OpsError::ValidationFailed(format!(
                "Node '{}' (type '{}') cannot have children",
                parent_id, node_type
            )),
            NodeServiceError::PlaybookValidationFailed { errors } => {
                OpsError::InvalidParams(errors)
            }
            NodeServiceError::CollectionNotFound(name) => OpsError::NotFound { id: name },
            NodeServiceError::InvalidUpdate(msg) => OpsError::ValidationFailed(msg),
            NodeServiceError::InvalidCollectionPath(msg) => OpsError::ValidationFailed(msg),
            NodeServiceError::CollectionCycle(msg) => OpsError::ValidationFailed(msg),
            NodeServiceError::CollectionDepthExceeded { path, max_depth } => {
                OpsError::ValidationFailed(format!(
                    "Collection path exceeds maximum depth of {max_depth} levels: {path}"
                ))
            }
            NodeServiceError::DatabaseError(e) => OpsError::Internal(e.to_string()),
            NodeServiceError::TransactionFailed { context } => OpsError::Internal(context),
            NodeServiceError::SerializationError(msg) => OpsError::Internal(msg),
            NodeServiceError::QueryFailed(msg) => OpsError::Internal(msg),
            NodeServiceError::BulkOperationFailed { context } => OpsError::Internal(context),
            NodeServiceError::InitializationError(msg) => OpsError::Internal(msg),
        }
    }
}
