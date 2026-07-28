//! `SqliteStore` methods — relationships concern (split from the god-object per ADR-053 prep).
use super::*;

impl SqliteStore {
    pub async fn create_mention(&self, source_id: &str, target_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to check for existing mention")?;

        if rows.next().await?.is_some() {
            return Ok(None);
        }

        let rel_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'mentions', '{}', 1, ?4, ?5)",
            libsql::params![rel_id.clone(), source_id.to_string(), target_id.to_string(), now.clone(), now],
        ).await.context("Failed to create mention")?;

        Ok(Some(rel_id))
    }

    pub async fn delete_mention(&self, source_id: &str, target_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to get mention id")?;

        let existing_id: Option<String> = if let Some(row) = rows.next().await? {
            Some(row.get(0)?)
        } else {
            None
        };

        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to delete mention")?;

        Ok(existing_id.map(|id| format!("relationship:{}", id)))
    }

    pub async fn get_outgoing_mentions(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = match self.db.query(
            "SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to query outgoing mentions for {}: {}", node_id, e);
                return Ok(Vec::new());
            }
        };

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn get_incoming_mentions(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get incoming mentions")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn get_incoming_mention_containers(
        &self,
        node_id: &str,
    ) -> Result<Vec<crate::models::NodeReference>> {
        let start = std::time::Instant::now();

        // Get all nodes that mention this node
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get mentioning sources")?;

        let mut source_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            source_ids.push(row.get(0)?);
        }

        if source_ids.is_empty() {
            return Ok(Vec::new());
        }

        // For each source, determine its container (task=itself, else walk up to root)
        let mut container_ids: HashSet<String> = HashSet::new();

        for source_id in &source_ids {
            let node_type = self.get_node_type(source_id).await?;
            if node_type.as_deref() == Some("task") {
                container_ids.insert(source_id.clone());
            } else {
                // Walk up to root using recursive CTE
                let mut root_rows = self.db.query(
                    r#"WITH RECURSIVE ancestors(node_id, depth) AS (
                        SELECT in_node, 1 FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'
                        UNION ALL
                        SELECT r.in_node, a.depth + 1 FROM relationship r
                        JOIN ancestors a ON r.out_node = a.node_id
                        WHERE r.relationship_type = 'has_child' AND a.depth < 100
                    )
                    SELECT node_id FROM ancestors ORDER BY depth DESC LIMIT 1"#,
                    libsql::params![source_id.clone()],
                ).await.context("Failed to get ancestor chain")?;

                if let Some(row) = root_rows.next().await? {
                    container_ids.insert(row.get(0)?);
                } else {
                    // Source is already a root
                    container_ids.insert(source_id.clone());
                }
            }
        }

        if container_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Batch fetch containers
        let placeholders: Vec<String> = (1..=container_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let sql = format!(
            "SELECT id, title, node_type FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<libsql::Value> = container_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut container_rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to fetch containers")?;

        let mut result = Vec::new();
        while let Some(row) = container_rows.next().await? {
            result.push(crate::models::NodeReference {
                id: row.get(0)?,
                title: row.get(1)?,
                node_type: row.get(2)?,
            });
        }

        tracing::debug!(
            "get_incoming_mention_containers: {} containers in {:?} for node_id={}",
            result.len(),
            start.elapsed(),
            node_id
        );

        Ok(result)
    }

    async fn get_next_order_for_relationship(
        &self,
        node_id: &str,
        relationship_type: &str,
        use_out_as_anchor: bool,
    ) -> Result<f64> {
        let anchor_field = if use_out_as_anchor {
            "out_node"
        } else {
            "in_node"
        };
        let sql = format!(
            "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE {} = ?1 AND relationship_type = ?2 ORDER BY json_extract(properties, '$.order') DESC LIMIT 1",
            anchor_field
        );

        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), relationship_type.to_string()],
            )
            .await
            .context("Failed to get last order for relationship")?;

        // presence-not-sign — append after the max sibling even when its
        // order is <= 0 (a `> 0.0` sentinel misreads a lone child at 0.0 as none).
        let last_order: Option<f64> = if let Some(row) = rows.next().await? {
            Some(row.get::<Option<f64>>(0)?.unwrap_or(0.0))
        } else {
            None
        };

        Ok(FractionalOrderCalculator::calculate_order(last_order, None))
    }

    pub async fn get_next_member_order(&self, collection_id: &str) -> Result<f64> {
        self.get_next_order_for_relationship(collection_id, "member_of", true)
            .await
    }

    pub async fn get_next_child_order(&self, parent_id: &str) -> Result<f64> {
        self.get_next_order_for_relationship(parent_id, "has_child", false)
            .await
    }

    /// ADR-059 §2 — a content node may hold a `member_of` edge only when it is a
    /// **root** node (no `has_child` parent). Collections (nesting) and person
    /// nodes (grantee membership, ADR-037 §4) are exempt. Enforced at the store's
    /// three `member_of` INSERT sites (`add_to_collection`,
    /// `bulk_add_to_collections`, and the generic `create_generic_relationship`
    /// when its `rel_type` is `member_of`), so every write path is covered without
    /// a per-path check: CLI, graph import, playbook `add_relationship`, and the
    /// sync-apply cold-sweep (which calls `bulk_add_to_collections` directly). A
    /// batched, chunked query keeps the bulk/cold-sweep path a single round trip.
    /// Members that don't exist yet are left to the INSERT's foreign-key check.
    async fn assert_root_only_membership(&self, member_ids: &[&str]) -> Result<()> {
        if member_ids.is_empty() {
            return Ok(());
        }
        let mut unique: Vec<&str> = member_ids.to_vec();
        unique.sort_unstable();
        unique.dedup();

        // Chunk the `IN (...)` under SQLite's ~999 bound-parameter ceiling.
        const ID_CHUNK: usize = 900;
        for chunk in unique.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT n.id, n.node_type, \
                 EXISTS(SELECT 1 FROM relationship r \
                        WHERE r.out_node = n.id AND r.relationship_type = 'has_child') \
                 FROM node n WHERE n.id IN ({})",
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
                .context("Failed to validate root-only membership")?;
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                let node_type: String = row.get(1)?;
                let has_parent: i64 = row.get(2)?;
                if has_parent != 0 && node_type != "collection" && node_type != "person" {
                    return Err(anyhow::anyhow!(
                        "member_of_not_root: content node '{}' (type '{}') has a parent, so it cannot be a member of a collection directly — file its root node instead",
                        id,
                        node_type
                    ));
                }
            }
        }
        Ok(())
    }

    pub async fn add_to_collection(
        &self,
        member_id: &str,
        collection_id: &str,
    ) -> Result<Option<String>> {
        self.assert_root_only_membership(&[member_id]).await?;

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin add_to_collection transaction")?;

        let mut rows = tx
            .query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of' LIMIT 1",
                libsql::params![member_id.to_string(), collection_id.to_string()],
            )
            .await
            .context("Failed to check existing membership")?;

        if rows.next().await?.is_some() {
            return Ok(None);
        }

        let mut order_rows = tx
            .query(
                "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE out_node = ?1 AND relationship_type = 'member_of' ORDER BY json_extract(properties, '$.order') DESC LIMIT 1",
                libsql::params![collection_id.to_string()],
            )
            .await
            .context("Failed to get last member order")?;

        // presence-not-sign — append after the max member even at order <= 0.
        let last_order: Option<f64> = if let Some(row) = order_rows.next().await? {
            Some(row.get::<Option<f64>>(0)?.unwrap_or(0.0))
        } else {
            None
        };
        let new_order = FractionalOrderCalculator::calculate_order(last_order, None);

        let rel_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let props = serde_json::json!({"order": new_order}).to_string();

        tx.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'member_of', ?4, 1, ?5, ?6)",
            libsql::params![rel_id.clone(), member_id.to_string(), collection_id.to_string(), props, now.clone(), now],
        )
        .await
        .context("Failed to add to collection")?;

        tx.commit()
            .await
            .context("Failed to commit add_to_collection")?;

        Ok(Some(rel_id))
    }

    pub async fn remove_from_collection(
        &self,
        member_id: &str,
        collection_id: &str,
    ) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of'",
            libsql::params![member_id.to_string(), collection_id.to_string()],
        ).await.context("Failed to get membership ID")?;

        let existing_id: Option<String> = if let Some(row) = rows.next().await? {
            Some(row.get(0)?)
        } else {
            None
        };

        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of'",
            libsql::params![member_id.to_string(), collection_id.to_string()],
        ).await.context("Failed to remove from collection")?;

        Ok(existing_id.map(|id| format!("relationship:{}", id)))
    }

    pub async fn get_node_memberships(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = self.db.query(
            "SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'member_of'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get node memberships")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    /// Every `member_of` edge belonging to a node that is a member of MORE THAN
    /// ONE collection, as `(member_id, collection_id)` pairs. A node's first
    /// membership rides its atomic cloud node insert, so only these SECONDARY
    /// memberships need a separate push; the cloud-sync membership sweep uses this
    /// to converge them. Bounding to multi-membership nodes keeps the sweep cheap
    /// — the single-membership majority (already covered atomically) is never
    /// enumerated. Idempotent re-push of the one atomic-covered edge among a
    /// multi-membership node's edges is a benign no-op on the cloud side.
    ///
    /// `person` members are excluded: their collection memberships are RBAC state
    /// managed server-side by the membership RPCs (invite / set_member) and are
    /// not the sweep's concern — re-asserting them would bypass those gates and
    /// waste round-trips. Content and nested-collection memberships are kept.
    pub async fn get_multi_membership_edges(&self) -> Result<Vec<(String, String)>> {
        let mut rows = self
            .db
            .query(
                "SELECT r.in_node, r.out_node FROM relationship r \
                 JOIN node n ON n.id = r.in_node \
                 WHERE r.relationship_type = 'member_of' \
                   AND n.node_type != 'person' \
                   AND r.in_node IN ( \
                     SELECT in_node FROM relationship \
                     WHERE relationship_type = 'member_of' \
                     GROUP BY in_node HAVING COUNT(*) > 1 \
                   )",
                (),
            )
            .await
            .context("Failed to get multi-membership edges")?;

        let mut edges = Vec::new();
        while let Some(row) = rows.next().await? {
            let member: String = row.get(0)?;
            let collection: String = row.get(1)?;
            edges.push((member, collection));
        }
        Ok(edges)
    }

    pub async fn get_collection_members(&self, collection_id: &str) -> Result<Vec<Node>> {
        let start = std::time::Instant::now();

        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'member_of' ORDER BY json_extract(r.properties, '$.order') ASC",
            libsql::params![collection_id.to_string()],
        ).await.context("Failed to get collection members")?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }

        tracing::debug!(
            "get_collection_members: {:?} for {} nodes",
            start.elapsed(),
            nodes.len()
        );

        Ok(nodes)
    }

    pub async fn get_collection_by_name(&self, name: &str) -> Result<Option<Node>> {
        let normalized = name.to_lowercase();

        let mut rows = self
            .db
            .query(
                "SELECT id FROM node WHERE node_type = 'collection' AND LOWER(title) = ?1 LIMIT 1",
                libsql::params![normalized],
            )
            .await
            .context("Failed to search for collection by name")?;

        if let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            self.get_node(&id).await
        } else {
            Ok(None)
        }
    }

    pub async fn get_collections_by_names(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Node>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }

        let normalized: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let placeholders: Vec<String> = (1..=normalized.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT id, title FROM node WHERE node_type = 'collection' AND LOWER(title) IN ({})",
            placeholders.join(", ")
        );

        let params: Vec<libsql::Value> = normalized
            .iter()
            .map(|n| libsql::Value::Text(n.clone()))
            .collect();
        let mut rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to batch search collections by names")?;

        let mut collections = HashMap::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            if let Ok(Some(node)) = self.get_node(&id).await {
                let key = title.unwrap_or_default().to_lowercase();
                collections.insert(key, node);
            }
        }

        Ok(collections)
    }

    pub async fn get_collection_members_recursive(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>> {
        // Get all collections in the subtree using WITH RECURSIVE.
        //
        // collection hierarchy is built from `member_of` edges (a
        // sub-collection is a member_of its parent), NOT `has_child` — the old
        // recursive arm followed `has_child` and so matched nothing, leaving
        // `coll_subtree` as just the seed and silently dropping every
        // sub-collection's members. member_of stores in_node = member/child,
        // out_node = collection/parent, so we descend parent→child by joining on
        // `r.out_node = cs.node_id` and taking `r.in_node`, restricted to
        // collection children (a content member isn't a sub-collection). A depth
        // cap bounds traversal in case a cycle slips in.
        let mut rows = self
            .db
            .query(
                r#"WITH RECURSIVE coll_subtree(node_id, depth) AS (
                SELECT ?1, 0
                UNION ALL
                SELECT r.in_node, cs.depth + 1 FROM relationship r
                JOIN coll_subtree cs ON r.out_node = cs.node_id
                JOIN node n ON n.id = r.in_node AND n.node_type = 'collection'
                WHERE r.relationship_type = 'member_of' AND cs.depth < 100
            )
            SELECT DISTINCT r.in_node FROM relationship r
            JOIN coll_subtree cs ON r.out_node = cs.node_id
            WHERE r.relationship_type = 'member_of'"#,
                libsql::params![collection_id.to_string()],
            )
            .await
            .context("Failed to get recursive collection members")?;

        let mut member_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            member_ids.push(row.get(0)?);
        }

        member_ids.sort();
        member_ids.dedup();
        Ok(member_ids)
    }

    pub async fn get_all_collection_names(&self) -> Result<Vec<String>> {
        let mut rows = self
            .db
            .query(
                "SELECT content FROM node WHERE node_type = 'collection' ORDER BY content ASC",
                (),
            )
            .await
            .context("Failed to get all collection names")?;

        let mut names = Vec::new();
        while let Some(row) = rows.next().await? {
            names.push(row.get(0)?);
        }
        Ok(names)
    }

    pub async fn get_all_collections_with_member_counts(
        &self,
    ) -> Result<Vec<(Node, usize, Vec<String>)>> {
        let collections = self.get_all_collections().await?;
        if collections.is_empty() {
            return Ok(vec![]);
        }

        // Count CONTENT members per collection. This count drives the sidebar's
        // member badge AND its empty-collection pruning, so it must match what the
        // collection viewer actually lists — user-authored content only. Counting
        // raw `member_of` edges instead let a collection whose only members are the
        // person roster (or other system nodes) report a non-zero count, show in the
        // sidebar, then open empty because the viewer filters those members out.
        //
        // The excluded types mirror the frontend's NON_CONTENT_NODE_TYPES
        // (collections.svelte.ts): `person`/`schema`/`database-settings` are
        // system/definition nodes, `collection` members are shown in the tree
        // itself, and `horizontal-line` is a decorative divider. Keep the two lists
        // in sync.
        let mut rows = self
            .db
            .query(
                "SELECT r.out_node, COUNT(*) \
             FROM relationship r \
             JOIN node n ON n.id = r.in_node \
             WHERE r.relationship_type = 'member_of' \
               AND n.node_type NOT IN \
                 ('schema', 'person', 'database-settings', 'collection', 'horizontal-line') \
             GROUP BY r.out_node",
                (),
            )
            .await
            .context("Failed to get member counts")?;

        let mut count_map: HashMap<String, usize> = HashMap::new();
        while let Some(row) = rows.next().await? {
            let coll_id: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            count_map.insert(coll_id, count as usize);
        }

        // Get collection-to-collection hierarchy edges
        let mut rows2 = self.db.query(
            "SELECT r.in_node, r.out_node FROM relationship r JOIN node n1 ON n1.id = r.in_node JOIN node n2 ON n2.id = r.out_node WHERE r.relationship_type = 'member_of' AND n1.node_type = 'collection' AND n2.node_type = 'collection'",
            (),
        ).await.context("Failed to get collection hierarchy")?;

        let mut parent_map: HashMap<String, Vec<String>> = HashMap::new();
        while let Some(row) = rows2.next().await? {
            let child: String = row.get(0)?;
            let parent: String = row.get(1)?;
            parent_map.entry(child).or_default().push(parent);
        }

        Ok(collections
            .into_iter()
            .map(|node| {
                let count = count_map.get(&node.id).copied().unwrap_or(0);
                let parents = parent_map.get(&node.id).cloned().unwrap_or_default();
                (node, count, parents)
            })
            .collect())
    }

    async fn get_all_collections(&self) -> Result<Vec<Node>> {
        self.query_nodes_from_sql(
            "SELECT * FROM node WHERE node_type = 'collection' ORDER BY content ASC",
            (),
        )
        .await
    }

    /// Bulk-create `has_child` edges (parent → child) in ONE transaction, for the
    /// batched reconnect edge sweep (issue #345). Each tuple is `(parent, child,
    /// order)` where `order` is the sender's sibling order; `get_children` sorts by
    /// `json_extract(properties, '$.order')` ASC, so a fresh parent's children
    /// reproduce that order exactly.
    ///
    /// Idempotent and safe for a from-scratch sweep: a child that ALREADY has a
    /// parent `has_child` edge is skipped (a node has at most one parent), so this
    /// only ever attaches genuinely-unparented children — it never re-parents.
    /// Direction matches `move_node`/`get_children`: `in_node = parent, out_node =
    /// child`. Returns the edges actually inserted, so the caller can emit one
    /// `RelationshipCreated` per edge.
    pub async fn bulk_create_has_child(
        &self,
        edges: &[(String, String, f64)],
    ) -> Result<Vec<(String, String, f64)>> {
        if edges.is_empty() {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk has_child transaction")?;

        let mut created = Vec::with_capacity(edges.len());
        for (parent, child, order) in edges {
            // A node has at most one parent — skip if this child is already parented
            // (idempotent re-run; never re-parent a child that landed via another path).
            let mut rows = tx
                .query(
                    "SELECT 1 FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child' LIMIT 1",
                    libsql::params![child.clone()],
                )
                .await
                .context("Failed to check existing parent edge")?;
            if rows.next().await?.is_some() {
                continue;
            }

            let rel_id = uuid::Uuid::new_v4().to_string();
            let props = serde_json::json!({ "order": order }).to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                libsql::params![rel_id, parent.clone(), child.clone(), props, now.clone(), now.clone()],
            )
            .await
            .context("Failed to insert has_child edge")?;
            created.push((parent.clone(), child.clone(), *order));
        }

        tx.commit()
            .await
            .context("Failed to commit bulk has_child")?;
        Ok(created)
    }

    /// Bulk-insert `member_of` edges, returning the edges actually created
    /// (skipping ones that already existed) as `(rel_id, node_id, collection_id,
    /// order)`. Callers that need cloud sync route through
    /// [`crate::services::NodeService::bulk_add_to_collections_notify`], which
    /// emits a `RelationshipCreated` event per returned edge — this raw store
    /// method emits nothing, so on its own the edges never push to cloud.
    pub async fn bulk_add_to_collections(
        &self,
        memberships: &[(String, String)],
    ) -> Result<Vec<(String, String, String, f64)>> {
        if memberships.is_empty() {
            return Ok(Vec::new());
        }

        let member_ids: Vec<&str> = memberships
            .iter()
            .map(|(node_id, _)| node_id.as_str())
            .collect();
        self.assert_root_only_membership(&member_ids).await?;

        let start = std::time::Instant::now();

        // Group by collection to calculate orders correctly
        let mut by_collection: HashMap<&str, Vec<&str>> = HashMap::new();
        for (node_id, collection_id) in memberships {
            by_collection
                .entry(collection_id.as_str())
                .or_default()
                .push(node_id.as_str());
        }

        let mut ordered: Vec<(String, String, f64)> = Vec::with_capacity(memberships.len());
        for (collection_id, node_ids) in &by_collection {
            let base_order = self.get_next_member_order(collection_id).await?;
            for (i, node_id) in node_ids.iter().enumerate() {
                ordered.push((
                    node_id.to_string(),
                    collection_id.to_string(),
                    base_order + i as f64,
                ));
            }
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk add transaction")?;

        let mut created: Vec<(String, String, String, f64)> = Vec::new();
        for (node_id, collection_id, order) in &ordered {
            let mut rows = tx.query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of' LIMIT 1",
                libsql::params![node_id.clone(), collection_id.clone()],
            ).await.context("Failed to check existing membership")?;

            if rows.next().await?.is_some() {
                continue;
            }

            let rel_id = uuid::Uuid::new_v4().to_string();
            let props = serde_json::json!({"order": order}).to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'member_of', ?4, 1, ?5, ?6)",
                libsql::params![rel_id.clone(), node_id.clone(), collection_id.clone(), props, now.clone(), now.clone()],
            ).await.context("Failed to insert membership")?;
            created.push((rel_id, node_id.clone(), collection_id.clone(), *order));
        }

        tx.commit().await.context("Failed to commit bulk add")?;

        tracing::debug!(
            "bulk_add_to_collections: {} memberships in {:?}",
            created.len(),
            start.elapsed()
        );

        Ok(created)
    }

    pub async fn bulk_create_mentions(&self, mentions: &[(String, String)]) -> Result<usize> {
        if mentions.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let candidate: Vec<_> = mentions.iter().filter(|(s, t)| s != t).collect();
        let candidate_len = candidate.len();
        if candidate_len == 0 {
            return Ok(0);
        }

        // a mention edge is FK-constrained — `relationship.in_node` /
        // `out_node` are `NOT NULL REFERENCES node(id)` with `PRAGMA foreign_keys
        // = ON` — so a mention to a NON-EXISTENT node (a dangling `[[link]]` to a
        // doc that wasn't imported, a typo, or an external ref) is an FK violation
        // on INSERT. Previously all mentions were inserted in ONE transaction with
        // `?`-propagation, so a single dangling link rolled back the WHOLE batch
        // and the import created ZERO cross-references. Pre-filter to pairs whose
        // BOTH endpoints exist; dangling links are skipped (and logged), never
        // fatal.
        let mut endpoints: std::collections::HashSet<String> = std::collections::HashSet::new();
        for m in &candidate {
            endpoints.insert(m.0.clone());
            endpoints.insert(m.1.clone());
        }
        let endpoint_ids: Vec<String> = endpoints.into_iter().collect();
        // Chunk the `IN (...)` under SQLite's ~999 bound-parameter ceiling.
        const ID_CHUNK: usize = 900;
        let mut existing: std::collections::HashSet<String> = std::collections::HashSet::new();
        for chunk in endpoint_ids.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT id FROM node WHERE id IN ({})",
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
                .context("Failed to check mention endpoints")?;
            while let Some(row) = rows.next().await? {
                let id: String = row.get(0)?;
                existing.insert(id);
            }
        }

        let valid_mentions: Vec<_> = candidate
            .into_iter()
            .filter(|m| existing.contains(m.0.as_str()) && existing.contains(m.1.as_str()))
            .collect();
        let skipped = candidate_len - valid_mentions.len();
        if skipped > 0 {
            tracing::warn!(
                "bulk_create_mentions: skipped {} mention(s) with a missing endpoint node (dangling [[link]]); keeping {} valid",
                skipped,
                valid_mentions.len()
            );
        }
        if valid_mentions.is_empty() {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk mentions transaction")?;

        let mut created = 0;
        for (source_id, target_id) in &valid_mentions {
            let mut rows = tx.query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions' LIMIT 1",
                libsql::params![source_id.clone().to_string(), target_id.clone().to_string()],
            ).await.context("Failed to check existing mention")?;

            if rows.next().await?.is_some() {
                continue;
            }

            let rel_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'mentions', '{}', 1, ?4, ?5)",
                libsql::params![rel_id, source_id.clone().to_string(), target_id.clone().to_string(), now.clone(), now.clone()],
            ).await.context("Failed to insert mention")?;
            created += 1;
        }

        tx.commit()
            .await
            .context("Failed to commit bulk mentions")?;

        tracing::debug!(
            "bulk_create_mentions: {} mentions in {:?}",
            created,
            start.elapsed()
        );

        Ok(created)
    }

    pub async fn check_relationship_exists(&self, source_id: &str, rel_type: &str) -> Result<i64> {
        let mut rows = self.db.query(
            "SELECT COUNT(*) as cnt FROM relationship WHERE in_node = ?1 AND relationship_type = ?2",
            libsql::params![source_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to check relationship existence")?;
        let row = rows
            .next()
            .await
            .context("No row returned")?
            .ok_or_else(|| anyhow::anyhow!("Empty result for relationship count"))?;
        Ok(row.get::<i64>(0).unwrap_or(0))
    }

    pub async fn relationship_exists(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<bool> {
        let mut rows = self.db.query(
            "SELECT 1 FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3 LIMIT 1",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to check relationship existence")?;
        Ok(rows.next().await?.is_some())
    }

    pub async fn create_generic_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: &serde_json::Value,
    ) -> Result<String> {
        // ADR-059 §2 applies to a `member_of` edge no matter which API builds it.
        // This generic path carries `member_of` whenever a caller supplies an
        // explicit `order` — `NodeService::create_relationship`'s non-auto-order
        // fork, the playbook `add_relationship` action, and the CLI
        // `relationship create --edge-data` — so it must be gated too. (Auto-order
        // `member_of` goes through `add_to_collection` instead; both are guarded.)
        if rel_type == "member_of" {
            self.assert_root_only_membership(&[source_id]).await?;
        }
        let now = chrono::Utc::now().to_rfc3339();
        let rel_id = uuid::Uuid::new_v4().to_string();
        let props_json = serde_json::to_string(properties).unwrap_or_else(|_| "{}".to_string());
        self.db.execute(
            "INSERT OR IGNORE INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
            libsql::params![rel_id.clone(), source_id.to_string(), target_id.to_string(), rel_type.to_string(), props_json, now.clone(), now],
        ).await.context("Failed to create generic relationship")?;
        Ok(rel_id)
    }

    pub async fn get_relationship_id(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3 LIMIT 1",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to get relationship ID")?;
        if let Some(row) = rows.next().await? {
            Ok(Some(row.get::<String>(0)?))
        } else {
            Ok(None)
        }
    }

    /// Fetch a single relationship edge (with its properties) by endpoints and type.
    ///
    /// Complements `get_relationship_id` when the caller needs the edge's stored
    /// properties (e.g. the `role`/`status` carried on a `has_role` edge), not just
    /// its id. Returns `None` when no such edge exists.
    pub async fn get_relationship_record(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<Option<RelationshipRecord>> {
        let mut rows = self.db.query(
            "SELECT id, in_node, out_node, relationship_type, properties FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3 LIMIT 1",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to get relationship record")?;
        if let Some(row) = rows.next().await? {
            Ok(Some(Self::row_to_relationship(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_generic_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<()> {
        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to delete relationship")?;
        Ok(())
    }

    pub async fn get_nodes_by_relationship(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<Vec<Node>> {
        let sql = match direction {
            "out" => "SELECT n.* FROM node n JOIN relationship r ON r.out_node = n.id WHERE r.in_node = ?1 AND r.relationship_type = ?2",
            "in" => "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = ?2",
            _ => return Err(anyhow::anyhow!("Invalid direction: {}", direction)),
        };
        let mut rows = self
            .db
            .query(
                sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to get related nodes")?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_relationship_orders(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<Vec<Option<f64>>> {
        let filter_col = if direction == "in_node" {
            "in_node"
        } else {
            "out_node"
        };
        let sql = format!(
            "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE {} = ?1 AND relationship_type = ?2 ORDER BY json_extract(properties, '$.order') ASC",
            filter_col
        );
        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to get relationship orders")?;
        let mut orders = Vec::new();
        while let Some(row) = rows.next().await? {
            let order: Option<f64> = row.get(0).ok();
            orders.push(order);
        }
        Ok(orders)
    }

    pub async fn get_relationship_count(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<usize> {
        let filter_col = if direction == "in_node" {
            "in_node"
        } else {
            "out_node"
        };
        let sql = format!(
            "SELECT COUNT(*) FROM relationship WHERE {} = ?1 AND relationship_type = ?2",
            filter_col
        );
        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to count relationships")?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No count result"))?;
        Ok(row.get::<i64>(0).unwrap_or(0) as usize)
    }
}
