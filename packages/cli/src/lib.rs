//! `nodespace` CLI library surface.
//!
//! Exposed primarily so integration tests can drive the command handlers
//! against an in-process daemon without shelling out to the built binary.

pub mod commands;
pub mod output;
pub mod terminal;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nodespace_daemon::{AgentSessionServiceClient, ImportServiceClient, NodeServiceClient};
use tonic::transport::Channel;

#[derive(Parser, Debug)]
#[command(
    name = "nodespace",
    version,
    about = "Command-line interface for NodeSpace — talks to the local nodespaced daemon over gRPC.",
    long_about = "nodespace is a stateless gRPC client that connects to the nodespaced daemon \
                  via Unix Domain Socket and exposes the knowledge graph as shell commands.\n\n\
                  Start the daemon with `nodespaced` before invoking subcommands."
)]
pub struct Cli {
    /// Emit raw JSON instead of human-readable output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Override the socket path (default: ~/.nodespace/daemon.sock).
    /// Honors the `NODESPACED_SOCKET` environment variable when this flag is absent.
    #[arg(long, global = true, env = "NODESPACED_SOCKET")]
    pub socket: Option<String>,

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
    /// Developer diagnostics: database path, size, node counts, schema count.
    Diagnostics(commands::diagnostics::DiagnosticsArgs),
    /// Import markdown files into NodeSpace.
    Import {
        #[command(subcommand)]
        action: commands::import::ImportAction,
    },
    /// Manage PTY agent sessions (launch, attach, list, kill).
    Session {
        #[command(subcommand)]
        action: commands::session::SessionAction,
    },
}

/// Resolve the socket path from an explicit override or env/default.
#[cfg(unix)]
pub fn resolve_socket_path(override_: Option<&str>) -> std::path::PathBuf {
    if let Some(p) = override_ {
        return std::path::PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".nodespace")
        .join("daemon.sock")
}

/// Build a tonic `Channel` connected over a Unix Domain Socket.
#[cfg(unix)]
async fn uds_channel(sock: &std::path::Path) -> Result<Channel> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let sock = sock.to_path_buf();
    // The URI host is ignored for UDS — tonic needs a syntactically valid URI.
    let channel = Endpoint::from_static("http://localhost")
        .connect_with_connector(service_fn(move |_: Uri| {
            let sock = sock.clone();
            async move { UnixStream::connect(&sock).await.map(TokioIo::new) }
        }))
        .await?;
    Ok(channel)
}

/// Connect to the daemon, returning a friendly error if it isn't running.
#[cfg(unix)]
pub async fn connect(sock: &std::path::Path) -> Result<NodeServiceClient<Channel>> {
    uds_channel(sock)
        .await
        .map(NodeServiceClient::new)
        .with_context(|| {
            format!(
                "Could not connect to nodespaced at {}.\n\
                 Is the daemon running? Start it with `nodespaced` in another terminal.",
                sock.display()
            )
        })
}

/// Connect an ImportServiceClient to the daemon.
#[cfg(unix)]
pub async fn connect_import(sock: &std::path::Path) -> Result<ImportServiceClient<Channel>> {
    uds_channel(sock)
        .await
        .map(ImportServiceClient::new)
        .with_context(|| {
            format!(
                "Could not connect to nodespaced at {}.\n\
                 Is the daemon running? Start it with `nodespaced` in another terminal.",
                sock.display()
            )
        })
}

/// Connect an AgentSessionServiceClient to the daemon.
#[cfg(unix)]
pub async fn connect_session(sock: &std::path::Path) -> Result<AgentSessionServiceClient<Channel>> {
    uds_channel(sock)
        .await
        .map(AgentSessionServiceClient::new)
        .with_context(|| {
            format!(
                "Could not connect to nodespaced at {}.\n\
                 Is the daemon running? Start it with `nodespaced` in another terminal.",
                sock.display()
            )
        })
}

/// Top-level dispatch — wired by `main.rs` and reused by integration tests.
#[cfg(unix)]
pub async fn run(cli: Cli) -> Result<()> {
    let sock = resolve_socket_path(cli.socket.as_deref());
    let json = cli.json;

    match cli.command {
        Command::Node { action } => {
            let mut client = connect(&sock).await?;
            commands::node::run(&mut client, action, json).await
        }
        Command::Search(args) => {
            let mut client = connect(&sock).await?;
            commands::search::run(&mut client, args, json).await
        }
        Command::Diagnostics(args) => {
            let mut client = connect(&sock).await?;
            commands::diagnostics::run(&mut client, args, json).await
        }
        Command::Import { action } => {
            let mut client = connect_import(&sock).await?;
            commands::import::run(&mut client, action, json).await
        }
        Command::Session { action } => {
            let mut client = connect_session(&sock).await?;
            commands::session::run(&mut client, action, json).await
        }
    }
}
