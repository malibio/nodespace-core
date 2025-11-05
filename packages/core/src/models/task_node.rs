//! Type-Safe TaskNode Wrapper
//!
//! Provides compile-time type safety and ergonomic API for task nodes while
//! maintaining the universal Node storage model.
//!
//! # Architecture
//!
//! - **Universal Storage**: Database continues using single `Node` struct
//! - **Type-Safe Wrapper**: Optional convenience layer for task-specific operations
//! - **Zero Overhead**: Wrappers are compile-time abstractions with no runtime cost
//!
//! # Examples
//!
//! ```rust
//! use nodespace_core::models::{Node, TaskNode, TaskStatus};
//! use serde_json::json;
//!
//! // Create a new task
//! let task = TaskNode::builder("Implement feature".to_string())
//!     .with_status(TaskStatus::InProgress)
//!     .with_priority(3)
//!     .build();
//!
//! // Type-safe property access
//! assert_eq!(task.status(), TaskStatus::InProgress);
//! assert_eq!(task.priority(), 3);
//!
//! // Convert to universal Node for storage
//! let node = task.into_node();
//! ```

use crate::models::{Node, ValidationError};
use serde_json::json;
use std::str::FromStr;

/// Task status enum for type-safe status management
///
/// Maps to string values in the properties JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task not yet started (default)
    Pending,
    /// Task currently being worked on
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task cancelled and will not be completed
    Cancelled,
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            // Also support uppercase variants for backward compatibility
            "PENDING" => Ok(Self::Pending),
            "IN_PROGRESS" => Ok(Self::InProgress),
            "COMPLETED" => Ok(Self::Completed),
            "CANCELLED" => Ok(Self::Cancelled),
            // Legacy status names from old behavior system
            "OPEN" => Ok(Self::Pending),
            "DONE" => Ok(Self::Completed),
            _ => Err(format!("Invalid task status: {}", s)),
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Type-safe wrapper for task nodes
///
/// Provides ergonomic API for working with task properties while maintaining
/// the universal Node storage model underneath.
///
/// # Examples
///
/// ```rust
/// use nodespace_core::models::{Node, TaskNode, TaskStatus};
/// use serde_json::json;
///
/// // Wrap an existing Node
/// let node = Node::new(
///     "task".to_string(),
///     "Write tests".to_string(),
///     None,
///     json!({"task": {"status": "in_progress", "priority": 2}}),
/// );
/// let task = TaskNode::from_node(node)?;
///
/// // Type-safe access
/// assert_eq!(task.status(), TaskStatus::InProgress);
/// assert_eq!(task.priority(), 2);
/// # Ok::<(), nodespace_core::models::ValidationError>(())
/// ```
pub struct TaskNode {
    node: Node,
}

impl TaskNode {
    /// Create TaskNode from universal Node with validation
    ///
    /// # Arguments
    ///
    /// * `node` - Universal Node to wrap (must have node_type = "task")
    ///
    /// # Errors
    ///
    /// Returns `ValidationError::InvalidNodeType` if node_type is not "task"
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nodespace_core::models::{Node, TaskNode};
    /// use serde_json::json;
    ///
    /// let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let task = TaskNode::from_node(node)?;
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn from_node(node: Node) -> Result<Self, ValidationError> {
        if node.node_type != "task" {
            return Err(ValidationError::InvalidNodeType(format!(
                "Expected node_type 'task', got '{}'",
                node.node_type
            )));
        }
        Ok(Self { node })
    }

    /// Get reference to underlying universal Node
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let task = TaskNode::from_node(node)?;
    /// let node_ref = task.as_node();
    /// assert_eq!(node_ref.node_type, "task");
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn as_node(&self) -> &Node {
        &self.node
    }

    /// Get mutable reference to underlying Node
    ///
    /// Allows direct manipulation of Node fields when needed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let mut task = TaskNode::from_node(node)?;
    /// task.as_node_mut().parent_id = Some("parent-123".to_string());
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn as_node_mut(&mut self) -> &mut Node {
        &mut self.node
    }

