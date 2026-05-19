//! Shared constants used across the application.

/// Relative path from HOME to the nodespaced Unix Domain Socket.
/// Shared by daemon_setup (launchd plist) and lib.rs (health check command).
/// watcher.rs uses its own resolver that also honors NODESPACED_SOCKET env override.
pub const DAEMON_SOCKET_RELATIVE: &str = ".nodespace/daemon.sock";
