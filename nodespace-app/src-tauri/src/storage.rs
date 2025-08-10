use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};

use crate::{NodeData, NodeError};

#[async_trait]
pub trait DataStore {
    async fn save_node(&self, node: &NodeData) -> Result<String, NodeError>;
    async fn get_node(&self, id: &str) -> Result<Option<NodeData>, NodeError>;
    async fn delete_node(&self, id: &str) -> Result<bool, NodeError>;
    async fn search_nodes(&self, query: &str) -> Result<Vec<NodeData>, NodeError>;
    async fn get_all_nodes(&self) -> Result<Vec<NodeData>, NodeError>;
}

// Mock implementation for testing and early development
pub struct MockDataStore {
    nodes: Arc<Mutex<HashMap<String, NodeData>>>,
    simulate_delays: bool,
}

impl MockDataStore {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            simulate_delays: true,
        }
    }

    pub fn new_fast() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            simulate_delays: false,
        }
    }

    pub fn with_test_data(test_nodes: Vec<NodeData>) -> Self {
        let mut nodes_map = HashMap::new();
        for node in test_nodes {
            nodes_map.insert(node.id.clone(), node);
        }

        Self {
            nodes: Arc::new(Mutex::new(nodes_map)),
            simulate_delays: false,
        }
    }

    async fn simulate_network_delay(&self) {
        if self.simulate_delays {
            sleep(Duration::from_millis(50)).await;
        }
    }
}

#[async_trait]
impl DataStore for MockDataStore {
    async fn save_node(&self, node: &NodeData) -> Result<String, NodeError> {
        self.simulate_network_delay().await;
        
        let mut nodes = self.nodes.lock()
            .map_err(|_| NodeError::StorageError("Failed to acquire lock".to_string()))?;
        
        nodes.insert(node.id.clone(), node.clone());
        Ok(node.id.clone())
    }

    async fn get_node(&self, id: &str) -> Result<Option<NodeData>, NodeError> {
        self.simulate_network_delay().await;
        
        let nodes = self.nodes.lock()
            .map_err(|_| NodeError::StorageError("Failed to acquire lock".to_string()))?;
        
        Ok(nodes.get(id).cloned())
    }

    async fn delete_node(&self, id: &str) -> Result<bool, NodeError> {
        self.simulate_network_delay().await;
        
        let mut nodes = self.nodes.lock()
            .map_err(|_| NodeError::StorageError("Failed to acquire lock".to_string()))?;
        
        Ok(nodes.remove(id).is_some())
    }

