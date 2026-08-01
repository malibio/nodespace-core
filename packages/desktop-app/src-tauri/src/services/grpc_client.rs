//! gRPC client that connects to the external `nodespaced` daemon over a Unix
//! Domain Socket.
//!
//! Socket path resolution order:
//!   1. `NODESPACED_SOCKET` environment variable
//!   2. Build-variant-scoped default (see daemon_setup::daemon_socket_relative)
//!
//! The `GrpcClient` is registered as Tauri managed state once and cloned
//! cheaply per command (tonic `Channel` is an `Arc` internally).

use std::sync::Arc;

use nodespace_proto::{
    AgentSessionServiceClient, DatabaseServiceClient, EmbeddingsServiceClient, ImportServiceClient,
    LocalAgentServiceClient, NodeServiceClient, SettingsServiceClient, CLIENT_ID_HEADER,
    DATABASE_ID_HEADER,
};
use tokio::sync::{watch, RwLock};
use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::interceptor::InterceptedService;
use tonic::service::Interceptor;
use tonic::transport::Channel;

/// Stamps the ADR-053 `x-ns-database-id` routing header and the ADR-026 C5
/// extension's `x-ns-client-id` identity header on every outgoing request.
///
/// Concrete (not a closure) so the intercepted client types stay nameable and
/// aliasable — see [`NodeClient`] and friends. `database_id: None` stamps
/// nothing, letting the daemon fall back to its default database; the variant
/// is applied uniformly so a routed client's type is the same whether or not a
/// database is currently selected. Same-named struct as the CLI's interceptor
/// (`packages/cli/src/lib.rs`) but an independent implementation in a
/// different crate — the CLI's copy deliberately does not stamp
/// `x-ns-client-id`; see its doc comment for why.
///
/// `client_id` is always `Some` in production — generated once per
/// `GrpcClient` (i.e. once per app process/window) in [`GrpcClient::from_channel`]
/// and carried through every interceptor rebuild (database switches rebuild
/// the routed clients but must keep the SAME client id, or the daemon would
/// stop recognizing this window's own writes on echo suppression after every
/// switch). Stamping it lets the daemon scope this connection's writes via
/// `NodeService::with_client()` and drop their echo on this connection's own
/// `WatchNodes` stream (see `watcher.rs`).
#[derive(Clone)]
pub struct DatabaseIdInterceptor {
    // Some(id) → stamp header on every request; None → stamp nothing (daemon
    // uses its default database).
    database_id: Option<MetadataValue<Ascii>>,
    client_id: MetadataValue<Ascii>,
}

impl DatabaseIdInterceptor {
    /// No routing header — the daemon serves its default database. Still
    /// stamps `client_id` so writes made before any database is selected are
    /// still attributable.
    pub fn none(client_id: MetadataValue<Ascii>) -> Self {
        Self {
            database_id: None,
            client_id,
        }
    }

    /// Stamp `x-ns-database-id: <id>` on every request when `id` is `Some`,
    /// and `x-ns-client-id: <client_id>` unconditionally.
    /// `id` must be an already resolved registry identifier (ULID); the daemon
    /// resolves the header as an id only, never a name. An id that is not a
    /// valid gRPC header value falls back to no routing (default database)
    /// rather than poisoning every subsequent request.
    pub fn for_id(id: Option<&str>, client_id: MetadataValue<Ascii>) -> Self {
        let database_id = id.and_then(|id| match MetadataValue::try_from(id) {
            Ok(value) => Some(value),
            Err(_) => {
                // A resolved ULID is always valid ASCII, so this should be
                // unreachable — but log it rather than silently serving the
                // default database, which would be a hard-to-spot wrong-database
                // fallback.
                tracing::warn!(
                    "database id {id:?} is not a valid routing header; \
                     falling back to the default database"
                );
                None
            }
        });
        Self {
            database_id,
            client_id,
        }
    }
}

