//! `nodespaced` library surface.
//!
//! The daemon crate ships both a binary (`nodespaced`) and a library so
//! integration tests can spin the gRPC server up in-process without shelling
//! out. Proto types are provided by the `nodespace-proto` crate; this lib
//! re-exports them alongside the service implementations.

pub mod router;
pub mod services;
pub mod tray;

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Resolve the on-disk database path the daemon (and any in-process clients
/// such as the CLI's `diagnostics` subcommand) should consult.
///
/// Honors `NODESPACED_DB_PATH` if set so integration tests and alternate
/// deployments can redirect storage without recompiling; otherwise defaults
/// to `$HOME/.nodespace/database/nodespace.db`.
pub fn resolve_db_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("NODESPACED_DB_PATH") {
        return Ok(PathBuf::from(custom));
    }

    let home = dirs::home_dir().context(
        "Cannot determine database path: home directory is unknown and NODESPACED_DB_PATH not provided",
    )?;
    Ok(home
        .join(".nodespace")
        .join("database")
        .join("nodespace.db"))
}

// Re-export proto types from the lightweight nodespace-proto crate so existing
// consumers of `nodespace-daemon` types continue to work without changing imports.
pub use nodespace_proto::nodespace;
pub use nodespace_proto::{
    AgentAvailability, AgentSessionServiceClient, AgentSessionServiceServer, CaptureContentLevel,
    CaptureSettingsResponse, CheckAvailabilityRequest, CheckAvailabilityResponse,
    EmbeddingsServiceClient, EmbeddingsServiceServer, GetCaptureSettingsRequest,
    ImportServiceClient, ImportServiceServer, LaunchSessionRequest, LaunchSessionResponse,
    ListSessionsRequest, ListSessionsResponse, LocalAgentServiceClient, LocalAgentServiceServer,
    NodeData, NodeServiceClient, NodeServiceServer, ResizeRequest, ResizeResponse, SessionInfo,
    SettingsServiceClient, SettingsServiceServer, StreamOutputRequest, TerminateSessionRequest,
    TerminateSessionResponse, UpdateCaptureSettingsRequest, WriteInputRequest, WriteInputResponse,
};

pub use router::{build_base_router, BaseServices};
pub use services::{
    build_database_services, build_shared_services, AgentSessionHandler, DatabaseServices,
    EmbeddingsServiceImpl, ImportServiceImpl, LocalAgentServiceImpl, NodeServiceImpl,
    SettingsServiceImpl, SharedContext, SharedServices,
};
