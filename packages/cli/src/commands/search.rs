//! `nodespace search <query>` — semantic search via NodeService.SearchNodes.
//!
//! This wraps `SearchRequest` (the `NodeService.SearchNodes` RPC), not the
//! richer `search_semantic` local-agent tool (which calls the separate
//! `EmbeddingsService.SearchSemantic` RPC via `SearchSemanticInput`).
//! `SearchRequest` exposes `collection`/`collection_id`/`filters`/`threshold`,
//! all wired here. It does NOT have `graph_boost`, `scope`, or
//! `exclude_collections`/`include_edges`/`include_markdown` — those are
//! `SearchSemanticInput`-only fields with no proto/RPC surface on
//! `NodeService` today. Full `search_semantic` parity (a new RPC exposing
//! `EmbeddingsService.SearchSemantic` to the CLI, or extending `SearchRequest`)
//! is deferred to a follow-up; this is the documented CLI subset.

use anyhow::{Context, Result};
use clap::Args;
use nodespace_daemon::nodespace::SearchRequest;

use crate::output;
use crate::NodeClient;

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Free-text query (pass empty string when using --type for type-only listing).
    #[arg(default_value = "")]
    pub query: String,
    /// Filter results to one or more node types (e.g. `--type task --type text`).
    #[arg(long = "type", value_name = "TYPE")]
    pub node_types: Vec<String>,
    /// Filter to a collection by path (mutually exclusive with --collection-id).
    #[arg(long, conflicts_with = "collection_id")]
    pub collection: Option<String>,
    /// Filter to a collection by ID (mutually exclusive with --collection).
    #[arg(long)]
    pub collection_id: Option<String>,
    /// JSON-encoded array of {field, operator, value} filter objects.
    #[arg(long)]
    pub filters: Option<String>,
    /// Semantic similarity threshold, 0.0-1.0 (0.0 = server default of 0.7).
    #[arg(long, default_value_t = 0.0)]
    pub threshold: f32,
    /// Maximum number of results to return (0 = server default, currently 20).
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(i32).range(0..))]
    pub limit: i32,
}

pub async fn run(client: &mut NodeClient, args: SearchArgs, json: bool) -> Result<()> {
    let response = client
        .search_nodes(SearchRequest {
            query: args.query,
            node_types: args.node_types,
            collection: args.collection,
            collection_id: args.collection_id,
            limit: args.limit,
            offset: 0,
            threshold: args.threshold,
            semantic: true,
            filters: args.filters.unwrap_or_default(),
        })
        .await
        .context("SearchNodes RPC failed")?
        .into_inner();

    output::print_node_list(&response, json)
}