    /// Convert back to universal Node (consumes wrapper)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let task = TaskNode::from_node(node)?;
    /// let node = task.into_node();
    /// assert_eq!(node.node_type, "task");
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn into_node(self) -> Node {
        self.node
    }

    /// Get task status with type safety
    ///
    /// Returns `TaskStatus::Pending` as default if status is missing or invalid.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode, TaskStatus};
    /// # use serde_json::json;
    /// let node = Node::new(
    ///     "task".to_string(),
    ///     "Test".to_string(),
    ///     None,
    ///     json!({"task": {"status": "in_progress"}}),
    /// );
    /// let task = TaskNode::from_node(node)?;
    /// assert_eq!(task.status(), TaskStatus::InProgress);
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn status(&self) -> TaskStatus {
        self.node
            .properties
            .get("task")
            .and_then(|task_props| task_props.get("status"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(TaskStatus::Pending)
    }

    /// Set task status with type safety
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode, TaskStatus};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let mut task = TaskNode::from_node(node)?;
    /// task.set_status(TaskStatus::Completed);
    /// assert_eq!(task.status(), TaskStatus::Completed);
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn set_status(&mut self, status: TaskStatus) {
        self.ensure_task_properties();
        if let Some(task_props) = self.node.properties.get_mut("task") {
            if let Some(obj) = task_props.as_object_mut() {
                obj.insert("status".to_string(), json!(status.to_string()));
            }
        }
    }

    /// Get task priority (1-4 scale, 2 = default medium)
    ///
    /// Returns 2 as default if priority is missing or invalid.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// let node = Node::new(
    ///     "task".to_string(),
    ///     "Test".to_string(),
    ///     None,
    ///     json!({"task": {"priority": 3}}),
    /// );
    /// let task = TaskNode::from_node(node)?;
    /// assert_eq!(task.priority(), 3);
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn priority(&self) -> i32 {
        self.node
            .properties
            .get("task")
            .and_then(|task_props| task_props.get("priority"))
            .and_then(|v| v.as_i64())
            .map(|p| p as i32)
            .unwrap_or(2)
    }

    /// Set task priority (1-4 scale)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let mut task = TaskNode::from_node(node)?;
    /// task.set_priority(3);
    /// assert_eq!(task.priority(), 3);
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn set_priority(&mut self, priority: i32) {
        self.ensure_task_properties();
        if let Some(task_props) = self.node.properties.get_mut("task") {
            if let Some(obj) = task_props.as_object_mut() {
                obj.insert("priority".to_string(), json!(priority));
            }
        }
    }

    /// Get task due date (ISO 8601 format)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// let node = Node::new(
    ///     "task".to_string(),
    ///     "Test".to_string(),
    ///     None,
    ///     json!({"task": {"due_date": "2025-12-31"}}),
    /// );
    /// let task = TaskNode::from_node(node)?;
    /// assert_eq!(task.due_date(), Some("2025-12-31".to_string()));
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn due_date(&self) -> Option<String> {
        self.node
            .properties
            .get("task")
            .and_then(|task_props| task_props.get("due_date"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Set task due date (ISO 8601 format recommended)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let mut task = TaskNode::from_node(node)?;
    /// task.set_due_date(Some("2025-12-31".to_string()));
    /// assert_eq!(task.due_date(), Some("2025-12-31".to_string()));
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn set_due_date(&mut self, due_date: Option<String>) {
        self.ensure_task_properties();
        if let Some(task_props) = self.node.properties.get_mut("task") {
            if let Some(obj) = task_props.as_object_mut() {
                obj.insert(
                    "due_date".to_string(),
                    due_date.map(|d| json!(d)).unwrap_or(json!(null)),
                );
            }
        }
    }

    /// Get task assignee ID
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// let node = Node::new(
    ///     "task".to_string(),
    ///     "Test".to_string(),
    ///     None,
    ///     json!({"task": {"assignee_id": "user-123"}}),
    /// );
    /// let task = TaskNode::from_node(node)?;
    /// assert_eq!(task.assignee_id(), Some("user-123".to_string()));
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn assignee_id(&self) -> Option<String> {
        self.node
            .properties
            .get("task")
            .and_then(|task_props| task_props.get("assignee_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Set task assignee ID
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use nodespace_core::models::{Node, TaskNode};
    /// # use serde_json::json;
    /// # let node = Node::new("task".to_string(), "Test".to_string(), None, json!({}));
    /// let mut task = TaskNode::from_node(node)?;
    /// task.set_assignee_id(Some("user-123".to_string()));
    /// assert_eq!(task.assignee_id(), Some("user-123".to_string()));
    /// # Ok::<(), nodespace_core::models::ValidationError>(())
    /// ```
    pub fn set_assignee_id(&mut self, assignee_id: Option<String>) {
        self.ensure_task_properties();
        if let Some(task_props) = self.node.properties.get_mut("task") {
            if let Some(obj) = task_props.as_object_mut() {
                obj.insert(
                    "assignee_id".to_string(),
                    assignee_id.map(|id| json!(id)).unwrap_or(json!(null)),
                );
            }
        }
    }

    /// Create a TaskNodeBuilder for building new TaskNode instances
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nodespace_core::models::{TaskNode, TaskStatus};
    ///
    /// let task = TaskNode::builder("Implement feature".to_string())
    ///     .with_status(TaskStatus::InProgress)
    ///     .with_priority(3)
    ///     .build();
    /// ```
    pub fn builder(content: String) -> TaskNodeBuilder {
        TaskNodeBuilder {
            content,
            parent_id: None,
            status: TaskStatus::Pending,
            priority: 2,
            due_date: None,
            assignee_id: None,
        }
    }

    /// Ensure task properties object exists
    fn ensure_task_properties(&mut self) {
        if !self.node.properties.is_object() {
            self.node.properties = json!({});
        }
        if self.node.properties.get("task").is_none() {
            if let Some(obj) = self.node.properties.as_object_mut() {
                obj.insert("task".to_string(), json!({}));
            }
        }
    }
}

/// Builder for creating new TaskNode instances
///
/// Provides fluent API for constructing tasks with properties.
///
/// # Examples
///
/// ```rust
/// use nodespace_core::models::{TaskNode, TaskStatus};
///
/// let task = TaskNode::builder("Write documentation".to_string())
///     .with_status(TaskStatus::InProgress)
///     .with_priority(3)
///     .with_due_date(Some("2025-12-31".to_string()))
///     .build();
///
/// assert_eq!(task.status(), TaskStatus::InProgress);
/// assert_eq!(task.priority(), 3);
/// ```
pub struct TaskNodeBuilder {
    content: String,
    parent_id: Option<String>,
    status: TaskStatus,
    priority: i32,
    due_date: Option<String>,
    assignee_id: Option<String>,
}

impl TaskNodeBuilder {
    /// Set parent ID for the task
    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set task status
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Set task priority (1-4 scale)
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set task due date
    pub fn with_due_date(mut self, due_date: Option<String>) -> Self {
        self.due_date = due_date;
        self
    }

    /// Set task assignee ID
    pub fn with_assignee_id(mut self, assignee_id: Option<String>) -> Self {
        self.assignee_id = assignee_id;
        self
    }

    /// Build the TaskNode
    pub fn build(self) -> TaskNode {
        let properties = json!({
            "task": {
                "status": self.status.to_string(),
                "priority": self.priority,
                "due_date": self.due_date,
                "assignee_id": self.assignee_id
            }
        });

        let node = Node::new(
            "task".to_string(),
            self.content,
            self.parent_id,
            properties,
        );

        TaskNode { node }
    }
}