impl Interceptor for DatabaseIdInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(id) = &self.database_id {
            req.metadata_mut().insert(DATABASE_ID_HEADER, id.clone());
        }
        req.metadata_mut()
            .insert(CLIENT_ID_HEADER, self.client_id.clone());
        Ok(req)
    }
}

/// Generate a fresh client id for this `GrpcClient` (one per app
/// process/window, per ADR-026's C5 extension). A UUID is always valid ASCII, so this
/// cannot fail in practice.
fn generate_client_id() -> MetadataValue<Ascii> {
    MetadataValue::try_from(uuid::Uuid::new_v4().to_string())
        .expect("uuid v4 string is always a valid ascii metadata value")
}

/// A channel wrapped so every request carries the database routing header.
pub type Intercepted = InterceptedService<Channel, DatabaseIdInterceptor>;
/// `NodeService` client bound to the active database (ADR-053).
pub type NodeClient = NodeServiceClient<Intercepted>;
/// `ImportService` client bound to the active database.
pub type ImportClient = ImportServiceClient<Intercepted>;
/// `EmbeddingsService` client bound to the active database.
pub type EmbeddingsClient = EmbeddingsServiceClient<Intercepted>;
/// `AgentSessionService` client bound to the active database.
pub type AgentSessionClient = AgentSessionServiceClient<Intercepted>;

struct GrpcClientInner {
    // Routed data-plane clients — rebuilt with a fresh interceptor whenever the
    // active database changes so their requests carry the routing header. The
    // daemon routes node/import/embeddings/agent_session by `x-ns-database-id`.
    node: NodeClient,
    import: ImportClient,
    embeddings: EmbeddingsClient,
    agent_session: AgentSessionClient,
    // Unrouted clients — registry-global (database_service) or daemon-global
    // (settings, local_agent); they never carry the routing header.
    settings: SettingsServiceClient<Channel>,
    local_agent: LocalAgentServiceClient<Channel>,
    database_service: DatabaseServiceClient<Channel>,
    /// The desktop-local "which database am I viewing" selection. `None` = the
    /// daemon's default database. Distinct from the daemon-wide default set via
    /// `DatabaseService::SetDefault`.
    active_database_id: Option<String>,
    /// This process/window's stable identity (ADR-026's C5 extension), generated once in
    /// [`GrpcClient::from_channel`] and re-stamped by every interceptor rebuild
    /// in [`GrpcClient::set_active_database`] — it must never change for the
    /// lifetime of this `GrpcClient`, or the daemon would stop recognizing this
    /// window's own writes on its `WatchNodes` echo-suppression check after a
    /// database switch.
    client_id: MetadataValue<Ascii>,
    /// Underlying transport channel — held so Pro-tier services can
    /// ride the same h2 connection via `GrpcClient::channel()`. One
    /// channel, multiple service surfaces. Opening a parallel channel
    /// caused "Service was not ready: transport error" during the
    /// PoC when ProClient's separately-built channel got into a bad
    /// state after the probe stream was dropped.
    channel: Channel,
}

/// Managed Tauri state wrapping the gRPC clients connected to `nodespaced`.
///
/// `Channel` is cheap to clone (it is an `Arc` internally). Commands clone
/// clients per call since tonic's generated methods take `&mut self`.
#[derive(Clone)]
pub struct GrpcClient {
    inner: Arc<RwLock<GrpcClientInner>>,
    /// Bumped by [`set_active_database`] on every switch so the node-event
    /// watcher can re-open its `WatchNodes` stream against the newly-active
    /// database. `watch::Sender` is not `Clone`, hence the `Arc`.
    db_generation: Arc<watch::Sender<u64>>,
}

impl GrpcClient {
    /// Connect to the `nodespaced` daemon over a Unix Domain Socket and return
    /// a fully-initialised client bundle.
    ///
    /// Returns an error if the socket cannot be reached. The Tauri app should
    /// treat this as a fatal startup error (daemon not running).
    #[cfg(unix)]
    pub async fn connect() -> Result<Self, GrpcClientError> {
        let sock = resolve_socket_path();
        tracing::info!(socket = %sock.display(), "Connecting to nodespaced");

        let channel = uds_channel(&sock).await.map_err(GrpcClientError::Connect)?;

        tracing::info!(socket = %sock.display(), "Connected to nodespaced");

        Ok(Self::from_channel(channel))
    }

