//! `nodespaced` — background daemon that owns the SQLite database lock and serves
//! NodeSpace operations over gRPC on a Unix Domain Socket.
//!
//! Lifecycle:
//!   1. Initialize tracing.
//!   2. Install signal handlers (fail-fast — a daemon that can't observe
//!      shutdown signals is broken).
//!   3. Open `SqliteStore` (embedded SQLite/libsql) at the configured path.
//!   4. Build `NodeService` from `nodespace-core`.
//!   5. Bring up the system tray on the main thread and spawn the tonic
//!      `NodeService` handler on a worker tokio runtime.
//!   6. Tear down cleanly on `SIGTERM`, `SIGINT`, or "Quit" from the tray.
//!
//! # Trust model (ADR-052)
//!
//! This daemon is single-user-local and has **no in-core authorization layer**
//! — no `user_id`, no actor concept, no per-request ACL. Every request that
//! reaches the gRPC services below is served with the daemon owner's full
//! authority over the entire knowledge graph. That is deliberate, not an
//! omission to be backfilled: for a single-user-local desktop app the **OS
//! socket/pipe permission is the entire authorization boundary**. Whoever can
//! open the socket gets the whole graph.
//!
//! On Unix, [`bind_uds_owner_only`] enforces that boundary as `0o600` with no
//! ambient-umask exposure window. There is currently no peer-credential check
//! (`SO_PEERCRED` / `LOCAL_PEERCRED`) backstopping the file permission — every
//! connection accepted by the listener is handed straight to the tonic
//! server. On Windows the Named Pipe below does **not** yet meet this trust
//! model: it is created with the default DACL and `first_pipe_instance(false)`,
//! so it is reachable by other local principals and squattable (tracked
//! separately; see ADR-052 for the full security review and remediation
//! plan). Do not assume a peer-identity check exists anywhere in this file.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use nodespace_agent::local_agent::otlp_tracer;
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{
    build_base_router, build_shared_services, create_dir_owner_only, resolve_db_path, tray,
    BaseServices, DatabaseManager, DatabaseServiceImpl, DatabaseServices, DbManagerLayer,
    SharedContext,
};
use nodespace_nlp_engine::EmbeddingService;
use tokio::sync::watch;
use tonic::transport::Server;

/// ADR-053: construct the [`DatabaseManager`], register + lazily open the
/// default database, and return the manager together with the default's shared
/// service set. The serve loops clone the per-database impls out of the returned
/// set into [`BaseServices`] and install [`DbManagerLayer`] so requests carrying
/// an `x-ns-database-id` header can reach other registered databases while
/// header-less requests keep hitting the default.
///
/// Opening the default through the manager (rather than building it directly)
/// means the *same* cached service set backs both header-less requests and
/// requests that name the default id explicitly — the file is never opened
/// twice.
/// Keep the tray's Databases submenu in step with the registry for the life of
/// the daemon.
///
/// Pushes the current registry once, then again on every registry or open-set
/// change. Without the follow-ups the submenu shows the registry exactly as it
/// was at daemon boot, so a database created, renamed or removed afterwards — or
/// opened by a switch, or closed by the idle reaper — reads wrong until the
/// daemon restarts.
///
/// The snapshot is re-read after each wake rather than carried on the channel,
/// so a burst of changes collapses into a single refresh.
fn spawn_tray_database_sync(controller: tray::TrayController, manager: Arc<DatabaseManager>) {
    tokio::spawn(async move {
        let mut changes = manager.subscribe_changes();
        controller.databases_changed(manager.list().await);
        while changes.changed().await.is_ok() {
            controller.databases_changed(manager.list().await);
        }
    });
}

