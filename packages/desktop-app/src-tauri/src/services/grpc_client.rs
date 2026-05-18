//! In-process gRPC client for the embedded `nodespaced` service.
//!
//! The Tauri app proxies node / collection / schema / settings commands through
//! tonic instead of calling `packages/core` directly. This module:
//!
//!   1. Spawns `NodeServiceImpl`, `ImportServiceImpl`, `EmbeddingsServiceImpl`,
//!      `SettingsServiceImpl`, `AgentSessionHandler`, and `LocalAgentServiceImpl`
//!      from `nodespace-daemon` on a localhost port.
//!   2. Connects clients to that endpoint and stashes the `Channel`
//!      so commands can clone the client cheaply.
//!
//! The same `Arc<NodeService>` AppServices already initializes is reused as
//! the backing implementation, so there is no second database open and no
//! RocksDB lock contention.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use nodespace_agent::acp::context_assembly::GraphContextAssembler;
use nodespace_agent::pty::PtySessionManager;
use nodespace_core::services::{EmbeddingProcessor, NodeEmbeddingService, NodeService};
use nodespace_daemon::{
    AgentSessionHandler, AgentSessionServiceClient, AgentSessionServiceServer,
    EmbeddingsServiceClient, EmbeddingsServiceImpl, EmbeddingsServiceServer, ImportServiceClient,
    ImportServiceImpl, ImportServiceServer, LocalAgentServiceClient, LocalAgentServiceImpl,
    LocalAgentServiceServer, NodeServiceClient, NodeServiceImpl, NodeServiceServer,
    SettingsServiceClient, SettingsServiceImpl, SettingsServiceServer,
};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint, Server};

/// Address pattern that asks the OS to choose a free port. The chosen port is
/// reported via `local_addr()` after the listener binds, then handed to the
/// gRPC client. Using port 0 prevents collisions when the dev workflow runs
/// the standalone `nodespaced` binary in parallel on `[::1]:50051`.
const BIND_ADDR_TEMPLATE: &str = "127.0.0.1:0";

/// Connection timeout for the in-process channel. The server starts in a
/// background task; the channel may be created before the server has
/// finished binding, so we give tonic a brief window to retry.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

struct GrpcClientInner {
    node: NodeServiceClient<Channel>,
    import: ImportServiceClient<Channel>,
    settings: SettingsServiceClient<Channel>,
    embeddings: Option<EmbeddingsServiceClient<Channel>>,
    agent_session: AgentSessionServiceClient<Channel>,
    local_agent: LocalAgentServiceClient<Channel>,
}

/// Managed Tauri state wrapping the gRPC clients.
///
/// `Channel` is cheap to clone (it is an `Arc` internally). Commands clone
/// clients per call since tonic's generated methods take `&mut self`.
pub struct GrpcClient {
    inner: Arc<RwLock<GrpcClientInner>>,
}

impl GrpcClient {
    /// Start the in-process gRPC server and return a connected client.
    ///
    /// `node_service`, `embedding_service`, and `processor` are the same
    /// instances used by the Tauri app — one database, no lock contention.
    /// The server is spawned on a tokio task and runs until the runtime shuts
    /// down.
    pub async fn start(
        node_service: Arc<NodeService>,
        embedding_service: Option<Arc<NodeEmbeddingService>>,
        processor: Option<Arc<EmbeddingProcessor>>,
    ) -> Result<Self, GrpcClientError> {
        let inner = Self::start_server(node_service, embedding_service, processor).await?;
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Core server-start logic.
    async fn start_server(
        node_service: Arc<NodeService>,
        embedding_service: Option<Arc<NodeEmbeddingService>>,
        processor: Option<Arc<EmbeddingProcessor>>,
    ) -> Result<GrpcClientInner, GrpcClientError> {
        let listener = TcpListener::bind(BIND_ADDR_TEMPLATE)
            .await
            .map_err(GrpcClientError::Bind)?;
        let addr: SocketAddr = listener.local_addr().map_err(GrpcClientError::Bind)?;

        tracing::info!(%addr, "Starting in-process gRPC server");

        let node_service_impl =
            NodeServiceImpl::new(node_service.clone(), embedding_service.clone());
        let import_impl = ImportServiceImpl::new(node_service.clone());
        let settings_impl =
            SettingsServiceImpl::with_default_path().map_err(GrpcClientError::InvalidEndpoint)?;
        let incoming = TcpListenerStream::new(listener);

        let embeddings_impl =
            embedding_service
                .as_ref()
                .zip(processor.as_ref())
                .map(|(svc, proc)| {
                    EmbeddingsServiceImpl::new(node_service.clone(), svc.clone(), proc.clone())
                });
        let has_embeddings = embeddings_impl.is_some();

        let pty_manager = Arc::new(PtySessionManager::new());
        let assembler = Arc::new(GraphContextAssembler::new(
            node_service.clone(),
            embedding_service.clone(),
        ));
        let agent_session_impl = AgentSessionHandler::new(pty_manager, assembler);
        let local_agent_impl = LocalAgentServiceImpl::new(node_service.clone());

        tokio::spawn(async move {
            let builder = Server::builder()
                .add_service(NodeServiceServer::new(node_service_impl))
                .add_service(ImportServiceServer::new(import_impl))
                .add_service(SettingsServiceServer::new(settings_impl))
                .add_service(AgentSessionServiceServer::new(agent_session_impl))
                .add_service(LocalAgentServiceServer::new(local_agent_impl));
            let result = if let Some(emb) = embeddings_impl {
                builder
                    .add_service(EmbeddingsServiceServer::new(emb))
                    .serve_with_incoming(incoming)
                    .await
            } else {
                builder.serve_with_incoming(incoming).await
            };
            if let Err(e) = result {
                tracing::error!(error = %e, "In-process gRPC server terminated unexpectedly");
            }
        });

        let endpoint = Endpoint::from_shared(format!("http://{}", addr))
            .map_err(|e| GrpcClientError::InvalidEndpoint(e.to_string()))?
            .connect_timeout(CONNECT_TIMEOUT);

        let channel = endpoint.connect().await.map_err(GrpcClientError::Connect)?;

        let embeddings_client = if has_embeddings {
            Some(EmbeddingsServiceClient::new(channel.clone()))
        } else {
            None
        };

        tracing::info!(%addr, "In-process gRPC client connected");

        Ok(GrpcClientInner {
            node: NodeServiceClient::new(channel.clone()),
            import: ImportServiceClient::new(channel.clone()),
            settings: SettingsServiceClient::new(channel.clone()),
            embeddings: embeddings_client,
            agent_session: AgentSessionServiceClient::new(channel.clone()),
            local_agent: LocalAgentServiceClient::new(channel),
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

    /// Borrow a clone of the `EmbeddingsServiceClient`, if available.
    pub async fn embeddings_client(&self) -> Option<EmbeddingsServiceClient<Channel>> {
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
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcClientError {
    #[error("Failed to bind gRPC listener: {0}")]
    Bind(std::io::Error),

    #[error("Invalid gRPC endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("Failed to connect to in-process gRPC server: {0}")]
    Connect(tonic::transport::Error),
}
