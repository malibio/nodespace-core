//! Generated protobuf types and gRPC service definitions for `nodespaced`.
//!
//! All types are generated at build time from:
//!   - `proto/node_service.proto`      — NodeService
//!   - `proto/agent_session_service.proto` — AgentSessionService
//!
//! Both proto files declare `package nodespace`, so all generated types
//! land in the same module.

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
