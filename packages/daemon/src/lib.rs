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
