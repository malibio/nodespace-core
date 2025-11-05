//! Tests for TaskNode wrapper
//!
//! Comprehensive test suite covering all TaskNode functionality including
//! conversion methods, property access, and edge cases.

#[cfg(test)]
mod tests {
    use crate::models::{Node, TaskNode, TaskStatus, ValidationError};
    use serde_json::json;

    // ========================================================================
    // from_node() Validation Tests
    // ========================================================================

    #[test]
    fn test_from_node_validates_node_type() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let result = TaskNode::from_node(node);
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_node_rejects_wrong_type() {
        let node = Node::new("text".to_string(), "Test".to_string(), None, json!({}));
        let result = TaskNode::from_node(node);
        assert!(result.is_err());
        assert!(matches!(result, Err(ValidationError::InvalidNodeType(_))));
    }

    #[test]
    fn test_from_node_rejects_text_node() {
        let text_node = Node::new("text".to_string(), "Not a task".to_string(), None, json!({}));
        let result = TaskNode::from_node(text_node);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_node_rejects_date_node() {
        let date_node = Node::new_with_id(
            "2025-01-03".to_string(),
            "date".to_string(),
            "2025-01-03".to_string(),
            None,
            json!({}),
        );
        let result = TaskNode::from_node(date_node);
        assert!(result.is_err());
    }

    // ========================================================================
    // Conversion Method Tests (as_node, into_node)
    // ========================================================================

    #[test]
    fn test_as_node_returns_reference() {
        let node = Node::new(
            "task".to_string(),
            "Test task".to_string(),
            None,
            json!({"task": {"status": "pending"}}),
        );
        let node_id = node.id.clone();
        let task = TaskNode::from_node(node).unwrap();

        let node_ref = task.as_node();
        assert_eq!(node_ref.id, node_id);
        assert_eq!(node_ref.node_type, "task");
        assert_eq!(node_ref.content, "Test task");
    }

    #[test]
    fn test_as_node_mut_allows_modification() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.as_node_mut().parent_id = Some("parent-123".to_string());
        assert_eq!(task.as_node().parent_id, Some("parent-123".to_string()));
    }

    #[test]
    fn test_into_node_preserves_all_data() {
        let original = Node::new(
            "task".to_string(),
            "Test task".to_string(),
            Some("parent-123".to_string()),
            json!({"task": {"status": "pending", "priority": 2}}),
        );
        let original_id = original.id.clone();

        let task = TaskNode::from_node(original).unwrap();
        let converted_back = task.into_node();

        assert_eq!(converted_back.id, original_id);
        assert_eq!(converted_back.node_type, "task");
        assert_eq!(converted_back.content, "Test task");
        assert_eq!(converted_back.parent_id, Some("parent-123".to_string()));
        assert_eq!(converted_back.properties["task"]["status"], "pending");
        assert_eq!(converted_back.properties["task"]["priority"], 2);
    }

    #[test]
    fn test_into_node_preserves_timestamps() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let created_at = node.created_at;
        let modified_at = node.modified_at;

        let task = TaskNode::from_node(node).unwrap();
        let converted = task.into_node();