async fn open_default_database(
    db_path: &std::path::Path,
    context: SharedContext,
) -> Result<(Arc<DatabaseManager>, Arc<DatabaseServices>)> {
    let manager =
        Arc::new(DatabaseManager::load(DatabaseManager::default_registry_path()?, context).await?);
    // Guard against a registry whose default was seeded with a throwaway temp
    // path (e.g. a test/dev run that redirected the database but not the home
    // dir): the OS purges temp dirs, so serving one silently loses user data.
    // Re-points the default to the standard path before we open it.
    manager.repair_doomed_default(db_path).await?;
    let default_id = manager
        .ensure_default_registered("Default".to_string(), db_path.to_path_buf())
        .await?;
    let bundle = manager.get_or_open(&default_id).await?;
    // Log the path the registry actually resolved the default to — not the
    // boot-time `db_path`, which the registry can and does override.
    if let Some(served) = manager.default_database_path().await {
        tracing::info!(served_db_path = %served.display(), "serving default database");
    }
    Ok((manager, bundle))
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".nodespace")
        .join("daemon.sock")
}

/// Binds a Unix domain socket so no other local user can ever reach it,
/// without mutating any process-global state (ADR-052 §3: the socket mode is
/// the entire local-authorization boundary for the gRPC surface below).
///
/// `UnixListener::bind` creates the socket file honoring the ambient umask, so
/// a plain `bind`-then-`chmod` leaves it briefly at a wider mode. The tempting
/// fix — narrowing the umask around the bind — is not usable here: `umask(2)`
/// is process-global rather than per-thread, so mutating it silently re-modes
/// every file and directory any *other* thread creates while it is narrowed.
/// This bind happens with a multi-threaded Tokio runtime already live (shared
/// services and their background tasks are built first), and `cargo test`
/// runs every test in one process on a thread pool, so that race is real both
/// in production and, more visibly, in the test binary — a umask-narrowing
/// bind test here previously corrupted directories concurrently created by
/// unrelated tests, failing them with a bogus `Permission denied` on a path
/// they had just created themselves.
///
/// The containing directory carries the guarantee instead: callers restrict it
/// to owner-only before binding (`create_dir_owner_only`), so nobody else can
/// traverse into it and the window between `bind` and `chmod` below is
/// unreachable. Because that precondition is the entire basis of the
/// guarantee, it is enforced here fail-closed rather than assumed.
#[cfg(unix)]
fn bind_uds_owner_only(sock: &std::path::Path) -> Result<tokio::net::UnixListener> {
    use std::os::unix::fs::PermissionsExt;

    let dir = sock
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .with_context(|| format!("socket path {} has no directory", sock.display()))?;
    let dir_mode = std::fs::metadata(dir)
        .with_context(|| format!("Failed to stat socket directory: {}", dir.display()))?
        .permissions()
        .mode()
        & 0o777;
    anyhow::ensure!(
        dir_mode & 0o077 == 0,
        "refusing to bind {}: its directory {} is mode {dir_mode:o} — group/other can reach \
         the socket during the window between bind and chmod",
        sock.display(),
        dir.display()
    );

    let listener = tokio::net::UnixListener::bind(sock)
        .with_context(|| format!("Failed to bind Unix socket: {}", sock.display()))?;
    std::fs::set_permissions(sock, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set socket permissions: {}", sock.display()))?;
    Ok(listener)
}

#[cfg(windows)]
fn pipe_name() -> String {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return p;
    }
    r"\\.\pipe\nodespace-daemon".to_string()
}

