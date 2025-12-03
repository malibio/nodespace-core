//! Query Service - Query Execution with SQL Translation
//!
//! This module provides query execution functionality for QueryNode, translating
//! structured query definitions to SurrealQL and executing against the hub-and-spoke
//! database architecture.
//!
//! # Architecture
//!
//! - **Hub-and-Spoke**: Queries target hub `node` table with type filters
//! - **Record Links**: Type-specific properties accessed via `data.property`
//! - **Graph Edges**: Relationships traversed via `->has_child->`, `->mentions->`
//! - **FETCH data**: All queries include FETCH to hydrate spoke properties
//!
//! # Query Pattern Examples
//!
//! - Type filter: `SELECT * FROM node WHERE node_type = 'task'`
//! - Property filter: `SELECT * FROM node WHERE data.status = 'open' FETCH data`
//! - Relationship: `SELECT * FROM node WHERE id IN (SELECT VALUE out FROM has_child WHERE in = node:⟨parent⟩)`
//!
//! # Examples
//!
//! ```rust,no_run
//! use nodespace_core::services::QueryService;
//! use nodespace_core::db::SurrealStore;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let store = Arc::new(SurrealStore::new("./data/db".into()).await?);
//! let query_service = QueryService::new(store);
//!
//! let query = QueryDefinition {
//!     target_type: "task".to_string(),
//!     filters: vec![],
//!     sorting: None,
//!     limit: Some(50),
//! };
//!
//! let results = query_service.execute(&query).await?;
//! # Ok(())
//! # }
//! ```

use crate::db::SurrealStore;
use crate::models::Node;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Structured query definition matching QueryNode spoke fields
///
/// This struct matches the TypeScript QueryNode interface from
/// `packages/desktop-app/src/lib/types/query.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryDefinition {
    /// Target node type: 'task', 'text', 'date', or '*' for all types
    pub target_type: String,
    /// Filter conditions to apply
    pub filters: Vec<QueryFilter>,
    /// Optional sorting configuration
    pub sorting: Option<Vec<SortConfig>>,
    /// Optional result limit (default: 50)
    pub limit: Option<usize>,
}

/// Individual filter condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryFilter {
    /// Filter category
    #[serde(rename = "type")]
    pub filter_type: String,
    /// Comparison operator
    pub operator: String,
    /// Property key for property filters
    pub property: Option<String>,
    /// Expected value
    pub value: Option<serde_json::Value>,
    /// Case sensitivity for text comparisons
    pub case_sensitive: Option<bool>,
    /// Relationship type for relationship filters
    pub relationship_type: Option<String>,
    /// Target node ID for relationship filters
    pub node_id: Option<String>,
}

/// Sorting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortConfig {
    /// Property or field to sort by
    pub field: String,
    /// Sort direction
    pub direction: String,
}

/// Service for executing queries against the database
pub struct QueryService {
    store: Arc<SurrealStore>,
}

impl QueryService {
    /// Create a new QueryService
    pub fn new(store: Arc<SurrealStore>) -> Self {
        Self { store }
    }

