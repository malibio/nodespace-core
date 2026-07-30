//! CRUD operations for NodeService.

use super::*;

impl NodeService {
    /// Create a new node
    ///
    /// Validates the node using the appropriate behavior (Text, Task, or Date),
    /// then inserts it into the database.
    ///
    /// # Arguments
    ///
    /// * `node` - The node to create
    ///
    /// # Returns
    ///
    /// The ID of the created node
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Node validation fails
    /// - Parent node doesn't exist (if parent_id is set)
    /// - Root node doesn't exist (if root_id is set)
    /// - Database insertion fails
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
    /// let node = Node::new(
    ///     "text".to_string(),
    ///     "My note".to_string(),
    ///     json!({}),
    /// );
    /// let id = service.create_node(node).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_node(&self, mut node: Node) -> Result<String, NodeServiceError> {
        let start = std::time::Instant::now();
        tracing::debug!(node_type = %node.node_type, node_id = %node.id, "create_node: START");

        // Auto-detect date nodes by ID format (YYYY-MM-DD) to ensure correct node_type.
        // This maintains data integrity regardless of caller mistakes.
        // NOTE: Date nodes can have custom content (not required to match ID).
        // We only enforce the node_type, not the content.
        if is_date_node_id(&node.id) {
            node.node_type = "date".to_string();
            // Content is preserved - date nodes can have custom content like "Custom Date Content"
        }

        // DatabaseSettingsNode is a singleton (ADR-037). The fixed reserved ID makes
        // creation idempotent: if one already exists, treat a second create as a no-op
        // and return the existing id rather than erroring. Mirrors the collection-name
        // uniqueness guard in SqliteStore::create_node, but non-fatal.
        if node.node_type == "database-settings" {
            if let Some(existing) = self
                .query_nodes_by_type("database-settings", None)
                .await?
                .into_iter()
                .next()
            {
                tracing::debug!(
                    node_id = %existing.id,
                    "create_node: database-settings singleton already exists, no-op"
                );
                return Ok(existing.id);
            }
        }

        // Step 1: Core behavior validation (PROTECTED)
        // Validates basic data integrity (non-empty content, correct types, etc.)
        self.behaviors.validate_node(&node)?;
        tracing::debug!(
            "create_node: behavior validation at {}ms",
            start.elapsed().as_millis()
        );

