//! Bulk operations for NodeService.

use super::*;

impl NodeService {
    /// Bulk create multiple nodes in a transaction
    ///
    /// Creates multiple nodes atomically. If any node fails validation or insertion,
    /// the entire transaction is rolled back.
    ///
    /// # Arguments
    ///
    /// * `nodes` - Vector of nodes to create
    ///
    /// # Returns
    ///
    /// Vector of created node IDs in the same order as input
    ///
    /// # Errors
    ///
    /// Returns error if any node fails validation or insertion fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use nodespace_core::models::Node;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// let nodes = vec![
    ///     Node::new("text".to_string(), "Note 1".to_string(), json!({})),
    ///     Node::new("text".to_string(), "Note 2".to_string(), json!({})),
    /// ];
    /// let ids = service.bulk_create(nodes).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bulk_create(&self, nodes: Vec<Node>) -> Result<Vec<String>, NodeServiceError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Validate all nodes first (two-step validation)
        for node in &nodes {
            // Step 1: Core behavior validation
            self.behaviors.validate_node(node)?;

            // Step 2: Schema validation
            if node.node_type != "schema" {
                self.validate_node_against_schema(node).await?;
            }
        }

        // Call store trait to execute batch insert in transaction
        let created_nodes = self
            .store
            .batch_create_nodes(nodes)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // NOTE: NodeCreated events are now automatically emitted by store notifier (Issue #718)

