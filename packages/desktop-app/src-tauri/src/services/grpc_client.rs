//! In-process gRPC client for the embedded `nodespaced` service.
//!
//! During the gRPC migration (Issue #1113) the Tauri app proxies node /
//! collection / schema commands through tonic instead of calling
//! `packages/core` directly. To avoid running a separate `nodespaced` process
//! while migration is in flight, this module:
//!
//!   1. Spawns `NodeServiceImpl`, `ImportServiceImpl`, and `EmbeddingsServiceImpl`
//!      from `nodespace-daemon` on a localhost port.
//!   2. Connects clients to that endpoint and stashes the `Channel`
//!      so commands can clone the client cheaply.
//!
//! The same `Arc<NodeService>` AppServices already initializes is reused as
//! the backing implementation, so there is no second database open and no
//! RocksDB lock contention. Once all command files migrate (#1135–#1138)
//! the in-process server can be replaced by a real `nodespaced` subprocess
//! without touching the Tauri command handlers.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use nodespace_core::services::{EmbeddingProcessor, NodeEmbeddingService, NodeService};
use nodespace_daemon::{
    EmbeddingsServiceClient, EmbeddingsServiceImpl, EmbeddingsServiceServer, ImportServiceClient,
    ImportServiceImpl, ImportServiceServer, NodeServiceClient, NodeServiceImpl, NodeServiceServer,
};
use tokio::net::TcpListener;
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

/// Managed Tauri state wrapping the gRPC clients.
///
/// `Channel` is cheap to clone (it is an `Arc` internally). Commands clone
/// clients per call since tonic's generated methods take `&mut self`.
#[derive(Clone)]
pub struct GrpcClient {
    node: NodeServiceClient<Channel>,
    import: ImportServiceClient<Channel>,
    embeddings: Option<EmbeddingsServiceClient<Channel>>,
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
        let listener = TcpListener::bind(BIND_ADDR_TEMPLATE)
            .await
            .map_err(GrpcClientError::Bind)?;
        let addr: SocketAddr = listener.local_addr().map_err(GrpcClientError::Bind)?;

        tracing::info!(%addr, "Starting in-process gRPC server");

        let node_service_impl =
            NodeServiceImpl::new(node_service.clone(), embedding_service.clone());
        let import_impl = ImportServiceImpl::new(node_service.clone());
        let incoming = TcpListenerStream::new(listener);

        let embeddings_impl =
            embedding_service
                .as_ref()
                .zip(processor.as_ref())
                .map(|(svc, proc)| {
                    EmbeddingsServiceImpl::new(node_service.clone(), svc.clone(), proc.clone())
                });

        tokio::spawn(async move {
            let builder = Server::builder()
                .add_service(NodeServiceServer::new(node_service_impl))
                .add_service(ImportServiceServer::new(import_impl));
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

        let embeddings_client = if embedding_service.is_some() {
            Some(EmbeddingsServiceClient::new(channel.clone()))
        } else {
            None
        };

        tracing::info!(%addr, "In-process gRPC client connected");

        Ok(Self {
            node: NodeServiceClient::new(channel.clone()),
            import: ImportServiceClient::new(channel),
            embeddings: embeddings_client,
        })
    }

    /// Borrow a clone of the `NodeServiceClient`.
    pub fn client(&self) -> NodeServiceClient<Channel> {
        self.node.clone()
    }

    /// Borrow a clone of the `ImportServiceClient`.
    pub fn import_client(&self) -> ImportServiceClient<Channel> {
        self.import.clone()
    }

    /// Borrow a clone of the `EmbeddingsServiceClient`, if available.
    pub fn embeddings_client(&self) -> Option<EmbeddingsServiceClient<Channel>> {
        self.embeddings.clone()
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
