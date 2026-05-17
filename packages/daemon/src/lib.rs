//! `nodespaced` library surface.
//!
//! The daemon crate ships both a binary (`nodespaced`) and a library so
//! integration tests can spin the gRPC server up in-process without shelling
//! out. The library is intentionally thin: it exposes the generated proto
//! module and the service implementations that adapt `nodespace-core` to
//! tonic.

pub mod services;
pub mod tray;

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
pub use nodespace::node_service_client::NodeServiceClient;
pub use nodespace::node_service_server::NodeServiceServer;
pub use nodespace::{NodeData, SessionInfo};

pub use services::NodeServiceImpl;
