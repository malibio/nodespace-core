//! Node Operations Business Logic Layer
//!
//! This module provides a centralized business logic layer that enforces
//! data integrity rules for node creation, updates, and hierarchy management.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  MCP Handlers + Tauri Commands          │
//! └──────────────┬──────────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │  NodeOperations (Business Logic) ✅     │
//! │  - Container validation                 │
//! │  - Sibling position calculation         │
//! │  - Parent-container consistency         │
//! │  - Type-specific rules                  │
//! └──────────────┬──────────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │  NodeService (Database CRUD)            │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Business Rules Enforced
//!
//! 1. **Container Type Validation**: Container types (date, topic, project) force
//!    all hierarchy fields (parent_id, container_node_id, before_sibling_id) to None.
//!
//! 2. **Container Requirement**: Every non-container node MUST have a container_node_id.
//!    If not provided, it's inferred from the parent.
//!
//! 3. **Sibling Position Calculation**: If before_sibling_id is not provided,
//!    the node is placed as the last sibling automatically.
//!
//! 4. **Parent-Container Consistency**: Parent and child must be in the same container.
//!
//! 5. **Update Operations Restrictions**: Content updates cannot change hierarchy.
//!    Use move_node() or reorder_node() explicitly for hierarchy changes.
//!
//! # Examples
//!
//! ```no_run
//! use nodespace_core::operations::NodeOperations;
//! use nodespace_core::services::NodeService;
//! use nodespace_core::db::DatabaseService;
//! use std::path::PathBuf;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = DatabaseService::new(PathBuf::from("./data/test.db")).await?;
//!     let node_service = NodeService::new(db)?;
//!     let operations = NodeOperations::new(node_service);
//!
//!     // Create a node with automatic container inference
//!     let node_id = operations.create_node(
//!         "text".to_string(),
//!         "Hello World".to_string(),
//!         Some("parent-id".to_string()),
//!         None, // container_id will be inferred from parent
//!         None, // will be placed as last sibling
//!         json!({}),
//!     ).await?;
//!
//!     println!("Created node: {}", node_id);
//!     Ok(())
//! }
//! ```

pub mod error;

use crate::models::{DeleteResult, Node, NodeFilter};
use crate::services::NodeService;
use error::NodeOperationError;
use serde_json::Value;
use std::sync::Arc;

/// Core business logic layer for node operations
///
/// This struct enforces all business rules for node creation, updates, and
/// hierarchy management. It wraps NodeService and provides a higher-level
/// API that guarantees data integrity.
///
/// # Thread Safety
///
/// NodeOperations is `Clone` and thread-safe, using Arc internally for
/// efficient sharing across threads.
///
/// # Examples
///
/// ```no_run
/// # use nodespace_core::operations::NodeOperations;
/// # use nodespace_core::services::NodeService;
/// # use nodespace_core::db::DatabaseService;
/// # use std::path::PathBuf;
/// # use serde_json::json;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
/// # let node_service = NodeService::new(db)?;
/// let operations = NodeOperations::new(node_service);
///
/// // All operations automatically enforce business rules
/// let node_id = operations.create_node(
///     "text".to_string(),
///     "Content".to_string(),
///     Some("parent-id".to_string()),
///     None, // container_id inferred
///     None, // placed as last sibling
///     json!({}),
/// ).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct NodeOperations {
    /// Underlying NodeService for database operations
    node_service: Arc<NodeService>,
}

