//! `nodespaced` library surface.
//!
//! The daemon crate ships both a binary (`nodespaced`) and a library so
//! integration tests can spin the gRPC server up in-process without shelling
//! out. The library is intentionally thin: it exposes the generated proto
//! module and the service implementations that adapt `nodespace-core` to
//! tonic.

pub mod services;
pub mod tray;

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Resolve the on-disk database path the daemon (and any in-process clients
/// such as the CLI's `diagnostics` subcommand) should consult.
///
/// Honors `NODESPACED_DB_PATH` if set so integration tests and alternate
/// deployments can redirect storage without recompiling; otherwise defaults
/// to `$HOME/.nodespace/daemon-db`.
pub fn resolve_db_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("NODESPACED_DB_PATH") {
        return Ok(PathBuf::from(custom));
    }

    let home = std::env::var("HOME").context(
        "Cannot determine database path: $HOME is unset and NODESPACED_DB_PATH not provided",
    )?;
    Ok(PathBuf::from(home).join(".nodespace").join("daemon-db"))
}

/// Re-exports of prost/tonic generated types for the `nodespace` proto package.
///
/// Includes:
///   - `NodeService` client and server traits
///   - `AgentSessionService` client and server traits
///   - All request/response/event message types
pub mod nodespace {
    #![allow(clippy::all)]
    tonic::include_proto!("nodespace");
}

// Compile-time presence check: if either proto was missing from the combined
// compilation, the corresponding type would be absent and this module would
// fail to compile. No allow attributes needed — pub use is the canonical way
// to re-export and simultaneously verify that generated symbols exist.
pub use nodespace::agent_session_service_client::AgentSessionServiceClient;
pub use nodespace::agent_session_service_server::AgentSessionServiceServer;
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
    CheckAvailabilityResponse, GetCaptureSettingsRequest, LaunchSessionRequest,
    LaunchSessionResponse, ListSessionsRequest, ListSessionsResponse, NodeData, ResizeRequest,
    ResizeResponse, SessionInfo, StreamOutputRequest, TerminateSessionRequest,
    TerminateSessionResponse, UpdateCaptureSettingsRequest, WriteInputRequest, WriteInputResponse,
};

pub use services::{
    AgentSessionHandler, EmbeddingsServiceImpl, ImportServiceImpl, LocalAgentServiceImpl,
    NodeServiceImpl, SettingsServiceImpl,
};
