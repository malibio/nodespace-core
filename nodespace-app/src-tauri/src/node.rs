use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{NodeError, DataStore};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeType {
    Text,
    Task,
    AIChat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    pub id: String,
    pub node_type: NodeType,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NodeData {
    pub fn new(node_type: NodeType, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            node_type,
            content,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), NodeError> {
        if self.content.is_empty() {
            return Err(NodeError::ValidationError("Content cannot be empty".to_string()));
        }

        if self.content.len() > 10_000 {
            return Err(NodeError::ValidationError("Content exceeds maximum length of 10,000 characters".to_string()));
        }

        if self.id.is_empty() {
            return Err(NodeError::ValidationError("ID cannot be empty".to_string()));
        }

        Ok(())
    }

    pub fn update_content(&mut self, content: String) -> Result<(), NodeError> {
        if content.is_empty() {
            return Err(NodeError::ValidationError("Content cannot be empty".to_string()));
        }

        self.content = content;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
    }
}

// Request/Response DTOs for Tauri commands
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeCreateRequest {
    pub node_type: NodeType,
    pub content: String,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeUpdateRequest {
    pub id: String,
    pub content: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeResponse {
    pub id: String,
    pub node_type: NodeType,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<NodeCreateRequest> for NodeData {
    fn from(request: NodeCreateRequest) -> Self {
        let mut node = NodeData::new(request.node_type, request.content);
        if let Some(metadata) = request.metadata {
            node.metadata = metadata;
        }
        node
    }
}

impl From<NodeData> for NodeResponse {
    fn from(node: NodeData) -> Self {
        Self {
            id: node.id,
            node_type: node.node_type,
            content: node.content,
            metadata: node.metadata,
            created_at: node.created_at,
            updated_at: node.updated_at,
        }
    }
}

// Node Manager - Core business logic
pub struct NodeManager {
    storage: Arc<dyn DataStore + Send + Sync>,
}

impl NodeManager {
    pub fn new(storage: Arc<dyn DataStore + Send + Sync>) -> Self {
        Self { storage }
    }

    pub async fn create_node(&self, mut node: NodeData) -> Result<NodeData, NodeError> {
        node.validate()?;
        
        let saved_id = self.storage.save_node(&node).await?;
        node.id = saved_id;
        
        Ok(node)
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<NodeData>, NodeError> {
        self.storage.get_node(id).await
    }

    pub async fn update_node(&self, request: NodeUpdateRequest) -> Result<NodeData, NodeError> {
        let mut node = self.storage.get_node(&request.id).await?
            .ok_or_else(|| NodeError::NotFound(format!("Node with ID {}", request.id)))?;

        if let Some(content) = request.content {
            node.update_content(content)?;
        }

        if let Some(metadata) = request.metadata {
            node.metadata = metadata;
            node.updated_at = Utc::now();
        }

        node.validate()?;
        self.storage.save_node(&node).await?;
        
        Ok(node)
    }

    pub async fn delete_node(&self, id: &str) -> Result<bool, NodeError> {
        self.storage.delete_node(id).await
    }

    pub async fn search_nodes(&self, query: &str) -> Result<Vec<NodeData>, NodeError> {
        self.storage.search_nodes(query).await
    }

    pub async fn get_nodes_by_type(&self, node_type: NodeType) -> Result<Vec<NodeData>, NodeError> {
        let all_nodes = self.storage.get_all_nodes().await?;
        Ok(all_nodes.into_iter().filter(|node| node.node_type == node_type).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockDataStore;

    fn create_test_node() -> NodeData {
        NodeData::new(NodeType::Text, "Test content".to_string())
    }

    #[test]
    fn test_node_creation() {
        let node = create_test_node();
        assert_eq!(node.content, "Test content");
        assert_eq!(node.node_type, NodeType::Text);
        assert!(!node.id.is_empty());
        assert!(node.metadata.is_empty());
    }

    #[test]
    fn test_node_validation_success() {
        let node = create_test_node();
        assert!(node.validate().is_ok());
    }

    #[test]
    fn test_node_validation_empty_content() {
        let mut node = create_test_node();
        node.content = String::new();
        
        let result = node.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::ValidationError(msg) => {
                assert_eq!(msg, "Content cannot be empty");
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_node_validation_content_too_long() {
        let mut node = create_test_node();
        node.content = "x".repeat(10_001);
        
        let result = node.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::ValidationError(msg) => {
                assert!(msg.contains("exceeds maximum length"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_node_validation_empty_id() {
        let mut node = create_test_node();
        node.id = String::new();
        
        let result = node.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::ValidationError(msg) => {
                assert_eq!(msg, "ID cannot be empty");
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_node_update_content() {
        let mut node = create_test_node();
        let original_updated_at = node.updated_at;
        
        // Sleep to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(1));
        
        let result = node.update_content("Updated content".to_string());
        assert!(result.is_ok());
        assert_eq!(node.content, "Updated content");
        assert!(node.updated_at > original_updated_at);
    }

    #[test]
    fn test_node_update_content_empty() {
        let mut node = create_test_node();
        
        let result = node.update_content(String::new());
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::ValidationError(msg) => {
                assert_eq!(msg, "Content cannot be empty");
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_node_set_metadata() {
        let mut node = create_test_node();
        let original_updated_at = node.updated_at;
        
        // Sleep to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(1));
        
        node.set_metadata("priority".to_string(), serde_json::Value::String("high".to_string()));
        
        assert_eq!(node.metadata.len(), 1);
        assert_eq!(
            node.metadata.get("priority").unwrap(),
            &serde_json::Value::String("high".to_string())
        );
        assert!(node.updated_at > original_updated_at);
    }

    #[test]
    fn test_node_create_request_conversion() {
        let mut metadata = HashMap::new();
        metadata.insert("priority".to_string(), serde_json::Value::String("medium".to_string()));
        
        let request = NodeCreateRequest {
            node_type: NodeType::Task,
            content: "Test task".to_string(),
            metadata: Some(metadata.clone()),
        };
        
        let node: NodeData = request.into();
        assert_eq!(node.node_type, NodeType::Task);
        assert_eq!(node.content, "Test task");
        assert_eq!(node.metadata, metadata);
    }

    #[test]
    fn test_node_response_conversion() {
        let node = create_test_node();
        let response: NodeResponse = node.clone().into();
        
        assert_eq!(response.id, node.id);
        assert_eq!(response.node_type, node.node_type);
        assert_eq!(response.content, node.content);
        assert_eq!(response.metadata, node.metadata);
        assert_eq!(response.created_at, node.created_at);
        assert_eq!(response.updated_at, node.updated_at);
    }

    mod node_manager_tests {
        use super::*;
        use tokio_test;

        async fn setup_node_manager() -> NodeManager {
            let storage = Arc::new(MockDataStore::new());
            NodeManager::new(storage)
        }

        #[tokio::test]
        async fn test_create_node_success() {
            let manager = setup_node_manager().await;
            let node = create_test_node();
            
            let result = manager.create_node(node.clone()).await;
            assert!(result.is_ok());
            
            let saved_node = result.unwrap();
            assert_eq!(saved_node.content, node.content);
            assert_eq!(saved_node.node_type, node.node_type);
        }

        #[tokio::test]
        async fn test_create_node_validation_error() {
            let manager = setup_node_manager().await;
            let mut node = create_test_node();
            node.content = String::new(); // Invalid empty content
            
            let result = manager.create_node(node).await;
            assert!(result.is_err());
            match result.unwrap_err() {
                NodeError::ValidationError(_) => {}, // Expected
                _ => panic!("Expected ValidationError"),
            }
        }

        #[tokio::test]
        async fn test_get_node_success() {
            let manager = setup_node_manager().await;
            let node = create_test_node();
            
            // First create a node
            let created_node = manager.create_node(node).await.unwrap();
            
            // Then retrieve it
            let result = manager.get_node(&created_node.id).await;
            assert!(result.is_ok());
            
            let retrieved_node = result.unwrap();
            assert!(retrieved_node.is_some());
            assert_eq!(retrieved_node.unwrap().id, created_node.id);
        }

        #[tokio::test]
        async fn test_get_node_not_found() {
            let manager = setup_node_manager().await;
            
            let result = manager.get_node("non-existent-id").await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }

        #[tokio::test]
        async fn test_update_node_success() {
            let manager = setup_node_manager().await;
            let node = create_test_node();
            
            // Create node
            let created_node = manager.create_node(node).await.unwrap();
            
            // Update node
            let update_request = NodeUpdateRequest {
                id: created_node.id.clone(),
                content: Some("Updated content".to_string()),
                metadata: None,
            };
            
            let result = manager.update_node(update_request).await;
            assert!(result.is_ok());
            
            let updated_node = result.unwrap();
            assert_eq!(updated_node.content, "Updated content");
            assert!(updated_node.updated_at > created_node.updated_at);
        }

        #[tokio::test]
        async fn test_update_node_not_found() {
            let manager = setup_node_manager().await;
            
            let update_request = NodeUpdateRequest {
                id: "non-existent-id".to_string(),
                content: Some("Updated content".to_string()),
                metadata: None,
            };
            
            let result = manager.update_node(update_request).await;
            assert!(result.is_err());
            match result.unwrap_err() {
                NodeError::NotFound(_) => {}, // Expected
                _ => panic!("Expected NotFound error"),
            }
        }

        #[tokio::test]
        async fn test_delete_node_success() {
            let manager = setup_node_manager().await;
            let node = create_test_node();
            
            // Create node
            let created_node = manager.create_node(node).await.unwrap();
            
            // Delete node
            let result = manager.delete_node(&created_node.id).await;
            assert!(result.is_ok());
            assert!(result.unwrap()); // Should return true for successful deletion
        }

        #[tokio::test]
        async fn test_search_nodes() {
            let manager = setup_node_manager().await;
            
            // Create test nodes
            let node1 = NodeData::new(NodeType::Text, "Search test content".to_string());
            let node2 = NodeData::new(NodeType::Text, "Another document".to_string());
            let node3 = NodeData::new(NodeType::Text, "Test search functionality".to_string());
            
            manager.create_node(node1).await.unwrap();
            manager.create_node(node2).await.unwrap();
            manager.create_node(node3).await.unwrap();
            
            // Search for nodes
            let result = manager.search_nodes("test").await;
            assert!(result.is_ok());
            
            let found_nodes = result.unwrap();
            // Should find nodes that contain "test"
            assert!(!found_nodes.is_empty());
        }

        #[tokio::test]
        async fn test_get_nodes_by_type() {
            let manager = setup_node_manager().await;
            
            // Create nodes of different types
            let text_node = NodeData::new(NodeType::Text, "Text content".to_string());
            let task_node = NodeData::new(NodeType::Task, "Task content".to_string());
            let ai_chat_node = NodeData::new(NodeType::AIChat, "AI chat content".to_string());
            
            manager.create_node(text_node).await.unwrap();
            manager.create_node(task_node).await.unwrap();
            manager.create_node(ai_chat_node).await.unwrap();
            
            // Get only text nodes
            let result = manager.get_nodes_by_type(NodeType::Text).await;
            assert!(result.is_ok());
            
            let text_nodes = result.unwrap();
            assert!(!text_nodes.is_empty());
            for node in text_nodes {
                assert_eq!(node.node_type, NodeType::Text);
            }
        }
    }
}