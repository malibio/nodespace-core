use std::time::{Duration, SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::collections::HashMap;

use crate::{NodeData, NodeType};

pub fn generate_test_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

pub fn create_test_node_with_id(id: &str, node_type: NodeType, content: &str) -> NodeData {
    let now = Utc::now();
    NodeData {
        id: id.to_string(),
        node_type,
        content: content.to_string(),
        metadata: HashMap::new(),
        created_at: now,
        updated_at: now,
    }
}

pub fn create_test_nodes_batch(count: usize, prefix: &str) -> Vec<NodeData> {
    (0..count)
        .map(|i| {
            let node_type = match i % 3 {
                0 => NodeType::Text,
                1 => NodeType::Task,
                _ => NodeType::AIChat,
            };
            NodeData::new(node_type, format!("{} node {}", prefix, i))
        })
        .collect()
}

pub fn create_node_with_metadata(
    node_type: NodeType,
    content: &str,
    metadata: Vec<(&str, serde_json::Value)>,
) -> NodeData {
    let mut node = NodeData::new(node_type, content.to_string());
    for (key, value) in metadata {
        node.set_metadata(key.to_string(), value);
    }
    node
}

pub fn measure_async_operation<F, Fut, T>(operation: F) -> impl std::future::Future<Output = (T, Duration)>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    async move {
        let start = SystemTime::now();
        let result = operation().await;
        let duration = start.elapsed().unwrap_or(Duration::from_secs(0));
        (result, duration)
    }
}

pub struct TestDataBuilder {
    nodes: Vec<NodeData>,
}

impl TestDataBuilder {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_text_node(mut self, content: &str) -> Self {
        self.nodes.push(NodeData::new(NodeType::Text, content.to_string()));
        self
    }

    pub fn add_task_node(mut self, content: &str) -> Self {
        self.nodes.push(NodeData::new(NodeType::Task, content.to_string()));
        self
    }

    pub fn add_ai_chat_node(mut self, content: &str) -> Self {
        self.nodes.push(NodeData::new(NodeType::AIChat, content.to_string()));
        self
    }

    pub fn add_node_with_metadata(
        mut self,
        node_type: NodeType,
        content: &str,
        metadata: Vec<(&str, serde_json::Value)>,
    ) -> Self {
        let node = create_node_with_metadata(node_type, content, metadata);
        self.nodes.push(node);
        self
    }

    pub fn build(self) -> Vec<NodeData> {
        self.nodes
    }
}

impl Default for TestDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub mod test_assertions {
    use super::*;
    use crate::NodeError;

    pub fn assert_node_equals(actual: &NodeData, expected: &NodeData) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.node_type, expected.node_type);
        assert_eq!(actual.content, expected.content);
        assert_eq!(actual.metadata, expected.metadata);
        assert_eq!(actual.created_at, expected.created_at);
        assert_eq!(actual.updated_at, expected.updated_at);
    }

    pub fn assert_validation_error(result: Result<(), NodeError>, expected_message: &str) {
        match result {
            Err(NodeError::ValidationError(msg)) => {
                assert!(
                    msg.contains(expected_message),
                    "Expected validation error to contain '{}', got '{}'",
                    expected_message,
                    msg
                );
            }
            Ok(_) => panic!("Expected ValidationError, got Ok"),
            Err(other) => panic!("Expected ValidationError, got {:?}", other),
        }
    }

    pub fn assert_storage_error(result: Result<String, NodeError>, expected_message: &str) {
        match result {
            Err(NodeError::StorageError(msg)) => {
                assert!(
                    msg.contains(expected_message),
                    "Expected storage error to contain '{}', got '{}'",
                    expected_message,
                    msg
                );
            }
            Ok(_) => panic!("Expected StorageError, got Ok"),
            Err(other) => panic!("Expected StorageError, got {:?}", other),
        }
    }

    pub fn assert_not_found_error(result: Result<NodeData, NodeError>, expected_id: &str) {
        match result {
            Err(NodeError::NotFound(msg)) => {
                assert!(
                    msg.contains(expected_id),
                    "Expected not found error to contain ID '{}', got '{}'",
                    expected_id,
                    msg
                );
            }
            Ok(_) => panic!("Expected NotFound error, got Ok"),
            Err(other) => panic!("Expected NotFound error, got {:?}", other),
        }
    }
}

pub mod performance_testing {
    use super::*;
    use std::time::Instant;

    pub struct PerformanceTestResult {
        pub operation_count: usize,
        pub total_duration: Duration,
        pub average_duration: Duration,
        pub min_duration: Duration,
        pub max_duration: Duration,
    }

    impl PerformanceTestResult {
        pub fn assert_average_under(&self, threshold: Duration) {
            assert!(
                self.average_duration <= threshold,
                "Average duration {:?} exceeds threshold {:?}",
                self.average_duration,
                threshold
            );
        }

        pub fn assert_max_under(&self, threshold: Duration) {
            assert!(
                self.max_duration <= threshold,
                "Maximum duration {:?} exceeds threshold {:?}",
                self.max_duration,
                threshold
            );
        }
    }

    pub async fn measure_repeated_operation<F, Fut, T>(
        operation_count: usize,
        operation: impl Fn() -> F,
    ) -> PerformanceTestResult
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let mut durations = Vec::with_capacity(operation_count);
        let total_start = Instant::now();

        for _ in 0..operation_count {
            let start = Instant::now();
            operation()().await;
            durations.push(start.elapsed());
        }

        let total_duration = total_start.elapsed();
        let sum: Duration = durations.iter().sum();
        let average_duration = sum / operation_count as u32;
        let min_duration = *durations.iter().min().unwrap();
        let max_duration = *durations.iter().max().unwrap();

