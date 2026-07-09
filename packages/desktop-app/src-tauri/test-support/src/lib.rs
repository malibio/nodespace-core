//! Reusable fixtures for ADR-048 Tauri-seam integration tests: a real,
//! headless `nodespaced` with a controlled lifecycle (spawn now, bind the
//! socket whenever the caller lets it), plus a `GrpcClient` connected to it
//! so tests can call the actual `#[tauri::command]` handler functions
//! directly — no webview.
//!
//! `tauri::State<'_, T>` has no public constructor outside Tauri's own IPC
//! machinery (its inner field is private, and there is no `From<&T>` impl —
//! confirmed against the `tauri` 2.11 source). The sanctioned way to get one
//! is `Manager::state::<T>()` after `Manager::manage(state)`, both of which
//! `tauri::test::mock_app()`'s `App<MockRuntime>` implements — and neither
//! needs a `WebviewWindow`. So every test uses `mock_app()` + `.manage(client)`
//! + `.state()` to obtain a real `State<GrpcClient>`, never a hand-built one.
//!
//! This also means the same `app` doubles as the real event bus the
//! optimistic-echo-race test needs (`Emitter`/`Listener`), so there is only
//! one fixture shape, not two.
//!
//! This is its own crate (rather than a `mod support;` duplicated into each
//! `tests/*.rs` integration test binary) so clippy's dead-code lint sees one
//! compilation of these helpers used across every test file, instead of
//! flagging whichever subset any single test binary doesn't happen to call
//! as unused — `-D warnings` in `rust:lint` would otherwise hard-fail on
//! that per-binary view.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nodespace_app_lib::services::GrpcClient;
use tauri::Manager;

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

        let mut child = Command::new(&binary)
            .env("NODESPACED_SOCKET", &socket_path)
            .env("NODESPACED_DB_PATH", &db_path)
            .env("NODESPACED_HEADLESS", "1")
            .env("RUST_LOG", "warn")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

        // MUST drain both pipes continuously: `Stdio::piped()` gives each a
        // small OS pipe buffer (~64KB), and llama.cpp's own stderr logging
        // (Metal kernel compilation alone is dozens of lines per model load,
        // outside the `tracing`/`RUST_LOG` filter entirely) fills that buffer
        // during any test that loads a real model. Once full, the daemon
        // blocks on its next write() call and the whole test hangs forever —
        // this is not hypothetical, it reproduced directly while writing the
        // ai-chat integration test. E2E_VERBOSE prints instead of discarding,
        // matching the TypeScript harness's same-named escape hatch.
        let verbose = std::env::var_os("E2E_VERBOSE").is_some();
        if let Some(stdout) = child.stdout.take() {
            spawn_drain_thread(stdout, "stdout", verbose);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_drain_thread(stderr, "stderr", verbose);
        }

        Self {
            child,
            socket_path,
            _tmp_dir: tmp_dir,
        }
    }
}

/// Continuously read `pipe` to EOF on a background thread, either discarding
/// it or echoing it line-by-line to this process's own stderr under
/// `E2E_VERBOSE`. Must run for the daemon's entire lifetime — dropping the
/// thread (not reading to EOF) reintroduces the exact deadlock this exists
/// to prevent.
fn spawn_drain_thread(pipe: impl Read + Send + 'static, label: &'static str, verbose: bool) {
    std::thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if verbose {
                eprintln!("[nodespaced {label}] {line}");
            }
        }
    });
}

