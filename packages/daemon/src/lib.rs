//! `nodespaced` library surface.
//!
//! The daemon crate ships both a binary (`nodespaced`) and a library so
//! integration tests can spin the gRPC server up in-process without shelling
//! out. Proto types are provided by the `nodespace-proto` crate; this lib
//! re-exports them alongside the service implementations.

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

pub use services::{
    AgentSessionHandler, EmbeddingsServiceImpl, ImportServiceImpl, LocalAgentServiceImpl,
    NodeServiceImpl, SettingsServiceImpl,
};
