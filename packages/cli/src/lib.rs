//! `nodespace` CLI library surface.
//!
//! Exposed primarily so integration tests can drive the command handlers
//! against an in-process daemon without shelling out to the built binary.

pub mod commands;
pub mod output;
pub mod terminal;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nodespace_daemon::{
    AgentSessionServiceClient, DatabaseServiceClient, ImportServiceClient, LocalAgentServiceClient,
    NodeServiceClient,
};
use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;

/// Stamps the ADR-053 `x-ns-database-id` routing header on every outgoing
/// request so the daemon routes it to a specific local database.
///
/// Concrete (not a closure) so the intercepted client types stay nameable and
/// aliasable — see [`NodeClient`] and friends. `database_id: None` stamps
/// nothing, letting the daemon fall back to its default database; that variant
/// is applied uniformly so a client's type is the same whether or not a
/// database was selected.
#[derive(Clone)]
pub struct DatabaseIdInterceptor {
    // Some(id) → stamp header on every request; None → stamp nothing (daemon
    // uses its default database).
    database_id: Option<MetadataValue<Ascii>>,
}

impl DatabaseIdInterceptor {
    /// No routing header — the daemon serves its default database.
    pub fn none() -> Self {
        Self { database_id: None }
    }

    /// Stamp `x-ns-database-id: <id>` on every request. `id` must be an already
    /// resolved registry identifier (ULID); the daemon resolves the header as an
    /// id only, never a name.
    pub fn for_id(id: &str) -> Result<Self> {
        let value = MetadataValue::try_from(id)
            .with_context(|| format!("database id '{id}' is not a valid gRPC header value"))?;
        Ok(Self {
            database_id: Some(value),
        })
    }
}

impl Interceptor for DatabaseIdInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(id) = &self.database_id {
            req.metadata_mut()
                .insert(nodespace_daemon::DATABASE_ID_HEADER, id.clone());
        }
        Ok(req)
    }
}

/// A UDS channel wrapped so every request carries the database routing header.
pub type Intercepted = InterceptedService<Channel, DatabaseIdInterceptor>;
/// `NodeService` client bound to a selected database.
pub type NodeClient = NodeServiceClient<Intercepted>;
/// `ImportService` client bound to a selected database.
pub type ImportClient = ImportServiceClient<Intercepted>;
/// `AgentSessionService` client bound to a selected database.
pub type SessionClient = AgentSessionServiceClient<Intercepted>;
/// `LocalAgentService` client bound to a selected database.
pub type LocalAgentClient = LocalAgentServiceClient<Intercepted>;

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

    /// Target a specific local database by name or id (ADR-053).
    /// When omitted, requests route to the daemon's default database.
    /// Honors the `NODESPACE_DATABASE` environment variable when this flag is absent.
    #[arg(long, global = true, env = "NODESPACE_DATABASE")]
    pub database: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Operate on individual nodes (get, create, update, delete, children, query, export, batch-get, batch-update).
    Node {
        #[command(subcommand)]
        action: commands::node::NodeAction,
    },
    /// Manage the local inference model (list, load, recommended).
    Model {
        #[command(subcommand)]
        action: commands::model::ModelAction,
    },
    /// Semantic search across the knowledge graph.
    Search(commands::search::SearchArgs),
    /// Structured property query with comparison operators (equals/contains/gt/lt/gte/lte/in/exists).
    Query(commands::query::QueryArgs),
    /// Developer diagnostics: database path, size, node counts, schema count.
    Diagnostics(commands::diagnostics::DiagnosticsArgs),
    /// Import markdown files into NodeSpace.
    Import {
        #[command(subcommand)]
        action: commands::import::ImportAction,
    },
    /// Manage mention relationships between nodes.
    Mention {
        #[command(subcommand)]
        action: commands::mention::MentionAction,
    },
    /// Inspect and manage node type schema definitions.
    Schema {
        #[command(subcommand)]
        action: commands::schema::SchemaAction,
    },
    /// Manage typed relationship edges between nodes (distinct from mentions).
    Relationship {
        #[command(subcommand)]
        action: commands::relationship::RelationshipAction,
    },
    /// Manage PTY agent sessions (launch, attach, list, kill).
    Session {
        #[command(subcommand)]
        action: commands::session::SessionAction,
    },
    /// Manage the daemon's registry of local databases (list, create, register, remove, rename, use).
    Database {
        #[command(subcommand)]
        action: commands::database::DatabaseAction,
    },
    /// Uninstall NodeSpace: stop daemon, remove binaries and service registration.
    Uninstall(commands::uninstall::UninstallArgs),
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

/// Friendly "daemon isn't running" context for a failed connect.
#[cfg(unix)]
fn connect_error_context(sock: &std::path::Path) -> String {
    format!(
        "Could not connect to nodespaced at {}.\n\
         Is the daemon running? Start it with `nodespaced` in another terminal.",
        sock.display()
    )
}