/// `tao`'s event loop must own the main thread on macOS (NSApplication is
/// main-thread-only). So `main` builds the tokio runtime explicitly, hands
/// it to a worker thread that hosts the gRPC server, and lets `tray::run`
/// take over the main thread.
///
/// Headless mode is supported for systems that don't have a display (Linux
/// CI, headless servers): if `NODESPACED_HEADLESS=1` is set, the tray loop
/// is skipped and we fall back to a pure async `main` that exits on signals.
///
/// Live validation found the tray-mode shutdown sequence can intermittently
/// stall indefinitely after GPU/model teardown completes and before this
/// function's own "shutdown complete" log line -- observed durations ranged
/// from ~24s to several minutes, non-deterministic, root cause not fully
/// pinned down (a background task, e.g. from `SharedServices`, not finishing
/// promptly is the leading suspect, but this has not been conclusively
/// isolated). [`SHUTDOWN_WATCHDOG_TIMEOUT`] bounds how long a deliberate quit
/// (tray "Quit", SIGTERM/SIGINT, or the app's own quit path) waits for
/// graceful shutdown before forcing an exit -- otherwise a hang here means
/// Quit does nothing at all, forever, which is worse than a slightly abrupt
/// exit. Exits `0` (not an error code) even when forced, since this always
/// fires only after the user (or the OS) already asked to quit -- treating it
/// as a "failure" would make launchd's now-conditional `KeepAlive` (see
/// `write_plist` in `daemon_setup.rs`) restart the daemon right back up,
/// undoing the very quit this exists to guarantee.
const SHUTDOWN_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);

fn main() -> Result<()> {
    // Early-exit flags — handled before tracing/runtime init so the installer
    // postinstall script can query these without spinning up the full daemon.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--edition") {
        println!("{}", edition());
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Initialise OTLP tracing when NODESPACE_MLFLOW_URL is set (dev only).
    // Keep the provider alive for the duration of main so the background
    // exporter thread is not torn down prematurely.
    let _otlp_provider = otlp_tracer::init_tracer();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    if headless() {
        return runtime.block_on(async { serve_headless().await });
    }

    // The tray's seed closure runs synchronously when `tray::run` is called,
    // launching the gRPC server on the tokio runtime so the daemon is
    // serving as soon as the tray appears. The returned `JoinHandle` flows
    // back out of `tray::run` once the user picks Quit.
    let runtime_handle = runtime.handle().clone();
    let grpc_handle = tray::run(move |controller| {
        runtime_handle.spawn(async move { serve_grpc(controller).await })
    })?;

    // `tray::run` returned, so the user picked Quit. Wait for the gRPC
    // server to finish draining before we drop the runtime — otherwise
    // in-flight RPCs would be killed mid-response. Bounded by a watchdog
    // (see SHUTDOWN_WATCHDOG_TIMEOUT's doc comment): if graceful shutdown
    // doesn't finish in time, force-exit rather than let a stuck teardown
    // make Quit hang forever.
    let defused = arm_shutdown_watchdog(SHUTDOWN_WATCHDOG_TIMEOUT, || {
        tracing::error!(
            timeout_secs = SHUTDOWN_WATCHDOG_TIMEOUT.as_secs(),
            "Graceful shutdown did not complete in time -- forcing exit. \
             If you see this, please report it: something in daemon \
             teardown is hanging (see core#2353's follow-up)."
        );
        std::process::exit(0);
    });

    runtime
        .block_on(grpc_handle)
        .context("gRPC task panicked")?
        .context("gRPC server returned an error")?;
    defused.store(true, Ordering::SeqCst);

    tracing::info!("nodespaced shutdown complete");
    Ok(())
}

/// Spawns a background thread that runs `on_timeout` once, after `timeout`,
/// unless the returned flag is set to `true` first (by the caller, once the
/// thing being bounded actually finishes). The watchdog thread only reads
/// the flag and calls `on_timeout` — it needs no cooperation from whatever
/// might be stuck, which is the whole point: it still fires even if the
/// bounded work is wedged on a synchronous call that can't be cancelled.
///
/// Split out from `main`'s tray-mode shutdown path so the arm/defuse timing
/// logic itself is unit-testable without needing to trigger a real process
/// exit — production passes `std::process::exit`, tests pass something
/// observable instead.
fn arm_shutdown_watchdog(
    timeout: Duration,
    on_timeout: impl FnOnce() + Send + 'static,
) -> Arc<AtomicBool> {
    let defused = Arc::new(AtomicBool::new(false));
    let watcher = defused.clone();
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        if !watcher.load(Ordering::SeqCst) {
            on_timeout();
        }
    });
    defused
}

