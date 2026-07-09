/// Generated gRPC types for the `nodespace` proto package.
///
/// Contains client/server stubs and all request/response message types.
/// This crate has no heavy dependencies (no storage engine, no tray-icon, no tokio features).
pub mod nodespace {
    #![allow(clippy::all)]
    tonic::include_proto!("nodespace");
}

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