        assert_eq!(converted.created_at, created_at);
        assert_eq!(converted.modified_at, modified_at);
    }

    // ========================================================================
    // Status Getter Tests
    // ========================================================================

    #[test]
    fn test_status_getter_pending() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "pending"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn test_status_getter_in_progress() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "in_progress"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::InProgress);
    }

    #[test]
    fn test_status_getter_completed() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "completed"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Completed);
    }

    #[test]
    fn test_status_getter_cancelled() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "cancelled"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Cancelled);
    }

    #[test]
    fn test_status_getter_defaults_to_pending_when_missing() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn test_status_getter_defaults_to_pending_on_invalid_value() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "invalid_status"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn test_status_getter_supports_uppercase_legacy_format() {
        // Test backward compatibility with old uppercase status values
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "IN_PROGRESS"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::InProgress);
    }

    #[test]
    fn test_status_getter_supports_legacy_open_status() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "OPEN"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn test_status_getter_supports_legacy_done_status() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"status": "DONE"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.status(), TaskStatus::Completed);
    }

    // ========================================================================
    // Status Setter Tests
    // ========================================================================

    #[test]
    fn test_status_setter_updates_correctly() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_status(TaskStatus::Completed);
        assert_eq!(task.status(), TaskStatus::Completed);
        assert_eq!(task.as_node().properties["task"]["status"], "completed");
    }

    #[test]
    fn test_status_setter_creates_task_properties_if_missing() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_status(TaskStatus::InProgress);
        assert_eq!(task.status(), TaskStatus::InProgress);
    }

    #[test]
    fn test_status_setter_all_values() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_status(TaskStatus::Pending);
        assert_eq!(task.status(), TaskStatus::Pending);

        task.set_status(TaskStatus::InProgress);
        assert_eq!(task.status(), TaskStatus::InProgress);

        task.set_status(TaskStatus::Completed);
        assert_eq!(task.status(), TaskStatus::Completed);

        task.set_status(TaskStatus::Cancelled);
        assert_eq!(task.status(), TaskStatus::Cancelled);
    }

    // ========================================================================
    // Priority Getter Tests
    // ========================================================================

    #[test]
    fn test_priority_getter_returns_correct_value() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"priority": 3}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.priority(), 3);
    }

    #[test]
    fn test_priority_getter_defaults_to_2_when_missing() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.priority(), 2);
    }

    #[test]
    fn test_priority_getter_handles_all_valid_values() {
        for priority in 1..=4 {
            let node = Node::new(
                "task".to_string(),
                "Test".to_string(),
                None,
                json!({"task": {"priority": priority}}),
            );
            let task = TaskNode::from_node(node).unwrap();
            assert_eq!(task.priority(), priority);
        }
    }

    // ========================================================================
    // Priority Setter Tests
    // ========================================================================

    #[test]
    fn test_priority_setter_updates_correctly() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_priority(3);
        assert_eq!(task.priority(), 3);
        assert_eq!(task.as_node().properties["task"]["priority"], 3);
    }

    #[test]
    fn test_priority_setter_all_values() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        for priority in 1..=4 {
            task.set_priority(priority);
            assert_eq!(task.priority(), priority);
        }
    }

    // ========================================================================
    // Due Date Getter Tests
    // ========================================================================

    #[test]
    fn test_due_date_getter_returns_value() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"due_date": "2025-12-31"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.due_date(), Some("2025-12-31".to_string()));
    }

    #[test]
    fn test_due_date_getter_returns_none_when_missing() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.due_date(), None);
    }

    #[test]
    fn test_due_date_getter_returns_none_when_null() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"due_date": null}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.due_date(), None);
    }

    // ========================================================================
    // Due Date Setter Tests
    // ========================================================================

    #[test]
    fn test_due_date_setter_sets_value() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_due_date(Some("2025-12-31".to_string()));
        assert_eq!(task.due_date(), Some("2025-12-31".to_string()));
        assert_eq!(task.as_node().properties["task"]["due_date"], "2025-12-31");
    }

    #[test]
    fn test_due_date_setter_sets_null() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"due_date": "2025-12-31"}}),
        );
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_due_date(None);
        assert_eq!(task.due_date(), None);
        assert!(task.as_node().properties["task"]["due_date"].is_null());
    }

    // ========================================================================
    // Assignee ID Getter Tests
    // ========================================================================

    #[test]
    fn test_assignee_id_getter_returns_value() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"assignee_id": "user-123"}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.assignee_id(), Some("user-123".to_string()));
    }

    #[test]
    fn test_assignee_id_getter_returns_none_when_missing() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.assignee_id(), None);
    }

    #[test]
    fn test_assignee_id_getter_returns_none_when_null() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"assignee_id": null}}),
        );
        let task = TaskNode::from_node(node).unwrap();
        assert_eq!(task.assignee_id(), None);
    }

    // ========================================================================
    // Assignee ID Setter Tests
    // ========================================================================

    #[test]
    fn test_assignee_id_setter_sets_value() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_assignee_id(Some("user-123".to_string()));
        assert_eq!(task.assignee_id(), Some("user-123".to_string()));
        assert_eq!(task.as_node().properties["task"]["assignee_id"], "user-123");
    }

    #[test]
    fn test_assignee_id_setter_sets_null() {
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({"task": {"assignee_id": "user-123"}}),
        );
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_assignee_id(None);
        assert_eq!(task.assignee_id(), None);
        assert!(task.as_node().properties["task"]["assignee_id"].is_null());
    }

    // ========================================================================
    // Builder Pattern Tests
    // ========================================================================

    #[test]
    fn test_builder_creates_valid_task() {
        let task = TaskNode::builder("Implement feature".to_string())
            .with_status(TaskStatus::InProgress)
            .with_priority(3)
            .build();

        assert_eq!(task.status(), TaskStatus::InProgress);
        assert_eq!(task.priority(), 3);
        assert_eq!(task.as_node().content, "Implement feature");
        assert_eq!(task.as_node().node_type, "task");
    }

    #[test]
    fn test_builder_with_all_fields() {
        let task = TaskNode::builder("Complete task".to_string())
            .with_parent_id("parent-123".to_string())
            .with_status(TaskStatus::Completed)
            .with_priority(4)
            .with_due_date(Some("2025-12-31".to_string()))
            .with_assignee_id(Some("user-456".to_string()))
            .build();

        assert_eq!(task.status(), TaskStatus::Completed);
        assert_eq!(task.priority(), 4);
        assert_eq!(task.due_date(), Some("2025-12-31".to_string()));
        assert_eq!(task.assignee_id(), Some("user-456".to_string()));
        assert_eq!(task.as_node().parent_id, Some("parent-123".to_string()));
    }

    #[test]
    fn test_builder_minimal_task() {
        let task = TaskNode::builder("Simple task".to_string()).build();

        assert_eq!(task.status(), TaskStatus::Pending);
        assert_eq!(task.priority(), 2);
        assert_eq!(task.due_date(), None);
        assert_eq!(task.assignee_id(), None);
        assert_eq!(task.as_node().parent_id, None);
    }

    #[test]
    fn test_builder_generates_unique_ids() {
        let task1 = TaskNode::builder("Task 1".to_string()).build();
        let task2 = TaskNode::builder("Task 2".to_string()).build();

        assert_ne!(task1.as_node().id, task2.as_node().id);
    }

    // ========================================================================
    // TaskStatus Enum Tests
    // ========================================================================

    #[test]
    fn test_task_status_to_string() {
        assert_eq!(TaskStatus::Pending.to_string(), "pending");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_task_status_from_str_lowercase() {
        assert_eq!("pending".parse::<TaskStatus>().unwrap(), TaskStatus::Pending);
        assert_eq!(
            "in_progress".parse::<TaskStatus>().unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(
            "completed".parse::<TaskStatus>().unwrap(),
            TaskStatus::Completed
        );
        assert_eq!(
            "cancelled".parse::<TaskStatus>().unwrap(),
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn test_task_status_from_str_uppercase() {
        assert_eq!("PENDING".parse::<TaskStatus>().unwrap(), TaskStatus::Pending);
        assert_eq!(
            "IN_PROGRESS".parse::<TaskStatus>().unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(
            "COMPLETED".parse::<TaskStatus>().unwrap(),
            TaskStatus::Completed
        );
        assert_eq!(
            "CANCELLED".parse::<TaskStatus>().unwrap(),
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn test_task_status_from_str_legacy_values() {
        assert_eq!("OPEN".parse::<TaskStatus>().unwrap(), TaskStatus::Pending);
        assert_eq!("DONE".parse::<TaskStatus>().unwrap(), TaskStatus::Completed);
    }

    #[test]
    fn test_task_status_from_str_invalid() {
        let result = "invalid".parse::<TaskStatus>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid task status: invalid");
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    #[test]
    fn test_roundtrip_with_all_properties() {
        // Create task with builder
        let original = TaskNode::builder("Test roundtrip".to_string())
            .with_status(TaskStatus::InProgress)
            .with_priority(3)
            .with_due_date(Some("2025-12-31".to_string()))
            .with_assignee_id(Some("user-123".to_string()))
            .build();

        // Convert to Node
        let node = original.into_node();

        // Convert back to TaskNode
        let roundtrip = TaskNode::from_node(node).unwrap();

        // Verify all properties preserved
        assert_eq!(roundtrip.status(), TaskStatus::InProgress);
        assert_eq!(roundtrip.priority(), 3);
        assert_eq!(roundtrip.due_date(), Some("2025-12-31".to_string()));
        assert_eq!(roundtrip.assignee_id(), Some("user-123".to_string()));
    }

    #[test]
    fn test_multiple_property_updates() {
        let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
        let mut task = TaskNode::from_node(node).unwrap();

        task.set_status(TaskStatus::InProgress);
        task.set_priority(4);
        task.set_due_date(Some("2025-06-15".to_string()));
        task.set_assignee_id(Some("user-789".to_string()));

        assert_eq!(task.status(), TaskStatus::InProgress);
        assert_eq!(task.priority(), 4);
        assert_eq!(task.due_date(), Some("2025-06-15".to_string()));
        assert_eq!(task.assignee_id(), Some("user-789".to_string()));
    }

    #[test]
    fn test_wrapper_preserves_non_task_properties() {
        // Node might have other properties besides task-specific ones
        let node = Node::new(
            "task".to_string(),
            "Test".to_string(),
            None,
            json!({
                "task": {"status": "pending"},
                "custom_field": "custom_value"
            }),
        );
        let task = TaskNode::from_node(node).unwrap();
        let converted = task.into_node();

        assert_eq!(converted.properties["custom_field"], "custom_value");
    }
}
