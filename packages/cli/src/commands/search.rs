//! `nodespace search <query>` — semantic search via NodeService.SearchNodes.

use anyhow::{Context, Result};
use clap::Args;
use nodespace_daemon::nodespace::SearchRequest;
use nodespace_daemon::NodeServiceClient;
use tonic::transport::Channel;

use crate::output;

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Free-text query.
    pub query: String,
    /// Maximum number of results to return (0 = server default, currently 20).
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(i32).range(0..))]
    pub limit: i32,
}

pub async fn run(
    client: &mut NodeServiceClient<Channel>,
    args: SearchArgs,
    json: bool,
) -> Result<()> {
    let response = client
        .search_nodes(SearchRequest {
            query: args.query,
            node_types: vec![],
            collection: String::new(),
            collection_id: String::new(),
            limit: args.limit,
            offset: 0,
            threshold: 0.0,
            semantic: true,
            filters: String::new(),
        })
        .await
        .context("SearchNodes RPC failed")?
        .into_inner();

    output::print_node_list(&response, json)
}