        // Step 1.5: Apply schema defaults, validate, and add version
        // Fetch schema ONCE and reuse for all operations (performance fix)
        // Schema processing: Only fetch schema from DB for types with meaningful schema fields.
        // Currently only "task" has schema-defined fields; text, date, etc. have no fields.
        // This avoids a ~760ms database lookup for every node creation.
        //
        // NOTE: We ONLY apply schema defaults, NOT behavior defaults.
        // Behavior defaults (markdown_enabled, auto_save, etc.) are UI preferences
        // that should be handled client-side, not stored in database properties.
        // The properties field is for user data and schema-defined fields only.
        if node.node_type == "task" {
            let schema_start = std::time::Instant::now();
            // Fetch schema ONCE and reuse it for all operations
            if let Some(schema_json) = self.get_schema_for_type(&node.node_type).await? {
                tracing::debug!(
                    "create_node: schema fetched in {}ms",
                    schema_start.elapsed().as_millis()
                );
                // Parse schema fields
                if let Some(fields_json) = schema_json.get("fields") {
                    if let Ok(fields) = serde_json::from_value::<Vec<crate::models::SchemaField>>(
                        fields_json.clone(),
                    ) {
                        // Normalize flat properties to namespaced format before processing
                        // Clients send: { "status": "open" }
                        // Storage format: { "task": { "status": "open" } }
                        node.properties = Self::normalize_flat_properties_to_namespace(
                            &node.node_type,
                            &node.properties,
                            Some(&fields),
                        );

                        // Apply defaults from schema fields only
                        self.apply_schema_defaults_with_fields(&mut node, &fields)?;

                        // Validate with the same fields
                        self.validate_node_with_fields(&node, &fields)?;

                        // Add schema version if schema has fields
                        // Using the already-fetched schema instead of fetching again
                        if !fields.is_empty() {
                            if let Some(version) =
                                schema_json.get("version").and_then(|v| v.as_i64())
                            {
                                if let Some(props_obj) = node.properties.as_object_mut() {
                                    let type_namespace = props_obj
                                        .entry(&node.node_type)
                                        .or_insert_with(|| serde_json::json!({}));
                                    if let Some(type_props) = type_namespace.as_object_mut() {
                                        type_props.insert(
                                            "_schema_version".to_string(),
                                            serde_json::json!(version),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            tracing::debug!(
                "create_node: schema processing complete at {}ms",
                start.elapsed().as_millis()
            );
        } else if node.node_type != "schema" {
            // Non-task, non-schema types: normalize properties without DB lookup
            node.properties = Self::normalize_flat_properties_to_namespace(
                &node.node_type,
                &node.properties,
                None,
            );
        }

        // NOTE: Parent/container validation removed - now handled by NodeOperations layer
        // The graph-native architecture uses edges for hierarchy, not fields on Node struct

        // NOTE: root_id filtering removed - hierarchy now managed via relationships

        // Populate title for @mention search
        // Schema-driven title_template support
        // Only set title if not already set (create_node_with_parent may have set it for root nodes)
        if node.title.is_none() {
            // For task/collection we know they're always titled; for others we need to check
            // is_root=None will only trigger a DB lookup for non-task/collection/date/schema types
            node.title = self.compute_title(&node, None).await?;
        }

        // Synchronous playbook validation gate — reject invalid playbooks before persist
        if node.node_type == "playbook" {
            self.validate_playbook_rules(&node.properties).await?;
        }

        // Schema nodes go through the normal create path
        let db_start = std::time::Instant::now();
        self.store
            .create_node(
                node.clone(),
                self.client_id.clone(),
                self.execution_context.clone(),
            )
            .await
            .map_err(|e| NodeServiceError::query_failed(format!("Failed to insert node: {}", e)))?;
        tracing::debug!(
            "create_node: database insert completed in {}ms",
            db_start.elapsed().as_millis()
        );

        // NOTE: NodeCreated event is now automatically emitted by store notifier

        tracing::debug!(
            node_id = %node.id,
            "create_node: COMPLETE at {}ms",
            start.elapsed().as_millis()
        );
        Ok(node.id)
    }

    /// Create a node with parent relationship in a single operation
    ///
    /// This is the primary node creation API that enforces all business rules:
    /// 1. Auto-creates date containers (YYYY-MM-DD) if parent is a date ID
    /// 2. Validates parent exists (if provided)
    /// 3. Creates the node with proper validation
    /// 4. Establishes parent-child edge with correct sibling ordering
    ///
    /// # Arguments
    ///
    /// * `params` - CreateNodeParams containing all node creation parameters
    ///
    /// # Returns
    ///
    /// The ID of the created node
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Parent doesn't exist (and isn't a valid date format)
    /// - Node validation fails
    /// - ID format is invalid (non-UUID for production nodes)
    ///
    /// Note: If `position` is `InsertPositionOwned::After(sibling_id)` and that
    /// sibling no longer exists or has moved to a different parent (stale hint from
    /// a race condition), the operation falls back to `End` rather than failing.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::{CreateNodeParams, InsertPositionOwned, NodeService};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Create a child node under a date container
    /// let id = service.create_node_with_parent(CreateNodeParams {
    ///     id: None,
    ///     node_type: "text".to_string(),
    ///     content: "My note".to_string(),
    ///     parent_id: Some("2025-01-15".to_string()),
    ///     position: InsertPositionOwned::Beginning,
    ///     properties: json!({}),
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_node_with_parent(
        &self,
        params: CreateNodeParams,
    ) -> Result<String, NodeServiceError> {
        // Make params mutable so we can resolve InsertPositionOwned::End
        let mut params = params;
        let start = std::time::Instant::now();
        tracing::debug!(
            node_type = %params.node_type,
            has_parent = params.parent_id.is_some(),
            "create_node_with_parent: START"
        );

        // Step 1: Auto-create date container if parent is a date ID
        if let Some(ref parent_id) = params.parent_id {
            self.ensure_date_exists(parent_id).await?;
        }

        // Step 2: Validate parent exists and is a container (if provided)
        if let Some(ref parent_id) = params.parent_id {
            let parent_node = self
                .get_node(parent_id)
                .await?
                .ok_or_else(|| NodeServiceError::invalid_parent(parent_id.as_str()))?;

            if !self
                .behavior_for(&parent_node.node_type)
                .can_have_children()
            {
                return Err(NodeServiceError::not_a_container(
                    parent_id.as_str(),
                    &parent_node.node_type,
                ));
            }
        }

        // Step 3: Validate sibling (if After) - treat as best-effort hint.
        // If the sibling doesn't exist or has moved to a different parent, fall
        // back to End so new nodes land at the bottom rather than the top.
        // SQLite is synchronous/ACID: a node written by a prior awaited call is
        // immediately visible; a single check is sufficient.
        if let crate::services::InsertPositionOwned::After(ref sibling_id) = params.position.clone()
        {
            let sibling_valid = match self.get_node(sibling_id).await {
                Ok(Some(_)) => match self.get_parent(sibling_id).await {
                    Ok(sibling_parent) => {
                        let sibling_parent_id = sibling_parent.as_ref().map(|p| p.id.as_str());
                        sibling_parent_id == params.parent_id.as_deref()
                    }
                    Err(_) => false,
                },
                _ => false,
            };

            if !sibling_valid {
                tracing::warn!(
                    sibling_id = %sibling_id,
                    parent_id = ?params.parent_id,
                    "position sibling is stale (moved or deleted), falling back to End"
                );
                params.position = crate::services::InsertPositionOwned::End;
            }
        }

        // Step 4: Generate or validate node ID
        let node_id = if let Some(provided_id) = params.id {
            // Validate ID format based on node type
            if params.node_type == "date"
                || params.node_type == "schema"
                || provided_id.starts_with("test-")
            {
                // Date, schema, and test nodes can use their own ID format
                provided_id
            } else {
                // Production nodes must use UUID format
                uuid::Uuid::parse_str(&provided_id).map_err(|_| {
                    NodeServiceError::invalid_update(format!(
                        "Provided ID '{}' is not a valid UUID format (required for non-date/non-schema nodes)",
                        provided_id
                    ))
                })?;
                provided_id
            }
        } else if params.node_type == "date" {
            params.content.clone()
        } else if params.node_type == "schema" {
            let id = normalize_schema_id(&params.content);
            if id.is_empty() {
                return Err(NodeServiceError::invalid_update(
                    "Schema content must not be empty or contain only special characters"
                        .to_string(),
                ));
            }
            id
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // Step 5: Create the node
        // Save node_type before moving into Node (needed for embedding check)
        let node_type = params.node_type.clone();

        // Determine title for @mention search
        // Schema-driven title_template support
        // Normalize properties to namespaced format so compute_title can find fields correctly.
        // (create_node will normalize again, but the result is idempotent)
        let title = {
            let normalized_props = if params.node_type != "schema" {
                Self::normalize_flat_properties_to_namespace(
                    &params.node_type,
                    &params.properties,
                    None,
                )
            } else {
                params.properties.clone()
            };
            let temp_node = Node {
                id: node_id.clone(),
                node_type: params.node_type.clone(),
                content: params.content.clone(),
                version: 1,
                properties: normalized_props,
                mentions: vec![],
                mentioned_in: vec![],
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                title: None,
                lifecycle_status: "active".to_string(),
            };
            // is_root = parent_id.is_none() — avoids a DB lookup at create time
            self.compute_title(&temp_node, Some(params.parent_id.is_none()))
                .await?
        };

        let node = Node {
            id: node_id,
            node_type: params.node_type,
            content: params.content,
            version: 1,
            properties: params.properties,
            mentions: vec![],
            mentioned_in: vec![],
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            title,
            lifecycle_status: "active".to_string(),
        };

        tracing::debug!(
            "create_node_with_parent: about to call create_node at {}ms",
            start.elapsed().as_millis()
        );
        let created_id = self.create_node(node).await?;
        tracing::debug!(
            node_id = %created_id,
            "create_node_with_parent: create_node completed at {}ms",
            start.elapsed().as_millis()
        );

        // Step 6: Create parent relationship if parent specified
        if let Some(parent_id) = params.parent_id {
            self.create_parent_edge(&created_id, &parent_id, params.position.as_ref())
                .await?;

            // Step 7a: Child node created - queue root for embedding regeneration
            // The new child's content should be included in the root's aggregate embedding
            // (root-aggregate model)
            #[cfg(feature = "nlp")]
            self.queue_root_for_embedding(&created_id).await;
        } else {
            // Step 7b: Root node created - queue for embedding if embeddable type
            // (root-aggregate model)
            // Stale markers are written unconditionally (even without the `nlp` feature) so
            // that a build re-enabled with NLP picks up existing roots without a manual resync.
            if self.is_embeddable_type(&node_type) {
                if let Err(e) = self.store.create_stale_embedding_marker(&created_id).await {
                    // Log warning but don't fail the creation - embedding will be regenerated later
                    tracing::warn!(
                        "Failed to create embedding marker for new root {}: {}",
                        created_id,
                        e
                    );
                } else {
                    // Wake the embedding processor to process the new root
                    tracing::debug!(
                        "Queued new root {} for embedding (direct creation)",
                        created_id
                    );
                    #[cfg(feature = "nlp")]
                    if let Some(waker) = self.embedding_waker.get() {
                        waker.wake();
                    }
                }
            }
        }

        Ok(created_id)
    }

    /// Auto-create date container if it doesn't exist in the database
    ///
    /// Date nodes (YYYY-MM-DD format) are lazily created when children reference them.
    /// This ensures date containers exist before child nodes are created under them.
    ///
    /// # Arguments
    ///
    /// * `node_id` - Potential date node ID to check/create
    ///
    /// # Returns
    ///
    /// `Ok(())` if not a date or date container exists/was created
    pub async fn ensure_date_exists(&self, node_id: &str) -> Result<(), NodeServiceError> {
        // Check if this is a date format (YYYY-MM-DD)
        if !is_date_node_id(node_id) {
            return Ok(()); // Not a date, nothing to do
        }

        // Check if date container already exists IN THE DATABASE
        // IMPORTANT: Call store.get_node() directly to bypass virtual date node logic
        // in get_node(). The virtual date nodes are only for read operations,
        // we need to check actual database state for auto-creation.
        let exists = self
            .store
            .get_node(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(format!("Database error: {}", e)))?
            .is_some();

        if exists {
            return Ok(()); // Already exists in database
        }

        // Auto-create the date container
        let date_node = Node::new_with_id(
            node_id.to_string(),
            "date".to_string(),
            node_id.to_string(), // Default content to date
            serde_json::json!({}),
        );

        self.create_node(date_node).await?;

        Ok(())
    }

    /// Get a node by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The node ID to fetch
    ///
    /// # Returns
    ///
    /// `Some(Node)` if found, `None` if not found
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
    /// if let Some(node) = service.get_node("node-id-123").await? {
    ///     println!("Found: {}", node.content);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_node(&self, id: &str) -> Result<Option<Node>, NodeServiceError> {
        // Delegate to SqliteStore
        if let Some(mut node) = self.store.get_node(id).await.map_err(|e| {
            NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                context: format!("Database operation failed: {}", e),
            })
        })? {
            self.populate_mentions(&mut node).await?;
            self.backfill_schema_version(&mut node).await?;
            self.apply_lazy_migration(&mut node).await?;
            Ok(Some(node))
        } else {
            // NOT in database - check if it's a virtual date node
            // Date nodes (YYYY-MM-DD format) are virtual until they have children
            if is_date_node_id(id) {
                // Return virtual date node (will auto-persist when children are added)
                // Date nodes are root-level containers (no parent/container relationships)
                let virtual_date = Node {
                    id: id.to_string(),
                    node_type: "date".to_string(),
                    content: id.to_string(), // Content MUST match ID for validation
                    version: 1,
                    created_at: chrono::Utc::now(),
                    modified_at: chrono::Utc::now(),
                    properties: serde_json::json!({}),
                    mentions: vec![],
                    mentioned_in: vec![],
                    title: None, // Date nodes don't have indexed titles
                    lifecycle_status: "active".to_string(),
                };
                return Ok(Some(virtual_date));
            }

            Ok(None)
        }
    }

    /// Update a node without version checking (no OCC).
    ///
    /// **Prefer `update_node()`** which enforces optimistic concurrency control.
    /// This unchecked variant is for internal operations (migrations, schema
    /// updates) where version conflicts are not a concern.
    ///
    /// Performs a partial update using the NodeUpdate struct. Only provided fields
    /// will be updated. Handles the double-Option pattern for nullable fields.
    ///
    /// # Arguments
    ///
    /// * `id` - The node ID to update
    /// * `update` - The fields to update
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Node doesn't exist
    /// - Validation fails after update
    /// - Database update fails
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
    /// let update = NodeUpdate::new()
    ///     .with_content("Updated content".to_string());
    /// service.update_node_unchecked("node-id", update).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_node_unchecked(
        &self,
        id: &str,
        update: NodeUpdate,
    ) -> Result<(), NodeServiceError> {
        if update.is_empty() {
            return Err(NodeServiceError::invalid_update(
                "Update contains no changes",
            ));
        }

        // Get existing node to validate update
        let existing = self
            .get_node(id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(id))?;

        // For simplicity with libsql, we'll fetch the node, apply updates, and replace entirely
        let mut updated = existing.clone();
        let mut content_changed = false;
        let mut node_type_changed = false;
        let mut properties_changed = false;

        if let Some(node_type) = update.node_type {
            node_type_changed = updated.node_type != node_type;
            updated.node_type = node_type;
        }

        if let Some(content) = update.content {
            if updated.content != content {
                content_changed = true;
            }
            updated.content = content;
        }

        // NOTE: Sibling ordering is now handled via has_child relationship order field.
        // Use reorder_siblings() or move_node() for ordering changes.

        if let Some(properties) = update.properties {
            properties_changed = true;
            // Normalize flat client properties to namespaced format before merging
            // Skip for schema nodes - they use a special non-namespaced format
            if updated.node_type == "schema" {
                // Schema nodes use flat properties format (relationships, fields, etc.)
                Self::deep_merge_namespaced_properties(&mut updated.properties, properties);
            } else {
                // Client sends: { "status": "done" }
                // We convert to: { "task": { "status": "done" } } before merging with existing namespaced properties
                let normalized_properties = Self::normalize_flat_properties_to_namespace(
                    &updated.node_type,
                    &properties,
                    None, // Schema fields are fetched later if needed
                );
                // Deep-merge namespaced properties
                Self::deep_merge_namespaced_properties(
                    &mut updated.properties,
                    normalized_properties,
                );
            }
        }

        // Step 1: Core behavior validation (PROTECTED)
        self.behaviors.validate_node(&updated)?;

        // Step 1.5: Apply schema defaults and validate (if node type changed)
        // Apply default values for missing fields when node type changes
        // Skip for schema nodes to avoid circular dependency
        if node_type_changed && updated.node_type != "schema" {
            // Fetch schema once and reuse it for both operations
            if let Some(schema_json) = self.get_schema_for_type(&updated.node_type).await? {
                // Parse schema fields
                if let Some(fields_json) = schema_json.get("fields") {
                    if let Ok(fields) = serde_json::from_value::<Vec<crate::models::SchemaField>>(
                        fields_json.clone(),
                    ) {
                        // Apply defaults for the new node type
                        self.apply_schema_defaults_with_fields(&mut updated, &fields)?;

                        // Validate with the same fields
                        self.validate_node_with_fields(&updated, &fields)?;
                    }
                }
            }
        } else if updated.node_type != "schema" {
            // Step 2: Schema validation only (node type didn't change)
            self.validate_node_against_schema(&updated).await?;
        }

        // Sync title when content, node_type, or properties change
        // Schema-driven title_template — also trigger on properties_changed
        let title_update = if content_changed || node_type_changed || properties_changed {
            let new_title = self.compute_title(&updated, None).await?;
            Some(new_title)
        } else {
            None // No title update needed
        };

        // Update node via store
        let node_update = crate::models::NodeUpdate {
            node_type: Some(updated.node_type.clone()),
            content: Some(updated.content.clone()),
            properties: Some(updated.properties.clone()),
            title: title_update,
            lifecycle_status: None, // Schema update doesn't change lifecycle_status
        };

        // Schema nodes go through the normal update path
        self.store
            .update_node(id, node_update, self.client_id.clone())
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // NOTE: NodeUpdated event is now automatically emitted by store notifier

        // Sync mentions if content changed
        if content_changed {
            if let Err(e) = self
                .sync_mentions(id, &existing.content, &updated.content)
                .await
            {
                // Log warning but don't fail the update - mention sync failures should not block content updates
                tracing::warn!("Failed to sync mentions for node {}: {}", id, e);
            }
        }

        Ok(())
    }

    /// Update node with optimistic concurrency control (version check)
    ///
    /// Internal method that returns the updated node directly to avoid redundant fetches.
    pub(crate) async fn update_with_version_check_returning_node(
        &self,
        id: &str,
        expected_version: i64,
        update: NodeUpdate,
    ) -> Result<Option<Node>, NodeServiceError> {
        if update.is_empty() {
            return Err(NodeServiceError::invalid_update(
                "Update contains no changes",
            ));
        }

        // Get existing node to validate update and build new state
        let existing = self
            .get_node(id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(id))?;

        // Build updated node state
        let mut updated = existing.clone();
        let mut content_changed = false;
        let mut node_type_changed = false;
        let mut properties_changed = false;

        if let Some(node_type) = update.node_type {
            node_type_changed = updated.node_type != node_type;
            updated.node_type = node_type;
        }

        if let Some(content) = update.content {
            if updated.content != content {
                content_changed = true;
            }
            updated.content = content;
        }

        // NOTE: Sibling ordering is now handled via has_child relationship order field.
        // Use reorder_siblings() or move_node() for ordering changes.

        if let Some(properties) = update.properties {
            properties_changed = true;
            // Normalize flat client properties to namespaced format before merging
            // Skip for schema nodes - they use a special non-namespaced format
            if updated.node_type == "schema" {
                // Schema nodes use flat properties format (relationships, fields, etc.)
                Self::deep_merge_namespaced_properties(&mut updated.properties, properties);
            } else {
                let normalized_properties = Self::normalize_flat_properties_to_namespace(
                    &updated.node_type,
                    &properties,
                    None,
                );
                // Deep-merge namespaced properties
                Self::deep_merge_namespaced_properties(
                    &mut updated.properties,
                    normalized_properties,
                );
            }
        }

        // Step 1: Core behavior validation (PROTECTED)
        self.behaviors.validate_node(&updated)?;

        // Step 2: Schema validation (USER-EXTENSIBLE)
        // Only validate against schema for node types that have meaningful schema fields.
        // Currently only "task" has schema-defined fields; text, date, etc. have no fields.
        // This avoids a ~760ms database lookup for every update.
        if updated.node_type == "task" {
            self.validate_node_against_schema(&updated).await?;
        }

        // Synchronous playbook validation gate — reject invalid rule changes before persist
        if updated.node_type == "playbook" && properties_changed {
            self.validate_playbook_rules(&updated.properties).await?;
        }

        // Sync title when content, node_type, or properties change
        // Schema-driven title_template — also trigger on properties_changed
        let title_update = if content_changed || node_type_changed || properties_changed {
            let new_title = self.compute_title(&updated, None).await?;
            Some(new_title)
        } else {
            None
        };

        // Create node update
        // Pass through lifecycle_status if provided
        let node_update = crate::models::NodeUpdate {
            node_type: Some(updated.node_type.clone()),
            content: Some(updated.content.clone()),
            properties: Some(updated.properties.clone()),
            title: title_update,
            lifecycle_status: update.lifecycle_status,
        };

        // Perform atomic update with version check
        let result = self
            .store
            .update_node_with_version_check(
                id,
                expected_version,
                node_update,
                self.client_id.clone(),
                self.execution_context.clone(),
            )
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Check if update succeeded (version matched)
        // If None, version mismatch occurred - return None for caller to handle
        let updated_node = match result {
            Some(node) => node,
            None => return Ok(None),
        };

        // NOTE: NodeUpdated event is now automatically emitted by store notifier

        // Queue root for embedding regeneration if content changed (root-aggregate model)
        // Fire-and-forget: don't block the update response on embedding queue operations
        #[cfg(feature = "nlp")]
        if content_changed {
            let store = self.store.clone();
            let behaviors = self.behaviors.clone();
            let node_id = id.to_string();
            let embedding_waker = self.embedding_waker.clone();
            tokio::spawn(async move {
                Self::queue_root_for_embedding_async(
                    &store,
                    &behaviors,
                    &node_id,
                    embedding_waker.get(),
                )
                .await;
            });
        }

        // Sync mentions if content changed
        if content_changed {
            if let Err(e) = self
                .sync_mentions(id, &existing.content, &updated.content)
                .await
            {
                // Log warning but don't fail the update
                tracing::warn!("Failed to sync mentions for node {}: {}", id, e);
            }
        }

        Ok(Some(updated_node))
    }

    /// Update a node with OCC and return the updated node
    ///
    /// This is the primary update API that:
    /// 1. Validates update has changes
    /// 2. Applies update with version check
    /// 3. Returns detailed error on version conflict
    /// 4. Returns the updated node on success
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node ID to update
    /// * `expected_version` - Version for optimistic concurrency control
    /// * `update` - Fields to update
    ///
    /// # Returns
    ///
    /// The updated Node with new version number
    ///
    /// # Errors
    ///
    /// Returns error on:
    /// - Empty update (no changes)
    /// - Node not found
    /// - Version conflict (with expected/actual versions)
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
    /// let update = NodeUpdate::new().with_content("Updated content".to_string());
    /// let updated = service.update_node("node-id", 5, update).await?;
    /// println!("New version: {}", updated.version);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_node(
        &self,
        node_id: &str,
        expected_version: i64,
        update: NodeUpdate,
    ) -> Result<Node, NodeServiceError> {
        // Validate update has changes
        if update.is_empty() {
            return Err(NodeServiceError::invalid_update(
                "Update contains no changes",
            ));
        }

        // A starter-tier seeded node becomes user-owned the moment its content
        // or properties are edited through the normal update path — reseed's
        // replace path goes through delete_node + create_node_with_parent, not
        // here, so it never trips this. Checked before the update so a
        // version-conflict below doesn't leave a partial flag write behind.
        let touches_content = update.content.is_some() || update.properties.is_some();
        let mark_user_modified = if touches_content {
            match self.store.get_node(node_id).await {
                Ok(Some(existing)) => {
                    let seed = existing.properties.get("_seed");
                    let is_starter = seed.and_then(|s| s.get("tier")).and_then(|v| v.as_str())
                        == Some("starter");
                    let already_modified = seed
                        .and_then(|s| s.get("user_modified"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    is_starter && !already_modified
                }
                _ => false,
            }
        } else {
            false
        };

        // NOTE: Removed redundant get_node() call here - update_with_version_check_returning_node
        // already fetches the node and handles not-found case

        // Apply update with version check - returns the updated node directly
        match self
            .update_with_version_check_returning_node(node_id, expected_version, update)
            .await?
        {
            Some(updated_node) => {
                if mark_user_modified {
                    if let Err(e) = self
                        .store
                        .set_property_bool(node_id, "$._seed.user_modified", true)
                        .await
                    {
                        tracing::warn!(
                            node_id,
                            error = %e,
                            "Failed to stamp seed_user_modified after edit"
                        );
                    }
                }
                Ok(updated_node)
            }
            None => {
                // The version-gated UPDATE matched no row for one of two
                // reasons — the node was concurrently DELETED, or its version
                // moved. Disambiguate against the REAL persisted row: `get_node`
                // virtualizes a date page, so it would report a phantom version 1
                // for a deleted date node → a false `version_conflict{actual:1}`
                // instead of `NotFound` (and an absent regular node → `actual:0`).
                match self
                    .store
                    .persisted_version(node_id)
                    .await
                    .map_err(|e| NodeServiceError::query_failed(e.to_string()))?
                {
                    None => Err(NodeServiceError::node_not_found(node_id)),
                    Some(actual) => Err(NodeServiceError::version_conflict(
                        node_id,
                        expected_version,
                        actual,
                    )),
                }
            }
        }
    }

    /// Sync mention relationships when node content changes
    pub(crate) async fn sync_mentions(
        &self,
        node_id: &str,
        old_content: &str,
        new_content: &str,
    ) -> Result<(), NodeServiceError> {
        let old_mentions: HashSet<String> = extract_mentions(old_content).into_iter().collect();
        let new_mentions: HashSet<String> = extract_mentions(new_content).into_iter().collect();

        // Calculate diff
        let to_add: Vec<&String> = new_mentions.difference(&old_mentions).collect();
        let to_remove: Vec<&String> = old_mentions.difference(&new_mentions).collect();

        // Get parent ID once for all mention checks (optimized: use get_parent_id instead of get_parent)
        let parent_id = self
            .store
            .get_parent_id(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Add new mentions (filter out self-references and root-level self-references)
        for mentioned_id in to_add {
            // Skip direct self-references
            if mentioned_id.as_str() == node_id {
                tracing::debug!("Skipping self-reference: {} -> {}", node_id, mentioned_id);
                continue;
            }

            // Skip root-level self-references (child mentioning its own parent)
            if let Some(ref pid) = parent_id {
                if mentioned_id.as_str() == pid.as_str() {
                    tracing::debug!(
                        "Skipping root-level self-reference: {} -> {} (parent: {})",
                        node_id,
                        mentioned_id,
                        pid
                    );
                    continue;
                }
            }

            // Auto-create date nodes when mentioned.
            // Date nodes are lazily created, but we need them to exist for the
            // "Mentioned by" panel to work. This ensures the relationship can be created.
            if is_date_node_id(mentioned_id) {
                if let Err(e) = self.ensure_date_exists(mentioned_id).await {
                    tracing::warn!(
                        "Failed to ensure date node exists for mention: {} -> {}: {}",
                        node_id,
                        mentioned_id,
                        e
                    );
                    // Continue anyway - the mention creation will fail if node doesn't exist
                }
            }

            if let Err(e) = self.create_mention(node_id, mentioned_id).await {
                tracing::warn!(
                    "Failed to create mention: {} -> {}: {}",
                    node_id,
                    mentioned_id,
                    e
                );
            }
        }

        // Remove old mentions
        for mentioned_id in to_remove {
            // Skip direct self-references (shouldn't exist, but be safe)
            if mentioned_id.as_str() == node_id {
                continue;
            }

            if let Err(e) = self.delete_mention(node_id, mentioned_id).await {
                tracing::warn!(
                    "Failed to delete mention: {} -> {}: {}",
                    node_id,
                    mentioned_id,
                    e
                );
            }
        }

        Ok(())
    }

    /// Delete a node without version checking (no OCC).
    ///
    /// **Prefer `delete_node()`** which enforces optimistic concurrency control.
    /// This unchecked variant is for internal operations (diagnostics cleanup)
    /// where version conflicts are not a concern.
    ///
    /// Deletes a node and all its children (cascade delete).
    ///
    /// # Arguments
    ///
    /// * `id` - The node ID to delete
    ///
    /// # Errors
    ///
    /// Returns error if node doesn't exist or database deletion fails
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
    /// service.delete_node_unchecked("node-id-123").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_node_unchecked(
        &self,
        id: &str,
    ) -> Result<crate::models::DeleteResult, NodeServiceError> {
        // Delegate to SqliteStore
        let result = self
            .store
            .delete_node(id, self.client_id.clone())
            .await
            .map_err(|e| {
                NodeServiceError::DatabaseError(crate::db::DatabaseError::SqlExecutionError {
                    context: format!("Database operation failed: {}", e),
                })
            })?;

        // NOTE: NodeDeleted event is now automatically emitted by store notifier

        // Idempotent delete: return success even if node doesn't exist
        Ok(result)
    }

    /// Delete node with optimistic concurrency control (version check)
    ///
    /// This method performs an atomic delete with version checking to prevent
    /// race conditions when multiple clients attempt to delete or modify the same node.
    ///
    /// # Arguments
    ///
    /// * `id` - Node ID to delete
    /// * `expected_version` - Version the client expects (from their last read)
    ///
    /// # Returns
    ///
    /// * `Ok(rows_affected)` - Number of rows deleted (0 = version mismatch or not found, 1 = success)
    /// * `Err(NodeServiceError)` - Database errors
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
    /// let rows = service.delete_with_version_check("node-123", 5).await?;
    ///
    /// if rows == 0 {
    ///     // Either version conflict or node doesn't exist
    ///     // Caller should check if node still exists to distinguish
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_with_version_check(
        &self,
        id: &str,
        expected_version: i64,
    ) -> Result<usize, NodeServiceError> {
        let rows_affected = self
            .store
            .delete_with_version_check(id, expected_version, self.client_id.clone())
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!(
                    "Failed to delete node with version check: {}",
                    e
                ))
            })?;

        // NOTE: NodeDeleted event is now automatically emitted by store notifier

        Ok(rows_affected)
    }

    /// Delete a node and its entire `has_child` subtree atomically with OCC.
    ///
    /// The target node's version is checked at `expected_version`. Descendants are removed
    /// unconditionally inside the same transaction — a conflict or failure leaves the subtree
    /// fully intact. One `NodeDeleted` event is emitted per deleted node after commit.
    ///
    /// Returns `DeleteResult` with `existed=true` and `deleted_count` (target + all descendants)
    /// on success, or `existed=false` when the target node was already gone.
    pub async fn delete_node(
        &self,
        node_id: &str,
        expected_version: i64,
    ) -> Result<crate::models::DeleteResult, NodeServiceError> {
        // Capture root before deletion for embedding queue.
        let root_id_for_embedding = self.get_root_id(node_id).await.ok();

        let (existed, deleted_nodes) = self
            .store
            .delete_subtree_atomic(node_id, expected_version, self.client_id.clone())
            .await
            .map_err(|e| {
                let msg = e.to_string();
                // Parse version_conflict sentinel: "version_conflict:<id>:<expected>:<actual>"
                if let Some(rest) = msg.strip_prefix("version_conflict:") {
                    let parts: Vec<&str> = rest.splitn(3, ':').collect();
                    if parts.len() == 3 {
                        let exp: i64 = parts[1].parse().unwrap_or(expected_version);
                        let act: i64 = parts[2].parse().unwrap_or(0);
                        return NodeServiceError::version_conflict(node_id, exp, act);
                    }
                }
                NodeServiceError::query_failed(msg)
            })?;

        if !existed {
            return Ok(crate::models::DeleteResult {
                existed: false,
                deleted_count: 0,
            });
        }

        // Queue root for embedding regeneration when a non-root node was deleted.
        #[cfg(feature = "nlp")]
        if let Some(root_id) = root_id_for_embedding {
            if root_id != node_id {
                self.queue_root_for_embedding(&root_id).await;
            }
        }

        Ok(crate::models::DeleteResult {
            existed: true,
            deleted_count: deleted_nodes.len() as u64,
        })
    }

    /// Bump a node's version without changing any content.
    ///
    /// Used by operations like reorder that need OCC (optimistic concurrency control)
    /// even though they don't modify the node's content directly.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The ID of the node to update
    /// * `expected_version` - The version the caller expects (for OCC)
    ///
    /// # Returns
    ///
    /// Ok(Node) with updated version if bump succeeds, Err if version mismatch or node not found
    pub async fn update_node_with_version_bump(
        &self,
        node_id: &str,
        expected_version: i64,
    ) -> Result<Node, NodeServiceError> {
        // Get current node to preserve its values
        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Create update with current values (no actual changes, just version bump)
        let node_update = crate::models::NodeUpdate {
            node_type: Some(node.node_type.clone()),
            content: Some(node.content.clone()),
            properties: Some(node.properties.clone()),
            title: None,            // Don't update title on version bump
            lifecycle_status: None, // Don't update lifecycle_status on version bump
        };

        // Perform atomic update with version check
        let result = self
            .store
            .update_node_with_version_check(
                node_id,
                expected_version,
                node_update,
                self.client_id.clone(),
                self.execution_context.clone(),
            )
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Check if update succeeded (version matched)
        let updated_node = result.ok_or_else(|| {
            NodeServiceError::query_failed(format!(
                "Version conflict: expected version {} for node {}",
                expected_version, node_id
            ))
        })?;

        // NOTE: NodeUpdated event is now automatically emitted by store notifier

        Ok(updated_node)
    }

    /// Upsert a node with automatic parent creation - single transaction
    ///
    /// Creates parent node if it doesn't exist, then upserts the child node.
    /// All operations happen in a single transaction to prevent database locking.
    ///
    /// # Arguments
    /// * `node_id` - ID of the node to upsert
    /// * `content` - Node content
    /// * `node_type` - Type of node (text, task, date)
    /// * `parent_id` - Parent node ID (will be created as date node if missing)
    ///
    /// # Returns
    /// * `Ok(())` - Operation successful
    /// * `Err(NodeServiceError)` - If transaction fails
    pub async fn upsert_node_with_parent(
        &self,
        node_id: &str,
        content: &str,
        node_type: &str,
        parent_id: &str,
        _root_id: &str, // Deprecated: hierarchy now managed via relationships
        before_sibling_id: Option<&str>,
    ) -> Result<(), NodeServiceError> {
        // Ensure parent exists (create if missing)
        if self
            .store
            .get_node(parent_id)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to check parent existence: {}", e))
            })?
            .is_none()
        {
            // Create parent as date node
            let parent_node = Node::new(
                "date".to_string(),
                parent_id.to_string(),
                serde_json::json!({}),
            );
            self.store
                .create_node(
                    parent_node,
                    self.client_id.clone(),
                    self.execution_context.clone(),
                )
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to create parent node: {}", e))
                })?;

            // NOTE: NodeCreated event is now automatically emitted by store notifier
        }

        // Enforce container rule: the (now-existent) parent must accept children
        {
            let parent_node = self
                .store
                .get_node(parent_id)
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to fetch parent: {}", e))
                })?
                .ok_or_else(|| NodeServiceError::invalid_parent(parent_id))?;
            if !self
                .behavior_for(&parent_node.node_type)
                .can_have_children()
            {
                return Err(NodeServiceError::not_a_container(
                    parent_id,
                    &parent_node.node_type,
                ));
            }
        }

        // Upsert the node (update if exists, create if not)
        if let Some(existing) = self.store.get_node(node_id).await.map_err(|e| {
            NodeServiceError::query_failed(format!("Failed to check node existence: {}", e))
        })? {
            // Update existing node
            let update = NodeUpdate {
                content: Some(content.to_string()),
                // NOTE: Sibling ordering now handled via has_child relationship order field
                ..Default::default()
            };
            self.store
                .update_node(&existing.id, update, self.client_id.clone())
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to update node: {}", e))
                })?;

            // NOTE: NodeUpdated event is now automatically emitted by store notifier

            // Update parent relationship via edge (handles sibling ordering)
            let actual_order = self
                .store
                .move_node(node_id, Some(parent_id), before_sibling_id)
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to update parent: {}", e))
                })?;

            // Emit RelationshipUpdated event (unified relationship events)
            self.emit_event(DomainEvent::RelationshipUpdated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", parent_id, node_id),
                    parent_id,
                    node_id,
                    "has_child",
                    serde_json::json!({"order": actual_order}),
                ),
            });
        } else {
            // Create new node
            let node = Node {
                id: node_id.to_string(),
                node_type: node_type.to_string(),
                content: content.to_string(),
                version: 1,
                properties: serde_json::json!({}),
                mentions: vec![],
                mentioned_in: vec![],
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                title: None, // Title managed by NodeService for root/task nodes
                lifecycle_status: "active".to_string(),
            };
            self.store
                .create_node(node, self.client_id.clone(), self.execution_context.clone())
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to create node: {}", e))
                })?;

            // NOTE: NodeCreated event is now automatically emitted by store notifier

            // Create parent relationship via edge (handles sibling ordering)
            let actual_order = self
                .store
                .move_node(node_id, Some(parent_id), before_sibling_id)
                .await
                .map_err(|e| {
                    NodeServiceError::query_failed(format!("Failed to set parent: {}", e))
                })?;

            // Emit RelationshipCreated event (unified relationship events)
            self.emit_event(DomainEvent::RelationshipCreated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", parent_id, node_id),
                    parent_id,
                    node_id,
                    "has_child",
                    serde_json::json!({"order": actual_order}),
                ),
            });
        }

        Ok(())
    }

    // =========================================================================
    // Private CRUD helpers
    // =========================================================================

    /// Validate a node's properties against its schema definition
    pub(crate) async fn validate_node_against_schema(
        &self,
        node: &Node,
    ) -> Result<(), NodeServiceError> {
        // Try to get schema for this node type
        // If no schema exists, validation passes (not all types have schemas)
        let schema_json = match self.get_schema_for_type(&node.node_type).await? {
            Some(s) => s,
            None => return Ok(()), // No schema = no validation needed
        };

        // Parse schema fields from properties
        // If parsing fails (e.g., old schema format), skip schema validation gracefully
        let fields: Vec<crate::models::SchemaField> = match schema_json.get("fields") {
            Some(fields_json) => match serde_json::from_value(fields_json.clone()) {
                Ok(f) => f,
                Err(_) => return Ok(()), // Can't parse fields - skip validation
            },
            None => return Ok(()), // No fields defined - skip validation
        };

        // Use the helper function to validate with the parsed fields
        self.validate_node_with_fields(node, &fields)
    }

    /// Validate playbook rules before persisting.
    pub(crate) async fn validate_playbook_rules(
        &self,
        properties: &serde_json::Value,
    ) -> Result<(), NodeServiceError> {
        use crate::playbook::types::{parse_rule, parse_rules_from_properties};

        // Step 1: Parse rules from properties
        let rule_defs = match parse_rules_from_properties(properties) {
            Ok(defs) => defs,
            Err(e) => {
                return Err(NodeServiceError::PlaybookValidationFailed {
                    errors: format!("Failed to parse playbook rules: {}", e),
                });
            }
        };

        // Step 2: Parse each rule definition into a ParsedRule
        let mut parsed_rules = Vec::with_capacity(rule_defs.len());
        for def in &rule_defs {
            match parse_rule(def) {
                Ok(rule) => parsed_rules.push(std::sync::Arc::new(rule)),
                Err(e) => {
                    return Err(NodeServiceError::PlaybookValidationFailed {
                        errors: format!("Failed to parse rule '{}': {}", def.name, e),
                    });
                }
            }
        }

        // Step 3: Run the full validation pipeline (schema checks, CEL compile, paths)
        if let Err(errors) =
            crate::playbook::validation::validate_playbook(&parsed_rules, self).await
        {
            return Err(NodeServiceError::playbook_validation_failed(&errors));
        }

        Ok(())
    }

    /// Apply schema default values to missing fields using pre-loaded fields
    pub(crate) fn apply_schema_defaults_with_fields(
        &self,
        node: &mut Node,
        fields: &[crate::models::SchemaField],
    ) -> Result<(), NodeServiceError> {
        // Ensure properties is an object
        if !node.properties.is_object() {
            node.properties = serde_json::json!({});
        }

        // Get mutable reference to properties object
        let props_obj = node.properties.as_object_mut().unwrap();

        // Get or create the type namespace
        // Properties are stored under properties[node_type][field_name]
        let type_namespace = props_obj
            .entry(&node.node_type)
            .or_insert_with(|| serde_json::json!({}));

        let type_props = type_namespace.as_object_mut().ok_or_else(|| {
            NodeServiceError::invalid_update(format!(
                "Type namespace for '{}' is not an object",
                node.node_type
            ))
        })?;

        // Apply defaults for missing fields within the type namespace
        for field in fields {
            // Check if field is missing in the type namespace
            if !type_props.contains_key(&field.name) {
                // Apply default value if one is defined
                if let Some(default_value) = &field.default {
                    type_props.insert(field.name.clone(), default_value.clone());
                }
            }
        }

        Ok(())
    }

    /// Deep-merge namespaced properties
    pub(crate) fn deep_merge_namespaced_properties(
        existing: &mut serde_json::Value,
        new: serde_json::Value,
    ) {
        if let (Some(existing_obj), Some(new_obj)) = (existing.as_object_mut(), new.as_object()) {
            for (key, value) in new_obj {
                // If both existing and new have the same key as objects, deep merge
                if let (Some(existing_ns), Some(new_ns)) = (
                    existing_obj.get_mut(key).and_then(|v| v.as_object_mut()),
                    value.as_object(),
                ) {
                    // Deep merge: update fields within the namespace
                    for (field_key, field_value) in new_ns {
                        existing_ns.insert(field_key.clone(), field_value.clone());
                    }
                } else {
                    // Otherwise replace the key (for new namespaces or non-object values)
                    existing_obj.insert(key.clone(), value.clone());
                }
            }
        } else {
            // If either is not an object, just replace (shouldn't happen normally)
            *existing = new;
        }
    }

    /// Normalize flat properties input into namespaced storage format
    pub(crate) fn normalize_flat_properties_to_namespace(
        node_type: &str,
        properties: &serde_json::Value,
        schema_fields: Option<&[crate::models::SchemaField]>,
    ) -> serde_json::Value {
        let Some(props_obj) = properties.as_object() else {
            return properties.clone();
        };

        // Build a set of known schema field names for the current type
        let schema_field_names: std::collections::HashSet<&str> = schema_fields
            .map(|fields| fields.iter().map(|f| f.name.as_str()).collect())
            .unwrap_or_default();

        // Check if properties are already namespaced by looking for the node_type key
        // with an object value containing schema fields
        if let Some(type_namespace) = props_obj.get(node_type) {
            if type_namespace.is_object() {
                // Already namespaced - return as-is (preserves dormant namespaces too)
                return properties.clone();
            }
        }

        // Separate flat properties (to be namespaced) from already-namespaced ones
        let mut namespaced = serde_json::Map::new();
        let mut flat_props = serde_json::Map::new();

        for (key, value) in props_obj {
            // Check if this key looks like a namespace (an object with nested properties)
            if value.is_object() && !schema_field_names.contains(key.as_str()) {
                // This is likely a namespace (dormant or active) - preserve it
                namespaced.insert(key.clone(), value.clone());
            } else {
                // This is a flat property - collect for namespacing
                flat_props.insert(key.clone(), value.clone());
            }
        }

        // Move flat properties into the current type's namespace
        if !flat_props.is_empty() {
            let type_ns = namespaced
                .entry(node_type.to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(type_obj) = type_ns.as_object_mut() {
                for (key, value) in flat_props {
                    type_obj.insert(key, value);
                }
            }
        } else if !namespaced.contains_key(node_type) {
            // Ensure the current type namespace exists even if empty
            namespaced.insert(node_type.to_string(), serde_json::json!({}));
        }

        serde_json::Value::Object(namespaced)
    }

    /// Validate a node against pre-loaded schema fields
    pub(crate) fn validate_node_with_fields(
        &self,
        node: &Node,
        fields: &[crate::models::SchemaField],
    ) -> Result<(), NodeServiceError> {
        // Get properties for this node type from the type namespace
        // Properties are stored under properties[node_type][field_name]
        let node_props = node
            .properties
            .get(&node.node_type)
            .and_then(|p| p.as_object());

        // Validate each field in the schema
        for field in fields {
            let field_value = node_props.and_then(|props| props.get(&field.name));

            // Check required fields
            // Allow missing required fields if they have a default value defined
            if field.required.unwrap_or(false) && field_value.is_none() && field.default.is_none() {
                return Err(NodeServiceError::invalid_update(format!(
                    "Required field '{}' is missing from {} node",
                    field.name, node.node_type
                )));
            }

            // Validate enum fields
            if field.field_type == "enum" {
                if let Some(value) = field_value {
                    if let Some(value_str) = value.as_str() {
                        // Get all valid enum values (core + user)
                        let mut valid_values = Vec::new();
                        if let Some(core_vals) = &field.core_values {
                            valid_values.extend(core_vals.clone());
                        }
                        if let Some(user_vals) = &field.user_values {
                            valid_values.extend(user_vals.clone());
                        }

                        // Check if the value matches any EnumValue.value
                        let is_valid = valid_values.iter().any(|ev| ev.value == value_str);
                        if !is_valid {
                            let valid_labels: Vec<_> = valid_values
                                .iter()
                                .map(|ev| format!("{} ({})", ev.label, ev.value))
                                .collect();
                            return Err(NodeServiceError::invalid_update(format!(
                                "Invalid value '{}' for enum field '{}'. Valid values: {}",
                                value_str,
                                field.name,
                                valid_labels.join(", ")
                            )));
                        }
                    } else if !value.is_null() {
                        return Err(NodeServiceError::invalid_update(format!(
                            "Enum field '{}' must be a string or null",
                            field.name
                        )));
                    }
                }
            }

            // Future: Add more type validation (number ranges, string formats, etc.)
        }

        Ok(())
    }

    /// Backfill _schema_version for a node if it doesn't have one (Phase 1 lazy migration)
    pub(crate) async fn backfill_schema_version(
        &self,
        node: &mut Node,
    ) -> Result<(), NodeServiceError> {
        // Only backfill for types that have schema fields
        let schema = match self.get_schema_for_type(&node.node_type).await? {
            Some(s) => s,
            None => return Ok(()), // No schema = no version needed
        };

        // Check if schema has any fields
        let has_fields = schema
            .get("fields")
            .and_then(|f| f.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);

        if !has_fields {
            return Ok(()); // Empty schema = no version needed
        }

        // Check if _schema_version exists in the type namespace
        let has_version = node
            .properties
            .get(&node.node_type)
            .and_then(|ns| ns.get("_schema_version"))
            .is_some();

        if !has_version {
            let version = schema.get("version").and_then(|v| v.as_i64()).unwrap_or(1);

            // Add version to type namespace IN-MEMORY ONLY
            if let Some(props_obj) = node.properties.as_object_mut() {
                let type_namespace = props_obj
                    .entry(&node.node_type)
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(type_props) = type_namespace.as_object_mut() {
                    type_props.insert("_schema_version".to_string(), serde_json::json!(version));
                }
            }
        }
        Ok(())
    }

    /// Backfill schema version using pre-fetched schema cache (no database calls).
    /// Used by query_nodes for batch operations.
    pub(crate) fn backfill_schema_version_with_cache(
        &self,
        node: &mut Node,
        schema_cache: &std::collections::HashMap<String, Option<serde_json::Value>>,
    ) {
        // Get schema from cache
        let schema = match schema_cache.get(&node.node_type) {
            Some(Some(s)) => s,
            _ => return, // No schema = no version needed
        };

        // Check if schema has any fields
        let has_fields = schema
            .get("fields")
            .and_then(|f| f.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);

        if !has_fields {
            return; // Empty schema = no version needed
        }

        // Check if _schema_version exists in the type namespace
        let has_version = node
            .properties
            .get(&node.node_type)
            .and_then(|ns| ns.get("_schema_version"))
            .is_some();

        if !has_version {
            let version = schema.get("version").and_then(|v| v.as_i64()).unwrap_or(1);

            // Add version to type namespace IN-MEMORY ONLY
            if let Some(props_obj) = node.properties.as_object_mut() {
                let type_namespace = props_obj
                    .entry(&node.node_type)
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(type_props) = type_namespace.as_object_mut() {
                    type_props.insert("_schema_version".to_string(), serde_json::json!(version));
                }
            }
        }
    }

    /// Apply lazy migration to upgrade node to latest schema version
    pub(crate) async fn apply_lazy_migration(
        &self,
        node: &mut Node,
    ) -> Result<(), NodeServiceError> {
        // Get current version from type namespace
        let current_version = node
            .properties
            .get(&node.node_type)
            .and_then(|ns| ns.get("_schema_version"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        // Get target version from schema
        let target_version = if let Some(schema) = self.get_schema_for_type(&node.node_type).await?
        {
            schema.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as u32
        } else {
            1 // No schema found - no migration needed
        };

        // Check if migration is needed
        if current_version >= target_version {
            return Ok(()); // Already up-to-date
        }

        // Apply migrations
        let migrated_node = self
            .migration_registry
            .apply_migrations(node, target_version)?;

        // Persist migrated node to database using SqliteStore
        let update = NodeUpdate {
            properties: Some(migrated_node.properties.clone()),
            ..Default::default()
        };
        self.store
            .update_node(&node.id, update, self.client_id.clone())
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to persist migrated node: {}", e))
            })?;

        // Update the in-memory node
        *node = migrated_node;

        Ok(())
    }

    /// Apply lazy migration using pre-fetched schema cache.
    /// Used by query_nodes for batch operations.
    pub(crate) async fn apply_lazy_migration_with_cache(
        &self,
        node: &mut Node,
        schema_cache: &std::collections::HashMap<String, Option<serde_json::Value>>,
    ) -> Result<(), NodeServiceError> {
        // Get current version from type namespace
        let current_version = node
            .properties
            .get(&node.node_type)
            .and_then(|ns| ns.get("_schema_version"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        // Get target version from cached schema
        let target_version = match schema_cache.get(&node.node_type) {
            Some(Some(schema)) => {
                schema.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as u32
            }
            _ => 1, // No schema found - no migration needed
        };

        // Check if migration is needed
        if current_version >= target_version {
            return Ok(()); // Already up-to-date
        }

        // Apply migrations
        let migrated_node = self
            .migration_registry
            .apply_migrations(node, target_version)?;

        // Persist migrated node to database using SqliteStore
        let update = NodeUpdate {
            properties: Some(migrated_node.properties.clone()),
            ..Default::default()
        };
        self.store
            .update_node(&node.id, update, self.client_id.clone())
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to persist migrated node: {}", e))
            })?;

        // Update the in-memory node
        *node = migrated_node;

        Ok(())
    }

    /// Compute the indexed title for a node.
    pub(crate) async fn compute_title(
        &self,
        node: &Node,
        is_root: Option<bool>,
    ) -> Result<Option<String>, NodeServiceError> {
        // date/schema nodes never get titles regardless of template
        if node.node_type == "date" || node.node_type == "schema" {
            return Ok(None);
        }

        // Check for title_template in the schema for this node type
        match self.get_schema_node(&node.node_type).await {
            Ok(Some(schema)) => {
                if let Some(template) = &schema.title_template {
                    // Properties are stored namespaced: { "node_type": { "field": value } }
                    // Unwrap to the inner namespace object for template interpolation
                    let flat_props = node
                        .properties
                        .get(&node.node_type)
                        .unwrap_or(&node.properties);
                    return Ok(Some(crate::utils::interpolate_title_template_with_schema(
                        template,
                        flat_props,
                        &schema.fields,
                    )));
                }
            }
            Ok(None) => {} // No schema for this type — fall through to content-based logic
            Err(e) => {
                // Schema lookup failed; fall through to content-based title rather than
                // blocking the create/update operation
                tracing::warn!(
                    node_type = %node.node_type,
                    error = %e,
                    "compute_title: schema lookup failed, falling back to content-based title"
                );
            }
        }

        // Fall back to content-based title
        let title = match node.node_type.as_str() {
            "task" | "collection" => Some(crate::utils::strip_markdown(&node.content)),
            _ => {
                let root = match is_root {
                    Some(v) => v,
                    None => self
                        .store
                        .get_parent_id(&node.id)
                        .await
                        .map_err(|e| NodeServiceError::query_failed(e.to_string()))?
                        .is_none(),
                };
                if root {
                    Some(crate::utils::strip_markdown(&node.content))
                } else {
                    None
                }
            }
        };
        Ok(title)
    }

    /// Check if a node exists
    pub(crate) async fn node_exists(&self, id: &str) -> Result<bool, NodeServiceError> {
        let node = self.store.get_node(id).await.map_err(|e| {
            NodeServiceError::query_failed(format!("Failed to check node existence: {}", e))
        })?;
        Ok(node.is_some())
    }
}