#[cfg(test)]
mod shutdown_watchdog_tests {
    use super::arm_shutdown_watchdog;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// The bug this exists to prevent (core#2353's follow-up): if graceful
    /// shutdown never completes, `on_timeout` must still fire so the process
    /// doesn't hang forever.
    #[test]
    fn fires_on_timeout_when_never_defused() {
        let fired = Arc::new(AtomicBool::new(false));
        let fired_writer = fired.clone();
        let _defused = arm_shutdown_watchdog(Duration::from_millis(20), move || {
            fired_writer.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(150));
        assert!(
            fired.load(Ordering::SeqCst),
            "on_timeout must fire once the timeout elapses with no defuse"
        );
    }

    /// The normal, fast-shutdown path: `on_timeout` must NOT fire once the
    /// caller marks the watched work as actually finished.
    #[test]
    fn does_not_fire_once_defused_before_timeout() {
        let fired = Arc::new(AtomicBool::new(false));
        let fired_writer = fired.clone();
        let defused = arm_shutdown_watchdog(Duration::from_millis(100), move || {
            fired_writer.store(true, Ordering::SeqCst);
        });

        defused.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !fired.load(Ordering::SeqCst),
            "on_timeout must not fire once the caller defused it before the deadline"
        );
    }
}

fn headless() -> bool {
    matches!(std::env::var("NODESPACED_HEADLESS").as_deref(), Ok("1"))
}

/// Returns the build edition: "pro" when compiled with `--features pro`, otherwise "community".
fn edition() -> &'static str {
    if cfg!(feature = "pro") {
        "pro"
    } else {
        "community"
    }
}