    /// Execute a query and return matching nodes
    ///
    /// Translates the QueryDefinition to SurrealQL and executes against the database.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Query building fails (invalid filter syntax)
    /// - Database query execution fails
    /// - Result deserialization fails
    pub async fn execute(&self, query: &QueryDefinition) -> Result<Vec<Node>> {
        let sql = self.build_query(query)?;

        // Execute query to get basic node data (without FETCH to avoid Thing deserialization)
        // Use SurrealStore's internal query_nodes for proper handling
        // For now we'll use a simple direct query and manually fetch properties

        // Get node IDs that match the query
        let node_ids = self.execute_query_for_ids(&sql).await?;

        // If no results, return empty vector
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch full nodes using the store's get_node method
        let mut nodes = Vec::new();
        for id in node_ids {
            if let Some(node) = self.store.get_node(&id).await? {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Execute query and return matching node IDs
    async fn execute_query_for_ids(&self, sql: &str) -> Result<Vec<String>> {
        // Keep SELECT * for proper sorting, but extract only IDs from results
        // This avoids "Missing order idiom" errors with SELECT VALUE id

        let mut response = self
            .store
            .db()
            .query(sql)
            .await
            .context("Failed to execute ID query")?;

        // Extract records with Thing IDs
        use serde::{Deserialize, Serialize};
        use surrealdb::sql::Thing;

        #[derive(Debug, Serialize, Deserialize)]
        struct IdRecord {
            id: Thing,
        }

        let records: Vec<IdRecord> = response
            .take(0)
            .context("Failed to extract IDs from query")?;

        // Convert Thing to String (extract UUID part)
        let ids: Vec<String> = records
            .into_iter()
            .filter_map(|record| {
                if let surrealdb::sql::Id::String(id) = record.id.id {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        Ok(ids)
    }

    /// Translate QueryDefinition to SurrealQL
    ///
    /// Builds a complete SELECT query with WHERE clause, ORDER BY, and LIMIT.
    /// All queries include `FETCH data` to hydrate type-specific properties.
    fn build_query(&self, query: &QueryDefinition) -> Result<String> {
        let mut sql = String::from("SELECT * FROM node");
        let mut conditions = Vec::new();

        // Type filter (using node_type field)
        if query.target_type != "*" {
            conditions.push(format!(
                "node_type = '{}'",
                self.escape_string(&query.target_type)
            ));
        }

        // Build filter conditions
        for filter in &query.filters {
            match filter.filter_type.as_str() {
                "property" => conditions.push(self.build_property_filter(filter)?),
                "content" => conditions.push(self.build_content_filter(filter)?),
                "relationship" => conditions.push(self.build_relationship_filter(filter)?),
                "metadata" => conditions.push(self.build_metadata_filter(filter)?),
                _ => anyhow::bail!("Unknown filter type: {}", filter.filter_type),
            }
        }

        // Apply WHERE clause
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        // Add sorting
        if let Some(sorting) = &query.sorting {
            if !sorting.is_empty() {
                sql.push_str(" ORDER BY ");
                let clauses: Vec<String> = sorting
                    .iter()
                    .map(|s| {
                        format!(
                            "{} {}",
                            self.resolve_field(&s.field),
                            s.direction.to_uppercase()
                        )
                    })
                    .collect();
                sql.push_str(&clauses.join(", "));
            }
        }

        // Add limit
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        // Note: We don't use FETCH data because it causes Thing deserialization issues
        // Properties are fetched separately using get_node
        sql.push(';');

        Ok(sql)
    }

    /// Resolve field name to hub or spoke reference
    ///
    /// Metadata fields (created_at, modified_at, content, node_type) are on hub table.
    /// Type-specific fields are accessed via data link (data.field_name).
    fn resolve_field(&self, field: &str) -> String {
        // Metadata fields are on hub table
        if ["created_at", "modified_at", "content", "node_type"].contains(&field) {
            field.to_string()
        } else {
            // Type-specific fields accessed via data link
            format!("data.{}", field)
        }
    }

    /// Build property filter condition
    ///
    /// Property filters target type-specific fields via record link (`data.property`).
    fn build_property_filter(&self, filter: &QueryFilter) -> Result<String> {
        let property = filter
            .property
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Property filter missing 'property' field"))?;

        let field = format!("data.{}", property);

        match filter.operator.as_str() {
            "equals" => {
                let value = self.format_value(filter.value.as_ref())?;
                Ok(format!("{} = {}", field, value))
            }
            "contains" => {
                let value = filter
                    .value
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Contains requires string value"))?;
                if filter.case_sensitive.unwrap_or(true) {
                    Ok(format!(
                        "{} CONTAINS '{}'",
                        field,
                        self.escape_string(value)
                    ))
                } else {
                    Ok(format!(
                        "string::lowercase({}) CONTAINS string::lowercase('{}')",
                        field,
                        self.escape_string(value)
                    ))
                }
            }
            "gt" | "lt" | "gte" | "lte" => {
                let op = match filter.operator.as_str() {
                    "gt" => ">",
                    "lt" => "<",
                    "gte" => ">=",
                    "lte" => "<=",
                    _ => unreachable!(),
                };
                let value = self.format_value(filter.value.as_ref())?;
                Ok(format!("{} {} {}", field, op, value))
            }
            "in" => {
                let values = filter
                    .value
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| anyhow::anyhow!("In requires array value"))?;
                let list: Vec<String> = values
                    .iter()
                    .map(|v| self.format_value(Some(v)))
                    .collect::<Result<_>>()?;
                Ok(format!("{} IN [{}]", field, list.join(", ")))
            }
            "exists" => Ok(format!("{} IS NOT NONE", field)),
            _ => anyhow::bail!("Unsupported operator: {}", filter.operator),
        }
    }

    /// Build content filter condition
    ///
    /// Content filters target the hub table's content field.
    fn build_content_filter(&self, filter: &QueryFilter) -> Result<String> {
        let value = filter
            .value
            .as_ref()
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Content filter requires string value"))?;

        match filter.operator.as_str() {
            "contains" => {
                if filter.case_sensitive.unwrap_or(true) {
                    Ok(format!("content CONTAINS '{}'", self.escape_string(value)))
                } else {
                    Ok(format!(
                        "string::lowercase(content) CONTAINS string::lowercase('{}')",
                        self.escape_string(value)
                    ))
                }
            }
            "equals" => Ok(format!("content = '{}'", self.escape_string(value))),
            _ => anyhow::bail!("Unsupported content operator: {}", filter.operator),
        }
    }

    /// Build relationship filter condition
    ///
    /// Relationship filters use graph edge queries to traverse has_child and mentions edges.
    fn build_relationship_filter(&self, filter: &QueryFilter) -> Result<String> {
        let rel_type = filter
            .relationship_type
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing relationshipType"))?;
        let node_id = filter
            .node_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing nodeId"))?;

        match rel_type.as_str() {
            "children" => Ok(format!(
                "id IN (SELECT VALUE out FROM has_child WHERE in = node:⟨{}⟩)",
                self.escape_string(node_id)
            )),
            "parent" => Ok(format!(
                "id IN (SELECT VALUE in FROM has_child WHERE out = node:⟨{}⟩)",
                self.escape_string(node_id)
            )),
            "mentions" => Ok(format!(
                "id IN (SELECT VALUE out FROM mentions WHERE in = node:⟨{}⟩)",
                self.escape_string(node_id)
            )),
            "mentioned_by" => Ok(format!(
                "id IN (SELECT VALUE in FROM mentions WHERE out = node:⟨{}⟩)",
                self.escape_string(node_id)
            )),
            _ => anyhow::bail!("Unsupported relationship type: {}", rel_type),
        }
    }

    /// Build metadata filter condition
    ///
    /// Metadata filters target hub table fields (created_at, modified_at, node_type, content).
    fn build_metadata_filter(&self, filter: &QueryFilter) -> Result<String> {
        let property = filter
            .property
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Metadata filter missing property"))?;

        if !["created_at", "modified_at", "node_type", "content"].contains(&property.as_str()) {
            anyhow::bail!("Invalid metadata field: {}", property);
        }

        let op = match filter.operator.as_str() {
            "equals" => "=",
            "gt" => ">",
            "lt" => "<",
            "gte" => ">=",
            "lte" => "<=",
            _ => anyhow::bail!("Unsupported metadata operator: {}", filter.operator),
        };

        let value = self.format_value(filter.value.as_ref())?;
        Ok(format!("{} {} {}", property, op, value))
    }

    /// Format a JSON value for SQL
    fn format_value(&self, value: Option<&serde_json::Value>) -> Result<String> {
        match value {
            Some(serde_json::Value::String(s)) => Ok(format!("'{}'", self.escape_string(s))),
            Some(serde_json::Value::Number(n)) => Ok(n.to_string()),
            Some(serde_json::Value::Bool(b)) => Ok(b.to_string()),
            Some(serde_json::Value::Null) => Ok("NONE".to_string()),
            Some(v) => anyhow::bail!("Unsupported value type: {:?}", v),
            None => anyhow::bail!("Missing value"),
        }
    }

    /// Escape single quotes in strings for SQL safety
    fn escape_string(&self, s: &str) -> String {
        s.replace('\'', "\\'")
    }
}

#[cfg(test)]
mod query_service_test;