        // Extract IDs for return (maintaining backward compatibility)
        Ok(created_nodes.into_iter().map(|n| n.id).collect())
    }

    /// Bulk create nodes with hierarchy in a single transaction (Issue #737)
    ///
    /// Creates multiple nodes with parent-child relationships atomically.
    /// This method is optimized for markdown import where all node data
    /// (IDs, hierarchy, ordering) is pre-calculated.
    ///
    /// # Arguments
    ///
    /// * `nodes` - Vector of tuples: (id, node_type, content, parent_id, order, properties)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - Vector of created node IDs in insertion order
    /// * `Err` - If validation or transaction fails
    ///
    /// # Performance
    ///
    /// This method provides ~10-15x speedup over sequential node creation
    /// by batching all database operations into a single transaction.
    pub async fn bulk_create_hierarchy(
        &self,
        nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )>,
    ) -> Result<Vec<String>, NodeServiceError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Performance optimization (Issue #760): Cache schema lookups by node_type
        // Instead of querying the database for each node, we query once per unique type
        let unique_types: std::collections::HashSet<&str> = nodes
            .iter()
            .map(|(_, node_type, _, _, _, _)| node_type.as_str())
            .collect();

        // Pre-fetch schemas for all unique types (excluding "schema" type itself)
        let mut schema_cache: std::collections::HashMap<
            String,
            Option<Vec<crate::models::SchemaField>>,
        > = std::collections::HashMap::new();
        for node_type in unique_types {
            if node_type != "schema" {
                let fields = match self.get_schema_for_type(node_type).await? {
                    Some(schema_json) => match schema_json.get("fields") {
                        Some(fields_json) => serde_json::from_value(fields_json.clone()).ok(),
                        None => None,
                    },
                    None => None,
                };
                schema_cache.insert(node_type.to_string(), fields);
            }
        }

        // Issue #854: Normalize flat properties to namespaced format before validation
        // Parser emits: { "status": "open" }
        // Storage expects: { "task": { "status": "open" } }
        let nodes_normalized: Vec<_> = nodes
            .into_iter()
            .map(|(id, node_type, content, parent_id, order, properties)| {
                let schema_fields = schema_cache.get(&node_type).and_then(|opt| opt.as_ref());
                let normalized_props = Self::normalize_flat_properties_to_namespace(
                    &node_type,
                    &properties,
                    schema_fields.map(|v| v.as_slice()),
                );
                (id, node_type, content, parent_id, order, normalized_props)
            })
            .collect();

        // Validate all nodes before insertion using cached schemas
        for (id, node_type, content, _, _, properties) in &nodes_normalized {
            // Build temporary Node for validation
            let temp_node = Node {
                id: id.clone(),
                node_type: node_type.clone(),
                content: content.clone(),
                version: 1,
                properties: properties.clone(),
                mentions: vec![],
                mentioned_in: vec![],
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                title: None, // Bulk nodes don't need titles (validated only)
                lifecycle_status: "active".to_string(),
            };

            // Validate via behaviors
            self.behaviors.validate_node(&temp_node)?;

            // Validate against cached schema (skip for schema nodes themselves)
            if node_type != "schema" {
                if let Some(Some(fields)) = schema_cache.get(node_type) {
                    self.validate_node_with_fields(&temp_node, fields)?;
                }
            }
        }

        // Find the root ID once - all nodes in a bulk import share the same root
        // Performance optimization (Issue #760): Single DB query instead of N queries
        let root_id = if let Some((_, _, _, Some(first_parent), _, _)) = nodes_normalized.first() {
            self.get_root_id(first_parent).await.ok()
        } else {
            None
        };

        // Delegate to store for atomic batch insert
        let result = self
            .store
            .bulk_create_hierarchy(nodes_normalized)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Queue root for embedding regeneration once (Issue #729, #760)
        // All nodes share the same root, so we only need one queue operation
        #[cfg(feature = "nlp")]
        if let Some(root_id) = root_id {
            self.queue_root_for_embedding(&root_id).await;
        }

        Ok(result)
    }

    /// Bulk create nodes with root-only notification (for large imports)
    ///
    /// Same as `bulk_create_hierarchy` but only emits domain events for the root node,
    /// making it more efficient for bulk import scenarios where per-node notifications
    /// would overwhelm the system.
    pub async fn bulk_create_hierarchy_root_notify(
        &self,
        nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )>,
    ) -> Result<Vec<String>, NodeServiceError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Performance optimization (Issue #760): Cache schema lookups by node_type
        let unique_types: std::collections::HashSet<&str> = nodes
            .iter()
            .map(|(_, node_type, _, _, _, _)| node_type.as_str())
            .collect();

        // Pre-fetch schemas for all unique types (excluding "schema" type itself)
        let mut schema_cache: std::collections::HashMap<
            String,
            Option<Vec<crate::models::SchemaField>>,
        > = std::collections::HashMap::new();
        for node_type in unique_types {
            if node_type != "schema" {
                let fields = match self.get_schema_for_type(node_type).await? {
                    Some(schema_json) => match schema_json.get("fields") {
                        Some(fields_json) => serde_json::from_value(fields_json.clone()).ok(),
                        None => None,
                    },
                    None => None,
                };
                schema_cache.insert(node_type.to_string(), fields);
            }
        }

        // Issue #854: Normalize flat properties to namespaced format before validation
        // Parser emits: { "status": "open" }
        // Storage expects: { "task": { "status": "open" } }
        let nodes_normalized: Vec<_> = nodes
            .into_iter()
            .map(|(id, node_type, content, parent_id, order, properties)| {
                let schema_fields = schema_cache.get(&node_type).and_then(|opt| opt.as_ref());
                let normalized_props = Self::normalize_flat_properties_to_namespace(
                    &node_type,
                    &properties,
                    schema_fields.map(|v| v.as_slice()),
                );
                (id, node_type, content, parent_id, order, normalized_props)
            })
            .collect();

        // Validate all nodes before insertion using cached schemas
        for (id, node_type, content, _, _, properties) in &nodes_normalized {
            let temp_node = Node {
                id: id.clone(),
                node_type: node_type.clone(),
                content: content.clone(),
                version: 1,
                properties: properties.clone(),
                mentions: vec![],
                mentioned_in: vec![],
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                title: None,
                lifecycle_status: "active".to_string(),
            };

            self.behaviors.validate_node(&temp_node)?;

            if node_type != "schema" {
                if let Some(Some(fields)) = schema_cache.get(node_type) {
                    self.validate_node_with_fields(&temp_node, fields)?;
                }
            }
        }

        // Find the root ID once
        let root_id = if let Some((_, _, _, Some(first_parent), _, _)) = nodes_normalized.first() {
            self.get_root_id(first_parent).await.ok()
        } else {
            None
        };

        // Delegate to store - use root-only notify variant
        let result = self
            .store
            .bulk_create_hierarchy_root_notify(nodes_normalized, vec![])
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Queue root for embedding regeneration once
        #[cfg(feature = "nlp")]
        if let Some(root_id) = root_id {
            self.queue_root_for_embedding(&root_id).await;
        }

        Ok(result)
    }

    /// Bulk create nodes with trusted input (skips schema validation)
    ///
    /// Optimized for import paths where the source is trusted (like markdown parser).
    /// This method:
    /// - Normalizes flat properties to namespaced format (Issue #854)
    /// - Skips schema DB queries (no lookup overhead)
    /// - Skips schema validation (parser output is trusted)
    /// - Still validates via behaviors (type-specific rules)
    ///
    /// # Issue #854: Import Pipeline Optimization
    ///
    /// The markdown parser only creates known node types with correct properties:
    /// - Task nodes get `{"status": "open"}`
    /// - Header, text, code-block nodes get `{}`
    ///
    /// Since the parser is trusted, we skip the expensive schema lookup and
    /// validation, but still normalize properties to the correct storage format.
    ///
    /// # Arguments
    ///
    /// * `nodes` - Vector of (id, node_type, content, parent_id, order, properties) tuples
    ///
    /// # Returns
    ///
    /// Vector of created node IDs
    pub async fn bulk_create_hierarchy_trusted(
        &self,
        nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )>,
    ) -> Result<Vec<String>, NodeServiceError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Issue #854: Normalize flat properties to namespaced format
        // Parser emits: { "status": "open" }
        // Storage expects: { "task": { "status": "open" } }
        // No schema fields needed - import properties are always simple values
        let nodes_normalized: Vec<_> = nodes
            .into_iter()
            .map(|(id, node_type, content, parent_id, order, properties)| {
                let normalized_props = Self::normalize_flat_properties_to_namespace(
                    &node_type,
                    &properties,
                    None, // No schema fields - import properties are simple
                );
                (id, node_type, content, parent_id, order, normalized_props)
            })
            .collect();

        // Validate via behaviors only (type-specific rules, no schema)
        for (id, node_type, content, _, _, properties) in &nodes_normalized {
            let temp_node = Node {
                id: id.clone(),
                node_type: node_type.clone(),
                content: content.clone(),
                version: 1,
                properties: properties.clone(),
                mentions: vec![],
                mentioned_in: vec![],
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                title: None,
                lifecycle_status: "active".to_string(),
            };

            // Only behavior validation - skip schema validation
            self.behaviors.validate_node(&temp_node)?;
        }

        // Collect embeddable root node IDs (nodes with no parent AND embeddable type)
        // Only these need embedding markers - matches single-create logic
        let root_ids: Vec<String> = nodes_normalized
            .iter()
            .filter_map(|(id, node_type, _, parent_id, _, _)| {
                if parent_id.is_none() && self.is_embeddable_type(node_type) {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        // Delegate to store - use root-only notify variant for efficiency
        let result = self
            .store
            .bulk_create_hierarchy_root_notify(nodes_normalized, vec![])
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Create stale embedding markers in bulk (single transaction)
        if !root_ids.is_empty() {
            match self
                .store
                .create_stale_embedding_markers_bulk(&root_ids)
                .await
            {
                Ok(count) => {
                    tracing::debug!("Created {} stale embedding markers", count);
                    // Wake the embedding processor once for all new roots
                    #[cfg(feature = "nlp")]
                    if let Some(ref waker) = self.embedding_waker {
                        tracing::debug!(
                            "🔔 Waking embedding processor for {} bulk-imported roots",
                            count
                        );
                        waker.wake();
                    }
                }
                Err(e) => {
                    // Log but don't fail - embeddings can be regenerated later
                    tracing::warn!("Failed to create stale embedding markers: {}", e);
                }
            }
        }

        Ok(result)
    }

    /// Bulk update multiple nodes in a transaction
    ///
    /// Updates multiple nodes atomically using a map of node IDs to NodeUpdate structs.
    ///
    /// # Arguments
    ///
    /// * `updates` - Vector of (node_id, NodeUpdate) tuples
    ///
    /// # Errors
    ///
    /// Returns error if any update fails. Transaction is rolled back on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use nodespace_core::models::NodeUpdate;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// let updates = vec![
    ///     ("node-1".to_string(), NodeUpdate::new().with_content("Updated 1".to_string())),
    ///     ("node-2".to_string(), NodeUpdate::new().with_content("Updated 2".to_string())),
    /// ];
    /// service.bulk_update(updates).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bulk_update(
        &self,
        updates: Vec<(String, NodeUpdate)>,
    ) -> Result<(), NodeServiceError> {
        if updates.is_empty() {
            return Ok(());
        }

        // Step 1: Batch-fetch all nodes in a single query (Issue #143)
        // This replaces the N+1 pattern where we called get_node() for each update
        let ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();
        let existing_nodes = self.store.get_nodes_by_ids(&ids).await.map_err(|e| {
            NodeServiceError::bulk_operation_failed(format!(
                "Failed to batch fetch nodes for validation: {}",
                e
            ))
        })?;

        // Step 2: Validate all nodes BEFORE performing atomic update.
        // This ensures we fail fast before any database changes.
        // Note: this validation snapshot is taken before the store transaction; any
        // concurrent write landing between here and store.bulk_update is silently
        // overwritten — intentional under the last-write-wins contract (see store doc).
        for (id, update) in &updates {
            // Look up existing node from batch result
            let existing = existing_nodes
                .get(id)
                .ok_or_else(|| NodeServiceError::node_not_found(id))?;

            let mut updated = existing.clone();

            // Apply partial updates to build validation candidate
            if let Some(node_type) = &update.node_type {
                updated.node_type = node_type.clone();
            }

            if let Some(content) = &update.content {
                updated.content = content.clone();
            }

            // NOTE: Sibling ordering is now handled via has_child relationship order field.
            // Bulk updates don't support sibling reordering - use move_node instead.

            if let Some(properties) = &update.properties {
                updated.properties = properties.clone();
            }

            // Validate behavior (PROTECTED rules)
            self.behaviors.validate_node(&updated).map_err(|e| {
                NodeServiceError::bulk_operation_failed(format!(
                    "Failed to validate node {}: {}",
                    id, e
                ))
            })?;

            // Validate schema (USER-EXTENSIBLE rules)
            if updated.node_type != "schema" {
                self.validate_node_against_schema(&updated)
                    .await
                    .map_err(|e| {
                        NodeServiceError::bulk_operation_failed(format!(
                            "Failed schema validation for node {}: {}",
                            id, e
                        ))
                    })?;
            }
        }

        // Step 3: All validations passed - perform atomic bulk update
        self.store.bulk_update(updates.clone()).await.map_err(|e| {
            NodeServiceError::bulk_operation_failed(format!(
                "Failed to execute bulk update transaction: {}",
                e
            ))
        })?;

        // NOTE: NodeUpdated events are now automatically emitted by store notifier (Issue #718)

        Ok(())
    }

    /// Bulk delete multiple nodes in a transaction
    ///
    /// Deletes multiple nodes atomically. If any deletion fails, the entire
    /// transaction is rolled back.
    ///
    /// # Arguments
    ///
    /// * `ids` - Vector of node IDs to delete
    ///
    /// # Errors
    ///
    /// Returns error if any deletion fails
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
    /// let ids = vec!["node-1".to_string(), "node-2".to_string()];
    /// service.bulk_delete(ids).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bulk_delete(&self, ids: Vec<String>) -> Result<(), NodeServiceError> {
        if ids.is_empty() {
            return Ok(());
        }

        // Delete nodes one by one using SqliteStore
        // SQLite handles atomicity within each delete operation
        for id in &ids {
            self.store
                .delete_node(id, self.client_id.clone())
                .await
                .map_err(|e| {
                    NodeServiceError::bulk_operation_failed(format!(
                        "Failed to delete node {}: {}",
                        id, e
                    ))
                })?;

            // NOTE: NodeDeleted event is now automatically emitted by store notifier (Issue #718)
        }

        Ok(())
    }
}