/// Connect a `NodeService` client bound to the selected database, returning a
/// friendly error if the daemon isn't running.
#[cfg(unix)]
pub async fn connect(
    sock: &std::path::Path,
    interceptor: DatabaseIdInterceptor,
) -> Result<NodeClient> {
    uds_channel(sock)
        .await
        .map(|channel| NodeServiceClient::with_interceptor(channel, interceptor))
        .with_context(|| connect_error_context(sock))
}

/// Connect an `ImportService` client bound to the selected database.
#[cfg(unix)]
pub async fn connect_import(
    sock: &std::path::Path,
    interceptor: DatabaseIdInterceptor,
) -> Result<ImportClient> {
    uds_channel(sock)
        .await
        .map(|channel| ImportServiceClient::with_interceptor(channel, interceptor))
        .with_context(|| connect_error_context(sock))
}

/// Connect an `AgentSessionService` client bound to the selected database.
#[cfg(unix)]
pub async fn connect_session(
    sock: &std::path::Path,
    interceptor: DatabaseIdInterceptor,
) -> Result<SessionClient> {
    uds_channel(sock)
        .await
        .map(|channel| AgentSessionServiceClient::with_interceptor(channel, interceptor))
        .with_context(|| connect_error_context(sock))
}

/// Connect a `LocalAgentService` client bound to the selected database.
#[cfg(unix)]
pub async fn connect_local_agent(
    sock: &std::path::Path,
    interceptor: DatabaseIdInterceptor,
) -> Result<LocalAgentClient> {
    uds_channel(sock)
        .await
        .map(|channel| LocalAgentServiceClient::with_interceptor(channel, interceptor))
        .with_context(|| connect_error_context(sock))
}

/// Connect a `DatabaseService` client. This operates on the daemon's database
/// registry globally, so it carries no routing header (unlike the data-plane
/// clients above).
#[cfg(unix)]
pub async fn connect_database(sock: &std::path::Path) -> Result<DatabaseServiceClient<Channel>> {
    uds_channel(sock)
        .await
        .map(DatabaseServiceClient::new)
        .with_context(|| connect_error_context(sock))
}

/// Resolve the `--database` selection into a routing interceptor plus the
/// resolved database id (if any).
///
/// The daemon resolves the `x-ns-database-id` header as an id (ULID) only, so a
/// selection given as a name is resolved to its id here, against the registry,
/// before any data-plane request is made. `None` selection routes to the
/// daemon's default database. The returned id is `None` for the default and
/// `Some(id)` for an explicit selection — diagnostics needs it to identify which
/// registry entry it targeted.
#[cfg(unix)]
async fn resolve_routing(
    sock: &std::path::Path,
    selection: Option<&str>,
) -> Result<(DatabaseIdInterceptor, Option<String>)> {
    match selection {
        None => Ok((DatabaseIdInterceptor::none(), None)),
        Some(sel) => {
            let mut db = connect_database(sock).await?;
            let id = commands::database::resolve_database_id_by_selection(&mut db, sel).await?;
            let interceptor = DatabaseIdInterceptor::for_id(&id)?;
            Ok((interceptor, Some(id)))
        }
    }
}

#[cfg(windows)]
pub async fn run(_cli: Cli) -> Result<()> {
    anyhow::bail!("The nodespace CLI is not supported on Windows (Unix socket transport only).")
}

/// Top-level dispatch — wired by `main.rs` and reused by integration tests.
#[cfg(unix)]
pub async fn run(cli: Cli) -> Result<()> {
    let sock = resolve_socket_path(cli.socket.as_deref());
    let json = cli.json;
    let selection = cli.database.as_deref();

    match cli.command {
        Command::Node { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::node::run(&mut client, action, json).await
        }
        Command::Model { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect_local_agent(&sock, interceptor).await?;
            commands::model::run(&mut client, action, json).await
        }
        Command::Search(args) => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::search::run(&mut client, args, json).await
        }
        Command::Query(args) => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::query::run(&mut client, args, json).await
        }
        Command::Diagnostics(args) => {
            let (interceptor, target_id) = resolve_routing(&sock, selection).await?;
            let mut node_client = connect(&sock, interceptor).await?;
            let mut db_client = connect_database(&sock).await?;
            commands::diagnostics::run(
                &mut node_client,
                &mut db_client,
                target_id.as_deref(),
                args,
                json,
            )
            .await
        }
        Command::Import { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect_import(&sock, interceptor).await?;
            commands::import::run(&mut client, action, json).await
        }
        Command::Mention { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::mention::run(&mut client, action, json).await
        }
        Command::Schema { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::schema::run(&mut client, action, json).await
        }
        Command::Relationship { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect(&sock, interceptor).await?;
            commands::relationship::run(&mut client, action, json).await
        }
        Command::Session { action } => {
            let (interceptor, _) = resolve_routing(&sock, selection).await?;
            let mut client = connect_session(&sock, interceptor).await?;
            commands::session::run(&mut client, action, json).await
        }
        // The `database` subcommands operate on the registry globally and are
        // never routed by `--database` — they use a plain DatabaseService client.
        Command::Database { action } => {
            let mut client = connect_database(&sock).await?;
            commands::database::run(&mut client, action, json).await
        }
        Command::Uninstall(args) => commands::uninstall::run(args),
    }
}