    /// Connect to the `nodespaced` daemon over a Named Pipe and return
    /// a fully-initialised client bundle.
    #[cfg(windows)]
    pub async fn connect() -> Result<Self, GrpcClientError> {
        let pipe = resolve_pipe_name();
        tracing::info!(pipe = %pipe, "Connecting to nodespaced (Named Pipe)");
        let channel = pipe_channel(&pipe)
            .await
            .map_err(GrpcClientError::Connect)?;
        tracing::info!(pipe = %pipe, "Connected to nodespaced");
        Ok(Self::from_channel(channel))
    }

    /// Wrap an established (or lazy) channel in the full service-client bundle.
    /// Shared by [`connect`] and [`connect_lazy`] so the set of service clients
    /// stays in sync as new services are added. `Channel` is platform-agnostic.
    fn from_channel(channel: Channel) -> Self {
        // One stable id for this GrpcClient's whole lifetime (ADR-026's C5 extension) —
        // generated once here, never regenerated on a database switch.
        let client_id = generate_client_id();
        // No database selected initially → the routed clients carry an empty
        // interceptor and requests fall back to the daemon's default database,
        // exactly as before ADR-053 client routing existed. The client-id
        // header is still stamped.
        let interceptor = DatabaseIdInterceptor::none(client_id.clone());
        let inner = GrpcClientInner {
            node: NodeServiceClient::with_interceptor(channel.clone(), interceptor.clone()),
            import: ImportServiceClient::with_interceptor(channel.clone(), interceptor.clone()),
            embeddings: EmbeddingsServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            agent_session: AgentSessionServiceClient::with_interceptor(
                channel.clone(),
                interceptor,
            ),
            settings: SettingsServiceClient::new(channel.clone()),
            local_agent: LocalAgentServiceClient::new(channel.clone()),
            database_service: DatabaseServiceClient::new(channel.clone()),
            active_database_id: None,
            client_id,
            channel,
        };
        let (db_generation, _) = watch::channel(0u64);
        Self {
            inner: Arc::new(RwLock::new(inner)),
            db_generation: Arc::new(db_generation),
        }
    }

    /// Build a client over a LAZY UDS channel — it connects on the first RPC
    /// rather than now, so this returns synchronously and can be `.manage()`'d at
    /// app setup BEFORE the daemon socket is reachable. This removes the startup
    /// race where the frontend invoked a command (e.g. `get_children_tree`) before
    /// the async `connect()` had run, getting a fatal "state not managed for field
    /// `client`" that closed the view. With a lazy client managed up front, an
    /// early call instead waits / yields a retryable transport error.
    #[cfg(unix)]
    pub fn connect_lazy() -> Self {
        let sock = resolve_socket_path();
        tracing::info!(socket = %sock.display(), "gRPC client (lazy) — connects on first use");
        let channel = uds_channel_lazy(&sock);
        Self::from_channel(channel)
    }

    /// Lazy Named Pipe variant for Windows — connects on the first RPC.
    #[cfg(windows)]
    pub fn connect_lazy() -> Self {
        let pipe = resolve_pipe_name();
        tracing::info!(pipe = %pipe, "gRPC client (lazy) — connects on first use");
        let channel = pipe_channel_lazy(&pipe);
        Self::from_channel(channel)
    }

    /// Borrow a clone of the routed `NodeService` client (carries the active
    /// database's `x-ns-database-id` header).
    pub async fn client(&self) -> NodeClient {
        self.inner.read().await.node.clone()
    }

    /// Borrow a clone of the routed `ImportService` client.
    pub async fn import_client(&self) -> ImportClient {
        self.inner.read().await.import.clone()
    }

    /// Borrow a clone of the `SettingsServiceClient`. Settings are daemon-global
    /// and never routed by database.
    pub async fn settings_client(&self) -> SettingsServiceClient<Channel> {
        self.inner.read().await.settings.clone()
    }

