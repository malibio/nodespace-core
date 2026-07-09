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

use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_agent::local_agent::otlp_tracer;
use nodespace_daemon::services::embeddings_service::EmbeddingReady;
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{
    build_base_router, build_database_services, build_shared_services, resolve_db_path, tray,
    BaseServices,
};
use tokio::sync::RwLock;
use tonic::transport::Server;

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

/// Binds a Unix domain socket with no window where it is exposed at the
/// ambient umask (ADR-052 §3). `UnixListener::bind` creates the socket file
/// honoring the process umask, so a plain `bind` then `set_permissions` leaves
/// the socket briefly group/other-reachable if the umask is permissive. We
/// narrow the umask to `0o177` (owner rw only) for the duration of the bind
/// so the socket is created at `0o600` from the instant it appears, then
/// restore the prior umask — `umask` is process-global, so the narrowed
/// window must be as short as possible and restored even on error.
///
/// Precondition: callers must not invoke this concurrently with another bind
/// on the same process (`umask` is process-global, not per-thread). Both call
/// sites in this file are safe because `main` runs exactly one of
/// `serve_headless` / `serve_grpc` per process, before any other task binds
/// a socket of its own.
#[cfg(unix)]
fn bind_uds_owner_only(sock: &std::path::Path) -> Result<tokio::net::UnixListener> {
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: umask(2) is a plain libc call; no pointers involved.
    let prev_umask = unsafe { libc::umask(0o177) };
    let result = tokio::net::UnixListener::bind(sock);
    unsafe { libc::umask(prev_umask) };

    let listener =
        result.with_context(|| format!("Failed to bind Unix socket: {}", sock.display()))?;
    // Defense-in-depth: the umask already guarantees 0o600 at creation, but
    // set it explicitly in case the umask was somehow not honored.
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
    // in-flight RPCs would be killed mid-response.
    runtime
        .block_on(grpc_handle)
        .context("gRPC task panicked")?
        .context("gRPC server returned an error")?;

    tracing::info!("nodespaced shutdown complete");
    Ok(())
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

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared.context).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = bind_uds_owner_only(&sock)?;

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let base_services = BaseServices {
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(Server::builder(), base_services)
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await
        .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
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

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared.context).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
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
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(
        Server::builder().layer(TrayMetricsLayer::new(controller)),
        base_services,
    )
    .serve_with_incoming_shutdown(UnixListenerStream::new(listener), combined_shutdown)
    .await
    .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
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

    tracing::info!(db_path = %db_path.display(), pipe = %name, "Starting nodespaced (headless, Windows)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared.context).await?;

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
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(Server::builder(), base_services)
        .serve_with_incoming_shutdown(incoming, async move {
            shutdown.await;
            cancel.cancel();
        })
        .await
        .context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Tray-driven server loop for Windows — uses a Named Pipe instead of UDS.
#[cfg(windows)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio_util::sync::CancellationToken;

    let name = pipe_name();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), pipe = %name, "Starting nodespaced (tray, Windows)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared.context).await?;

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
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(
        Server::builder().layer(TrayMetricsLayer::new(controller)),
        base_services,
    )
    .serve_with_incoming_shutdown(incoming, async move {
        combined_shutdown.await;
        cancel.cancel();
    })
    .await
    .context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// GPU drain protocol: drop processor first (shuts down background task),
/// then release the GPU context from the NLP engine.
async fn drain_gpu(state: Arc<RwLock<Option<EmbeddingReady>>>) {
    let ready = state.write().await.take();
    if let Some(ready) = ready {
        drop(ready.processor);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tracing::info!("Releasing GPU context...");
        ready.embedding_service.nlp_engine().release_gpu_context();
        tracing::info!("GPU context released");
    }
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

    /// `umask` is process-global, so both assertions run in one test to avoid
    /// racing against a second test thread mutating it concurrently.
    ///
    /// Covers: even under a permissive ambient umask (which would otherwise
    /// yield a group/other-reachable socket), the bound socket ends up
    /// owner-only; and the ambient umask is restored afterward so later code
    /// in the process does not inherit the narrowed `0o177`.
    #[tokio::test]
    async fn bind_uds_owner_only_is_0o600_and_restores_umask() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("race-test.sock");

        // SAFETY: umask(2) is a plain libc call with no pointers. This test
        // is the sole owner of process umask mutation for its duration —
        // no other test in this module runs concurrently with it.
        let ambient_prev = unsafe { libc::umask(0o022) };
        let listener = bind_uds_owner_only(&sock).expect("bind should succeed");
        let observed = unsafe { libc::umask(ambient_prev) };

        assert_eq!(
            observed, 0o022,
            "umask must be restored after bind, not left narrowed"
        );

        let mode = std::fs::metadata(&sock)
            .expect("socket file should exist")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "socket must be owner-only regardless of ambient umask"
        );

        drop(listener);
    }
}
