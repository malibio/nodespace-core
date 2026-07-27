//! Relationship and mention operations for NodeService.

use super::*;

impl NodeService {
    /// Create a mention relationship between two existing nodes
    ///
    /// Adds an entry to the relationship table (relationship_type = 'mentions') to track that one node mentions another.
    /// This enables backlink/references functionality.
    ///
    /// # Arguments
    ///
    /// * `mentioning_node_id` - ID of the node that contains the mention
    /// * `mentioned_node_id` - ID of the node being mentioned
    ///
    /// # Returns
    ///
    /// `Ok(())` if successful
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Either node doesn't exist
    /// - Database insertion fails
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
    /// // Create mention: "daily-note" mentions "project-planning"
    /// service.create_mention("daily-note-id", "project-planning-id").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_mention(
        &self,
        mentioning_node_id: &str,
        mentioned_node_id: &str,
    ) -> Result<(), NodeServiceError> {
        // Prevent direct self-references
        if mentioning_node_id == mentioned_node_id {
            return Err(NodeServiceError::ValidationFailed(
                crate::models::ValidationError::InvalidParent(
                    "Cannot create self-referencing mention".to_string(),
                ),
            ));
        }

        // Validate both nodes exist
        if !self.node_exists(mentioning_node_id).await? {
            return Err(NodeServiceError::node_not_found(mentioning_node_id));
        }
        if !self.node_exists(mentioned_node_id).await? {
            return Err(NodeServiceError::node_not_found(mentioned_node_id));
        }

        // Prevent root-level self-references (child mentioning its own root)
        // Get root ID via edge traversal for validation only
        let root_id = self.get_root_id(mentioning_node_id).await?;

        if root_id == mentioned_node_id {
            return Err(NodeServiceError::ValidationFailed(
                crate::models::ValidationError::InvalidParent(
                    "Cannot mention own root (root-level self-reference)".to_string(),
                ),
            ));
        }

        // Store returns relationship ID, service emits event
        // root_id no longer stored - computed dynamically via graph traversal
        let relationship_id = self
            .store
            .create_mention(mentioning_node_id, mentioned_node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit event if relationship was created (not already existing)
        if let Some(rel_id) = relationship_id {
            self.emit_event(DomainEvent::RelationshipCreated {
                relationship: crate::db::events::RelationshipEvent::new(
                    rel_id,
                    mentioning_node_id,
                    mentioned_node_id,
                    "mentions",
                    serde_json::json!({}),
                ),
            });
        }

        Ok(())
    }

    /// Delete a mention relationship between two nodes
    ///
    /// Removes an entry from the relationship table (relationship_type = 'mentions').
    ///
    /// # Arguments
    ///
    /// * `mentioning_node_id` - ID of the node that contains the mention
    /// * `mentioned_node_id` - ID of the node being mentioned
    ///
    /// # Returns
    ///
    /// `Ok(())` if successful (idempotent - succeeds even if mention doesn't exist)
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
    /// service.delete_mention("daily-note-id", "project-planning-id").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_mention(
        &self,
        mentioning_node_id: &str,
        mentioned_node_id: &str,
    ) -> Result<(), NodeServiceError> {
        // Store returns relationship ID, service emits event
        let relationship_id = self
            .store
            .delete_mention(mentioning_node_id, mentioned_node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit event if relationship was deleted (existed). Normalize
        // ids the same way `RelationshipEvent::new` does for the
        // created/updated variants — see `db::events::node_thing` for
        // the rationale (consumers parse on `:` and reject bare ids).
        if let Some(rel_id) = relationship_id {
            self.emit_event(DomainEvent::RelationshipDeleted {
                id: rel_id,
                from_id: crate::db::events::node_thing(mentioning_node_id),
                to_id: crate::db::events::node_thing(mentioned_node_id),
                relationship_type: "mentions".to_string(),
            });
        }

        Ok(())
    }

    /// Populate outgoing mentions from the relationship table (relationship_type = 'mentions')
    ///
    /// Queries the relationship table to populate outgoing mentions for a node.
    /// Note: mentioned_in (backlinks) is populated separately by get_children_tree
    /// with full NodeReference data {id, title, nodeType} for efficient UI display.
    pub(crate) async fn populate_mentions(&self, node: &mut Node) -> Result<(), NodeServiceError> {
        // Query outgoing mentions (nodes that THIS node references)
        let mentions = self
            .store
            .get_outgoing_mentions(&node.id)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to get outgoing mentions: {}", e))
            })?;
        node.mentions = mentions;

        // Note: mentioned_in is populated by get_children_tree with full NodeReference data
        // This allows the UI to display backlinks without N+1 queries

        Ok(())
    }

    /// Add a mention from one node to another
    ///
    /// Creates a mention relationship in the relationship table (relationship_type = 'mentions').
    ///
    /// # Arguments
    ///
    /// * `source_id` - ID of the node that is mentioning
    /// * `target_id` - ID of the node being mentioned
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
    /// service.add_mention("node-123", "node-456").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_mention(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> Result<(), NodeServiceError> {
        // Prevent direct self-references
        if source_id == target_id {
            return Err(NodeServiceError::ValidationFailed(
                crate::models::ValidationError::InvalidParent(
                    "Cannot create self-referencing mention".to_string(),
                ),
            ));
        }

        // Verify both nodes exist
        if !self.node_exists(source_id).await? {
            return Err(NodeServiceError::node_not_found(source_id));
        }
        if !self.node_exists(target_id).await? {
            return Err(NodeServiceError::node_not_found(target_id));
        }

        // Prevent root-level self-references (child mentioning its own parent)
        if let Ok(Some(parent)) = self.get_parent(source_id).await {
            if parent.id == target_id {
                return Err(NodeServiceError::ValidationFailed(
                    crate::models::ValidationError::InvalidParent(
                        "Cannot mention own parent (root-level self-reference)".to_string(),
                    ),
                ));
            }
        }

        // Store returns relationship ID, service emits event
        // root_id no longer stored - computed dynamically via graph traversal
        let relationship_id = self
            .store
            .create_mention(source_id, target_id)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to insert mention: {}", e))
            })?;

        // Emit event if relationship was created (not already existing)
        if let Some(rel_id) = relationship_id {
            self.emit_event(DomainEvent::RelationshipCreated {
                relationship: crate::db::events::RelationshipEvent::new(
                    rel_id,
                    source_id,
                    target_id,
                    "mentions",
                    serde_json::json!({}),
                ),
            });
        }

        Ok(())
    }

    /// Remove a mention from one node to another
    ///
    /// Deletes a mention relationship from the relationship table (relationship_type = 'mentions').
    ///
    /// # Arguments
    ///
    /// * `source_id` - ID of the node that is mentioning
    /// * `target_id` - ID of the node being mentioned
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
    /// service.remove_mention("node-123", "node-456").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_mention(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> Result<(), NodeServiceError> {
        // Store returns relationship ID, service emits event
        let relationship_id = self
            .store
            .delete_mention(source_id, target_id)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to delete mention: {}", e))
            })?;

        // Emit event if relationship was deleted (existed). Normalize
        // ids — same rationale as the other `RelationshipDeleted`
        // sites; see `db::events::node_thing`.
        if let Some(rel_id) = relationship_id {
            self.emit_event(DomainEvent::RelationshipDeleted {
                id: rel_id,
                from_id: crate::db::events::node_thing(source_id),
                to_id: crate::db::events::node_thing(target_id),
                relationship_type: "mentions".to_string(),
            });
        }

        Ok(())
    }

    /// Get all nodes that a specific node mentions (outgoing references)
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node ID to get mentions for
    ///
    /// # Returns
    ///
    /// Vector of node IDs that this node mentions
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
    /// let mentions = service.get_mentions("node-123").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_mentions(&self, node_id: &str) -> Result<Vec<String>, NodeServiceError> {
        self.store
            .get_outgoing_mentions(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Get all nodes that mention a specific node (incoming references/backlinks)
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node ID to get backlinks for
    ///
    /// # Returns
    ///
    /// Vector of node IDs that mention this node
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
    /// let backlinks = service.get_mentioned_by("node-456").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_mentioned_by(&self, node_id: &str) -> Result<Vec<String>, NodeServiceError> {
        self.store
            .get_incoming_mentions(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Get containers (root or task nodes) that mention the target node (backlinks).
    ///
    /// This resolves incoming mentions to their container nodes and deduplicates.
    /// Returns `NodeReference` with {id, title, nodeType} for efficient UI display.
    ///
    /// # Container Resolution Logic
    /// - For task nodes: Uses the task node itself (tasks are their own containers)
    /// - For other nodes: Traverses up the hierarchy to find the root node
    ///
    /// # Performance
    ///
    /// Uses optimized batch queries with recursive ancestor traversal:
    /// - Single query to get all mentioning sources with their ancestor chains
    /// - Single batch query to fetch container nodes
    ///
    /// # Example
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // If nodes A and B (both children of Container X) mention target node,
    /// // returns [NodeReference { id: "container-x-id", title: "...", nodeType: "text" }]
    /// let containers = service.get_mentioning_containers("target-node-id").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_mentioning_containers(
        &self,
        node_id: &str,
    ) -> Result<Vec<crate::models::NodeReference>, NodeServiceError> {
        self.store
            .get_incoming_mention_containers(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    // ========================================================================
    // Relationship CRUD Operations (Phase 4)
    // ========================================================================

    /// Create a relationship between two nodes
    ///
    /// Creates an edge in the appropriate relationship table based on the schema definition.
    /// Validates that both nodes exist, enforces cardinality constraints, and supports
    /// edge field data.
    ///
    /// # TODO: UI components needed for relationship interaction
    /// The backend API is complete, but users need UI components to:
    /// - Select nodes to relate (search/dropdown)
    /// - View existing relationships
    /// - Remove relationships
    ///
    /// # Arguments
    ///
    /// * `source_id` - ID of the source node
    /// * `relationship_name` - Name of the relationship (e.g., "assigned_to")
    /// * `target_id` - ID of the target node
    /// * `edge_data` - Optional JSON data for edge fields
    ///
    /// # Returns
    ///
    /// Ok(()) if successful
    ///
    /// # Errors
    ///
    /// - `NodeNotFound` - Source or target node doesn't exist
    /// - `SchemaNotFound` - Source node's schema doesn't exist
    /// - `RelationshipNotFound` - Relationship not defined in schema
    /// - `TargetTypeMismatch` - Target node type doesn't match schema definition
    /// - `CardinalityViolation` - Cardinality constraint would be violated
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Create relationship with edge field data
    /// service.create_relationship(
    ///     "task-123",
    ///     "assigned_to",
    ///     "person-456",
    ///     json!({"role": "owner", "assigned_at": "2025-01-15"})
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    /// Enforce ADR-059 §2 root-only content membership: a content node may hold a
    /// `member_of` edge only if it is a root (no `has_child` parent). Membership
    /// carries access classification (ADR-037), so an interior node carrying
    /// independent classification would make access a property of outline
    /// position — the conflation ADR-059 exists to remove. Person `member_of`
    /// edges (grantee membership, ADR-037 §4) and collection nesting are EXEMPT.
    /// One validation shared by every membership-write path (`create_relationship`
    /// and the bulk import path), so the invariant can't be broken by whichever
    /// surface wrote the edge — including a remote edge applied by sync, which
    /// routes through `create_relationship`.
    async fn assert_root_only_membership(&self, source: &Node) -> Result<(), NodeServiceError> {
        if source.node_type == "collection" || source.node_type == "person" {
            return Ok(());
        }
        if !self.is_root_node(&source.id).await? {
            return Err(NodeServiceError::invalid_update(format!(
                "content node '{}' has a parent, so it cannot be a collection member; \
                 only root nodes may hold a member_of edge (ADR-059 root-only membership)",
                source.id
            )));
        }
        Ok(())
    }

    pub async fn create_relationship(
        &self,
        source_id: &str,
        relationship_name: &str,
        target_id: &str,
        edge_data: serde_json::Value,
    ) -> Result<(), NodeServiceError> {
        // Unified relationship creation - ALL relationships use the `relationship` table
        // The relationship_type field distinguishes between different relationship types

        // Built-in type validation
        let is_builtin = matches!(
            relationship_name,
            "member_of" | "has_child" | "mentions" | "has_role"
        );

        if is_builtin {
            // Built-in type-specific validation
            if relationship_name == "member_of" {
                let target = self
                    .get_node(target_id)
                    .await?
                    .ok_or_else(|| NodeServiceError::node_not_found(target_id))?;
                if target.node_type != "collection" {
                    return Err(NodeServiceError::invalid_update(format!(
                        "member_of target must be a collection node, got '{}'",
                        target.node_type
                    )));
                }
                // Collection hierarchy (collection member_of collection) is a
                // DAG, but nothing enforced it — `a member_of b` + `b member_of a`
                // created a cycle that makes the recursive members walk loop. Reject
                // a hierarchy edge that would close a cycle. Only relevant when the
                // source is itself a collection; a content node has no member_of
                // descendants, so the check is a cheap no-op for ordinary membership.
                let source = self
                    .get_node(source_id)
                    .await?
                    .ok_or_else(|| NodeServiceError::node_not_found(source_id))?;
                if source.node_type == "collection" {
                    self.store
                        .validate_no_member_of_cycle(source_id, target_id)
                        .await
                        .map_err(|e| NodeServiceError::collection_cycle(e.to_string()))?;
                }
                // Root-only content membership (ADR-059 §2). No-op for collection
                // (nesting) and person (grantee) sources; enforced for content.
                self.assert_root_only_membership(&source).await?;
            }
        } else {
            // Custom relationship: validate against source node's schema
            let source = self
                .get_node(source_id)
                .await?
                .ok_or_else(|| NodeServiceError::node_not_found(source_id))?;

            let schema_id = &source.node_type;
            let schema_node = self.get_node(schema_id).await?.ok_or_else(|| {
                NodeServiceError::query_failed(format!("Schema '{}' not found", schema_id))
            })?;

            let relationships: Vec<crate::models::schema::SchemaRelationship> = schema_node
                .properties
                .get("relationships")
                .and_then(|r| serde_json::from_value(r.clone()).ok())
                .unwrap_or_default();

            let relationship = relationships
                .iter()
                .find(|r| r.name == relationship_name)
                .ok_or_else(|| {
                    NodeServiceError::invalid_update(format!(
                        "Relationship '{}' not defined in schema '{}'. Built-in relationships (member_of, has_child, mentions, has_role) are universal.",
                        relationship_name, schema_id
                    ))
                })?;

            // Validate target node type (skip when target_type is None — accepts any type)
            if let Some(expected_type) = &relationship.target_type {
                let target = self
                    .get_node(target_id)
                    .await?
                    .ok_or_else(|| NodeServiceError::node_not_found(target_id))?;

                if target.node_type != *expected_type {
                    return Err(NodeServiceError::invalid_update(format!(
                        "Target node type '{}' doesn't match expected type '{}' for relationship '{}'",
                        target.node_type, expected_type, relationship_name
                    )));
                }
            }

            // Check cardinality constraint
            if relationship.cardinality == crate::models::schema::RelationshipCardinality::One {
                let existing_count = self
                    .store
                    .check_relationship_exists(source_id, relationship_name)
                    .await
                    .map_err(|e| {
                        NodeServiceError::query_failed(format!(
                            "Failed to check cardinality: {}",
                            e
                        ))
                    })?;
                if existing_count > 0 {
                    return Err(NodeServiceError::invalid_update(format!(
                        "Relationship '{}' has cardinality 'one' but an edge already exists",
                        relationship_name
                    )));
                }
            }
        }

        // For member_of relationships with auto-order, use the atomic
        // add_to_collection method to prevent race conditions. This ensures the
        // order calculation and relationship creation happen in a single query.
        if relationship_name == "member_of" {
            let has_explicit_order = edge_data
                .as_object()
                .map(|o| o.contains_key("order"))
                .unwrap_or(false);

            if !has_explicit_order {
                // Use atomic add_to_collection for auto-ordered member_of
                let rel_id = self
                    .store
                    .add_to_collection(source_id, target_id)
                    .await
                    .map_err(|e| {
                        NodeServiceError::query_failed(format!(
                            "Failed to add to collection: {}",
                            e
                        ))
                    })?;

                // Emit event if relationship was created (not idempotent hit)
                if let Some(id) = rel_id {
                    // Order is assigned atomically by add_to_collection; use 1.0 as placeholder for event
                    let order = 1.0_f64;

                    self.emit_event(DomainEvent::RelationshipCreated {
                        relationship: crate::db::events::RelationshipEvent::new(
                            id,
                            source_id,
                            target_id,
                            "member_of",
                            serde_json::json!({"order": order}),
                        ),
                    });
                }
                return Ok(());
            }
        }

        // Check for existing relationship (idempotency)
        let already_exists = self
            .store
            .relationship_exists(source_id, target_id, relationship_name)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!(
                    "Failed to check existing relationship: {}",
                    e
                ))
            })?;
        if already_exists {
            // Relationship already exists, idempotent success
            return Ok(());
        }

        // Auto-calculate order for built-in ordered relationships if not provided
        let final_edge_data = if is_builtin {
            let mut data = edge_data.as_object().cloned().unwrap_or_default();

            // Auto-calculate order if not provided for ordered relationship types
            // Note: member_of with auto-order is handled above via atomic add_to_collection
            if data.get("order").is_none() {
                let order = match relationship_name {
                    "has_child" => Some(self.store.get_next_child_order(source_id).await.map_err(
                        |e| {
                            NodeServiceError::query_failed(format!(
                                "Failed to calculate child order: {}",
                                e
                            ))
                        },
                    )?),
                    _ => None, // "mentions" doesn't need ordering, member_of handled above
                };
                if let Some(ord) = order {
                    data.insert("order".to_string(), serde_json::json!(ord));
                }
            }
            serde_json::json!(data)
        } else {
            edge_data.clone()
        };

        let rel_id = self
            .store
            .create_generic_relationship(source_id, target_id, relationship_name, &final_edge_data)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to create relationship: {}", e))
            })?;

        self.emit_event(DomainEvent::RelationshipCreated {
            relationship: crate::db::events::RelationshipEvent::new(
                rel_id,
                source_id,
                target_id,
                relationship_name,
                final_edge_data,
            ),
        });

        Ok(())
    }

    /// Bulk-create `member_of` edges AND emit a `RelationshipCreated` event for
    /// each newly created edge, so the cloud-sync push consumer replicates them.
    ///
    /// The raw `store.bulk_add_to_collections` inserts the rows directly and
    /// emits nothing. That is fine for a purely local write, but the edges then
    /// never reach cloud: they join nodes that are *already* synced, so the
    /// node-oriented "unsynced push sweep" skips them, and every other device
    /// (and any first-time puller) sees those collections empty. Routing the
    /// batch importer's collection assignment through here puts each membership
    /// on the exact same event path as [`Self::create_relationship`], so bulk
    /// import and single-file import replicate identically. Returns the number of
    /// edges actually created (idempotent hits are skipped and not re-emitted).
    pub async fn bulk_add_to_collections_notify(
        &self,
        memberships: &[(String, String)],
    ) -> Result<usize, NodeServiceError> {
        // Root-only content membership (ADR-059 §2), same rule the single-add path
        // enforces. Validate the whole batch before writing any edge so an interior
        // content node can't be filed into a collection via bulk import.
        for (node_id, _collection_id) in memberships {
            let source = self
                .get_node(node_id)
                .await?
                .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;
            self.assert_root_only_membership(&source).await?;
        }

        let created = self
            .store
            .bulk_add_to_collections(memberships)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // The domain-event broadcast channel is bounded (128 slots). A large
        // import can create far more `member_of` edges than that; emitting them
        // all in a tight loop would overflow the channel, making the sync push
        // consumer lag and DROP edges — which then never reach cloud (defeating
        // the whole point). Yield every chunk so the consumer drains between
        // bursts. The chunk stays well under the channel capacity.
        const EMIT_CHUNK: usize = 50;
        for (i, (rel_id, node_id, collection_id, order)) in created.iter().enumerate() {
            self.emit_event(DomainEvent::RelationshipCreated {
                relationship: crate::db::events::RelationshipEvent::new(
                    rel_id.clone(),
                    node_id,
                    collection_id,
                    "member_of",
                    serde_json::json!({ "order": order }),
                ),
            });
            if (i + 1) % EMIT_CHUNK == 0 {
                tokio::task::yield_now().await;
            }
        }

        Ok(created.len())
    }

    /// Delete a relationship between two nodes
    ///
    /// Removes the edge between the source and target nodes for the specified relationship.
    ///
    /// # TODO: UI components needed for relationship interaction
    ///
    /// # Arguments
    ///
    /// * `source_id` - ID of the source node
    /// * `relationship_name` - Name of the relationship
    /// * `target_id` - ID of the target node
    ///
    /// # Returns
    ///
    /// Ok(()) if successful (idempotent - succeeds even if edge doesn't exist)
    ///
    /// # Errors
    ///
    /// - `NodeNotFound` - Source node doesn't exist
    /// - `SchemaNotFound` - Source node's schema doesn't exist
    /// - `RelationshipNotFound` - Relationship not defined in schema
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
    /// service.delete_relationship("task-123", "assigned_to", "person-456").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_relationship(
        &self,
        source_id: &str,
        relationship_name: &str,
        target_id: &str,
    ) -> Result<(), NodeServiceError> {
        // Unified relationship deletion - ALL relationships use the `relationship` table
        // The relationship_type field distinguishes between different relationship types

        let rel_id = self
            .store
            .get_relationship_id(source_id, target_id, relationship_name)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to get relationship ID: {}", e))
            })?;

        self.store
            .delete_generic_relationship(source_id, target_id, relationship_name)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to delete relationship: {}", e))
            })?;

        // Emit RelationshipDeleted event. Normalize ids — same
        // rationale as the other `RelationshipDeleted` sites; see
        // `db::events::node_thing`.
        if let Some(id) = rel_id {
            self.emit_event(DomainEvent::RelationshipDeleted {
                id,
                from_id: crate::db::events::node_thing(source_id),
                to_id: crate::db::events::node_thing(target_id),
                relationship_type: relationship_name.to_string(),
            });
        }

        Ok(())
    }

    /// Get all related nodes for a given relationship
    ///
    /// Queries the relationship table and returns all target nodes connected via the specified
    /// relationship. Supports both "out" and "in" directions.
    ///
    /// # TODO: UI components needed for relationship interaction
    ///
    /// # Arguments
    ///
    /// * `node_id` - ID of the node to get relationships for
    /// * `relationship_name` - Name of the relationship
    /// * `direction` - Direction to traverse ("out" for forward, "in" for reverse)
    ///
    /// # Returns
    ///
    /// Vector of related nodes
    ///
    /// # Errors
    ///
    /// - `NodeNotFound` - Source node doesn't exist
    /// - `SchemaNotFound` - Source node's schema doesn't exist
    /// - `RelationshipNotFound` - Relationship not defined in schema
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
    /// // Get all people assigned to this task
    /// let assigned = service.get_related_nodes("task-123", "assigned_to", "out").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_related_nodes(
        &self,
        node_id: &str,
        relationship_name: &str,
        direction: &str,
    ) -> Result<Vec<Node>, NodeServiceError> {
        if direction != "out" && direction != "in" {
            return Err(NodeServiceError::invalid_update(format!(
                "Invalid direction '{}', must be 'out' or 'in'",
                direction
            )));
        }
        self.store
            .get_nodes_by_relationship(node_id, relationship_name, direction)
            .await
            .map_err(|e| {
                NodeServiceError::query_failed(format!("Failed to get related nodes: {}", e))
            })
    }

    /// Get inbound relationships for a node type
    ///
    /// Returns all relationships from other schemas that point TO this node type.
    /// This is a computed lookup (not cached) - for frequently accessed data,
    /// use `InboundRelationshipCache` instead.
    ///
    /// # Arguments
    ///
    /// * `target_type` - The node type to find inbound relationships for (e.g., "customer")
    ///
    /// # Returns
    ///
    /// Vector of tuples: (source_schema_id, relationship)
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
    /// // What relationships point TO customer?
    /// let inbound = service.get_inbound_relationships("customer").await?;
    /// for (source_type, rel) in inbound {
    ///     println!("{}.{} -> customer", source_type, rel.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_inbound_relationships(
        &self,
        target_type: &str,
    ) -> Result<Vec<(String, crate::models::schema::SchemaRelationship)>, NodeServiceError> {
        let schemas = self.get_all_schemas().await?;

        let mut inbound = Vec::new();
        for schema in schemas {
            for relationship in schema.relationships {
                // Include typed relationships matching this target, and untyped (None) relationships
                let matches = relationship
                    .target_type
                    .as_deref()
                    .map(|t| t == target_type)
                    .unwrap_or(true); // None = untyped, applies to all types
                if matches {
                    inbound.push((schema.id.clone(), relationship));
                }
            }
        }

        Ok(inbound)
    }

    /// Get relationship graph summary for NLP
    ///
    /// Returns a summary of all relationships in the system, useful for
    /// NLP to understand the overall data model structure.
    ///
    /// # Returns
    ///
    /// Vector of tuples: (source_type, relationship_name, target_type)
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
    /// let graph = service.get_relationship_graph().await?;
    /// for (source, rel_name, target) in graph {
    ///     let target_str = target.as_deref().unwrap_or("*");
    ///     println!("{} --{}-> {}", source, rel_name, target_str);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_relationship_graph(
        &self,
    ) -> Result<Vec<(String, String, Option<String>)>, NodeServiceError> {
        let schemas = self.get_all_schemas().await?;

        let mut edges = Vec::new();
        for schema in schemas {
            for relationship in schema.relationships {
                edges.push((
                    schema.id.clone(),
                    relationship.name.clone(),
                    relationship.target_type.clone(),
                ));
            }
        }

        Ok(edges)
    }
}