    /// Borrow a clone of the routed `EmbeddingsService` client.
    ///
    /// Embeddings are always available in the daemon (unlike the old in-process
    /// optional configuration), so this returns the client directly.
    pub async fn embeddings_client(&self) -> EmbeddingsClient {
        self.inner.read().await.embeddings.clone()
    }

    /// Borrow a clone of the routed `AgentSessionService` client.
    pub async fn agent_session_client(&self) -> AgentSessionClient {
        self.inner.read().await.agent_session.clone()
    }

    /// Borrow a clone of the `LocalAgentServiceClient`. The local inference
    /// model is a daemon-global resource and is never routed by database.
    pub async fn local_agent_client(&self) -> LocalAgentServiceClient<Channel> {
        self.inner.read().await.local_agent.clone()
    }

    /// Borrow a clone of the `DatabaseServiceClient`. The registry is global to
    /// the daemon and is never routed by database.
    pub async fn database_service_client(&self) -> DatabaseServiceClient<Channel> {
        self.inner.read().await.database_service.clone()
    }

    /// Select which local database the routed data-plane clients target
    /// (ADR-053). `Some(id)` stamps `x-ns-database-id: <id>` on every
    /// node/import/embeddings/agent_session request; `None` clears the header
    /// so requests route to the daemon's default database.
    ///
    /// Only the routed clients are rebuilt — over the SAME channel, so no
    /// reconnection happens. `settings`/`local_agent`/`database_service` stay
    /// unrouted. Bumps a generation counter so the node-event watcher re-opens
    /// its `WatchNodes` stream against the newly-active database.
    pub async fn set_active_database(&self, id: Option<String>) {
        {
            let mut inner = self.inner.write().await;
            if inner.active_database_id == id {
                return;
            }
            let interceptor = DatabaseIdInterceptor::for_id(id.as_deref(), inner.client_id.clone());
            let channel = inner.channel.clone();
            inner.node = NodeServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.import =
                ImportServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.embeddings =
                EmbeddingsServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.agent_session = AgentSessionServiceClient::with_interceptor(channel, interceptor);
            inner.active_database_id = id;
        }
        // Signal the watcher (outside the lock) to re-subscribe to the new
        // database's event stream.
        self.db_generation.send_modify(|g| *g = g.wrapping_add(1));
    }

    /// Subscribe to active-database switches. The value is an opaque generation
    /// counter bumped on every [`set_active_database`]; the node-event watcher
    /// awaits changes to re-open its `WatchNodes` stream against the new
    /// database.
    pub fn subscribe_active_database(&self) -> watch::Receiver<u64> {
        self.db_generation.subscribe()
    }

    /// Clone of the underlying `tonic::transport::Channel`. Used by
    /// `ProClient` so the Pro-tier service rides the same h2
    /// connection (one channel, multiple service surfaces).
    pub async fn channel(&self) -> Channel {
        self.inner.read().await.channel.clone()
    }

    /// Rebuild the underlying lazy channel and every service client from
    /// scratch, preserving the active-database routing — what a full app
    /// restart does, but in place.
    ///
    /// Recovers a **wedged** h2 connection: the daemon can be healthy (it
    /// answers a freshly-dialed client) while this long-lived channel is stuck,
    /// so `get_node`/writes and the WatchNodes stream hang indefinitely (the
    /// lazy channel has no client-side timeout). A stream churn during a heavy
    /// sync can put the single shared connection into that state. Replacing the
    /// channel gives every surface a clean connection on its next call; bumping
    /// `db_generation` makes the node-event watcher re-open its stream on it.
    #[cfg(unix)]
    pub async fn reconnect(&self) {
        let sock = resolve_socket_path();
        let channel = uds_channel_lazy(&sock);
        self.swap_channel(channel).await;
        tracing::info!(socket = %sock.display(), "gRPC client: channel rebuilt (reconnect)");
    }

