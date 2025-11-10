use serde::{Deserialize, Serialize};

/// Universal Node structure for LanceDB entity-centric storage
///
/// This mirrors the structure from nodespace-data-store PoC but adapted
/// for compatibility with nodespace-core's Node type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalNode {
    pub id: String,
    pub node_type: String,
    pub content: String,

    // Vector embedding for semantic search
    pub vector: Vec<f32>, // 384-dimensional embedding (compatible with FastEmbed)

    // Hierarchical relationships
    pub parent_id: Option<String>,
    pub container_node_id: Option<String>,
    pub before_sibling_id: Option<String>,
    pub children_ids: Vec<String>,
    pub mentions: Vec<String>, // References to other entities

    // Timestamps
    pub created_at: String, // ISO 8601 timestamp
    pub modified_at: String,

    // Flexible properties for entity-specific fields (stored as JSON string in Arrow)
    pub properties: Option<serde_json::Value>,

    // Optimistic concurrency control
    pub version: i64,
}

/// LanceDB-specific error types
#[derive(Debug, thiserror::Error)]
pub enum LanceDBError {
    #[error("LanceDB connection failed: {0}")]
    Connection(String),

    #[error("LanceDB operation failed: {0}")]
    Operation(String),

    #[error("Arrow data conversion failed: {0}")]
    Arrow(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Invalid node data: {0}")]
    InvalidNode(String),

    #[error("Database error: {0}")]
    Database(String),
}

impl From<LanceDBError> for nodespace_core::services::NodeServiceError {
    fn from(err: LanceDBError) -> Self {
        // Convert LanceDB errors to DatabaseError
        let db_error = nodespace_core::db::DatabaseError::sql_execution(err.to_string());
        nodespace_core::services::NodeServiceError::DatabaseError(db_error)
    }
}
