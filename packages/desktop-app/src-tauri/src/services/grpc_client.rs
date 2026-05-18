//! In-process gRPC client for the embedded `nodespaced` service.
//!
//! During the gRPC migration (Issue #1113) the Tauri app proxies node /
//! collection / schema commands through tonic instead of calling
//! `packages/core` directly. To avoid running a separate `nodespaced` process
//! while migration is in flight, this module:
//!
//!   1. Spawns the `NodeServiceImpl` from `nodespace-daemon` on a localhost
//!      port (default `127.0.0.1:50051`).
//!   2. Connects a `NodeServiceClient` to that endpoint and stashes the
//!      `Channel` so commands can clone the client cheaply.
//!
//! The same `Arc<NodeService>` AppServices already initializes is reused as
//! the backing implementation, so there is no second database open and no
//! RocksDB lock contention. Once all command files migrate (#1135–#1138)
//! the in-process server can be replaced by a real `nodespaced` subprocess
//! without touching the Tauri command handlers.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use nodespace_core::services::{NodeEmbeddingService, NodeService};
use nodespace_daemon::{
    ImportServiceClient, ImportServiceImpl, ImportServiceServer, NodeServiceClient,
    NodeServiceImpl, NodeServiceServer,
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
/// the client per call.
#[derive(Clone)]
pub struct GrpcClient {
    node_client: NodeServiceClient<Channel>,
    import_client: ImportServiceClient<Channel>,
}

impl GrpcClient {
    /// Start the in-process gRPC server and return a connected client.
    ///
    /// `node_service` and `embedding_service` are the same instances used by
    /// the unmigrated Tauri command handlers, so there is exactly one
    /// `NodeService` per process. The server is spawned on a tokio task and
    /// runs until the runtime shuts down.
    pub async fn start(
        node_service: Arc<NodeService>,
        embedding_service: Option<Arc<NodeEmbeddingService>>,
    ) -> Result<Self, GrpcClientError> {
        let listener = TcpListener::bind(BIND_ADDR_TEMPLATE)
            .await
            .map_err(GrpcClientError::Bind)?;
        let addr: SocketAddr = listener.local_addr().map_err(GrpcClientError::Bind)?;

        tracing::info!(%addr, "Starting in-process gRPC server");

        let node_impl = NodeServiceImpl::new(Arc::clone(&node_service), embedding_service);
        let import_impl = ImportServiceImpl::new(node_service);
        let incoming = TcpListenerStream::new(listener);

        tokio::spawn(async move {
            if let Err(e) = Server::builder()
                .add_service(NodeServiceServer::new(node_impl))
                .add_service(ImportServiceServer::new(import_impl))
                .serve_with_incoming(incoming)
                .await
            {
                tracing::error!(error = %e, "In-process gRPC server terminated unexpectedly");
            }
        });

        let endpoint = Endpoint::from_shared(format!("http://{}", addr))
            .map_err(|e| GrpcClientError::InvalidEndpoint(e.to_string()))?
            .connect_timeout(CONNECT_TIMEOUT);

        let channel = endpoint.connect().await.map_err(GrpcClientError::Connect)?;
        let node_client = NodeServiceClient::new(channel.clone());
        let import_client = ImportServiceClient::new(channel);

        tracing::info!(%addr, "In-process gRPC client connected");

        Ok(Self {
            node_client,
            import_client,
        })
    }

    /// Borrow a clone of the node service client. Commands should clone per
    /// call because tonic's generated methods take `&mut self`.
    pub fn client(&self) -> NodeServiceClient<Channel> {
        self.node_client.clone()
    }

    /// Borrow a clone of the import service client.
    pub fn import_client(&self) -> ImportServiceClient<Channel> {
        self.import_client.clone()
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
