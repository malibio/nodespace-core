//! Shared gRPC router factory for community and Pro daemons (ADR-043).
//!
//! `build_base_router` is the single source of truth for which services belong
//! in a NodeSpace daemon. The Pro daemon (`nodespaced-pro` in `nodespace-sync`)
//! calls this and chains `.add_service(CloudSyncServiceServer::new(...))` on
//! top. Adding a field to `BaseServices` causes a compile error in
//! `nodespaced-pro` until it provides the new implementation.

use tonic::service::Routes;
use tonic::transport::Server;
use tower::Layer;

use crate::{
    AgentSessionHandler, AgentSessionServiceServer, DatabaseServiceImpl, DatabaseServiceServer,
    EmbeddingsServiceImpl, EmbeddingsServiceServer, ImportServiceImpl, ImportServiceServer,
    LocalAgentServiceImpl, LocalAgentServiceServer, NodeServiceImpl, NodeServiceServer,
    SettingsServiceImpl, SettingsServiceServer,
};

/// All base service implementations required by a NodeSpace daemon.
///
/// Both the community daemon (`nodespaced`) and the Pro daemon
/// (`nodespaced-pro`) construct this struct and pass it to
/// [`build_base_router`]. Pro-specific services are added after.
pub struct BaseServices {
    pub node_service: NodeServiceImpl,
    pub agent_session: AgentSessionHandler,
    pub import: ImportServiceImpl,
    pub settings: SettingsServiceImpl,
    pub local_agent: LocalAgentServiceImpl,
    /// `None` only when no NLP model file exists at daemon startup.
    pub embeddings: Option<EmbeddingsServiceImpl>,
    /// Registry manager for the daemon's local databases (ADR-053). Process-global
    /// (not routed): it operates on the registry itself, not a single database.
    pub database: DatabaseServiceImpl,
}

/// Build the base tonic router with all community services registered.
///
/// Accepts a `Server<L>` (already configured with any transport layers such as
/// `TrayMetricsLayer`) so callers can inject middleware before services are
/// registered. The returned `Router` can be extended with Pro-tier services:
///
/// ```rust,ignore
/// // No middleware:
/// let router = build_base_router(Server::builder(), base_services);
///
/// // With a middleware layer:
/// let router = build_base_router(
///     Server::builder().layer(TrayMetricsLayer::new(controller)),
///     base_services,
/// );
///
/// // Pro extension:
/// let router = build_base_router(Server::builder(), base_services)
///     .add_service(CloudSyncServiceServer::new(cloud_sync));
/// ```
pub fn build_base_router<L>(
    mut server: Server<L>,
    services: BaseServices,
) -> tonic::transport::server::Router<L>
where
    L: Layer<Routes> + Clone,
{
    let router = server
        .add_service(NodeServiceServer::new(services.node_service))
        .add_service(AgentSessionServiceServer::new(services.agent_session))
        .add_service(ImportServiceServer::new(services.import))
        .add_service(SettingsServiceServer::new(services.settings))
        .add_service(LocalAgentServiceServer::new(services.local_agent))
        .add_service(DatabaseServiceServer::new(services.database));

    match services.embeddings {
        Some(emb) => router.add_service(EmbeddingsServiceServer::new(emb)),
        None => router,
    }
}
