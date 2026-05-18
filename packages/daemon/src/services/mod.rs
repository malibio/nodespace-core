//! gRPC service implementations exposed by `nodespaced`.
//!
//! Each module wraps a slice of `packages/core` or `packages/agent` business
//! logic and adapts it to the tonic-generated service trait.

pub mod agent_session_service;
pub mod embeddings_service;
pub mod import_service;
pub mod local_agent_service;
pub mod node_service;
pub mod settings_service;

pub use agent_session_service::AgentSessionHandler;
pub use embeddings_service::EmbeddingsServiceImpl;
pub use import_service::ImportServiceImpl;
pub use local_agent_service::LocalAgentServiceImpl;
pub use node_service::NodeServiceImpl;
pub use settings_service::SettingsServiceImpl;
