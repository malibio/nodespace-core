//! Error type for the transport-agnostic markdown and schema library functions.
//!
//! Replaces the former MCP `MCPError` dependency now that these functions live
//! in core as a normal library (consumed by `nodespace-agent`, `nodespace-daemon`,
//! and benches) rather than behind the deleted MCP JSON-RPC transport.

use std::fmt;

/// Error returned by markdown/schema library functions.
///
/// Consumers only need a `Debug`/`Display` error (the agent formats with `{:?}`,
/// the daemon logs `?e`), so this intentionally stays small.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownError {
    /// Request parameters were missing or malformed.
    InvalidParams(String),
    /// A referenced node could not be found.
    NotFound(String),
    /// Node creation failed.
    CreationFailed(String),
    /// An unexpected internal error occurred.
    Internal(String),
}

impl MarkdownError {
    /// Construct an `InvalidParams` error.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        MarkdownError::InvalidParams(message.into())
    }

    /// Construct an `Internal` error.
    pub fn internal_error(message: impl Into<String>) -> Self {
        MarkdownError::Internal(message.into())
    }

    /// Construct a `NotFound` error for the given node id.
    pub fn node_not_found(node_id: &str) -> Self {
        MarkdownError::NotFound(format!("Node not found: {node_id}"))
    }

    /// Construct a `CreationFailed` error.
    pub fn node_creation_failed(message: impl Into<String>) -> Self {
        MarkdownError::CreationFailed(message.into())
    }

    /// The human-readable message for this error.
    pub fn message(&self) -> &str {
        match self {
            MarkdownError::InvalidParams(m)
            | MarkdownError::NotFound(m)
            | MarkdownError::CreationFailed(m)
            | MarkdownError::Internal(m) => m,
        }
    }
}

impl fmt::Display for MarkdownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarkdownError::InvalidParams(m) => write!(f, "invalid params: {m}"),
            MarkdownError::NotFound(m) => write!(f, "not found: {m}"),
            MarkdownError::CreationFailed(m) => write!(f, "node creation failed: {m}"),
            MarkdownError::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for MarkdownError {}