    async fn search_nodes(&self, query: &str) -> Result<Vec<NodeData>, NodeError> {
        self.simulate_network_delay().await;
        
        let nodes = self.nodes.lock()
            .map_err(|_| NodeError::StorageError("Failed to acquire lock".to_string()))?;
        
        let query_lower = query.to_lowercase();
        let results: Vec<NodeData> = nodes.values()
            .filter(|node| {
                node.content.to_lowercase().contains(&query_lower) ||
                serde_json::to_string(&node.metadata).unwrap_or_default()
                    .to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();
        
        Ok(results)
    }

    async fn get_all_nodes(&self) -> Result<Vec<NodeData>, NodeError> {
        self.simulate_network_delay().await;
        
        let nodes = self.nodes.lock()
            .map_err(|_| NodeError::StorageError("Failed to acquire lock".to_string()))?;
        
        Ok(nodes.values().cloned().collect())
    }
}

impl Default for MockDataStore {
    fn default() -> Self {
        Self::new()
    }
}

// Future implementation interface for LanceDB
pub struct LanceDBConfig {
    pub path: String,
    pub table_name: String,
}

pub struct LanceDBStore {
    #[allow(dead_code)]
    config: LanceDBConfig,
}

impl LanceDBStore {
    #[allow(dead_code)]
    pub fn new(config: LanceDBConfig) -> Self {
        Self { config }
    }
}

// Note: LanceDB implementation will be added in future iterations
#[async_trait]
impl DataStore for LanceDBStore {
    async fn save_node(&self, _node: &NodeData) -> Result<String, NodeError> {
        Err(NodeError::InternalError("LanceDB implementation not yet available".to_string()))
    }

    async fn get_node(&self, _id: &str) -> Result<Option<NodeData>, NodeError> {
        Err(NodeError::InternalError("LanceDB implementation not yet available".to_string()))
    }

    async fn delete_node(&self, _id: &str) -> Result<bool, NodeError> {
        Err(NodeError::InternalError("LanceDB implementation not yet available".to_string()))
    }

    async fn search_nodes(&self, _query: &str) -> Result<Vec<NodeData>, NodeError> {
        Err(NodeError::InternalError("LanceDB implementation not yet available".to_string()))
    }

    async fn get_all_nodes(&self) -> Result<Vec<NodeData>, NodeError> {
        Err(NodeError::InternalError("LanceDB implementation not yet available".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeType;
    use tokio_test;
    use std::time::Instant;

    fn create_test_node(content: &str) -> NodeData {
        NodeData::new(NodeType::Text, content.to_string())
    }

    fn create_test_nodes() -> Vec<NodeData> {
        vec![
            NodeData::new(NodeType::Text, "First test document".to_string()),
            NodeData::new(NodeType::Text, "Second document with test".to_string()),
            NodeData::new(NodeType::Task, "Complete testing task".to_string()),
            NodeData::new(NodeType::AIChat, "AI conversation about testing".to_string()),
        ]
    }

    #[tokio::test]
    async fn test_mock_store_creation() {
        let store = MockDataStore::new();
        let all_nodes = store.get_all_nodes().await.unwrap();
        assert!(all_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_mock_store_with_test_data() {
        let test_nodes = create_test_nodes();
        let store = MockDataStore::with_test_data(test_nodes.clone());
        
        let all_nodes = store.get_all_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), test_nodes.len());
    }

    #[tokio::test]
    async fn test_save_and_get_node() {
        let store = MockDataStore::new_fast();
        let node = create_test_node("Test content");
        
        // Save node
        let saved_id = store.save_node(&node).await.unwrap();
        assert_eq!(saved_id, node.id);
        
        // Retrieve node
        let retrieved = store.get_node(&node.id).await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved_node = retrieved.unwrap();
        assert_eq!(retrieved_node.id, node.id);
        assert_eq!(retrieved_node.content, node.content);
        assert_eq!(retrieved_node.node_type, node.node_type);
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let store = MockDataStore::new_fast();
        let result = store.get_node("non-existent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_existing_node() {
        let store = MockDataStore::new_fast();
        let node = create_test_node("Original content");
        
        // Save original node
        store.save_node(&node).await.unwrap();
        
        // Update node content
        let mut updated_node = node.clone();
        updated_node.content = "Updated content".to_string();
        
        // Save updated node (same ID)
        store.save_node(&updated_node).await.unwrap();
        
        // Retrieve and verify
        let retrieved = store.get_node(&node.id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "Updated content");
    }

    #[tokio::test]
    async fn test_delete_node() {
        let store = MockDataStore::new_fast();
        let node = create_test_node("Content to delete");
        
        // Save node
        store.save_node(&node).await.unwrap();
        
        // Verify it exists
        assert!(store.get_node(&node.id).await.unwrap().is_some());
        
        // Delete node
        let deleted = store.delete_node(&node.id).await.unwrap();
        assert!(deleted);
        
        // Verify it's gone
        assert!(store.get_node(&node.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_node() {
        let store = MockDataStore::new_fast();
        let deleted = store.delete_node("non-existent-id").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_search_nodes_by_content() {
        let test_nodes = create_test_nodes();
        let store = MockDataStore::with_test_data(test_nodes);
        
        // Search for nodes containing "test"
        let results = store.search_nodes("test").await.unwrap();
        assert!(!results.is_empty());
        
        // All results should contain "test" in content or metadata
        for node in results {
            let content_match = node.content.to_lowercase().contains("test");
            let metadata_match = serde_json::to_string(&node.metadata)
                .unwrap_or_default()
                .to_lowercase()
                .contains("test");
            assert!(content_match || metadata_match);
        }
    }

    #[tokio::test]
    async fn test_search_nodes_case_insensitive() {
        let test_nodes = vec![
            NodeData::new(NodeType::Text, "Test Content".to_string()),
            NodeData::new(NodeType::Text, "TEST content".to_string()),
            NodeData::new(NodeType::Text, "test CONTENT".to_string()),
        ];
        let store = MockDataStore::with_test_data(test_nodes);
        
        let results = store.search_nodes("TEST").await.unwrap();
        assert_eq!(results.len(), 3); // Should find all variations
    }

    #[tokio::test]
    async fn test_search_nodes_no_results() {
        let test_nodes = create_test_nodes();
        let store = MockDataStore::with_test_data(test_nodes);
        
        let results = store.search_nodes("nonexistent").await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_nodes_by_metadata() {
        let mut node = NodeData::new(NodeType::Task, "Simple task".to_string());
        node.set_metadata("priority".to_string(), serde_json::Value::String("urgent".to_string()));
        
        let store = MockDataStore::with_test_data(vec![node]);
        
        let results = store.search_nodes("urgent").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Simple task");
    }

    #[tokio::test]
    async fn test_get_all_nodes() {
        let test_nodes = create_test_nodes();
        let expected_count = test_nodes.len();
        let store = MockDataStore::with_test_data(test_nodes);
        
        let all_nodes = store.get_all_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), expected_count);
    }

    #[tokio::test]
    async fn test_network_delay_simulation() {
        let store = MockDataStore::new(); // With delays enabled
        let node = create_test_node("Delay test");
        
        let start = Instant::now();
        store.save_node(&node).await.unwrap();
        let elapsed = start.elapsed();
        
        // Should take at least 50ms due to simulated delay
        assert!(elapsed >= Duration::from_millis(40)); // Allow some variance
    }

    #[tokio::test]
    async fn test_fast_store_no_delays() {
        let store = MockDataStore::new_fast(); // No delays
        let node = create_test_node("Fast test");
        
        let start = Instant::now();
        store.save_node(&node).await.unwrap();
        let elapsed = start.elapsed();
        
        // Should be very fast
        assert!(elapsed < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let store = Arc::new(MockDataStore::new_fast());
        let mut handles = vec![];
        
        // Create multiple tasks that save nodes concurrently
        for i in 0..10 {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                let node = NodeData::new(NodeType::Text, format!("Concurrent node {}", i));
                store_clone.save_node(&node).await.unwrap()
            });
            handles.push(handle);
        }
        
        // Wait for all operations to complete
        let mut saved_ids = vec![];
        for handle in handles {
            let id = handle.await.unwrap();
            saved_ids.push(id);
        }
        
        // Verify all nodes were saved
        let all_nodes = store.get_all_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), 10);
        assert_eq!(saved_ids.len(), 10);
    }

    #[tokio::test]
    async fn test_storage_error_handling() {
        // This test simulates lock poisoning, but that's hard to trigger
        // In a real scenario, we'd test with actual database errors
        let store = MockDataStore::new_fast();
        let node = create_test_node("Error test");
        
        // This should work normally
        let result = store.save_node(&node).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_lancedb_store_placeholder() {
        let config = LanceDBConfig {
            path: "/tmp/test.db".to_string(),
            table_name: "nodes".to_string(),
        };
        
        let store = LanceDBStore::new(config);
        
        // Just test creation - actual implementation will come later
        assert!(!store.config.path.is_empty());
        assert!(!store.config.table_name.is_empty());
    }

    #[tokio::test]
    async fn test_lancedb_not_implemented() {
        let config = LanceDBConfig {
            path: "/tmp/test.db".to_string(),
            table_name: "nodes".to_string(),
        };
        
        let store = LanceDBStore::new(config);
        let node = create_test_node("Test");
        
        let result = store.save_node(&node).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::InternalError(msg) => {
                assert!(msg.contains("LanceDB implementation not yet available"));
            }
            _ => panic!("Expected InternalError"),
        }
    }
}