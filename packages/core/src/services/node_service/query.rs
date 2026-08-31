//! Query operations for NodeService.

use super::*;

impl NodeService {
    /// Query nodes with filtering
    ///
    /// Executes a filtered query using NodeFilter.
    ///
    /// # Arguments
    ///
    /// * `filter` - The filter criteria
    ///
    /// # Returns
    ///
    /// Vector of matching nodes
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use nodespace_core::models::NodeFilter;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// let filter = NodeFilter::new()
    ///     .with_node_type("task".to_string())
    ///     .with_limit(10);
    /// let nodes = service.query_nodes(filter).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_nodes(&self, filter: NodeFilter) -> Result<Vec<Node>, NodeServiceError> {
        // When property filters are present, fetch all matching rows from DB and
        // filter in memory. Safety cap prevents accidental OOM on large datasets.
        const PROPERTY_FILTER_FETCH_CAP: usize = 10_000;
        let (db_limit, db_offset) = if filter.property_filters.is_some() {
            (Some(PROPERTY_FILTER_FETCH_CAP), None)
        } else {
            (filter.limit, filter.offset)
        };

        // Convert NodeFilter to NodeQuery. order_by is forwarded through so the
        // store applies it in SQL (ORDER BY before LIMIT/OFFSET) rather than
        // relying on in-memory sorting that never actually happened.
        let query = crate::models::NodeQuery {
            id: None,
            ids: filter.ids.clone(),
            node_type: filter.node_type.clone(),
            content_contains: filter.content_contains.clone(),
            title_contains: filter.title_contains.clone(),
            mentioned_by: None,
            order_by: filter.order_by.clone(),
            limit: db_limit,
            offset: db_offset,
        };

        let nodes = self
            .store
            .query_nodes(query)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // OPTIMIZATION: Pre-fetch schemas for all unique node types in the result set.
        // This avoids N*2 database calls (one per node for backfill + one for migration).
        // Instead, we do at most K calls where K = number of unique node types.
        let unique_types: std::collections::HashSet<&str> =
            nodes.iter().map(|n| n.node_type.as_str()).collect();

        let mut schema_cache: std::collections::HashMap<String, Option<serde_json::Value>> =
            std::collections::HashMap::new();
        for node_type in unique_types {
            let schema = self.get_schema_for_type(node_type).await?;
            schema_cache.insert(node_type.to_string(), schema);
        }

        // Apply migrations using cached schemas
        let mut migrated_nodes = Vec::new();
        for mut node in nodes {
            self.backfill_schema_version_with_cache(&mut node, &schema_cache);
            self.apply_lazy_migration_with_cache(&mut node, &schema_cache)
                .await?;
            migrated_nodes.push(node);
        }

        // Apply property filters in-memory if present
        let result_nodes = if let Some(ref property_filters) = filter.property_filters {
            let mut filtered = Self::apply_property_filters(migrated_nodes, property_filters);
            // Apply offset in memory
            if let Some(offset) = filter.offset {
                if offset < filtered.len() {
                    filtered = filtered.split_off(offset);
                } else {
                    filtered.clear();
                }
            }
            // Apply limit in memory
            if let Some(limit) = filter.limit {
                filtered.truncate(limit);
            }
            filtered
        } else {
            migrated_nodes
        };

