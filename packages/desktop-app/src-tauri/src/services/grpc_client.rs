//! gRPC client that connects to the external `nodespaced` daemon over a Unix
//! Domain Socket.
//!
//! Socket path resolution order:
//!   1. `NODESPACED_SOCKET` environment variable
//!   2. `~/.nodespace/daemon.sock` (default)
//!
//! The `GrpcClient` is registered as Tauri managed state once and cloned
//! cheaply per command (tonic `Channel` is an `Arc` internally).

use std::sync::Arc;

use nodespace_proto::{
    AgentSessionServiceClient, EmbeddingsServiceClient, ImportServiceClient,
    LocalAgentServiceClient, NodeServiceClient, SettingsServiceClient,
};
use tokio::sync::RwLock;
use tonic::transport::Channel;

struct GrpcClientInner {
    node: NodeServiceClient<Channel>,
    import: ImportServiceClient<Channel>,
    settings: SettingsServiceClient<Channel>,
    embeddings: EmbeddingsServiceClient<Channel>,
    agent_session: AgentSessionServiceClient<Channel>,
    local_agent: LocalAgentServiceClient<Channel>,
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
pub struct GrpcClient {
    inner: Arc<RwLock<GrpcClientInner>>,
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

        let inner = GrpcClientInner {
            node: NodeServiceClient::new(channel.clone()),
            import: ImportServiceClient::new(channel.clone()),
            settings: SettingsServiceClient::new(channel.clone()),
            embeddings: EmbeddingsServiceClient::new(channel.clone()),
            agent_session: AgentSessionServiceClient::new(channel.clone()),
            local_agent: LocalAgentServiceClient::new(channel.clone()),
            channel,
        };

        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Borrow a clone of the `NodeServiceClient`.
    pub async fn client(&self) -> NodeServiceClient<Channel> {
        self.inner.read().await.node.clone()
    }

    /// Borrow a clone of the `ImportServiceClient`.
    pub async fn import_client(&self) -> ImportServiceClient<Channel> {
        self.inner.read().await.import.clone()
    }

    /// Borrow a clone of the `SettingsServiceClient`.
    pub async fn settings_client(&self) -> SettingsServiceClient<Channel> {
        self.inner.read().await.settings.clone()
    }

    /// Borrow a clone of the `EmbeddingsServiceClient`.
    ///
    /// Embeddings are always available in the daemon (unlike the old in-process
    /// optional configuration), so this returns the client directly.
    pub async fn embeddings_client(&self) -> EmbeddingsServiceClient<Channel> {
        self.inner.read().await.embeddings.clone()
    }

    /// Borrow a clone of the `AgentSessionServiceClient`.
    pub async fn agent_session_client(&self) -> AgentSessionServiceClient<Channel> {
        self.inner.read().await.agent_session.clone()
    }

    /// Borrow a clone of the `LocalAgentServiceClient`.
    pub async fn local_agent_client(&self) -> LocalAgentServiceClient<Channel> {
        self.inner.read().await.local_agent.clone()
    }

    /// Clone of the underlying `tonic::transport::Channel`. Used by
    /// `ProClient` so the Pro-tier service rides the same h2
    /// connection (one channel, multiple service surfaces).
    pub async fn channel(&self) -> Channel {
        self.inner.read().await.channel.clone()
    }
}

/// Resolve the daemon socket path.
///
/// Checks `NODESPACED_SOCKET` env var first, then falls back to
/// `~/.nodespace/daemon.sock`.
#[cfg(unix)]
fn resolve_socket_path() -> std::path::PathBuf {
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

#[derive(Debug, thiserror::Error)]
pub enum GrpcClientError {
    #[error("Failed to connect to nodespaced: {0}")]
    Connect(tonic::transport::Error),
}
