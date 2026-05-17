//! gRPC service implementations exposed by `nodespaced`.
//!
//! Each module wraps a slice of `packages/core` business logic and adapts it
//! to the tonic-generated service trait.

pub mod node_service;

pub use node_service::NodeServiceImpl;