/// Headless server loop. Used by Linux CI and any environment without a
/// display server. Shutdown is signal-driven (SIGTERM / SIGINT), there is
/// no tray.
#[cfg(unix)]
async fn serve_headless() -> Result<()> {
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(requested_db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (manager, bundle) = open_default_database(&db_path, shared.context.clone()).await?;
    // Reap idle non-default databases so a switched-away database stops consuming
    // compute (ADR-053: per-database compute scoping).
    manager.spawn_idle_reaper();
    let shared_model = shared.context.model.clone();
    let shutdown_manager = manager.clone();

    if let Some(parent) = sock.parent() {
        // Owner-only from birth (and re-restricted if it already existed at a
        // wider mode) — this directory is what `bind_uds_owner_only` checks
        // fail-closed before binding, and what closes the bind-to-chmod window
        // on the socket itself (ADR-052).
        create_dir_owner_only(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = bind_uds_owner_only(&sock)?;

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let base_services = BaseServices {
        node_service: bundle.node_service_grpc.clone(),
        agent_session: bundle.agent_session.clone(),
        import: bundle.import.clone(),
        settings: shared.settings,
        local_agent: bundle.local_agent.clone(),
        embeddings: bundle.embeddings_service_grpc.clone(),
        database: DatabaseServiceImpl::new(manager.clone()),
    };
    build_base_router(
        Server::builder().layer(DbManagerLayer::new(manager)),
        base_services,
    )
    .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
    .await
    .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    // Drain every open database's compute, then release the shared GPU once.
    shutdown_manager.shutdown_all().await;
    release_shared_gpu(&shared_model).await;
    Ok(())
}

/// Tray-driven server loop. Shutdown is owned by [`tray::TrayController`];
/// signal handlers still apply so packaged installs can `kill -TERM` the
/// daemon without going through the menu.
#[cfg(unix)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(requested_db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (manager, bundle) = open_default_database(&db_path, shared.context.clone()).await?;
    // Reap idle non-default databases so a switched-away database stops consuming
    // compute (ADR-053: per-database compute scoping).
    manager.spawn_idle_reaper();
    // Fill the tray's Databases submenu and keep it in step. The registry is built
    // here, after the tray loop is already running, so the tray cannot be handed it
    // at startup — it is told instead. Safe if the tray hasn't finished
    // initializing: the snapshot is held and applied when it does.
    spawn_tray_database_sync(controller.clone(), manager.clone());
    let shared_model = shared.context.model.clone();
    let shutdown_manager = manager.clone();

    if let Some(parent) = sock.parent() {
        // Owner-only from birth (and re-restricted if it already existed at a
        // wider mode) — this directory is what `bind_uds_owner_only` checks
        // fail-closed before binding, and what closes the bind-to-chmod window
        // on the socket itself (ADR-052).
        create_dir_owner_only(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = bind_uds_owner_only(&sock)?;

    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let base_services = BaseServices {
        node_service: bundle.node_service_grpc.clone(),
        agent_session: bundle.agent_session.clone(),
        import: bundle.import.clone(),
        settings: shared.settings,
        local_agent: bundle.local_agent.clone(),
        embeddings: bundle.embeddings_service_grpc.clone(),
        database: DatabaseServiceImpl::new(manager.clone()),
    };
    build_base_router(
        Server::builder()
            .layer(TrayMetricsLayer::new(controller))
            .layer(DbManagerLayer::new(manager)),
        base_services,
    )
    .serve_with_incoming_shutdown(UnixListenerStream::new(listener), combined_shutdown)
    .await
    .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    // Drain every open database's compute, then release the shared GPU once.
    //
    // Timed (core#2353 follow-up): live validation found this tail can
    // intermittently stall for a long time (24s to several minutes observed)
    // with no further log output, root cause not yet pinned down. These two
    // timings are the diagnostic a future occurrence needs -- without them,
    // a hang here is invisible until `main`'s SHUTDOWN_WATCHDOG_TIMEOUT
    // (15s) forces the process to exit anyway.
    let shutdown_started = std::time::Instant::now();
    shutdown_manager.shutdown_all().await;
    tracing::info!(elapsed = ?shutdown_started.elapsed(), "shutdown_all finished");
    let gpu_release_started = std::time::Instant::now();
    release_shared_gpu(&shared_model).await;
    tracing::info!(elapsed = ?gpu_release_started.elapsed(), "release_shared_gpu finished");
    Ok(())
}

/// Newtype so we can implement tonic's `Connected` for `NamedPipeServer`.
/// `NamedPipeServer` is a transport; tonic's blanket bound requires `Connected`
/// on stream items so it can extract peer metadata for request extensions.
#[cfg(windows)]
struct NamedPipeConn(tokio::net::windows::named_pipe::NamedPipeServer);

#[cfg(windows)]
impl tonic::transport::server::Connected for NamedPipeConn {
    type ConnectInfo = ();
    fn connect_info(&self) -> Self::ConnectInfo {}
}

#[cfg(windows)]
impl tokio::io::AsyncRead for NamedPipeConn {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

#[cfg(windows)]
impl tokio::io::AsyncWrite for NamedPipeConn {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

/// Headless server loop for Windows — uses a Named Pipe instead of UDS.
#[cfg(windows)]
async fn serve_headless() -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio_util::sync::CancellationToken;

    let name = pipe_name();
    let db_path = resolve_db_path()?;

    tracing::info!(requested_db_path = %db_path.display(), pipe = %name, "Starting nodespaced (headless, Windows)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (manager, bundle) = open_default_database(&db_path, shared.context.clone()).await?;
    // Reap idle non-default databases so a switched-away database stops consuming
    // compute (ADR-053: per-database compute scoping).
    manager.spawn_idle_reaper();
    let shared_model = shared.context.model.clone();
    let shutdown_manager = manager.clone();

    tracing::info!(pipe = %name, "gRPC server listening (Named Pipe)");

    // CancellationToken is cloned into the acceptor stream so that
    // `server.connect().await` races against shutdown rather than blocking
    // indefinitely after tonic stops polling the stream.
    let cancel = CancellationToken::new();
    let cancel_stream = cancel.clone();
    let incoming = {
        let name = name.clone();
        async_stream::stream! {
            loop {
                // .first_pipe_instance(false): multiple clients connect serially
                // to the same pipe name — each iteration creates a fresh instance.
                // SECURITY GAP (ADR-052): this pipe is created with the default
                // DACL and does not restrict access to the owning user,
                // and `first_pipe_instance(false)` permits another local process to
                // pre-create/squat the pipe name. Unlike the Unix socket path (see
                // `bind_uds_owner_only`), the "OS permission is the authorization
                // boundary" trust model does NOT hold here until both are fixed.
                let server = match ServerOptions::new().first_pipe_instance(false).create(&name) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create Named Pipe server instance");
                        yield Err(e);
                        return;
                    }
                };
                tokio::select! {
                    res = server.connect() => {
                        if let Err(e) = res { yield Err(e); return; }
                        yield Ok::<_, std::io::Error>(NamedPipeConn(server));
                    }
                    _ = cancel_stream.cancelled() => return,
                }
            }
        }
    };

    let base_services = BaseServices {
        node_service: bundle.node_service_grpc.clone(),
        agent_session: bundle.agent_session.clone(),
        import: bundle.import.clone(),
        settings: shared.settings,
        local_agent: bundle.local_agent.clone(),
        embeddings: bundle.embeddings_service_grpc.clone(),
        database: DatabaseServiceImpl::new(manager.clone()),
    };
    build_base_router(
        Server::builder().layer(DbManagerLayer::new(manager)),
        base_services,
    )
    .serve_with_incoming_shutdown(incoming, async move {
        shutdown.await;
        cancel.cancel();
    })
    .await
    .context("gRPC server terminated with error")?;
    // Drain every open database's compute, then release the shared GPU once.
    shutdown_manager.shutdown_all().await;
    release_shared_gpu(&shared_model).await;
    Ok(())
}

/// Tray-driven server loop for Windows — uses a Named Pipe instead of UDS.
#[cfg(windows)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio_util::sync::CancellationToken;

    let name = pipe_name();
    let db_path = resolve_db_path()?;

    tracing::info!(requested_db_path = %db_path.display(), pipe = %name, "Starting nodespaced (tray, Windows)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (manager, bundle) = open_default_database(&db_path, shared.context.clone()).await?;
    // Reap idle non-default databases so a switched-away database stops consuming
    // compute (ADR-053: per-database compute scoping).
    manager.spawn_idle_reaper();
    // Fill the tray's Databases submenu and keep it in step. The registry is built
    // here, after the tray loop is already running, so the tray cannot be handed it
    // at startup — it is told instead. Safe if the tray hasn't finished
    // initializing: the snapshot is held and applied when it does.
    spawn_tray_database_sync(controller.clone(), manager.clone());
    let shared_model = shared.context.model.clone();
    let shutdown_manager = manager.clone();

    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(pipe = %name, "gRPC server listening (Named Pipe)");

    let cancel = CancellationToken::new();
    let cancel_stream = cancel.clone();
    let incoming = {
        let name = name.clone();
        async_stream::stream! {
            loop {
                // See the security-gap note on the equivalent call in `serve_headless`
                // above (ADR-052): this pipe is not yet owner-only.
                let server = match ServerOptions::new().first_pipe_instance(false).create(&name) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create Named Pipe server instance");
                        yield Err(e);
                        return;
                    }
                };
                tokio::select! {
                    res = server.connect() => {
                        if let Err(e) = res { yield Err(e); return; }
                        yield Ok::<_, std::io::Error>(NamedPipeConn(server));
                    }
                    _ = cancel_stream.cancelled() => return,
                }
            }
        }
    };

    let base_services = BaseServices {
        node_service: bundle.node_service_grpc.clone(),
        agent_session: bundle.agent_session.clone(),
        import: bundle.import.clone(),
        settings: shared.settings,
        local_agent: bundle.local_agent.clone(),
        embeddings: bundle.embeddings_service_grpc.clone(),
        database: DatabaseServiceImpl::new(manager.clone()),
    };
    build_base_router(
        Server::builder()
            .layer(TrayMetricsLayer::new(controller))
            .layer(DbManagerLayer::new(manager)),
        base_services,
    )
    .serve_with_incoming_shutdown(incoming, async move {
        combined_shutdown.await;
        cancel.cancel();
    })
    .await
    .context("gRPC server terminated with error")?;
    // Drain every open database's compute, then release the shared GPU once.
    shutdown_manager.shutdown_all().await;
    release_shared_gpu(&shared_model).await;
    Ok(())
}

/// Release the process-global GPU context after every database has been drained
/// (ADR-053: per-database compute scoping).
///
/// `DatabaseManager::shutdown_all` has already dropped each database's embedding
/// processor, so this releases the single shared NLP engine's GPU context
/// exactly once. `release_gpu_context` is one-way and global — it must never run
/// on a per-database close, only here on daemon shutdown. A short settle lets any
/// in-flight batch unwind before the model is torn down.
async fn release_shared_gpu(model: &watch::Receiver<Option<Arc<EmbeddingService>>>) {
    let Some(nlp) = model.borrow().clone() else {
        return; // no model was ever loaded
    };
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    tracing::info!("Releasing GPU context...");
    nlp.release_gpu_context();
    tracing::info!("GPU context released");
}

/// Install the shutdown signal future at boot time so a failure to register
/// the handlers becomes a startup error rather than a silent runtime fault.
///
/// On Unix we listen for SIGTERM and SIGINT. On other platforms we fall back
/// to `tokio::signal::ctrl_c`, which fails synchronously here only if the
/// platform doesn't support it.
#[cfg(unix)]
fn install_shutdown_handler() -> Result<impl std::future::Future<Output = ()>> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?;
    let mut sigint = signal(SignalKind::interrupt()).context("install SIGINT handler")?;

    Ok(async move {
        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received — initiating graceful shutdown"),
            _ = sigint.recv()  => tracing::info!("SIGINT received — initiating graceful shutdown"),
        }
    })
}