        Ok(result_nodes)
    }

    /// Apply property filters in-memory to a list of nodes.
    ///
    /// Properties are stored in namespaced format: `{ "task": { "status": "open" } }`.
    /// PropertyFilter paths use JSONPath: `"$.status"`.
    /// This resolves the path against each node's type namespace.
    fn apply_property_filters(nodes: Vec<Node>, filters: &[PropertyFilter]) -> Vec<Node> {
        nodes
            .into_iter()
            .filter(|node| {
                filters
                    .iter()
                    .all(|f| Self::node_matches_property_filter(node, f))
            })
            .collect()
    }

    /// Check if a single node matches a single property filter.
    fn node_matches_property_filter(node: &Node, filter: &PropertyFilter) -> bool {
        // Extract property path from JSONPath "$.field" or "$.field.subfield"
        // PropertyFilter::new() validates the "$." prefix, so strip_prefix should always succeed.
        let path = match filter.path.strip_prefix("$.") {
            Some(p) => p,
            None => {
                tracing::warn!(
                    "PropertyFilter path '{}' missing expected '$.' prefix — skipping filter",
                    filter.path
                );
                return false;
            }
        };
        let segments: Vec<&str> = path.split('.').collect();

        // Resolve value from namespaced properties: properties[node_type][field...]
        let mut current = node.properties.get(&node.node_type);
        for segment in &segments {
            current = current.and_then(|v| v.get(*segment));
        }

        let Some(actual_value) = current else {
            return false; // Property not found = doesn't match
        };

        match &filter.operator {
            FilterOperator::Equals => actual_value == &filter.value,
            FilterOperator::NotEquals => actual_value != &filter.value,
            FilterOperator::Contains => match (actual_value.as_str(), filter.value.as_str()) {
                (Some(actual), Some(expected)) => {
                    actual.to_lowercase().contains(&expected.to_lowercase())
                }
                _ => false,
            },
            FilterOperator::StartsWith => match (actual_value.as_str(), filter.value.as_str()) {
                (Some(actual), Some(expected)) => {
                    actual.to_lowercase().starts_with(&expected.to_lowercase())
                }
                _ => false,
            },
            FilterOperator::EndsWith => match (actual_value.as_str(), filter.value.as_str()) {
                (Some(actual), Some(expected)) => {
                    actual.to_lowercase().ends_with(&expected.to_lowercase())
                }
                _ => false,
            },
            FilterOperator::GreaterThan => {
                Self::compare_property_values(actual_value, &filter.value)
                    == Some(std::cmp::Ordering::Greater)
            }
            FilterOperator::GreaterThanOrEqual => {
                matches!(
                    Self::compare_property_values(actual_value, &filter.value),
                    Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                )
            }
            FilterOperator::LessThan => {
                Self::compare_property_values(actual_value, &filter.value)
                    == Some(std::cmp::Ordering::Less)
            }
            FilterOperator::LessThanOrEqual => {
                matches!(
                    Self::compare_property_values(actual_value, &filter.value),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            }
        }
    }

    /// Compare two JSON values for ordering (used by GT/LT operators)
    fn compare_property_values(
        a: &serde_json::Value,
        b: &serde_json::Value,
    ) -> Option<std::cmp::Ordering> {
        match (a, b) {
            (serde_json::Value::Number(na), serde_json::Value::Number(nb)) => {
                let fa = na.as_f64()?;
                let fb = nb.as_f64()?;
                fa.partial_cmp(&fb)
            }
            (serde_json::Value::String(sa), serde_json::Value::String(sb)) => Some(sa.cmp(sb)),
            (serde_json::Value::Bool(ba), serde_json::Value::Bool(bb)) => Some(ba.cmp(bb)),
            _ => None,
        }
    }

    /// Query nodes with simple query parameters
    ///
    /// This is a simpler alternative to `query_nodes` for common query patterns.
    /// Supports queries by ID, mentioned_by, content_contains, and node_type.
    ///
    /// # Arguments
    ///
    /// * `query` - Query parameters (see NodeQuery for details)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Node>)` - List of matching nodes
    /// * `Err(NodeServiceError)` - If database operation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::models::NodeQuery;
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Query by ID
    /// let query = NodeQuery::by_id("node-123".to_string());
    /// let nodes = service.query_nodes_simple(query).await?;
    ///
    /// // Query nodes that mention another node
    /// let query = NodeQuery::mentioned_by("target-node".to_string());
    /// let nodes = service.query_nodes_simple(query).await?;
    ///
    /// // Full-text search
    /// let query = NodeQuery::content_contains("search term".to_string()).with_limit(10);
    /// let nodes = service.query_nodes_simple(query).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Query Priority Order
    ///
    /// Queries are evaluated in the following priority order:
    /// 1. `id` - Direct node lookup (exact match)
    /// 2. `mentioned_by` - Nodes that reference the specified node
    /// 3. `content_contains` + optional `node_type` - Full-text content search
    /// 4. `node_type` - Filter by node type
    /// 5. Empty query - Returns empty vec (safer than returning all nodes)
    ///
    /// # Note on Empty Queries
    ///
    /// Queries with no parameters (all fields `None` or `false`) will return an empty vector.
    /// This is intentional to prevent accidentally fetching all nodes from the database.
    ///
    /// # Default Limit
    ///
    /// If no limit is specified in the query, a default limit of [`DEFAULT_QUERY_LIMIT`] (100)
    /// is applied to prevent unbounded queries and potential performance issues.
    /// Callers can override this by explicitly setting a limit via `query.with_limit(n)`.
    pub async fn query_nodes_simple(
        &self,
        query: crate::models::NodeQuery,
    ) -> Result<Vec<Node>, NodeServiceError> {
        // Direct delegation to store.query_nodes for simple queries
        // Complex filtering handled by the SQLite query engine
        tracing::debug!("query_nodes_simple: Delegating to store.query_nodes");

        // Priority 1: Query by ID (exact match)
        if let Some(ref id) = query.id {
            if let Some(node) = self.get_node(id).await? {
                return Ok(vec![node]);
            } else {
                return Ok(vec![]);
            }
        }

        // Apply default limit if not specified to prevent unbounded queries
        let query = if query.limit.is_none() {
            query.with_limit(DEFAULT_QUERY_LIMIT)
        } else {
            query
        };

        // Priority 2+: Delegate to store.query_nodes
        // Complex query features (mentioned_by, content_contains, filters) delegated to store
        let nodes = self
            .store
            .query_nodes(query)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Apply migrations to results
        let mut migrated_nodes = Vec::new();
        for mut node in nodes {
            self.backfill_schema_version(&mut node).await?;
            self.apply_lazy_migration(&mut node).await?;
            migrated_nodes.push(node);
        }

        Ok(migrated_nodes)
    }
}

impl NodeService {
    /// Search nodes for mention autocomplete with proper filtering
    pub async fn mention_autocomplete(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Node>, NodeServiceError> {
        self.store
            .mention_autocomplete(query, limit.map(|l| l as i64))
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }
}
