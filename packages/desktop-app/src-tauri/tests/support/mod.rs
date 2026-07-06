//! Reusable fixture for integration tests that need a real, headless
//! `nodespaced` with a controlled lifecycle (spawn now, bind the socket
//! whenever the caller lets it).
//!
//! This is deliberately narrow: it spawns the real daemon binary against a
//! real socket path and a real temp-dir SQLite store, and gives the caller
//! the socket path plus a handle to kill the process. It does not attempt
//! to be the full ADR-048 Tauri-command harness (state injection, watch
//! streams, multi-client convergence) — those are properly scoped to the
//! node-CRUD round-trip flow that actually needs them.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// A running headless `nodespaced` pointed at a temp-dir socket and database.
/// Killed on drop so a panicking test never leaks the process.
pub struct SpawnedDaemon {
    child: Child,
    pub socket_path: PathBuf,
    _tmp_dir: tempfile::TempDir,
}

impl SpawnedDaemon {
    /// Spawn a real headless `nodespaced` with its socket at a fresh temp path.
    /// Does NOT wait for the socket to be bound — the daemon binds it only
    /// after finishing startup (embedding model load included), so the
    /// window between spawn and bind is exactly the "not ready" state
    /// readiness checks need to observe.
    pub fn spawn() -> Self {
        let tmp_dir = tempfile::tempdir().expect("create temp dir for daemon fixture");
        let socket_path = tmp_dir.path().join("daemon.sock");
        let db_path = tmp_dir.path().join("db");

        let binary = resolve_daemon_binary();
        tracing::info!(binary = %binary.display(), "spawning nodespaced for readiness test");

        let child = Command::new(&binary)
            .env("NODESPACED_SOCKET", &socket_path)
            .env("NODESPACED_DB_PATH", &db_path)
            .env("NODESPACED_HEADLESS", "1")
            .env("RUST_LOG", "warn")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

        Self {
            child,
            socket_path,
            _tmp_dir: tmp_dir,
        }
    }
}

impl Drop for SpawnedDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sets an env var and restores its prior value on drop — including on a
/// panicking test, where plain "restore at the end of the function" code
/// never runs. Tests that mutate a process-global env var (like
/// `NODESPACED_SOCKET`) under a shared mutex should hold one of these for
/// the duration, so a failed assertion can't leak the mutated value to
/// whichever test acquires the mutex next.
pub struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Resolve the `nodespaced` binary to spawn.
///
/// Checked in order:
///   1. `NODESPACED_TEST_BIN` env override
///   2. The triple-suffixed sidecar under `src-tauri/binaries/`, the same
///      path (and naming convention) the TypeScript `DaemonTestHarness`
///      resolves — keeping one binary-provisioning story across both
///      harnesses rather than inventing a second convention.
///
/// Neither is built automatically; this intentionally mirrors the existing
/// e2e harness rather than adding `nodespace-daemon` as a dev-dependency of
/// this crate; the daemon crate pulls in `nodespace-nlp-engine` (llama.cpp
/// bindgen), which would tax every `cargo test` of this crate to save an
/// occasional manual build step.
fn resolve_daemon_binary() -> PathBuf {
    if let Ok(p) = std::env::var("NODESPACED_TEST_BIN") {
        return PathBuf::from(p);
    }

    let triple =
        tauri::utils::platform::target_triple().expect("could not determine host target triple");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sidecar = manifest_dir
        .join("binaries")
        .join(format!("nodespaced-{triple}"));

    assert!(
        sidecar.exists(),
        "nodespaced sidecar not found at {}. Build it with \
         `bun run --cwd packages/desktop-app dev:tauri:sidecars` (or \
         `cargo build -p nodespace-daemon --bin nodespaced` and copy it to \
         that path), or set NODESPACED_TEST_BIN to an existing binary.",
        sidecar.display()
    );

    sidecar
}
