//! MCP Search Handlers
//!
//! Semantic search operations for AI agent access.
//! Pure business logic - no Tauri dependencies.

use crate::mcp::types::MCPError;
use crate::services::NodeEmbeddingService;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Parameters for search_containers method
#[derive(Debug, Deserialize)]
pub struct SearchContainersParams {
    /// Natural language search query
    pub query: String,

    /// Similarity threshold (0.0-1.0, lower = more similar)
    /// Default: 0.7
    #[serde(default)]
    pub threshold: Option<f32>,

    /// Maximum number of results
    /// Default: 20
    #[serde(default)]
    pub limit: Option<usize>,

    /// Use exact search instead of approximate (slower but more accurate)
    /// Default: false (use DiskANN approximate search)
    #[serde(default)]
    pub exact: Option<bool>,
}

/// Search containers by semantic similarity
///
/// Uses vector embeddings to find containers whose content is semantically
/// similar to the query. This enables AI agents to discover relevant content
/// using natural language instead of knowing exact IDs.
///
/// # Example
///
/// ```rust,no_run
/// let params = json!({
///     "query": "Q4 planning and budget",
///     "threshold": 0.7,
///     "limit": 10
/// });
/// let result = handle_search_containers(&embedding_service, params).await?;
/// // Returns top 10 most relevant containers
/// ```
pub async fn handle_search_containers(
    service: &Arc<NodeEmbeddingService>,
    params: Value,
) -> Result<Value, MCPError> {
    // Parse parameters
    let params: SearchContainersParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Apply defaults
    let threshold = params.threshold.unwrap_or(0.7);
    let limit = params.limit.unwrap_or(20);
    let exact = params.exact.unwrap_or(false);

    // Validate parameters
    if !(0.0..=1.0).contains(&threshold) {
        return Err(MCPError::invalid_params(
            "threshold must be between 0.0 and 1.0".to_string(),
        ));
    }

    if params.query.is_empty() {
        return Err(MCPError::invalid_params(
            "query cannot be empty".to_string(),
        ));
    }

    // Execute search using existing NodeEmbeddingService methods
    let results = if exact {
        service
            .exact_search_containers(&params.query, threshold, limit)
            .await
    } else {
        service
            .search_containers(&params.query, threshold, limit)
            .await
    };

    let nodes = results.map_err(|e| MCPError::internal_error(format!("Search failed: {}", e)))?;

    // Return results with metadata
    Ok(json!({
        "nodes": nodes,
        "count": nodes.len(),
        "query": params.query,
        "threshold": threshold,
        "exact": exact
    }))
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_search_containers_basic() {
        // This test will be implemented when we have a proper test setup for NodeEmbeddingService
        // For now, we verify parameter parsing works
        let params = json!({
            "query": "machine learning"
        });

        let search_params: Result<SearchContainersParams, _> = serde_json::from_value(params);
        assert!(search_params.is_ok());

        let p = search_params.unwrap();
        assert_eq!(p.query, "machine learning");
        assert_eq!(p.threshold, None);
        assert_eq!(p.limit, None);
        assert_eq!(p.exact, None);
    }

    #[tokio::test]
    async fn test_search_containers_custom_params() {
        let params = json!({
            "query": "project planning",
            "threshold": 0.6,
            "limit": 5,
            "exact": true
        });

        let search_params: Result<SearchContainersParams, _> = serde_json::from_value(params);
        assert!(search_params.is_ok());

        let p = search_params.unwrap();
        assert_eq!(p.query, "project planning");
        assert_eq!(p.threshold, Some(0.6));
        assert_eq!(p.limit, Some(5));
        assert_eq!(p.exact, Some(true));
    }

    #[tokio::test]
    async fn test_search_containers_defaults() {
        let params = json!({
            "query": "test query"
        });

        let search_params: Result<SearchContainersParams, _> = serde_json::from_value(params);
        assert!(search_params.is_ok());

        let p = search_params.unwrap();
        assert_eq!(p.query, "test query");
        assert_eq!(p.threshold, None); // Will default to 0.7
        assert_eq!(p.limit, None); // Will default to 20
        assert_eq!(p.exact, None); // Will default to false
    }

    #[test]
    fn test_search_containers_threshold_validation_low() {
        // Threshold < 0 should be caught by validation
        let threshold = -0.1;
        assert!(!(0.0..=1.0).contains(&threshold));
    }

    #[test]
    fn test_search_containers_threshold_validation_high() {
        // Threshold > 1 should be caught by validation
        let threshold = 1.5;
        assert!(!(0.0..=1.0).contains(&threshold));
    }

    #[test]
    fn test_search_containers_threshold_validation_valid() {
        // Valid thresholds should pass
        assert!((0.0..=1.0).contains(&0.0));
        assert!((0.0..=1.0).contains(&0.5));
        assert!((0.0..=1.0).contains(&1.0));
    }
}
