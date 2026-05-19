//! Shared constants used across the application
//!
//! This module contains constants that are used in multiple places
//! to ensure consistency and avoid duplication.

/// Client ID for domain event filtering — prevents UI feedback loops.
/// Used in NodeService calls to identify the Tauri client so domain events
/// originating from this client are filtered out before forwarding to the frontend.
pub const TAURI_CLIENT_ID: &str = "tauri-main";

/// GGUF model filename for nomic-embed-text-v1.5 embeddings (768 dimensions).
/// Used by resolve_bundled_model_path() in commands/db.rs to find the model file.
pub const EMBEDDING_MODEL_FILENAME: &str = "nomic-embed-text-v1.5.Q8_0.gguf";

/// Relative path from HOME to the nodespaced Unix Domain Socket.
/// Shared by daemon_setup (launchd plist) and lib.rs (health check command).
/// watcher.rs uses its own resolver that also honors NODESPACED_SOCKET env override.
pub const DAEMON_SOCKET_RELATIVE: &str = ".nodespace/daemon.sock";
