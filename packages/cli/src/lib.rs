//! `nodespace` CLI library surface.
//!
//! Exposed primarily so integration tests can drive the command handlers
//! against an in-process daemon without shelling out to the built binary.

pub mod commands;
pub mod output;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nodespace_daemon::NodeServiceClient;
use tonic::transport::Channel;

/// Default endpoint the CLI dials. ADR-031 reserves `localhost:50051` for the
/// loopback-only gRPC endpoint exposed by `nodespaced`.
pub const DEFAULT_ENDPOINT: &str = "http://[::1]:50051";

#[derive(Parser, Debug)]
#[command(
    name = "nodespace",
    version,
    about = "Command-line interface for NodeSpace — talks to the local nodespaced daemon over gRPC.",
    long_about = "nodespace is a stateless gRPC client that connects to the nodespaced daemon \
                  (default: localhost:50051) and exposes the knowledge graph as shell commands.\n\n\
                  Start the daemon with `nodespaced` before invoking subcommands."
)]
pub struct Cli {
    /// Emit raw JSON instead of human-readable output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Override the daemon endpoint (default: http://[::1]:50051).
    /// Honors the `NODESPACE_ENDPOINT` environment variable when this flag is absent.
    #[arg(long, global = true, env = "NODESPACE_ENDPOINT")]
    pub endpoint: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Operate on individual nodes (get, create, update, delete, children).
    Node {
        #[command(subcommand)]
        action: commands::node::NodeAction,
    },
    /// Semantic search across the knowledge graph.
    Search(commands::search::SearchArgs),
}

/// Resolve the configured endpoint, falling back to `DEFAULT_ENDPOINT`.
pub fn resolve_endpoint(override_: Option<&str>) -> String {
    override_
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())
}

/// Connect to the daemon, returning a friendly error if it isn't running.
///
/// `tonic` surfaces "connection refused" as a transport error with a tower
/// hyper cause; users running `nodespace` without first launching `nodespaced`
/// need a clear remediation, not a stack of generic transport errors.
pub async fn connect(endpoint: &str) -> Result<NodeServiceClient<Channel>> {
    NodeServiceClient::connect(endpoint.to_string())
        .await
        .with_context(|| {
            format!(
                "Could not connect to nodespaced at {endpoint}.\n\
                 Is the daemon running? Start it with `nodespaced` in another terminal."
            )
        })
}

/// Top-level dispatch — wired by `main.rs` and reused by integration tests.
pub async fn run(cli: Cli) -> Result<()> {
    let endpoint = resolve_endpoint(cli.endpoint.as_deref());
    let mut client = connect(&endpoint).await?;
    let json = cli.json;

    match cli.command {
        Command::Node { action } => commands::node::run(&mut client, action, json).await,
        Command::Search(args) => commands::search::run(&mut client, args, json).await,
    }
}