impl Drop for SpawnedDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The ONE process-wide lock for every test in this crate's consumers that
/// mutates the process-global `NODESPACED_SOCKET` env var — whether via
/// `GrpcClient::connect()` (which resolves it internally; there is no
/// per-call socket argument), `GrpcClient::connect_lazy()`, or a raw
/// `EnvGuard::set` for a Tauri command that reads it directly (e.g.
/// `check_daemon_status`). Without this mutex, two
/// `#[tokio::test]`s running concurrently — the default — can interleave
/// their env mutations, so one test's client ends up dialing whichever
/// socket happened to be live at the moment its call actually resolved the
/// env var, not the daemon its own test spawned. Held across `.await`
/// points, so this must be a `tokio::sync::Mutex`, not `std::sync::Mutex`.
///
/// `watcher::run` no longer resolves `NODESPACED_SOCKET` itself — it rides the
/// shared `GrpcClient` channel, which is built from the env var once at
/// `GrpcClient::connect()` time. Tests that spawn the watcher must therefore
/// hold a guard from `hold_connect_mutex_and_socket_env` across the
/// `GrpcClient` connection (and, conservatively, for the watcher's lifetime)
/// so the shared channel dials the daemon the test spawned — see
/// `optimistic_echo_race_test.rs`.
///
/// `pub` so every `tests/*.rs` file that mutates `NODESPACED_SOCKET` shares
/// this ONE lock rather than each defining its own same-purpose mutex under
/// a different name (`daemon_readiness_test.rs` used to define its own
/// `ENV_MUTEX` before this was made shared — harmless in practice, since
/// each `tests/*.rs` file compiles to its own process and the env var is
/// process-local, but a duplicated concept under two names invites a third
/// file inventing a fourth).
pub static CONNECT_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Wait until the daemon's socket is reachable, then connect a real
/// `GrpcClient` to it — the same client type `#[tauri::command]` handlers
/// take as `State<'_, GrpcClient>`.
async fn connected_client(daemon: &SpawnedDaemon, timeout: Duration) -> GrpcClient {
    use nodespace_app_lib::daemon_setup::{wait_for_daemon, DaemonStatus};

    let _mutex_guard = CONNECT_MUTEX.lock().await;
    let _env_guard = EnvGuard::set("NODESPACED_SOCKET", &daemon.socket_path);

    let status = wait_for_daemon(&daemon.socket_path, timeout).await;
    assert_eq!(
        status,
        DaemonStatus::Healthy,
        "daemon never became healthy at {}",
        daemon.socket_path.display()
    );

    GrpcClient::connect()
        .await
        .unwrap_or_else(|e| panic!("failed to connect GrpcClient to spawned daemon: {e}"))
}

/// Holds `CONNECT_MUTEX` AND keeps `NODESPACED_SOCKET` set to `daemon`'s
/// socket for as long as the returned guard lives — for tests that spawn
/// `watcher::run`. The watcher rides the shared `GrpcClient` channel (built
/// from the env var at `connect()` time), so keeping the socket env stable
/// across the connection guarantees the channel dials the daemon the test
/// spawned. Drop the returned guard only after the watcher task has been
/// cancelled and joined.
pub async fn hold_connect_mutex_and_socket_env(
    daemon: &SpawnedDaemon,
) -> (tokio::sync::MutexGuard<'static, ()>, EnvGuard) {
    let mutex_guard = CONNECT_MUTEX.lock().await;
    let env_guard = EnvGuard::set("NODESPACED_SOCKET", &daemon.socket_path);
    (mutex_guard, env_guard)
}

/// A `tauri::test::mock_app()` (`MockRuntime`, no webview, no display) with a
/// real `GrpcClient` connected to `daemon` registered as managed state, so
/// tests can obtain `tauri::State<GrpcClient>` the sanctioned way —
/// `Manager::state()` — and call `#[tauri::command]` handler functions
/// directly. Also usable as the real Tauri event bus (`app.emit`/`app.listen`)
/// for tests that need to observe the watcher's forwarded events.
pub struct TauriTestApp {
    pub app: tauri::App<tauri::test::MockRuntime>,
}

impl TauriTestApp {
    /// Build a mock app and connect+manage a `GrpcClient` against `daemon`.
    pub async fn connect(daemon: &SpawnedDaemon, timeout: Duration) -> Self {
        let client = connected_client(daemon, timeout).await;
        let app = tauri::test::mock_app();
        app.manage(client);
        Self { app }
    }

    /// The managed `GrpcClient` as the `State<'_, GrpcClient>` a
    /// `#[tauri::command]` handler expects as its first argument.
    pub fn client_state(&self) -> tauri::State<'_, GrpcClient> {
        self.app.state::<GrpcClient>()
    }

    /// The real `AppHandle` — for commands (e.g. `chat_model_download`,
    /// `local_agent`'s streaming subscriptions) that take `app: AppHandle`
    /// to `emit` progress events, and for tests that `listen` on it.
    pub fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
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

/// True if `~/.nodespace/models/<filename>` already exists — the same
/// resolution `GgufModelManager::new()` uses (`$HOME/.nodespace/models`).
/// Tests that need a REAL loaded model (ai-chat send, model-download
/// terminal state) call this to skip cleanly with a clear message on a
/// machine that hasn't downloaded the model, instead of silently kicking
/// off a live multi-gigabyte fetch from HuggingFace and hanging the pre-push
/// gate for anyone but the one machine that already has it cached. There is
/// no provisioning step in `bun install`/`test-gate.ts` for this file today —
/// see the panic message for how to fetch it.
pub fn model_file_available(filename: &str) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    PathBuf::from(home)
        .join(".nodespace")
        .join("models")
        .join(filename)
        .exists()
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
        .join("..")
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
