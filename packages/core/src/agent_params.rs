//! Shared parameter types for agent tool execution.
//!
//! These structs deserialize the JSON arguments the local agent passes to its
//! search tools. They previously lived under `mcp::params` (re-exported from the
//! MCP search handler); they moved here when the MCP transport was deleted, since
//! the agent tool executor is now their only consumer.

use serde::Deserialize;
use std::collections::HashMap;

/// Parameters for the `search_nodes` (keyword/title search) tool.
#[derive(Debug, Deserialize)]
pub struct SearchNodesParams {
    /// Keyword or phrase to search for in node titles. Pass an empty string to
    /// skip the title filter (useful when filtering only by node_type or filters).
    pub query: String,

    /// Filter by node type (e.g., "task", "text").
    #[serde(default)]
    pub node_type: Option<String>,

    /// Maximum number of results. Default: 10.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Property filters as key-value pairs matched with equals.
    /// e.g. {"status": "open"} or {"company": "Acme"}.
    #[serde(default)]
    pub filters: Option<HashMap<String, String>>,
}

/// Parameters for the `search_semantic` tool.
#[derive(Debug, Deserialize)]
pub struct SearchSemanticParams {
    /// Natural language search query.
    pub query: String,

    /// Minimum similarity threshold (0.0-1.0, higher = stricter filter).
    /// Results must have similarity > threshold to be included. Default: 0.7.
    #[serde(default)]
    pub threshold: Option<f32>,

    /// Maximum number of results. Default: 20.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Filter by collection ID - returns only results from this collection.
    #[serde(default)]
    pub collection_id: Option<String>,

    /// Filter by collection path (e.g., "hr:policy") - resolves path to collection ID.
    #[serde(default)]
    pub collection: Option<String>,

    /// Exclude results from these collections (by path, e.g., ["archived", "drafts"]).
    /// Results in any of these collections will be filtered out.
    #[serde(default)]
    pub exclude_collections: Option<Vec<String>>,

    /// Number of top results to include full markdown content for (0-5).
    /// This saves AI agents from needing to call get_markdown_from_node_id separately.
    /// Default: 1 (include markdown for top result only). Set to 0 to disable, max 5.
    #[serde(default)]
    pub include_markdown: Option<usize>,

    /// Include archived nodes in search results (default: false).
    /// By default, search only returns active nodes. Nodes with
    /// lifecycle_status = "deleted" are never included.
    #[serde(default)]
    pub include_archived: Option<bool>,

    /// Search scope - controls which node types are included (Issue #1018).
    /// Values: "knowledge" (default), "conversations", "everything".
    #[serde(default)]
    pub scope: Option<String>,

    /// Filter by specific node types (e.g., ["task", "text"]).
    /// If set, only nodes whose node_type is in this list will be included.
    #[serde(default)]
    pub node_types: Option<Vec<String>>,

    /// Filter by node properties (key-value pairs).
    /// If set, only nodes whose properties contain all specified key-value pairs
    /// will be included. Multiple filters are combined with AND logic.
    #[serde(default)]
    pub property_filters: Option<serde_json::Value>,

    /// When true, attach outgoing relationships of each result node as an "edges" array.
    /// Each edge entry has: {"relationship": "...", "target_id": "...", "target_title": "..."}.
    /// Default: false (no edge data included).
    #[serde(default)]
    pub include_edges: Option<bool>,

    /// When true, re-rank results by blending vector similarity with graph connectivity degree.
    /// Blending formula: combined_score = 0.7 * similarity + 0.3 * normalized_degree.
    /// Surfaces well-connected, central knowledge nodes over isolated but textually similar ones.
    /// Default: false (pure similarity ranking).
    #[serde(default)]
    pub graph_boost: Option<bool>,
}
