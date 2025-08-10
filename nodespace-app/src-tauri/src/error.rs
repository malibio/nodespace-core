use std::fmt;
use anyhow::Error as AnyhowError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeError {
    ValidationError(String),
    StorageError(String),
    NotFound(String),
    InternalError(String),
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            NodeError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            NodeError::NotFound(msg) => write!(f, "Not found: {}", msg),
            NodeError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for NodeError {}

impl From<AnyhowError> for NodeError {
    fn from(err: AnyhowError) -> Self {
        NodeError::InternalError(err.to_string())
    }
}

impl From<serde_json::Error> for NodeError {
    fn from(err: serde_json::Error) -> Self {
        NodeError::ValidationError(format!("JSON serialization error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let validation_error = NodeError::ValidationError("Invalid content".to_string());
        assert_eq!(validation_error.to_string(), "Validation error: Invalid content");

        let storage_error = NodeError::StorageError("Database connection failed".to_string());
        assert_eq!(storage_error.to_string(), "Storage error: Database connection failed");

        let not_found_error = NodeError::NotFound("Node with ID 123".to_string());
        assert_eq!(not_found_error.to_string(), "Not found: Node with ID 123");

        let internal_error = NodeError::InternalError("Unexpected panic".to_string());
        assert_eq!(internal_error.to_string(), "Internal error: Unexpected panic");
    }

    #[test]
    fn test_error_from_anyhow() {
        let anyhow_error = anyhow::anyhow!("Test anyhow error");
        let node_error: NodeError = anyhow_error.into();
        
        match node_error {
            NodeError::InternalError(msg) => assert_eq!(msg, "Test anyhow error"),
            _ => panic!("Expected InternalError"),
        }
    }

    #[test]
    fn test_error_from_serde_json() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json")
            .expect_err("Should fail to parse");
        let node_error: NodeError = json_error.into();
        
        match node_error {
            NodeError::ValidationError(msg) => {
                assert!(msg.contains("JSON serialization error"));
            },
            _ => panic!("Expected ValidationError"),
        }
    }
}