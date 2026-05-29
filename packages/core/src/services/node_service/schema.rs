//! Schema-related operations for NodeService.

use super::*;

impl NodeService {
    /// Query nodes by type with optional lifecycle_status filter.
    ///
    /// Used by the playbook engine to load all active playbooks at startup.
    /// If `lifecycle_status` is `None`, returns all lifecycle statuses.
    pub async fn query_nodes_by_type(
        &self,
        node_type: &str,
        lifecycle_status: Option<&str>,
    ) -> Result<Vec<Node>, NodeServiceError> {
        let query = crate::models::NodeQuery {
            node_type: Some(node_type.to_string()),
            ..Default::default()
        };

        let nodes = self
            .store
            .query_nodes(query)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // In-memory filter: NodeQuery doesn't support lifecycle_status yet.
        // Acceptable for desktop (low playbook counts). If scaling becomes
        // a concern, add lifecycle_status to NodeQuery/SqliteStore query.
        let filtered: Vec<Node> = if let Some(status) = lifecycle_status {
            nodes
                .into_iter()
                .filter(|n| n.lifecycle_status == status)
                .collect()
        } else {
            nodes
        };

        Ok(filtered)
    }

    /// Get schema definition for a given node type
    pub async fn get_schema_for_type(
        &self,
        node_type: &str,
    ) -> Result<Option<serde_json::Value>, NodeServiceError> {
        self.store
            .get_schema(node_type)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Get a task node with strong typing
    ///
    /// Returns strongly-typed `TaskNode` instead of generic `Node`.
    ///
    /// # Arguments
    ///
    /// * `id` - The task node ID
    ///
    /// # Returns
    ///
    /// * `Ok(Some(TaskNode))` - Task found with strongly-typed fields
    /// * `Ok(None)` - Task not found
    /// * `Err(_)` - Service error
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// if let Some(task) = service.get_task_node("my-task-id").await? {
    ///     // Direct field access - no JSON parsing
    ///     println!("Status: {:?}", task.status);
    ///     println!("Content: {}", task.content);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_task_node(
        &self,
        id: &str,
    ) -> Result<Option<crate::models::TaskNode>, NodeServiceError> {
        self.store.get_task_node(id).await.map_err(|e| {
            NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                context: format!("Failed to get task node '{}': {}", id, e),
            })
        })
    }

    /// Update a task node with type-safe field updates
    ///
    /// Updates task-specific fields (status, priority, due_date, assignee).
    /// Uses optimistic concurrency control (OCC) to prevent lost updates.
    ///
    /// # Type Safety
    ///
    /// This method provides end-to-end type safety for task updates:
    /// - Frontend sends strongly-typed `TaskNodeUpdate` (not generic NodeUpdate)
    /// - Backend updates task fields directly (not via JSON properties)
    /// - Returns strongly-typed `TaskNode` with updated fields
    ///
    /// # Arguments
    ///
    /// * `id` - The task node ID
    /// * `expected_version` - Version for OCC check (prevents lost updates)
    /// * `update` - TaskNodeUpdate with fields to update
    ///
    /// # Returns
    ///
    /// * `Ok(TaskNode)` - Updated task with new version
    /// * `Err(VersionMismatch)` - Version conflict, refresh and retry
    /// * `Err(NodeNotFound)` - Task doesn't exist
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::models::{TaskNodeUpdate, TaskStatus};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Update task status
    /// let update = TaskNodeUpdate::new().with_status(TaskStatus::InProgress);
    /// let task = service.update_task_node("task-123", 1, update).await?;
    /// println!("New status: {:?}", task.status);
    /// println!("New version: {}", task.version);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_task_node(
        &self,
        id: &str,
        expected_version: i64,
        update: crate::models::TaskNodeUpdate,
    ) -> Result<crate::models::TaskNode, NodeServiceError> {
        if update.is_empty() {
            return Err(NodeServiceError::invalid_update(
                "TaskNodeUpdate contains no changes",
            ));
        }

        self.store
            .update_task_node(id, expected_version, update)
            .await
            .map_err(|e| {
                // Get the full error chain for pattern matching
                // anyhow errors chain with context, so we need to check the full string
                let error_msg = format!("{:#}", e); // Use alternate format for full chain
                let root_cause = e.root_cause().to_string();

                if error_msg.contains("VersionMismatch")
                    || root_cause.contains("VersionMismatch")
                    || root_cause.contains("failed transaction")
                {
                    // A failed transaction surfaces as a "failed transaction" error.
                    // Our only abort is for version mismatch, so treat failed transactions as OCC errors.
                    // Note: This is a simplification - ideally the abort message would be preserved.
                    NodeServiceError::VersionConflict {
                        node_id: id.to_string(),
                        expected_version,
                        actual_version: 0, // Actual version unknown when transaction fails
                    }
                } else if error_msg.contains("not found")
                    || error_msg.contains("Record not found")
                    || error_msg.contains("$current[0].version")
                    || root_cause.contains("not found")
                    || root_cause.contains("$current")
                {
                    // The store returns various error formats for missing records
                    // "Record not found" - explicit record error
                    // "$current[0].version" - when the LET query returns empty and IF fails
                    NodeServiceError::node_not_found(id)
                } else {
                    NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                        context: format!("Failed to update task node '{}': {}", id, e),
                    })
                }
            })
    }

    /// Get a schema node with strong typing
    ///
    /// Returns strongly-typed `SchemaNode` instead of generic `Node`.
    ///
    /// # Arguments
    ///
    /// * `id` - The schema node ID (e.g., "task", "date")
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SchemaNode))` - Schema found with strongly-typed fields
    /// * `Ok(None)` - Schema not found
    /// * `Err(_)` - Service error
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// if let Some(schema) = service.get_schema_node("task").await? {
    ///     // Direct field access - no JSON parsing
    ///     println!("Is core: {}", schema.is_core);
    ///     println!("Fields: {:?}", schema.fields.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_schema_node(
        &self,
        id: &str,
    ) -> Result<Option<crate::models::SchemaNode>, NodeServiceError> {
        self.store.get_schema_node(id).await.map_err(|e| {
            NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                context: format!("Failed to get schema node '{}': {}", id, e),
            })
        })
    }

    /// Rename a field across all node instances and update the schema definition (Issue #1088).
    pub async fn rename_schema_field(
        &self,
        type_id: &str,
        from: &str,
        to: &str,
    ) -> Result<u64, NodeServiceError> {
        // Validate schema exists
        let schema = self
            .get_schema_node(type_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(type_id))?;

        // Validate source field exists in schema
        if !schema.fields.iter().any(|f| f.name == from) {
            return Err(NodeServiceError::invalid_update(format!(
                "Field '{}' not found in schema '{}'",
                from, type_id
            )));
        }

        // Validate destination field does not already exist
        if schema.fields.iter().any(|f| f.name == to) {
            return Err(NodeServiceError::invalid_update(format!(
                "Field '{}' already exists in schema '{}'; cannot rename to an existing field",
                to, type_id
            )));
        }

        // Step 1: Migrate all node property data
        let affected = self
            .store
            .rename_schema_field(type_id, from, to)
            .await
            .map_err(|e| {
                NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                    context: format!(
                        "Failed to migrate field data '{}' -> '{}' for type '{}': {}",
                        from, to, type_id, e
                    ),
                })
            })?;

        // Step 2: Update schema definition — rename field in the fields list
        let updated_fields: Vec<crate::models::schema::SchemaField> = schema
            .fields
            .into_iter()
            .map(|mut f| {
                if f.name == from {
                    f.name = to.to_string();
                }
                f
            })
            .collect();

        let mut properties = serde_json::json!({
            "isCore": schema.is_core,
            "schemaVersion": schema.schema_version,
            "description": schema.description,
            "fields": updated_fields,
            "relationships": schema.relationships,
        });
        if let Some(ref t) = schema.title_template {
            properties["titleTemplate"] = serde_json::Value::String(t.clone());
        }
        if let Some(ref t) = schema.properties_header_summary_template {
            properties["propertiesHeaderSummaryTemplate"] = serde_json::Value::String(t.clone());
        }

        let update = crate::models::NodeUpdate {
            properties: Some(properties),
            ..Default::default()
        };

        self.update_node_unchecked(type_id, update)
            .await
            .map_err(|e| NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                context: format!(
                    "Failed to update schema definition after field rename '{}' -> '{}' for type '{}': {}",
                    from, to, type_id, e
                ),
            }))?;

        Ok(affected)
    }

    /// Get all schema nodes with their relationships
    ///
    /// Returns all schema definitions including fields and relationships.
    /// This is the primary entry point for NLP to understand the data model.
    ///
    /// # Returns
    ///
    /// Vector of all schema nodes, ordered by ID.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Get all schemas to understand the data model
    /// let schemas = service.get_all_schemas().await?;
    /// for schema in schemas {
    ///     println!("Type: {} ({} fields, {} relationships)",
    ///         schema.id, schema.fields.len(), schema.relationships.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_all_schemas(
        &self,
    ) -> Result<Vec<crate::models::SchemaNode>, NodeServiceError> {
        self.store.get_all_schemas().await.map_err(|e| {
            NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                context: format!("Failed to get all schemas: {}", e),
            })
        })
    }

    /// Get a schema with full relationship information
    ///
    /// Convenience method that returns a SchemaNode with its relationships.
    /// Use this when you need the complete schema definition including relationships.
    ///
    /// # Arguments
    ///
    /// * `schema_id` - The schema ID (e.g., "task", "invoice")
    ///
    /// # Returns
    ///
    /// The SchemaNode if found, None otherwise.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// if let Some(schema) = service.get_schema_with_relationships("invoice").await? {
    ///     for rel in &schema.relationships {
    ///         let target = rel.target_type.as_deref().unwrap_or("*");
    ///         println!("{} -> {} ({:?})", rel.name, target, rel.cardinality);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_schema_with_relationships(
        &self,
        schema_id: &str,
    ) -> Result<Option<crate::models::SchemaNode>, NodeServiceError> {
        // get_schema_node already includes relationships now
        self.get_schema_node(schema_id).await
    }

    /// Check whether a node satisfies all required relationships in its schema
    pub async fn check_node_completeness(
        &self,
        node_id: &str,
    ) -> Result<CompletenessResult, NodeServiceError> {
        // Look up the node
        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Look up the schema for the node's type
        let schema_node = self.get_schema_node(&node.node_type).await?;

        let Some(schema) = schema_node else {
            // No schema → nothing required → complete by definition
            return Ok(CompletenessResult {
                node_id: node_id.to_string(),
                is_complete: true,
                missing_relationships: vec![],
            });
        };

        let mut missing = Vec::new();

        for relationship in &schema.relationships {
            // Only check relationships explicitly marked as required
            if relationship.required != Some(true) {
                continue;
            }

            // Check whether at least one edge of this relationship type exists
            let existing_count = self
                .store
                .check_relationship_exists(node_id, &relationship.name)
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!(
                        "Failed to check required relationship '{}': {}",
                        relationship.name, e
                    ))
                })?;
            if existing_count == 0 {
                missing.push(relationship.name.clone());
            }
        }

        Ok(CompletenessResult {
            node_id: node_id.to_string(),
            is_complete: missing.is_empty(),
            missing_relationships: missing,
        })
    }
}