#[cfg(not(unix))]
fn install_shutdown_handler() -> Result<impl std::future::Future<Output = ()>> {
    Ok(async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("Ctrl-C received — initiating graceful shutdown"),
            Err(e) => tracing::error!(error = %e, "ctrl_c handler failed; shutting down"),
        }
    })
}

#[cfg(all(test, unix))]
mod uds_permission_tests {
    use super::bind_uds_owner_only;
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    // The UDS is the local authorization boundary (ADR-052): after bind it
    // must be 0o600 regardless of what the directory's own owner-only mode
    // would otherwise have let the ambient umask produce. Deliberately does
    // NOT touch the process umask — see the note on
    // `binding_the_socket_does_not_corrupt_a_concurrently_created_directory`
    // below for why.
    #[tokio::test]
    async fn bind_uds_owner_only_is_0o600() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let sock = dir.path().join("test.sock");

        let listener = bind_uds_owner_only(&sock).expect("bind should succeed");

        assert_eq!(
            mode_of(&sock),
            0o600,
            "socket must be owner-only once bound"
        );
        drop(listener);
    }

    // The owner-only socket directory is what makes the window between `bind`
    // and the chmod harmless, so a directory anyone else can traverse must
    // fail closed rather than bind and hope (ADR-052).
    #[tokio::test]
    async fn bind_uds_owner_only_refuses_a_group_or_other_reachable_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let sock = dir.path().join("test.sock");

        let err = bind_uds_owner_only(&sock).unwrap_err().to_string();

        assert!(
            err.contains("group/other can reach"),
            "a traversable socket directory must be refused, got: {err}"
        );
        assert!(!sock.exists(), "nothing may be bound when the check fails");
    }

    /// Regression test for the flakiness class behind this issue (mirrored
    /// from sync#450/#452, fixed there in sync#456): binding the daemon's UDS
    /// must never perturb ambient-umask-governed directory creation happening
    /// concurrently elsewhere in the process. `cargo test` runs the whole
    /// suite on one process's thread pool, so the old implementation — which
    /// narrowed the process-global umask to `0o177` around `bind()` — could
    /// strip the execute bit off a directory *any other test* created via
    /// `DirBuilder`/`tempfile::tempdir()` (both default to `0o777` shaped only
    /// by the ambient umask) at that instant, leaving it unusable:
    /// `Permission denied (os error 13)` on a path the victim test had just
    /// created itself.
    ///
    /// This test does not mutate the process umask itself — doing so here
    /// would reintroduce exactly that hazard against every other test running
    /// concurrently in this binary. Instead it drives a real bind concurrently
    /// (barrier-synced, on a second OS thread) with a directory creation under
    /// whatever ambient umask this test process already has, and asserts the
    /// concurrently-created directory ends up bit-for-bit identical to one
    /// created with no bind in flight at all — i.e. the bind has zero
    /// observable effect on it. If `bind_uds_owner_only` ever narrows the
    /// process umask again, this has a real (timing-dependent, like the
    /// original bug) chance of catching it; it can never itself corrupt a
    /// sibling test's directory, unlike the mutation it guards against.
    #[tokio::test]
    async fn binding_the_socket_does_not_corrupt_a_concurrently_created_directory() {
        use std::sync::{Arc, Barrier};

        // Baseline: what a solo directory creation looks like under today's
        // ambient umask, no bind happening at all.
        let baseline_parent = tempfile::tempdir().expect("baseline tempdir");
        let baseline_dir = baseline_parent.path().join("baseline");
        std::fs::DirBuilder::new()
            .create(&baseline_dir)
            .expect("baseline mkdir");
        let expected_mode = mode_of(&baseline_dir);
        assert_eq!(
            expected_mode & 0o100,
            0o100,
            "precondition: the ambient test-runner umask must not already strip \
             the owner-execute bit, or this test can't tell corruption from baseline"
        );

        let bind_dir = tempfile::tempdir().expect("bind tempdir");
        std::fs::set_permissions(bind_dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let sock = bind_dir.path().join("concurrent.sock");

        let victim_parent = tempfile::tempdir().expect("victim tempdir");
        let victim_dir = victim_parent.path().join("victim");

        let barrier = Arc::new(Barrier::new(2));
        let victim_barrier = barrier.clone();
        let victim_dir_thread = victim_dir.clone();
        let victim_handle = std::thread::spawn(move || {
            victim_barrier.wait();
            // Mirrors tempfile::tempdir(): DirBuilder at the OS default 0o777,
            // shaped only by the ambient umask — exactly what a concurrently
            // running unrelated test creates.
            std::fs::DirBuilder::new()
                .create(&victim_dir_thread)
                .expect("victim mkdir")
        });

        barrier.wait();
        let listener = bind_uds_owner_only(&sock);
        victim_handle.join().expect("victim thread panicked");
        let listener = listener.expect("bind should succeed");

        assert_eq!(
            mode_of(&victim_dir),
            expected_mode,
            "a directory created concurrently with the UDS bind must come out \
             identical to one created with no bind in flight — a mismatch means \
             bind_uds_owner_only is once again perturbing the process-global umask"
        );

        // The literal symptom from the linked issues: prove the directory is
        // actually usable, not merely correctly-moded.
        std::fs::write(victim_dir.join("f"), b"ok")
            .expect("victim dir must remain traversable/writable after a concurrent bind");

        drop(listener);
    }
}
