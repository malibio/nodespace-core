/// Generated gRPC types for the `nodespace` proto package.
///
/// Contains client/server stubs and all request/response message types.
/// This crate has no heavy dependencies (no storage engine, no tray-icon, no tokio features).
pub mod nodespace {
    #![allow(clippy::all)]
    tonic::include_proto!("nodespace");
}

/// The gRPC metadata/header key a client sets to route a request at a specific
/// registered local database (ADR-053: "One Daemon, Multiple Local Databases").
///
/// Absent → the daemon serves its default database (single-database clients are
/// unchanged); present but unregistered → the routed handler rejects the request
/// rather than silently serving the default (no cross-database leak). Defined
/// here in the wire-contract crate so every client (CLI, desktop app) and the
/// daemon reference one canonical key.
pub const DATABASE_ID_HEADER: &str = "x-ns-database-id";

/// The gRPC metadata/header key a client sets to identify itself for
/// same-origin write-echo suppression on the `WatchNodes` stream.
///
/// Every write RPC that carries this header is scoped through
/// `NodeService::with_client(id)`, so its emitted events are stamped with
/// `source_client_id = id`. `WatchNodes` reads the same header off the
/// subscribing request and drops any event whose `source_client_id` matches —
/// the daemon, not the frontend, is the authority on "is this my own echo".
///
/// Absent → writes emit no `source_client_id` and are never suppressed on any
/// stream (unchanged pre-issue-1689 behavior). Each process that opens its own
/// long-lived connection to the daemon (one desktop-app window, one CLI
/// invocation, one local-agent session) should generate its own stable id for
/// the lifetime of that connection — never share one across processes, or a
/// genuinely foreign writer's events would be suppressed too.
pub const CLIENT_ID_HEADER: &str = "x-ns-client-id";

pub use nodespace::agent_session_service_client::AgentSessionServiceClient;
pub use nodespace::agent_session_service_server::AgentSessionServiceServer;
pub use nodespace::database_service_client::DatabaseServiceClient;
pub use nodespace::database_service_server::DatabaseServiceServer;
pub use nodespace::embeddings_service_client::EmbeddingsServiceClient;
pub use nodespace::embeddings_service_server::EmbeddingsServiceServer;
pub use nodespace::import_service_client::ImportServiceClient;
pub use nodespace::import_service_server::ImportServiceServer;
pub use nodespace::local_agent_service_client::LocalAgentServiceClient;
pub use nodespace::local_agent_service_server::LocalAgentServiceServer;
pub use nodespace::node_service_client::NodeServiceClient;
pub use nodespace::node_service_server::NodeServiceServer;
pub use nodespace::settings_service_client::SettingsServiceClient;
pub use nodespace::settings_service_server::SettingsServiceServer;
pub use nodespace::{
    AgentAvailability, CaptureContentLevel, CaptureSettingsResponse, CheckAvailabilityRequest,
    CheckAvailabilityResponse, CreateDatabaseRequest, DatabaseInfo, DatabaseStatus,
    GetCaptureSettingsRequest, LaunchSessionRequest, LaunchSessionResponse, ListDatabasesRequest,
    ListDatabasesResponse, ListSessionsRequest, ListSessionsResponse, NodeData,
    RegisterDatabaseRequest, RemoveDatabaseRequest, RemoveDatabaseResponse, RenameDatabaseRequest,
    ResizeRequest, ResizeResponse, SessionInfo, SetDefaultDatabaseRequest, StreamOutputRequest,
    TerminateSessionRequest, TerminateSessionResponse, UpdateCaptureSettingsRequest,
    WriteInputRequest, WriteInputResponse,
};