    /// Named-pipe variant of [`reconnect`].
    #[cfg(windows)]
    pub async fn reconnect(&self) {
        let pipe = resolve_pipe_name();
        let channel = pipe_channel_lazy(&pipe);
        self.swap_channel(channel).await;
        tracing::info!(pipe = %pipe, "gRPC client: channel rebuilt (reconnect)");
    }

    /// Replace the channel behind every service client, keeping the current
    /// `x-ns-database-id` routing, then signal the watcher to re-subscribe.
    async fn swap_channel(&self, channel: Channel) {
        {
            let mut inner = self.inner.write().await;
            // Preserve this window's client id across the rebuild — the daemon
            // scopes echo suppression by it, so a fresh id would make our own
            // writes re-appear as remote changes on the new WatchNodes stream.
            let interceptor = DatabaseIdInterceptor::for_id(
                inner.active_database_id.as_deref(),
                inner.client_id.clone(),
            );
            inner.node = NodeServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.import =
                ImportServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.embeddings =
                EmbeddingsServiceClient::with_interceptor(channel.clone(), interceptor.clone());
            inner.agent_session =
                AgentSessionServiceClient::with_interceptor(channel.clone(), interceptor);
            inner.settings = SettingsServiceClient::new(channel.clone());
            inner.local_agent = LocalAgentServiceClient::new(channel.clone());
            inner.database_service = DatabaseServiceClient::new(channel.clone());
            inner.channel = channel;
        }
        self.db_generation.send_modify(|g| *g = g.wrapping_add(1));
    }
}

/// Resolve the daemon socket path.
///
/// Checks `NODESPACED_SOCKET` env var first, then falls back to the build-variant-
/// scoped default from `daemon_setup::daemon_socket_relative()`. `pub(crate)` so
/// the daemon-reachability checks in `lib.rs` probe the SAME socket the client
/// actually dials — otherwise a `NODESPACED_SOCKET` override (two-window demo,
/// custom setups) makes them check the wrong path and falsely report "not running".
#[cfg(unix)]
pub(crate) fn resolve_socket_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return std::path::PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(crate::daemon_setup::daemon_socket_relative())
}

/// Resolve the Named Pipe name used on Windows.
///
/// Checks `NODESPACED_SOCKET` env var first (mirrors Unix override convention),
/// then falls back to `\\.\pipe\nodespace-daemon`.
#[cfg(windows)]
pub(crate) fn resolve_pipe_name() -> String {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return p;
    }
    r"\\.\pipe\nodespace-daemon".to_string()
}

/// On Windows, return the pipe name as a `PathBuf` so callers that take a `Path`
/// (e.g. `check_daemon_socket`) work without platform-specific call sites.
#[cfg(windows)]
pub(crate) fn resolve_socket_path() -> std::path::PathBuf {
    std::path::PathBuf::from(resolve_pipe_name())
}

/// Build a tonic `Channel` connected over a Unix Domain Socket.
#[cfg(unix)]
async fn uds_channel(sock: &std::path::Path) -> Result<Channel, tonic::transport::Error> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let sock = sock.to_path_buf();
    // The URI host is ignored for UDS — tonic needs a syntactically valid URI.
    Endpoint::from_static("http://localhost")
        .connect_with_connector(service_fn(move |_: Uri| {
            let sock = sock.clone();
            async move { UnixStream::connect(&sock).await.map(TokioIo::new) }
        }))
        .await
}

/// Lazy variant of [`uds_channel`] — builds the channel without connecting; the
/// first RPC establishes (and later re-establishes) the UDS connection.
#[cfg(unix)]
fn uds_channel_lazy(sock: &std::path::Path) -> Channel {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let sock = sock.to_path_buf();
    Endpoint::from_static("http://localhost").connect_with_connector_lazy(service_fn(
        move |_: Uri| {
            let sock = sock.clone();
            async move { UnixStream::connect(&sock).await.map(TokioIo::new) }
        },
    ))
}