impl NodeOperations {
    /// Create a new NodeOperations instance
    ///
    /// # Arguments
    ///
    /// * `node_service` - The NodeService instance to use for database operations
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// let node_service = NodeService::new(db)?;
    /// let operations = NodeOperations::new(node_service);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(node_service: NodeService) -> Self {
        Self {
            node_service: Arc::new(node_service),
        }
    }

    // =========================================================================
    // CREATE Operations
    // =========================================================================

    /// Create a new node with business rule enforcement
    ///
    /// This method enforces all 5 business rules:
    /// 1. Container validation
    /// 2. Container requirement (with auto-inference)
    /// 3. Sibling position calculation
    /// 4. Parent-container consistency
    /// 5. Type-specific validation
    ///
    /// # Arguments
    ///
    /// * `node_type` - Type identifier (e.g., "text", "task", "date")
    /// * `content` - Primary content/text
    /// * `parent_id` - Optional parent node reference
    /// * `container_node_id` - Optional container reference (inferred from parent if None)
    /// * `before_sibling_id` - Optional sibling ordering (calculated as last if None)
    /// * `properties` - JSON object with entity-specific fields
    ///
    /// # Returns
    ///
    /// The ID of the created node
    ///
    /// # Errors
    ///
    /// Returns `NodeOperationError` if:
    /// - Container nodes have parent/container/sibling set
    /// - Non-container nodes lack container_node_id and parent doesn't exist
    /// - Parent-container mismatch
    /// - Invalid sibling chain
    /// - Node validation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// // Create a date node (container)
    /// let date_id = operations.create_node(
    ///     "date".to_string(),
    ///     "2025-01-03".to_string(),
    ///     None, // containers cannot have parent
    ///     None, // containers cannot have container
    ///     None, // containers cannot have sibling
    ///     json!({}),
    /// ).await?;
    ///
    /// // Create a text node with automatic container inference
    /// let text_id = operations.create_node(
    ///     "text".to_string(),
    ///     "Hello".to_string(),
    ///     Some(date_id.clone()), // parent
    ///     None, // container_id inferred from parent (date_id)
    ///     None, // placed as last sibling
    ///     json!({}),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn create_node(
        &self,
        _node_type: String,
        _content: String,
        _parent_id: Option<String>,
        _container_node_id: Option<String>,
        _before_sibling_id: Option<String>,
        _properties: Value,
    ) -> Result<String, NodeOperationError> {
        // Implementation will be added in Phase 4
        // For now, this is a placeholder to allow the module to compile
        todo!("Implementation in Phase 4")
    }

    // =========================================================================
    // READ Operations (simple delegation to NodeService)
    // =========================================================================

    /// Get a node by ID
    ///
    /// Simple delegation to NodeService with no additional business logic.
    ///
    /// # Arguments
    ///
    /// * `id` - The node ID to retrieve
    ///
    /// # Returns
    ///
    /// The node if found, None otherwise
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// let node = operations.get_node("node-id").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_node(&self, id: &str) -> Result<Option<Node>, NodeOperationError> {
        Ok(self.node_service.get_node(id).await?)
    }

    /// Query nodes with filter
    ///
    /// Simple delegation to NodeService with no additional business logic.
    ///
    /// # Arguments
    ///
    /// * `filter` - NodeFilter specifying query criteria
    ///
    /// # Returns
    ///
    /// Vector of matching nodes
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use nodespace_core::models::NodeFilter;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// let filter = NodeFilter::new().with_node_type("text".to_string());
    /// let nodes = operations.query_nodes(filter).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_nodes(&self, filter: NodeFilter) -> Result<Vec<Node>, NodeOperationError> {
        Ok(self.node_service.query_nodes(filter).await?)
    }

    // =========================================================================
    // UPDATE Operations
    // =========================================================================

    /// Update node content and properties (no hierarchy changes)
    ///
    /// This method enforces Rule 5: content updates cannot change hierarchy.
    /// Use move_node() or reorder_node() for hierarchy changes.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to update
    /// * `content` - Optional new content
    /// * `node_type` - Optional new node type
    /// * `properties` - Optional new properties
    ///
    /// # Errors
    ///
    /// Returns `NodeOperationError` if node doesn't exist or validation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// operations.update_node(
    ///     "node-id",
    ///     Some("Updated content".to_string()),
    ///     None,
    ///     Some(json!({"status": "done"})),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn update_node(
        &self,
        _node_id: &str,
        _content: Option<String>,
        _node_type: Option<String>,
        _properties: Option<Value>,
    ) -> Result<(), NodeOperationError> {
        // Implementation will be added in Phase 4
        todo!("Implementation in Phase 4")
    }

    /// Move a node to a new parent
    ///
    /// This method enforces Rules 2, 4, and 5:
    /// - Validates parent-container consistency
    /// - Updates container_node_id if needed
    /// - Separate from content updates
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to move
    /// * `new_parent_id` - The new parent (None to make it root)
    ///
    /// # Errors
    ///
    /// Returns `NodeOperationError` if parent-container mismatch or node doesn't exist
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// operations.move_node("node-id", Some("new-parent-id")).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn move_node(
        &self,
        _node_id: &str,
        _new_parent_id: Option<&str>,
    ) -> Result<(), NodeOperationError> {
        // Implementation will be added in Phase 4
        todo!("Implementation in Phase 4")
    }

    /// Reorder a node within its siblings
    ///
    /// This method enforces Rule 3: sibling position calculation.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to reorder
    /// * `before_sibling_id` - The sibling to place this node before (None = last)
    ///
    /// # Errors
    ///
    /// Returns `NodeOperationError` if invalid sibling chain or node doesn't exist
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// // Move node to end of sibling list
    /// operations.reorder_node("node-id", None).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::unused_async)]
    pub async fn reorder_node(
        &self,
        _node_id: &str,
        _before_sibling_id: Option<&str>,
    ) -> Result<(), NodeOperationError> {
        // Implementation will be added in Phase 4
        todo!("Implementation in Phase 4")
    }

    // =========================================================================
    // DELETE Operations
    // =========================================================================

    /// Delete a node
    ///
    /// Simple delegation to NodeService with cascade handling.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to delete
    ///
    /// # Returns
    ///
    /// DeleteResult indicating whether the node existed
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::operations::NodeOperations;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::DatabaseService;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DatabaseService::new(PathBuf::from("./test.db")).await?;
    /// # let node_service = NodeService::new(db)?;
    /// # let operations = NodeOperations::new(node_service);
    /// let result = operations.delete_node("node-id").await?;
    /// println!("Node existed: {}", result.existed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_node(&self, node_id: &str) -> Result<DeleteResult, NodeOperationError> {
        Ok(self.node_service.delete_node(node_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DatabaseService;
    use tempfile::TempDir;

    /// Helper to create a test database and NodeOperations instance
    async fn setup_test_operations() -> Result<(NodeOperations, TempDir), Box<dyn std::error::Error>>
    {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let db = DatabaseService::new(db_path).await?;
        let node_service = NodeService::new(db)?;
        let operations = NodeOperations::new(node_service);
        Ok((operations, temp_dir))
    }

    #[tokio::test]
    async fn test_operations_creation() {
        let result = setup_test_operations().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_operations_cloning() {
        let (operations, _temp_dir) = setup_test_operations().await.unwrap();
        let _cloned = operations.clone();

        // Both instances should work independently
        // (Full functionality tests will be added in Phase 4)
        // No assertion needed - just verifying clone compiles and works
    }

    // Additional tests will be added in Phase 4 when CRUD operations are implemented
}
