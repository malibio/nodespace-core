//! `SqliteStore` methods — nodes concern (split from the god-object per ADR-053 prep).
use super::*;

impl SqliteStore {
    pub async fn create_node(
        &self,
        node: Node,
        source: Option<String>,
        playbook_context: Option<crate::db::events::PlaybookExecutionContext>,
    ) -> Result<Node> {
        if node.node_type == "collection" {
            if let Some(existing) = self.get_collection_by_name(&node.content).await? {
                anyhow::bail!(
                    "Collection with name '{}' already exists (id: {})",
                    node.content,
                    existing.id
                );
            }
        }

        Self::validate_lifecycle_status(&node.lifecycle_status)?;

        let properties = if node.properties.is_null() {
            serde_json::json!({})
        } else {
            node.properties.clone()
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db
            .execute(
                "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                libsql::params![
                    node.id.clone(),
                    node.node_type.clone(),
                    node.content.clone(),
                    props_json,
                    node.title.clone(),
                    node.lifecycle_status.clone(),
                    node.version,
                    now.clone(),
                    now,
                ],
            )
            .await
            .context("Failed to create node")?;

        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context,
        });

        Ok(node)
    }

    pub async fn create_child_node_atomic(
        &self,
        parent_id: &str,
        node_type: &str,
        content: &str,
        properties: Value,
        source: Option<String>,
    ) -> Result<Node> {
        self.validate_node_type(node_type)?;

        let node_id = uuid::Uuid::new_v4().to_string();

        let parent_exists = self.get_node(parent_id).await?;
        if parent_exists.is_none() {
            return Err(anyhow::anyhow!("Parent node not found: {}", parent_id));
        }

        self.validate_no_cycle(parent_id, &node_id).await?;

        // Serialize the sibling-order read → compute → write against move_node and
        // other order mutations on the shared connection (held until return).
        // Without it a concurrent create/move under the same parent reads the same
        // max order and assigns a colliding key. No re-entrancy: this method calls
        // no other reorder_lock-taking path. See `reorder_lock`.
        let _reorder_guard = self.reorder_lock.lock().await;

        // Get last child order
        let mut rows = self.db.query(
            "SELECT json_extract(r.properties, '$.order') as ord FROM relationship r WHERE r.in_node = ?1 AND r.relationship_type = 'has_child' ORDER BY json_extract(r.properties, '$.order') DESC LIMIT 1",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get last child order")?;

        // distinguish "no siblings" from "a sibling at order <= 0".
        // Fractional ordering legitimately yields orders <= 0 (a prepend gives
        // 0.0, then -1.0…), and `unwrap_or(0.0)` maps a NULL order to 0.0. The old
        // `last_order > 0.0` sentinel misread a lone child at 0.0 as "no children",
        // so the next append also got 1.0 → two siblings collided at 1.0. Track
        // presence as Option and append after the max regardless of sign.
        let last_order: Option<f64> = if let Some(row) = rows.next().await? {
            Some(row.get::<Option<f64>>(0)?.unwrap_or(0.0))
        } else {
            None
        };

        let new_order = FractionalOrderCalculator::calculate_order(last_order, None);

        let properties = if properties.is_null() {
            serde_json::json!({})
        } else {
            properties
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin transaction")?;

        tx.execute(
            "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, NULL, 'active', 1, ?5, ?6)",
            libsql::params![node_id.clone(), node_type.to_string(), content.to_string(), props_json, now.clone(), now.clone()],
        ).await.context("Failed to insert child node")?;

        let rel_id = uuid::Uuid::new_v4().to_string();
        let rel_props = serde_json::json!({"order": new_order}).to_string();
        tx.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
            libsql::params![rel_id, parent_id.to_string(), node_id.clone(), rel_props, now.clone(), now],
        ).await.context("Failed to insert parent-child relationship")?;

        tx.commit().await.context("Failed to commit transaction")?;

        let node = self
            .get_node(&node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after creation: {}", node_id))?;

        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(node)
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to query node")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(Self::row_to_node(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn node_exists(&self, id: &str) -> Result<bool> {
        let mut rows = self
            .db
            .query(
                "SELECT 1 FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to check node existence")?;
        Ok(rows.next().await?.is_some())
    }

    pub async fn get_nodes_by_ids(&self, ids: &[String]) -> Result<HashMap<String, Node>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Chunk under SQLite's ~999 bound-parameter ceiling; a large `IN (...)`
        // would otherwise fail outright (a directory import can list thousands
        // of files). Mirrors the chunking in the other bulk store queries.
        const ID_CHUNK: usize = 900;
        let mut result = HashMap::new();
        for chunk in ids.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT * FROM node WHERE id IN ({})",
                placeholders.join(", ")
            );

            let params: Vec<libsql::Value> = chunk
                .iter()
                .map(|id| libsql::Value::Text(id.clone()))
                .collect();
            let mut rows = self
                .db
                .query(&sql, params)
                .await
                .context("Failed to batch query nodes")?;

            while let Some(row) = rows.next().await? {
                let node = Self::row_to_node(&row)?;
                result.insert(node.id.clone(), node);
            }
        }
        Ok(result)
    }

    pub async fn update_node(
        &self,
        id: &str,
        update: NodeUpdate,
        source: Option<String>,
    ) -> Result<Node> {
        if let Some(ref status) = update.lifecycle_status {
            Self::validate_lifecycle_status(status)?;
        }

        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", id))?;

        let updated_content = update.content.unwrap_or(current.content);
        let updated_node_type = update.node_type.unwrap_or(current.node_type.clone());

        let properties_update = if let Some(ref updated_props) = update.properties {
            let mut merged = current.properties.as_object().cloned().unwrap_or_default();
            if let Some(new_props) = updated_props.as_object() {
                for (key, value) in new_props {
                    merged.insert(key.clone(), value.clone());
                }
            }
            Some(serde_json::Value::Object(merged))
        } else {
            None
        };

        let now = Utc::now().to_rfc3339();

        if let Some(ref props) = properties_update {
            let props_json =
                serde_json::to_string(props).context("Failed to serialize properties")?;
            self.db.execute(
                "UPDATE node SET content = ?1, node_type = ?2, properties = ?3, version = version + 1, modified_at = ?4 WHERE id = ?5",
                libsql::params![updated_content.clone(), updated_node_type.clone(), props_json, now.clone(), id.to_string()],
            ).await.context("Failed to update node")?;
        } else {
            self.db.execute(
                "UPDATE node SET content = ?1, node_type = ?2, version = version + 1, modified_at = ?3 WHERE id = ?4",
                libsql::params![updated_content.clone(), updated_node_type.clone(), now.clone(), id.to_string()],
            ).await.context("Failed to update node")?;
        }

        if let Some(title) = update.title {
            self.db
                .execute(
                    "UPDATE node SET title = ?1 WHERE id = ?2",
                    libsql::params![title, id.to_string()],
                )
                .await
                .context("Failed to update title")?;
        }

        if let Some(status) = update.lifecycle_status {
            self.db
                .execute(
                    "UPDATE node SET lifecycle_status = ?1 WHERE id = ?2",
                    libsql::params![status, id.to_string()],
                )
                .await
                .context("Failed to update lifecycle_status")?;
        }

        let updated_node = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after update"))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: updated_node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(updated_node)
    }

    pub async fn switch_node_type_atomic(
        &self,
        node_id: &str,
        new_type: &str,
        new_properties: Value,
        source: Option<String>,
    ) -> Result<Node> {
        self.validate_node_type(new_type)?;

        let new_properties = if new_properties.is_null() {
            serde_json::json!({})
        } else {
            new_properties
        };
        let props_json =
            serde_json::to_string(&new_properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db
            .execute(
                "UPDATE node SET node_type = ?1, properties = ?2, version = version + 1, modified_at = ?3 WHERE id = ?4",
                libsql::params![new_type.to_string(), props_json, now, node_id.to_string()],
            )
            .await
            .context("Failed to switch node type")?;

        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after type switch: {}", node_id))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(node)
    }

    /// Version of the REAL persisted `node` row, or `None` if no such row exists.
    /// Unlike `get_node`, this does NOT virtualize a date-page node — so a
    /// concurrently-deleted date page reports `None`, not a phantom version 1.
    /// Used to disambiguate a no-op version-checked update.
    pub async fn persisted_version(&self, id: &str) -> Result<Option<i64>> {
        let mut rows = self
            .db
            .query(
                "SELECT version FROM node WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to read persisted node version")?;
        Ok(match rows.next().await? {
            Some(row) => Some(row.get::<i64>(0)?),
            None => None,
        })
    }

    pub async fn update_node_with_version_check(
        &self,
        id: &str,
        expected_version: i64,
        update: NodeUpdate,
        source: Option<String>,
        playbook_context: Option<crate::db::events::PlaybookExecutionContext>,
    ) -> Result<Option<Node>> {
        if let Some(ref status) = update.lifecycle_status {
            Self::validate_lifecycle_status(status)?;
        }

        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", id))?;

        let previous_node = current.clone();
        let new_version = expected_version + 1;

        let updated_content = update.content.unwrap_or(current.content);
        let updated_node_type = update.node_type.unwrap_or(current.node_type.clone());
        let updated_props = match update.properties {
            Some(p) => serde_json::to_string(&p).context("Failed to serialize properties")?,
            None => serde_json::to_string(&current.properties)
                .context("Failed to serialize current properties")?,
        };
        let updated_title = match update.title {
            Some(t) => t,          // Some(Some(x)) sets, Some(None) clears
            None => current.title, // None means no change
        };
        let updated_status = update
            .lifecycle_status
            .unwrap_or(current.lifecycle_status.clone());
        let now = Utc::now().to_rfc3339();

        let rows_affected = self.db.execute(
            "UPDATE node SET content = ?1, node_type = ?2, properties = ?3, title = ?4, lifecycle_status = ?5, version = ?6, modified_at = ?7 WHERE id = ?8 AND version = ?9",
            libsql::params![updated_content, updated_node_type, updated_props, updated_title, updated_status, new_version, now, id.to_string(), expected_version],
        ).await.context("Failed to update node with version check")?;

        if rows_affected == 0 {
            return Ok(None);
        }

        let node = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after update"))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: node.clone(),
            source,
            previous_node: Some(previous_node),
            playbook_context,
        });

        Ok(Some(node))
    }

    pub async fn update_lifecycle_status(&self, id: &str, status: &str) -> Result<()> {
        Self::validate_lifecycle_status(status)?;
        self.db
            .execute(
                "UPDATE node SET lifecycle_status = ?1 WHERE id = ?2",
                libsql::params![status.to_string(), id.to_string()],
            )
            .await
            .context("Failed to update lifecycle_status")?;
        Ok(())
    }

    pub async fn delete_node(&self, id: &str, source: Option<String>) -> Result<DeleteResult> {
        let node = match self.get_node(id).await? {
            Some(n) => n,
            None => {
                return Ok(DeleteResult {
                    existed: false,
                    deleted_count: 0,
                })
            }
        };

        // FK CASCADE handles relationship and embedding deletion
        self.db
            .execute(
                "DELETE FROM node WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to delete node")?;

        self.notify(StoreChange {
            operation: StoreOperation::Deleted,
            node,
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(DeleteResult {
            existed: true,
            deleted_count: 1,
        })
    }

    /// Atomically delete multiple nodes in a SINGLE transaction. Either
    /// every existing target row is removed or none are — a mid-batch failure rolls
    /// the whole thing back, so a caller that gets `Err` can rely on nothing having
    /// been deleted. FK CASCADE removes each node's relationships + embeddings (same
    /// as `delete_node`); children not in `ids` are left in place (edges cascade).
    /// Emits one `Deleted` notification per node that existed, AFTER commit. Returns
    /// the nodes that were deleted.
    pub async fn bulk_delete(&self, ids: &[String], source: Option<String>) -> Result<Vec<Node>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        // Snapshot the rows that actually exist (for the post-commit notifications).
        let existing = self.get_nodes_by_ids(ids).await?;
        let to_delete: Vec<Node> = ids
            .iter()
            .filter_map(|id| existing.get(id).cloned())
            .collect();
        if to_delete.is_empty() {
            return Ok(vec![]);
        }

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk delete transaction")?;
        let placeholders: Vec<String> = (1..=to_delete.len()).map(|i| format!("?{i}")).collect();
        let sql = format!("DELETE FROM node WHERE id IN ({})", placeholders.join(", "));
        let params: Vec<libsql::Value> = to_delete
            .iter()
            .map(|n| libsql::Value::Text(n.id.clone()))
            .collect();
        tx.execute(&sql, params)
            .await
            .context("Failed to delete nodes in bulk")?;
        tx.commit()
            .await
            .context("Failed to commit bulk delete transaction")?;

        for node in &to_delete {
            self.notify(StoreChange {
                operation: StoreOperation::Deleted,
                node: node.clone(),
                source: source.clone(),
                previous_node: None,
                playbook_context: None,
            });
        }
        Ok(to_delete)
    }

    /// Atomically delete a node and its entire `has_child` subtree in a single transaction.
    ///
    /// **OCC contract:** Version-checks only the target node at `expected_version`. Descendants
    /// are removed unconditionally inside the same transaction. A target version mismatch rolls
    /// back the entire transaction — no partial deletion is possible.
    ///
    /// **Event emission:** Emits one `NodeDeleted` notification per deleted node (target +
    /// every descendant) after the commit, so the frontend store reconciles each removal.
    ///
    /// Returns `(existed, deleted_nodes)` where `deleted_nodes` contains the target and all
    /// descendants that were deleted (empty vec when target didn't exist).
    pub async fn delete_subtree_atomic(
        &self,
        node_id: &str,
        expected_version: i64,
        source: Option<String>,
    ) -> Result<(bool, Vec<Node>)> {
        // Collect the target + all descendants before mutating.
        let target = match self.get_node(node_id).await? {
            Some(n) => n,
            None => return Ok((false, vec![])),
        };

        // OCC check on target before entering the transaction.
        if target.version != expected_version {
            return Err(anyhow::anyhow!(
                "version_conflict:{}:{}:{}",
                node_id,
                expected_version,
                target.version
            ));
        }

        // Walk has_child recursively to collect the full descendant set.
        let mut id_rows = self.db.query(
            r#"WITH RECURSIVE subtree(node_id) AS (
                SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node FROM relationship r
                JOIN subtree s ON r.in_node = s.node_id
                WHERE r.relationship_type = 'has_child'
            )
            SELECT DISTINCT node_id FROM subtree"#,
            libsql::params![node_id.to_string()],
        ).await.context("Failed to collect subtree descendants")?;

        let mut all_ids: Vec<String> = vec![node_id.to_string()];
        while let Some(row) = id_rows.next().await? {
            all_ids.push(row.get(0)?);
        }

        // Fetch all node records (needed for post-commit notifications).
        let placeholders: Vec<String> = (1..=all_ids.len()).map(|i| format!("?{}", i)).collect();
        let fetch_sql = format!(
            "SELECT * FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let fetch_params: Vec<libsql::Value> = all_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut node_rows = self
            .db
            .query(&fetch_sql, fetch_params)
            .await
            .context("Failed to fetch subtree nodes for deletion")?;
        let mut nodes_to_delete: Vec<Node> = Vec::new();
        while let Some(row) = node_rows.next().await? {
            nodes_to_delete.push(Self::row_to_node(&row)?);
        }

        // Single-transaction delete — FK CASCADE cleans relationship and embedding rows.
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin subtree delete transaction")?;

        // Re-check target version inside the transaction to close the TOCTOU window.
        let check_sql = "SELECT version FROM node WHERE id = ?1";
        let mut check_rows = tx
            .query(check_sql, libsql::params![node_id.to_string()])
            .await
            .context("Failed to re-check target version")?;
        let actual_version: i64 = match check_rows.next().await? {
            Some(row) => row.get(0)?,
            None => {
                // Node disappeared between pre-check and transaction — idempotent.
                return Ok((false, vec![]));
            }
        };
        if actual_version != expected_version {
            return Err(anyhow::anyhow!(
                "version_conflict:{}:{}:{}",
                node_id,
                expected_version,
                actual_version
            ));
        }

        let del_placeholders: Vec<String> =
            (1..=all_ids.len()).map(|i| format!("?{}", i)).collect();
        let del_sql = format!(
            "DELETE FROM node WHERE id IN ({})",
            del_placeholders.join(", ")
        );
        let del_params: Vec<libsql::Value> = all_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        tx.execute(&del_sql, del_params)
            .await
            .context("Failed to delete subtree nodes")?;

        tx.commit()
            .await
            .context("Failed to commit subtree delete transaction")?;

        // Emit one NodeDeleted notification per deleted node after commit.
        for node in &nodes_to_delete {
            self.notify(StoreChange {
                operation: StoreOperation::Deleted,
                node: node.clone(),
                source: source.clone(),
                previous_node: None,
                playbook_context: None,
            });
        }

        Ok((true, nodes_to_delete))
    }

    /// Delete all descendants of `parent_id` (the full child subtree) without touching the parent.
    ///
    /// Uses a recursive CTE to collect all descendant IDs, then deletes them in one statement.
    /// FK CASCADE cleans relationship and embedding rows automatically.
    /// No OCC check — intended for internal system operations where version checks are unnecessary.
    pub async fn delete_children_subtree_unchecked(&self, parent_id: &str) -> Result<()> {
        self.db
            .execute(
                r#"WITH RECURSIVE subtree(node_id) AS (
                    SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                    UNION ALL
                    SELECT r.out_node FROM relationship r
                    JOIN subtree s ON r.in_node = s.node_id
                    WHERE r.relationship_type = 'has_child'
                )
                DELETE FROM node WHERE id IN (SELECT DISTINCT node_id FROM subtree)"#,
                libsql::params![parent_id.to_string()],
            )
            .await
            .context("Failed to delete children subtree")?;
        Ok(())
    }

    /// Delete exactly the given node ids (and, via FK CASCADE, their
    /// relationship/embedding rows). No OCC check — for internal system
    /// operations. Used to prune a document's *previous* subtree during an
    /// idempotent re-import only after the fresh subtree has been inserted, so
    /// a failed insert never destroys the old content.
    pub async fn delete_nodes_by_ids_unchecked(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        // Chunk under SQLite's ~999 bound-parameter ceiling.
        const ID_CHUNK: usize = 900;
        for chunk in ids.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!("DELETE FROM node WHERE id IN ({})", placeholders.join(", "));
            let params: Vec<libsql::Value> = chunk
                .iter()
                .map(|id| libsql::Value::Text(id.clone()))
                .collect();
            self.db
                .execute(&sql, params)
                .await
                .context("Failed to delete nodes by id")?;
        }
        Ok(())
    }

    /// Remove a single top-level key from a node's properties JSON in-place using `json_remove`.
    ///
    /// Used by migrations to clear legacy fields (e.g. `description`) after they have been
    /// moved to the child-subtree representation, making the migration idempotent.
    pub async fn remove_property_key(&self, node_id: &str, key: &str) -> Result<()> {
        let json_path = format!("$.{}", key);
        self.db
            .execute(
                "UPDATE node SET properties = json_remove(properties, ?1), modified_at = ?2 WHERE id = ?3",
                libsql::params![json_path, chrono::Utc::now().to_rfc3339(), node_id.to_string()],
            )
            .await
            .context("Failed to remove property key")?;
        Ok(())
    }

    /// Set a boolean value at a JSON path within a node's properties in-place
    /// using `json_set`. `json_path` must start with `$.` (e.g. `$._seed.user_modified`).
    ///
    /// Bypasses OCC (no version check, no version bump) — used to stamp
    /// bookkeeping metadata (e.g. `_seed.user_modified`) alongside a caller's
    /// own version-checked update without racing it or requiring the caller
    /// to round-trip the flag through its own `NodeUpdate`.
    pub async fn set_property_bool(
        &self,
        node_id: &str,
        json_path: &str,
        value: bool,
    ) -> Result<()> {
        // json_set's value argument must be JSON, not SQLite's 0/1 integer
        // affinity, or the stored value round-trips as an int instead of a bool.
        let json_value = if value { "true" } else { "false" };
        self.db
            .execute(
                "UPDATE node SET properties = json_set(properties, ?1, json(?2)) WHERE id = ?3",
                libsql::params![json_path.to_string(), json_value, node_id.to_string()],
            )
            .await
            .context("Failed to set property key")?;
        Ok(())
    }

    pub async fn delete_with_version_check(
        &self,
        id: &str,
        expected_version: i64,
        source: Option<String>,
    ) -> Result<usize> {
        let node = match self.get_node(id).await? {
            Some(n) => n,
            None => return Ok(0),
        };

        if node.version != expected_version {
            return Ok(0);
        }

        let result = self.delete_node(id, source).await?;
        Ok(if result.existed { 1 } else { 0 })
    }

    pub async fn query_nodes(&self, query: NodeQuery) -> Result<Vec<Node>> {
        if let Some(ref mentioned_node_id) = query.mentioned_by {
            let mut rows = self.db.query(
                "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'mentions'",
                libsql::params![mentioned_node_id.clone()],
            ).await.context("Failed to query mentioned_by nodes")?;
            let mut nodes = Vec::new();
            while let Some(row) = rows.next().await? {
                nodes.push(Self::row_to_node(&row)?);
            }
            return Ok(nodes);
        }

        if let Some(ref search_q) = query.content_contains {
            let search_lower = format!("%{}%", search_q.to_lowercase());
            let sql = match (query.limit, query.offset) {
                (None, None) => "SELECT * FROM node WHERE LOWER(content) LIKE ?1".to_string(),
                (Some(l), None) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT {}",
                    l
                ),
                (None, Some(o)) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT -1 OFFSET {}",
                    o
                ),
                (Some(l), Some(o)) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT {} OFFSET {}",
                    l, o
                ),
            };
            return self
                .query_nodes_from_sql(&sql, libsql::params![search_lower])
                .await;
        }

        let mut conditions = Vec::new();
        let mut bind_values: Vec<libsql::Value> = Vec::new();
        let mut param_idx = 1usize;

        if let Some(ref search_q) = query.title_contains {
            let search_lower = format!("%{}%", search_q.to_lowercase());
            conditions.push(format!(
                "title IS NOT NULL AND LOWER(title) LIKE ?{}",
                param_idx
            ));
            bind_values.push(libsql::Value::Text(search_lower));
            param_idx += 1;
        }

        if let Some(ref nt) = query.node_type {
            conditions.push(format!("node_type = ?{}", param_idx));
            bind_values.push(libsql::Value::Text(nt.clone()));
        }

        // id-scoping (e.g. a collection's members). Build `id IN (…)` and
        // CHUNK it under SQLite's bound-parameter ceiling so a large member set
        // can't overflow the limit. Each chunk also carries the title/node_type
        // conditions already collected. The caller (NodeService::query_nodes)
        // applies order_by + limit/offset in memory, so per-chunk order is
        // irrelevant here. Empty id set ⇒ no rows can match.
        if let Some(ref ids) = query.ids {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            const ID_CHUNK: usize = 900;
            let mut nodes = Vec::new();
            for chunk in ids.chunks(ID_CHUNK) {
                let mut conds = conditions.clone();
                let mut binds = bind_values.clone();
                let start = binds.len();
                let placeholders: Vec<String> = chunk
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", start + i + 1))
                    .collect();
                conds.push(format!("id IN ({})", placeholders.join(", ")));
                for id in chunk {
                    binds.push(libsql::Value::Text(id.clone()));
                }
                let sql = format!("SELECT * FROM node WHERE {}", conds.join(" AND "));
                let mut rows = self
                    .db
                    .query(&sql, binds)
                    .await
                    .context("Failed to query nodes by id set")?;
                while let Some(row) = rows.next().await? {
                    nodes.push(Self::row_to_node(&row)?);
                }
            }
            return Ok(nodes);
        }

        let where_clause = if !conditions.is_empty() {
            format!("WHERE {}", conditions.join(" AND "))
        } else {
            String::new()
        };

        let limit_offset = match (query.limit, query.offset) {
            (None, None) => String::new(),
            (Some(l), None) => format!(" LIMIT {}", l),
            (None, Some(o)) => format!(" LIMIT -1 OFFSET {}", o),
            (Some(l), Some(o)) => format!(" LIMIT {} OFFSET {}", l, o),
        };

        let sql = format!("SELECT * FROM node {} {}", where_clause, limit_offset);
        let mut rows = self
            .db
            .query(&sql, bind_values)
            .await
            .context("Failed to query nodes")?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_children(&self, parent_id: &str) -> Result<Vec<Node>> {
        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.out_node = n.id WHERE r.in_node = ?1 AND r.relationship_type = 'has_child' ORDER BY json_extract(r.properties, '$.order') ASC",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get children")?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_roots(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Node>> {
        let limit_offset = match (limit, offset) {
            (None, None) => String::new(),
            (Some(l), None) => format!(" LIMIT {}", l),
            (None, Some(o)) => format!(" LIMIT -1 OFFSET {}", o),
            (Some(l), Some(o)) => format!(" LIMIT {} OFFSET {}", l, o),
        };

        let sql = format!(
            "SELECT * FROM node WHERE id NOT IN (SELECT out_node FROM relationship WHERE relationship_type = 'has_child') ORDER BY id ASC{}",
            limit_offset
        );

        self.query_nodes_from_sql(&sql, ()).await
    }

    pub async fn get_parent(&self, child_id: &str) -> Result<Option<Node>> {
        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'has_child' LIMIT 1",
            libsql::params![child_id.to_string()],
        ).await.context("Failed to get parent")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(Self::row_to_node(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_parent_id(&self, child_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child' LIMIT 1",
            libsql::params![child_id.to_string()],
        ).await.context("Failed to get parent id")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_node_type(&self, node_id: &str) -> Result<Option<String>> {
        let mut rows = self
            .db
            .query(
                "SELECT node_type FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to get node type")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_node_tree(&self, root_id: &str) -> Result<Option<serde_json::Value>> {
        let root_node = match self.get_node(root_id).await? {
            Some(node) => node,
            None => return Ok(None),
        };

        let tree = self.build_node_tree_recursive(&root_node).await?;
        Ok(Some(tree))
    }

    fn build_node_tree_recursive<'a>(
        &'a self,
        node: &'a Node,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        const MAX_DEPTH: usize = 100;
        Box::pin(async move {
            self.build_node_tree_with_guards(node, 0, MAX_DEPTH, &mut HashSet::new())
                .await
        })
    }

    fn build_node_tree_with_guards<'a>(
        &'a self,
        node: &'a Node,
        depth: usize,
        max_depth: usize,
        visited: &'a mut HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        Box::pin(async move {
            if depth >= max_depth {
                return Err(anyhow::anyhow!(
                    "Maximum tree depth ({}) exceeded at node '{}'",
                    max_depth,
                    node.id
                ));
            }
            if visited.contains(&node.id) {
                return Err(anyhow::anyhow!(
                    "Cycle detected: node '{}' appears multiple times",
                    node.id
                ));
            }
            visited.insert(node.id.clone());

            let children_nodes = self.get_children(&node.id).await?;
            let mut children_json = Vec::new();
            for child in &children_nodes {
                let child_tree = self
                    .build_node_tree_with_guards(child, depth + 1, max_depth, visited)
                    .await?;
                children_json.push(child_tree);
            }
            visited.remove(&node.id);

            Ok(serde_json::json!({
                "id": node.id,
                "type": node.node_type,
                "content": node.content,
                "version": node.version,
                "created_at": node.created_at,
                "modified_at": node.modified_at,
                "mentions": node.mentions,
                "mentionedIn": node.mentioned_in,
                "data": node.properties,
                "variants": serde_json::Value::Null,
                "_schema_version": 1,
                "children": children_json
            }))
        })
    }

    pub async fn get_nodes_in_subtree(&self, root_id: &str) -> Result<Vec<Node>> {
        let (all_nodes, _) = self.get_subtree_with_relationships(root_id).await?;
        Ok(all_nodes.into_iter().filter(|n| n.id != root_id).collect())
    }

    pub async fn get_subtree_with_relationships(
        &self,
        root_id: &str,
    ) -> Result<(Vec<Node>, Vec<RelationshipRecord>)> {
        let start = std::time::Instant::now();

        // WITH RECURSIVE to collect all descendants
        let mut id_rows = self.db.query(
            r#"WITH RECURSIVE subtree(node_id, depth) AS (
                SELECT out_node, 1 FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node, s.depth + 1 FROM relationship r
                JOIN subtree s ON r.in_node = s.node_id
                WHERE r.relationship_type = 'has_child' AND s.depth < 100
            )
            SELECT DISTINCT node_id FROM subtree"#,
            libsql::params![root_id.to_string()],
        ).await.context("Failed to query descendants")?;

        let mut descendant_ids = vec![root_id.to_string()];
        while let Some(row) = id_rows.next().await? {
            descendant_ids.push(row.get(0)?);
        }

        if descendant_ids.len() == 1 {
            // Just the root — verify it exists
            let root = match self.get_node(root_id).await? {
                Some(n) => n,
                None => return Ok((vec![], vec![])),
            };
            return Ok((vec![root], vec![]));
        }

        // Fetch all nodes
        let placeholders: Vec<String> = (1..=descendant_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let sql = format!(
            "SELECT * FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<libsql::Value> = descendant_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut node_rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to fetch subtree nodes")?;
        let mut all_nodes = Vec::new();
        while let Some(row) = node_rows.next().await? {
            all_nodes.push(Self::row_to_node(&row)?);
        }

        if all_nodes.is_empty() {
            return Ok((vec![], vec![]));
        }

        // Fetch relationships within subtree
        let rel_placeholders: Vec<String> = (1..=descendant_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let rel_sql = format!(
            "SELECT id, in_node, out_node, relationship_type, properties FROM relationship WHERE in_node IN ({}) AND relationship_type = 'has_child'",
            rel_placeholders.join(", ")
        );
        let rel_params: Vec<libsql::Value> = descendant_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut rel_rows = self
            .db
            .query(&rel_sql, rel_params)
            .await
            .context("Failed to fetch subtree relationships")?;
        let mut relationships = Vec::new();
        while let Some(row) = rel_rows.next().await? {
            relationships.push(Self::row_to_relationship(&row)?);
        }

        tracing::debug!(
            "get_subtree_with_relationships: {:?} for root_id={} ({} nodes)",
            start.elapsed(),
            root_id,
            all_nodes.len()
        );

        Ok((all_nodes, relationships))
    }

    pub async fn get_relationships_in_subtree(
        &self,
        root_id: &str,
    ) -> Result<Vec<RelationshipRecord>> {
        let (_, relationships) = self.get_subtree_with_relationships(root_id).await?;
        Ok(relationships)
    }

    async fn validate_no_cycle(&self, parent_id: &str, child_id: &str) -> Result<()> {
        // Check if parent_id is a descendant of child_id (would create cycle)
        let mut rows = self.db.query(
            r#"WITH RECURSIVE desc(node_id, depth) AS (
                SELECT out_node, 1 FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node, d.depth + 1 FROM relationship r
                JOIN desc d ON r.in_node = d.node_id
                WHERE r.relationship_type = 'has_child' AND d.depth < 100
            )
            SELECT node_id FROM desc WHERE node_id = ?2 LIMIT 1"#,
            libsql::params![child_id.to_string(), parent_id.to_string()],
        ).await.context("Failed to check for cycles")?;

        if rows.next().await?.is_some() {
            return Err(anyhow::anyhow!(
                "Cannot create parent-child relationship: would create cycle. \
                Node '{}' is a descendant of node '{}'.",
                parent_id,
                child_id
            ));
        }
        Ok(())
    }

    /// Cycle guard for collection-hierarchy `member_of` edges. The
    /// `has_child` tree has `validate_no_cycle`, but collection hierarchy is built
    /// from `member_of` (a sub-collection is a member_of its parent) and had no
    /// equivalent — so `a member_of b` + `b member_of a` produced a cycle in the
    /// supposed DAG, which now makes the recursive members walk loop.
    ///
    /// `member_of` stores in_node = child/member, out_node = parent/collection.
    /// Adding `source member_of target` makes `target` an ancestor of `source`;
    /// that is a cycle iff `target` is already a DESCENDANT of `source`. Walk the
    /// member_of subtree downward from `source` (out_node = current → in_node =
    /// child) and fail if it reaches `target`. A self-edge is trivially a cycle.
    pub async fn validate_no_member_of_cycle(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> Result<()> {
        if source_id == target_id {
            return Err(anyhow::anyhow!(
                "collection_cycle: '{}' cannot be a member of itself",
                source_id
            ));
        }
        let mut rows = self
            .db
            .query(
                r#"WITH RECURSIVE descendants(node_id, depth) AS (
                SELECT in_node, 1 FROM relationship
                  WHERE out_node = ?1 AND relationship_type = 'member_of'
                UNION ALL
                SELECT r.in_node, d.depth + 1 FROM relationship r
                JOIN descendants d ON r.out_node = d.node_id
                WHERE r.relationship_type = 'member_of' AND d.depth < 100
            )
            SELECT node_id FROM descendants WHERE node_id = ?2 LIMIT 1"#,
                libsql::params![source_id.to_string(), target_id.to_string()],
            )
            .await
            .context("Failed to check for member_of cycle")?;
        if rows.next().await?.is_some() {
            return Err(anyhow::anyhow!(
                "collection_cycle: '{}' is already a descendant of '{}', so making it the parent would create a cycle",
                target_id,
                source_id
            ));
        }
        Ok(())
    }

    async fn rebalance_children_for_parent(&self, parent_id: &str) -> Result<()> {
        let mut rows = self.db.query(
            "SELECT id, out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' ORDER BY json_extract(properties, '$.order') ASC",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get children for rebalancing")?;

        let mut rels: Vec<(String, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            rels.push((row.get(0)?, row.get(1)?));
        }

        if rels.is_empty() {
            return Ok(());
        }

        let new_orders = FractionalOrderCalculator::rebalance(rels.len());
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin rebalance transaction")?;

        for (i, (rel_id, _)) in rels.iter().enumerate() {
            let props = serde_json::json!({"order": new_orders[i]}).to_string();
            tx.execute(
                "UPDATE relationship SET properties = ?1 WHERE id = ?2",
                libsql::params![props, rel_id.clone()],
            )
            .await
            .context("Failed to rebalance relationship")?;
        }

        tx.commit().await.context("Failed to commit rebalance")?;
        Ok(())
    }

    /// ADR-059 §2 (reparent side of the root-only content-membership rule): a node
    /// that holds a `member_of` edge is a root member and must not be given a
    /// `has_child` parent — that would make it a forbidden interior member with no
    /// `member_of` write for the store's forward guard to catch. Called at BOTH
    /// store-level sites that attach an *existing* node to a parent — `move_node`
    /// (the service `move_node` and `upsert_node_with_parent` reparent paths) and
    /// `bulk_create_has_child` (the sync-apply cold-sweep) — so every reparent
    /// path is covered, symmetrically with the forward guard `assert_root_only_
    /// membership` on the `member_of` INSERT sites. (Fresh-node attach sites can't
    /// pre-hold a membership; `move_children_to_parent` only moves already-interior
    /// nodes.) Rejects rather than dropping the membership (a node can hold several
    /// grants, each an independent access path). `collection` (nesting) and
    /// `person` (grantee, ADR-037 §4) nodes are exempt. A single chunked query
    /// keeps the bulk/cold-sweep path a single round trip.
    pub(crate) async fn assert_may_gain_parent(&self, node_ids: &[&str]) -> Result<()> {
        if node_ids.is_empty() {
            return Ok(());
        }
        let mut unique: Vec<&str> = node_ids.to_vec();
        unique.sort_unstable();
        unique.dedup();

        // Chunk the `IN (...)` under SQLite's ~999 bound-parameter ceiling.
        const ID_CHUNK: usize = 900;
        for chunk in unique.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            // Offenders: non-exempt nodes that already hold a `member_of` edge.
            let sql = format!(
                "SELECT n.id FROM node n \
                 WHERE n.id IN ({}) \
                   AND n.node_type NOT IN ('collection', 'person') \
                   AND EXISTS(SELECT 1 FROM relationship r \
                              WHERE r.in_node = n.id AND r.relationship_type = 'member_of')",
                placeholders.join(", ")
            );
            let params: Vec<libsql::Value> = chunk
                .iter()
                .map(|id| libsql::Value::Text(id.to_string()))
                .collect();
            let mut rows = self
                .db
                .query(&sql, params)
                .await
                .context("Failed to validate root-only membership on reparent")?;
            if let Some(row) = rows.next().await? {
                let offender: String = row.get(0)?;
                let memberships = self.get_node_memberships(&offender).await?;
                return Err(anyhow::anyhow!(
                    "member_of_not_root: node '{}' holds collection membership ({}) and cannot be moved under a parent — only root nodes may hold collection membership (ADR-059 §2). Remove it from the collection(s) first, or move its root instead.",
                    offender,
                    memberships.join(", ")
                ));
            }
        }
        Ok(())
    }

    pub async fn move_node(
        &self,
        node_id: &str,
        new_parent_id: Option<&str>,
        insert_after_sibling_id: Option<&str>,
    ) -> Result<f64> {
        let node_id = node_id.to_string();
        let new_parent_id = new_parent_id.map(|s| s.to_string());
        let insert_after_sibling_id = insert_after_sibling_id.map(|s| s.to_string());

        if !self.node_exists(&node_id).await? {
            return Err(anyhow::anyhow!("Node not found: {}", node_id));
        }

        let current_parent_id = self.get_parent_id(&node_id).await?;
        let is_same_parent_reorder = match (&new_parent_id, &current_parent_id) {
            (Some(new_pid), Some(cur_pid)) => new_pid == cur_pid,
            (None, None) => true,
            _ => false,
        };

        if let Some(ref parent_id) = new_parent_id {
            if !self.node_exists(parent_id).await? {
                return Err(anyhow::anyhow!("Parent node not found: {}", parent_id));
            }
            self.validate_no_cycle(parent_id, &node_id).await?;
            // ADR-059 §2: a member cannot be moved into an interior position.
            self.assert_may_gain_parent(&[node_id.as_str()]).await?;
        }

        // Serialize the sibling-order read → fractional-key compute → write-back
        // (including any rebalance). Held until the function returns. Without this,
        // two concurrent same-parent reorders interleave their reads/writes on the
        // shared connection and compute overlapping order keys against the same
        // stale snapshot, corrupting the final sibling order. See `reorder_lock`.
        let _reorder_guard = self.reorder_lock.lock().await;

        let new_order = if let Some(ref parent_id) = new_parent_id {
            // Get ordered siblings excluding the moving node
            let mut rows = self.db.query(
                "SELECT out_node, json_extract(properties, '$.order') as ord FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' AND out_node != ?2 ORDER BY json_extract(properties, '$.order') ASC",
                libsql::params![parent_id.clone(), node_id.clone()],
            ).await.context("Failed to get sibling relationships")?;

            let mut siblings: Vec<(String, f64)> = Vec::new();
            while let Some(row) = rows.next().await? {
                let child_id: String = row.get(0)?;
                let ord: Option<f64> = row.get(1)?;
                siblings.push((child_id, ord.unwrap_or(0.0)));
            }

            if let Some(after_id) = insert_after_sibling_id {
                if let Some(after_index) = siblings.iter().position(|(id, _)| id == &after_id) {
                    let prev_order = siblings[after_index].1;
                    let next_order = siblings.get(after_index + 1).map(|(_, o)| *o);

                    if let Some(next) = next_order {
                        if (next - prev_order) < 0.0001 {
                            self.rebalance_children_for_parent(parent_id).await?;
                            // Re-query after rebalancing
                            let mut rows2 = self.db.query(
                                "SELECT out_node, json_extract(properties, '$.order') as ord FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' AND out_node != ?2 ORDER BY json_extract(properties, '$.order') ASC",
                                libsql::params![parent_id.clone(), node_id.clone()],
                            ).await.context("Failed to get siblings after rebalancing")?;
                            let mut siblings2: Vec<(String, f64)> = Vec::new();
                            while let Some(row) = rows2.next().await? {
                                let cid: String = row.get(0)?;
                                let ord: Option<f64> = row.get(1)?;
                                siblings2.push((cid, ord.unwrap_or(0.0)));
                            }
                            if let Some(after_index2) =
                                siblings2.iter().position(|(id, _)| id == &after_id)
                            {
                                let prev2 = siblings2[after_index2].1;
                                let next2 = siblings2.get(after_index2 + 1).map(|(_, o)| *o);
                                FractionalOrderCalculator::calculate_order(Some(prev2), next2)
                            } else {
                                let last = siblings2.last().map(|(_, o)| *o);
                                FractionalOrderCalculator::calculate_order(last, None)
                            }
                        } else {
                            FractionalOrderCalculator::calculate_order(Some(prev_order), next_order)
                        }
                    } else {
                        FractionalOrderCalculator::calculate_order(Some(prev_order), None)
                    }
                } else {
                    let last = siblings.last().map(|(_, o)| *o);
                    FractionalOrderCalculator::calculate_order(last, None)
                }
            } else {
                let first = siblings.first().map(|(_, o)| *o);
                FractionalOrderCalculator::calculate_order(None, first)
            }
        } else {
            0.0
        };

        let now = Utc::now().to_rfc3339();

        if let Some(ref parent_id) = new_parent_id {
            if is_same_parent_reorder {
                let props = serde_json::json!({"order": new_order}).to_string();
                self.db.execute(
                    "UPDATE relationship SET properties = ?1, version = version + 1, modified_at = ?2 WHERE in_node = ?3 AND out_node = ?4 AND relationship_type = 'has_child'",
                    libsql::params![props, now, parent_id.clone(), node_id.clone()],
                ).await.context("Failed to update relationship order")?;
            } else {
                // Cross-parent move: delete the old has_child edge, create the new
                // one. These MUST be one transaction — if the INSERT fails
                // (constraint / IO / crash / cancel) after the DELETE committed, the
                // node is left with NO has_child edge: a silently-orphaned root.
                // Wrapping in a tx makes it all-or-nothing, matching the atomicity
                // of `move_children_to_parent` / `delete_subtree_atomic`.
                let rel_id = uuid::Uuid::new_v4().to_string();
                let props = serde_json::json!({"order": new_order}).to_string();
                let tx = self
                    .db
                    .transaction()
                    .await
                    .context("Failed to begin move_node reparent transaction")?;
                tx.execute(
                    "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'",
                    libsql::params![node_id.clone()],
                ).await.context("Failed to delete old parent relationship")?;
                tx.execute(
                    "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                    libsql::params![rel_id, parent_id.clone(), node_id.clone(), props, now.clone(), now],
                ).await.context("Failed to create new parent relationship")?;
                tx.commit()
                    .await
                    .context("Failed to commit move_node reparent transaction")?;
            }
        } else {
            // Make root: delete parent relationship
            self.db.execute(
                "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'",
                libsql::params![node_id.clone()],
            ).await.context("Failed to delete parent relationship")?;
        }

        Ok(new_order)
    }

    /// Re-parent an ordered set of existing children to `new_parent_id` in a
    /// single transaction. Validates each child's version inside the transaction
    /// using `SELECT changes()` after a version-gated DELETE — any mismatch causes
    /// the full transaction to roll back (all-or-nothing OCC).
    ///
    /// Returns the assigned fractional order for each child, preserving input
    /// array order as sibling order under the new parent.
    pub async fn move_children_to_parent(
        &self,
        new_parent_id: &str,
        children: &[(&str, i64)],
    ) -> Result<Vec<f64>> {
        if children.is_empty() {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();

        // Serialize the sibling-order read → compute → write against move_node and
        // other order mutations on the shared connection (held until return). No
        // re-entrancy: the transaction below issues raw DELETE/INSERT and calls no
        // other reorder_lock-taking path. See `reorder_lock`.
        let _reorder_guard = self.reorder_lock.lock().await;

        // Append the moved children AFTER new_parent_id's existing children: read
        // the current max has_child order under the new parent and seed the
        // sequence from it. Previously this assigned 1.0, 2.0, 3.0… ignoring
        // existing siblings, so moving into a NON-EMPTY parent collided their
        // order keys even single-threaded.
        let existing_max: Option<f64> = {
            let mut rows = self.db.query(
                "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' ORDER BY json_extract(properties, '$.order') DESC LIMIT 1",
                libsql::params![new_parent_id.to_string()],
            ).await.context("Failed to read existing child order for move_children_to_parent")?;
            if let Some(row) = rows.next().await? {
                Some(row.get::<Option<f64>>(0)?.unwrap_or(0.0))
            } else {
                None
            }
        };

        // Compute sequential fractional orders: the first appended after
        // `existing_max` (or from scratch when the parent is empty), each
        // subsequent after the previously assigned order.
        let mut orders: Vec<f64> = Vec::with_capacity(children.len());
        for _ in 0..children.len() {
            let prev = orders.last().copied().or(existing_max);
            orders.push(FractionalOrderCalculator::calculate_order(prev, None));
        }

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin move_children_to_parent transaction")?;

        for ((child_id, expected_version), &order) in children.iter().zip(orders.iter()) {
            let child_id = child_id.to_string();

            // Delete the old has_child edge only if the node's version matches.
            // If no rows are affected the version has been bumped by a concurrent
            // writer — detect that with SELECT changes() and abort the transaction.
            tx.execute(
                "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child' AND EXISTS (SELECT 1 FROM node WHERE id = ?1 AND version = ?2)",
                libsql::params![child_id.clone(), *expected_version],
            )
            .await
            .context("Failed to delete old has_child edge")?;

            // Verify the DELETE actually removed a row (i.e. the version matched).
            let mut changes_rows = tx
                .query("SELECT changes()", libsql::params![])
                .await
                .context("Failed to query changes()")?;
            if let Some(row) = changes_rows.next().await? {
                let affected: i64 = row.get(0)?;
                if affected == 0 {
                    return Err(anyhow::anyhow!(
                        "VERSION_CONFLICT: node '{}' version mismatch (expected {})",
                        child_id,
                        expected_version
                    ));
                }
            }

            // Insert new has_child edge under new_parent_id.
            let rel_id = uuid::Uuid::new_v4().to_string();
            let rel_props = serde_json::json!({"order": order}).to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                libsql::params![
                    rel_id,
                    new_parent_id.to_string(),
                    child_id,
                    rel_props,
                    now.clone(),
                    now.clone()
                ],
            )
            .await
            .context("Failed to insert new has_child edge")?;
        }

        tx.commit()
            .await
            .context("Failed to commit move_children_to_parent")?;

        Ok(orders)
    }

    pub async fn get_schema(&self, node_type: &str) -> Result<Option<Value>> {
        let node = self.get_node(node_type).await?;
        Ok(node.map(|n| n.properties))
    }

    pub async fn update_schema(&self, node_type: &str, schema: &Value) -> Result<()> {
        let schema_id = node_type.to_string();

        if self.get_node(&schema_id).await?.is_some() {
            let update = NodeUpdate {
                properties: Some(schema.clone()),
                ..Default::default()
            };
            self.update_node(&schema_id, update, None).await?;
        } else {
            let node = Node::new_with_id(
                schema_id,
                "schema".to_string(),
                node_type.to_string(),
                schema.clone(),
            );
            self.create_node(node, None, None).await?;
        }

        Ok(())
    }

    pub async fn rename_schema_field(&self, type_id: &str, from: &str, to: &str) -> Result<u64> {
        if from.is_empty() || to.is_empty() {
            return Err(anyhow::anyhow!("Field names must not be empty"));
        }
        if from == to {
            return Err(anyhow::anyhow!(
                "Source and destination field names are the same: '{}'",
                from
            ));
        }

        let mut rows = self
            .db
            .query(
                "SELECT id, properties FROM node WHERE node_type = ?1",
                libsql::params![type_id.to_string()],
            )
            .await
            .context("Failed to fetch nodes for field rename")?;

        let mut nodes: Vec<(String, Value)> = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let props_str: String = row.get(1)?;
            let props: Value = serde_json::from_str(&props_str).unwrap_or(serde_json::json!({}));
            nodes.push((id, props));
        }

        let mut affected = 0u64;
        let now = Utc::now().to_rfc3339();

        for (node_id, mut properties) in nodes {
            let had_field = if let Some(ns_obj) = properties
                .as_object_mut()
                .and_then(|p| p.get_mut(type_id))
                .and_then(|ns| ns.as_object_mut())
            {
                if let Some(value) = ns_obj.remove(from) {
                    ns_obj.insert(to.to_string(), value);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if had_field {
                let props_json =
                    serde_json::to_string(&properties).context("Failed to serialize properties")?;
                self.db
                    .execute(
                        "UPDATE node SET properties = ?1, modified_at = ?2 WHERE id = ?3",
                        libsql::params![props_json, now.clone(), node_id],
                    )
                    .await
                    .context("Failed to update node during field rename")?;
                affected += 1;
            }
        }

        tracing::info!(
            type_id = %type_id,
            from = %from,
            to = %to,
            affected = affected,
            "rename_schema_field: migrated {} node(s)",
            affected
        );

        Ok(affected)
    }

    /// Atomically update a batch of nodes in a single transaction.
    ///
    /// **OCC contract:** This is an intentional last-write-wins fast-path for trusted internal
    /// callers (e.g. AI skill pipelines, markdown import). It does NOT perform version-checked
    /// OCC. Callers that need conflict detection should use `update_node` (the single-node path)
    /// in a loop or use the daemon `update_nodes_batch` RPC which threads per-node versions.
    ///
    /// **TOCTOU note:** All fields are written without a pre-read. Unspecified fields
    /// (`None`) are preserved via SQL `COALESCE` — the entire read-modify-write is one
    /// atomic SQL statement per node inside the transaction with no pool reads escaping
    /// the tx.
    ///
    /// **`properties` semantics:** `Some(value)` **replaces** the existing JSON object
    /// entirely (unlike `update_node`, which merges key-by-key). `None` keeps the existing
    /// value unchanged.
    ///
    /// **`title` semantics:** `Some(Some("text"))` sets a new title; `Some(None)` clears
    /// it to NULL; `None` leaves the existing title unchanged.
    ///
    /// Returns an error (and rolls back) if any node id is not found.
    pub async fn bulk_update(&self, updates: Vec<(String, NodeUpdate)>) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        const MAX_BATCH_SIZE: usize = 1000;
        if updates.len() > MAX_BATCH_SIZE {
            return Err(anyhow::anyhow!(
                "Bulk update batch size ({}) exceeds maximum ({})",
                updates.len(),
                MAX_BATCH_SIZE
            ));
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk update transaction")?;

        for (id, update) in &updates {
            if let Some(ref status) = update.lifecycle_status {
                Self::validate_lifecycle_status(status)?;
            }

            let props_json = update
                .properties
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .context("Failed to serialize properties in bulk update")?;

            // All five NodeUpdate fields are covered. COALESCE preserves the existing
            // column value when the caller passes None, closing the TOCTOU window with
            // no pre-read escaping the transaction.
            let affected = tx
                .execute(
                    "UPDATE node SET \
                    content          = COALESCE(?1, content), \
                    node_type        = COALESCE(?2, node_type), \
                    properties       = COALESCE(?3, properties), \
                    lifecycle_status = COALESCE(?4, lifecycle_status), \
                    version          = version + 1, \
                    modified_at      = ?5 \
                WHERE id = ?6",
                    libsql::params![
                        update.content.clone(),
                        update.node_type.clone(),
                        props_json,
                        update.lifecycle_status.clone(),
                        now.clone(),
                        id.clone()
                    ],
                )
                .await
                .context("Failed to update node in bulk update")?;

            if affected == 0 {
                return Err(anyhow::anyhow!("Node not found: {}", id));
            }

            // `title` uses Option<Option<String>>: Some(Some(t)) sets, Some(None) clears
            // to NULL, None skips. COALESCE can't express "write NULL intentionally", so
            // we handle title with a separate statement only when the caller touches it.
            if let Some(title) = &update.title {
                tx.execute(
                    "UPDATE node SET title = ?1 WHERE id = ?2",
                    libsql::params![title.clone(), id.clone()],
                )
                .await
                .context("Failed to update title in bulk update")?;
            }
        }

        tx.commit().await.context("Failed to commit bulk update")?;
        Ok(())
    }

    pub async fn batch_create_nodes(&self, nodes: Vec<Node>) -> Result<Vec<Node>> {
        let mut created = Vec::new();
        for node in nodes {
            created.push(self.create_node(node, None, None).await?);
        }
        Ok(created)
    }

    /// Insert a batch of nodes (and optional parent→child relationships) in a single transaction.
    ///
    /// **Partial-failure contract:** All inserts occur inside one transaction. Any error
    /// (duplicate id, constraint violation, serialization failure) triggers an early `?`-return,
    /// which drops `tx` without calling `commit()` — the entire batch is rolled back atomically.
    /// Callers never observe a partial write.
    ///
    /// **Validation note:** `validate_node_type` is a pure in-memory check against
    /// `self.valid_node_types`; it does not touch the database and therefore cannot read
    /// uncommitted state from `tx`.
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
    ) -> Result<Vec<String>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk hierarchy transaction")?;

        for (id, node_type, content, parent_id, order, properties) in &nodes {
            self.validate_node_type(node_type)?;

            let properties = if properties.is_null() {
                serde_json::json!({})
            } else {
                properties.clone()
            };
            let props_json =
                serde_json::to_string(&properties).context("Failed to serialize properties")?;

            let title =
                Self::compute_title_for_bulk_insert(node_type, parent_id.as_deref(), content);

            tx.execute(
                "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, 'active', 1, ?6, ?7)",
                libsql::params![id.clone(), node_type.clone(), content.clone(), props_json, title, now.clone(), now.clone()],
            ).await.context("Failed to insert node in bulk hierarchy")?;

            if let Some(parent) = parent_id {
                let rel_id = uuid::Uuid::new_v4().to_string();
                let rel_props = serde_json::json!({"order": order}).to_string();
                tx.execute(
                    "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                    libsql::params![rel_id, parent.clone(), id.clone(), rel_props, now.clone(), now.clone()],
                ).await.context("Failed to insert relationship in bulk hierarchy")?;
            }
        }

        tx.commit()
            .await
            .context("Failed to commit bulk hierarchy")?;

        let ids: Vec<String> = nodes.into_iter().map(|(id, ..)| id).collect();

        // Notify for each created node
        for id in &ids {
            if let Ok(Some(node)) = self.get_node(id).await {
                self.notify(StoreChange {
                    operation: StoreOperation::Created,
                    node,
                    source: None,
                    previous_node: None,
                    playbook_context: None,
                });
            }
        }

        Ok(ids)
    }

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
        root_ids: Vec<String>,
    ) -> Result<Vec<String>> {
        let created = self.bulk_create_hierarchy(nodes).await?;

        // Create stale embedding markers for roots
        self.create_stale_embedding_markers_bulk(&root_ids).await?;

        Ok(created)
    }

    pub async fn create_node_streaming(
        &self,
        id: String,
        node_type: String,
        content: String,
        parent_id: Option<String>,
        order: f64,
        properties: serde_json::Value,
    ) -> Result<String> {
        self.validate_node_type(&node_type)?;

        let properties = if properties.is_null() {
            serde_json::json!({})
        } else {
            properties
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db.execute(
            "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, NULL, 'active', 1, ?5, ?6)",
            libsql::params![id.clone(), node_type.clone(), content.clone(), props_json, now.clone(), now.clone()],
        ).await.context("Failed to create node (streaming)")?;

        if let Some(ref parent) = parent_id {
            let rel_id = uuid::Uuid::new_v4().to_string();
            let rel_props = serde_json::json!({"order": order}).to_string();
            self.db.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                libsql::params![rel_id, parent.clone(), id.clone(), rel_props, now.clone(), now],
            ).await.context("Failed to create relationship (streaming)")?;
        }

        let node = Node {
            id: id.clone(),
            node_type: node_type.clone(),
            content,
            version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            properties,
            mentions: vec![],
            mentioned_in: vec![],
            title: None,
            lifecycle_status: "active".to_string(),
        };
        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node,
            source: Some("streaming_import".to_string()),
            previous_node: None,
            playbook_context: None,
        });

        Ok(id)
    }

    fn compute_title_for_bulk_insert(
        node_type: &str,
        parent_id: Option<&str>,
        content: &str,
    ) -> Option<String> {
        if matches!(node_type, "date" | "schema" | "checkbox") {
            None
        } else if parent_id.is_none() || matches!(node_type, "task" | "collection") {
            let stripped = crate::utils::strip_markdown(content);
            Some(stripped)
        } else {
            None
        }
    }

    pub async fn get_task_node(&self, id: &str) -> Result<Option<crate::models::TaskNode>> {
        let node = self.get_node(id).await?;
        Ok(node.and_then(|n| {
            if n.node_type != "task" {
                return None;
            }
            let props = &n.properties;
            let task_props = props.get("task").cloned().unwrap_or(serde_json::json!({}));
            Some(crate::models::TaskNode {
                id: n.id,
                node_type: n.node_type,
                content: n.content,
                version: n.version,
                created_at: n.created_at,
                modified_at: n.modified_at,
                properties: n.properties,
                status: task_props
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                priority: task_props
                    .get("priority")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok()),
                due_date: task_props
                    .get("due_date")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
                assignee: task_props
                    .get("assignee")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                started_at: task_props
                    .get("started_at")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
                completed_at: task_props
                    .get("completed_at")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
            })
        }))
    }

    pub async fn update_task_node(
        &self,
        id: &str,
        expected_version: i64,
        update: crate::models::TaskNodeUpdate,
    ) -> Result<crate::models::TaskNode> {
        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task node not found: {}", id))?;

        if current.version != expected_version {
            return Err(anyhow::anyhow!(
                "VersionMismatch: expected {}, got {}",
                expected_version,
                current.version
            ));
        }

        let mut props = current.properties.clone();
        let task_obj = props
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid properties"))?
            .entry("task")
            .or_insert(serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid task properties"))?
            .clone();

        let mut task_obj_owned = task_obj;

        if let Some(ref status) = update.status {
            task_obj_owned.insert("status".to_string(), serde_json::json!(status.as_str()));
        }
        if let Some(ref priority_opt) = update.priority {
            match priority_opt {
                Some(p) => {
                    task_obj_owned.insert("priority".to_string(), serde_json::json!(p.as_str()));
                }
                None => {
                    task_obj_owned.remove("priority");
                }
            }
        }
        if let Some(ref due_date_opt) = update.due_date {
            match due_date_opt {
                Some(s) => {
                    task_obj_owned.insert("due_date".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("due_date");
                }
            }
        }
        if let Some(ref assignee_opt) = update.assignee {
            match assignee_opt {
                Some(a) => {
                    task_obj_owned.insert("assignee".to_string(), serde_json::json!(a));
                }
                None => {
                    task_obj_owned.remove("assignee");
                }
            }
        }
        if let Some(ref started_at_opt) = update.started_at {
            match started_at_opt {
                Some(s) => {
                    task_obj_owned.insert("started_at".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("started_at");
                }
            }
        }
        if let Some(ref completed_at_opt) = update.completed_at {
            match completed_at_opt {
                Some(s) => {
                    task_obj_owned.insert("completed_at".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("completed_at");
                }
            }
        }

        // Re-insert updated task object back into properties
        if let Some(props_obj) = props.as_object_mut() {
            props_obj.insert(
                "task".to_string(),
                serde_json::Value::Object(task_obj_owned),
            );
        }

        let props_json = serde_json::to_string(&props).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();
        let new_version = expected_version + 1;

        let mut sql = "UPDATE node SET properties = ?1, version = ?2, modified_at = ?3".to_string();
        let mut sql_params: Vec<libsql::Value> = vec![
            libsql::Value::Text(props_json),
            libsql::Value::Integer(new_version),
            libsql::Value::Text(now),
        ];

        if let Some(ref content) = update.content {
            sql.push_str(", content = ?4");
            sql_params.push(libsql::Value::Text(content.clone()));
            sql.push_str(&format!(" WHERE id = ?{}", sql_params.len() + 1));
        } else {
            sql.push_str(&format!(" WHERE id = ?{}", sql_params.len() + 1));
        }
        sql_params.push(libsql::Value::Text(id.to_string()));

        self.db
            .execute(&sql, sql_params)
            .await
            .context("Failed to update task node")?;

        self.get_task_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task node '{}' not found after update", id))
    }

    pub async fn get_schema_node(&self, id: &str) -> Result<Option<crate::models::SchemaNode>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE id = ?1 AND node_type = 'schema' LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to query schema node")?;

        if let Some(row) = rows.next().await? {
            let node = Self::row_to_node(&row)?;
            match crate::models::SchemaNode::from_node(node) {
                Ok(schema) => Ok(Some(schema)),
                Err(e) => {
                    tracing::warn!("Failed to parse schema node '{}': {}", id, e);
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_all_schemas(&self) -> Result<Vec<crate::models::SchemaNode>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE node_type = 'schema' ORDER BY id",
                (),
            )
            .await
            .context("Failed to query all schema nodes")?;

        let mut schemas = Vec::new();
        while let Some(row) = rows.next().await? {
            let node = Self::row_to_node(&row)?;
            match crate::models::SchemaNode::from_node(node) {
                Ok(schema) => schemas.push(schema),
                Err(e) => tracing::warn!("Skipping invalid schema node: {}", e),
            }
        }
        Ok(schemas)
    }

    pub async fn count_nodes_by_type(&self, node_type: &str) -> Result<i64> {
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM node WHERE node_type = ?1",
                libsql::params![node_type.to_string()],
            )
            .await
            .context("Failed to count nodes by type")?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No result for count"))?;
        Ok(row.get::<i64>(0).unwrap_or(0))
    }

    pub async fn query_node_ids_raw(&self, sql: &str) -> Result<Vec<String>> {
        let mut rows = self
            .db
            .query(sql, ())
            .await
            .context("Failed to execute node query")?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Ok(id) = row.get::<String>(0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}