/// Build a tonic `Channel` connected over a Named Pipe (Windows).
#[cfg(windows)]
async fn pipe_channel(pipe: &str) -> Result<Channel, tonic::transport::Error> {
    use hyper_util::rt::TokioIo;
    use tokio::net::windows::named_pipe::ClientOptions;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let pipe = pipe.to_string();
    Endpoint::from_static("http://localhost")
        .connect_with_connector(service_fn(move |_: Uri| {
            let pipe = pipe.clone();
            async move { ClientOptions::new().open(&pipe).map(TokioIo::new) }
        }))
        .await
}

/// Lazy variant of [`pipe_channel`] — builds the channel without connecting.
#[cfg(windows)]
fn pipe_channel_lazy(pipe: &str) -> Channel {
    use hyper_util::rt::TokioIo;
    use tokio::net::windows::named_pipe::ClientOptions;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let pipe = pipe.to_string();
    Endpoint::from_static("http://localhost").connect_with_connector_lazy(service_fn(
        move |_: Uri| {
            let pipe = pipe.clone();
            async move { ClientOptions::new().open(&pipe).map(TokioIo::new) }
        },
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcClientError {
    #[error("Failed to connect to nodespaced: {0}")]
    Connect(tonic::transport::Error),
}

#[cfg(all(test, unix))]
mod tests {
    use super::resolve_socket_path;

    /// Regression for the false "background service not running" banner: the
    /// daemon-reachability checks must resolve the SAME socket the gRPC client
    /// dials, i.e. honor `NODESPACED_SOCKET` over the default. Single-threaded so
    /// the process-global env mutation doesn't race other tests.
    #[test]
    fn resolve_socket_path_honors_env_override_then_falls_back() {
        let prev = std::env::var_os("NODESPACED_SOCKET");

        std::env::set_var("NODESPACED_SOCKET", "/tmp/ns-demo-a.sock");
        assert_eq!(
            resolve_socket_path(),
            std::path::PathBuf::from("/tmp/ns-demo-a.sock"),
            "NODESPACED_SOCKET override must win"
        );

        std::env::remove_var("NODESPACED_SOCKET");
        let default_path = resolve_socket_path();
        assert!(
            default_path.starts_with(dirs::home_dir().unwrap()),
            "with no override, fall back to a path under HOME"
        );
        assert!(
            default_path.to_string_lossy().contains(".nodespace/daemon"),
            "with no override, fall back to a .nodespace/daemon* socket"
        );

        // Restore prior state so we don't leak into other tests.
        match prev {
            Some(v) => std::env::set_var("NODESPACED_SOCKET", v),
            None => std::env::remove_var("NODESPACED_SOCKET"),
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::{resolve_pipe_name, resolve_socket_path};

    #[test]
    fn resolve_pipe_name_honors_env_override_then_falls_back() {
        let prev = std::env::var_os("NODESPACED_SOCKET");

        std::env::set_var("NODESPACED_SOCKET", r"\\.\pipe\ns-test");
        assert_eq!(
            resolve_pipe_name(),
            r"\\.\pipe\ns-test",
            "NODESPACED_SOCKET override must win"
        );

        std::env::remove_var("NODESPACED_SOCKET");
        assert_eq!(
            resolve_pipe_name(),
            r"\\.\pipe\nodespace-daemon",
            "default pipe name must be correct"
        );

        match prev {
            Some(v) => std::env::set_var("NODESPACED_SOCKET", v),
            None => std::env::remove_var("NODESPACED_SOCKET"),
        }
    }

    #[test]
    fn resolve_socket_path_delegates_to_pipe_name() {
        let prev = std::env::var_os("NODESPACED_SOCKET");
        std::env::remove_var("NODESPACED_SOCKET");

        let path = resolve_socket_path();
        assert_eq!(
            path.to_string_lossy().as_ref(),
            r"\\.\pipe\nodespace-daemon"
        );

        match prev {
            Some(v) => std::env::set_var("NODESPACED_SOCKET", v),
            None => std::env::remove_var("NODESPACED_SOCKET"),
        }
    }
}