        PerformanceTestResult {
            operation_count,
            total_duration,
            average_duration,
            min_duration,
            max_duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_assertions::*;
    use super::performance_testing::*;

    #[test]
    fn test_generate_test_id() {
        let id1 = generate_test_id();
        let id2 = generate_test_id();
        
        assert_ne!(id1, id2);
        assert!(id1.starts_with("test-"));
        assert!(id2.starts_with("test-"));
    }

    #[test]
    fn test_create_test_node_with_id() {
        let id = "test-123";
        let node = create_test_node_with_id(id, NodeType::Text, "Test content");
        
        assert_eq!(node.id, id);
        assert_eq!(node.node_type, NodeType::Text);
        assert_eq!(node.content, "Test content");
        assert!(node.metadata.is_empty());
    }

    #[test]
    fn test_create_test_nodes_batch() {
        let nodes = create_test_nodes_batch(5, "batch");
        
        assert_eq!(nodes.len(), 5);
        
        // Check node types cycle through Text, Task, AIChat
        assert_eq!(nodes[0].node_type, NodeType::Text);
        assert_eq!(nodes[1].node_type, NodeType::Task);
        assert_eq!(nodes[2].node_type, NodeType::AIChat);
        assert_eq!(nodes[3].node_type, NodeType::Text);
        assert_eq!(nodes[4].node_type, NodeType::Task);
        
        // Check content pattern
        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(node.content, format!("batch node {}", i));
        }
    }

    #[test]
    fn test_create_node_with_metadata() {
        let metadata = vec![
            ("priority", serde_json::Value::String("high".to_string())),
            ("completed", serde_json::Value::Bool(false)),
            ("score", serde_json::Value::Number(serde_json::Number::from(95))),
        ];
        
        let node = create_node_with_metadata(NodeType::Task, "Task with metadata", metadata);
        
        assert_eq!(node.node_type, NodeType::Task);
        assert_eq!(node.content, "Task with metadata");
        assert_eq!(node.metadata.len(), 3);
        
        assert_eq!(
            node.metadata.get("priority").unwrap(),
            &serde_json::Value::String("high".to_string())
        );
        assert_eq!(
            node.metadata.get("completed").unwrap(),
            &serde_json::Value::Bool(false)
        );
        assert_eq!(
            node.metadata.get("score").unwrap(),
            &serde_json::Value::Number(serde_json::Number::from(95))
        );
    }

    #[tokio::test]
    async fn test_measure_async_operation() {
        let (result, duration) = measure_async_operation(|| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            "test result"
        }).await;
        
        assert_eq!(result, "test result");
        assert!(duration >= Duration::from_millis(90)); // Allow some variance
        assert!(duration <= Duration::from_millis(150));
    }

    #[test]
    fn test_data_builder() {
        let nodes = TestDataBuilder::new()
            .add_text_node("First text")
            .add_task_node("First task")
            .add_ai_chat_node("First chat")
            .add_node_with_metadata(
                NodeType::Task,
                "Task with meta",
                vec![("priority", serde_json::Value::String("urgent".to_string()))]
            )
            .build();
        
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].node_type, NodeType::Text);
        assert_eq!(nodes[0].content, "First text");
        assert_eq!(nodes[1].node_type, NodeType::Task);
        assert_eq!(nodes[1].content, "First task");
        assert_eq!(nodes[2].node_type, NodeType::AIChat);
        assert_eq!(nodes[2].content, "First chat");
        assert_eq!(nodes[3].node_type, NodeType::Task);
        assert_eq!(nodes[3].content, "Task with meta");
        assert_eq!(nodes[3].metadata.len(), 1);
    }

    #[test]
    fn test_assert_node_equals() {
        let node1 = NodeData::new(NodeType::Text, "Test content".to_string());
        let node2 = node1.clone();
        
        // Should not panic
        assert_node_equals(&node1, &node2);
    }

    #[test]
    #[should_panic]
    fn test_assert_node_equals_different_content() {
        let node1 = NodeData::new(NodeType::Text, "Content 1".to_string());
        let node2 = NodeData::new(NodeType::Text, "Content 2".to_string());
        
        // Should panic due to different content
        assert_node_equals(&node1, &node2);
    }

    #[test]
    fn test_assert_validation_error() {
        let error = Err(crate::NodeError::ValidationError("Content cannot be empty".to_string()));
        assert_validation_error(error, "cannot be empty");
    }

    #[test]
    #[should_panic]
    fn test_assert_validation_error_wrong_message() {
        let error = Err(crate::NodeError::ValidationError("Content cannot be empty".to_string()));
        assert_validation_error(error, "wrong message");
    }

    #[test]
    #[should_panic]
    fn test_assert_validation_error_not_validation() {
        let error = Err(crate::NodeError::StorageError("Database error".to_string()));
        assert_validation_error(error, "cannot be empty");
    }

    #[tokio::test]
    async fn test_performance_measurement() {
        let result = measure_repeated_operation(10, || || async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }).await;
        
        assert_eq!(result.operation_count, 10);
        assert!(result.total_duration >= Duration::from_millis(90)); // Allow some variance
        assert!(result.average_duration >= Duration::from_millis(8));
        assert!(result.average_duration <= Duration::from_millis(20));
        
        // Test assertion methods
        result.assert_average_under(Duration::from_millis(50));
        result.assert_max_under(Duration::from_millis(100));
    }

    #[tokio::test]
    #[should_panic]
    async fn test_performance_assertion_failure() {
        let result = measure_repeated_operation(3, || || async {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }).await;
        
        // Should panic because average is definitely over 5ms
        result.assert_average_under(Duration::from_millis(5));
    }
}