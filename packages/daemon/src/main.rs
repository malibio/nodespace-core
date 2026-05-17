//! `nodespaced` — background daemon that owns the RocksDB lock and serves
//! NodeSpace operations over gRPC on `localhost:50051`.
//!
//! Lifecycle:
//!   1. Initialize tracing.
//!   2. Install signal handlers (fail-fast — a daemon that can't observe
//!      shutdown signals is broken).
//!   3. Open `SurrealStore` (embedded RocksDB) at the configured path.
//!   4. Build `NodeService` from `nodespace-core`.
//!   5. Bring up the system tray on the main thread and spawn the tonic
//!      `NodeService` handler on a worker tokio runtime.
//!   6. Tear down cleanly on `SIGTERM`, `SIGINT`, or "Quit" from the tray.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_core::{NodeService as CoreNodeService, SurrealStore};
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{tray, NodeServiceImpl, NodeServiceServer};
use tonic::transport::Server;

/// Default address the daemon binds to. ADR-031 standardizes on
/// `localhost:50051` for the loopback-only gRPC endpoint.
const DEFAULT_ADDR: &str = "[::1]:50051";

/// Resolve the on-disk database path. Honors `NODESPACED_DB_PATH` if set so
/// integration tests and alternate deployments can redirect storage without
/// recompiling.
fn db_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("NODESPACED_DB_PATH") {
        return Ok(PathBuf::from(custom));
    }

    let home = std::env::var("HOME").context(
        "Cannot determine database path: $HOME is unset and NODESPACED_DB_PATH not provided",
    )?;
    Ok(PathBuf::from(home).join(".nodespace").join("daemon-db"))
}

/// Resolve the daemon's bind address. Honors `NODESPACED_ADDR`.
fn bind_addr() -> Result<SocketAddr> {
    let raw = std::env::var("NODESPACED_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    raw.parse()
        .with_context(|| format!("Invalid NODESPACED_ADDR: {raw}"))
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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

/// Headless server loop. Used by Linux CI and any environment without a
/// display server. Shutdown is signal-driven (SIGTERM / SIGINT), there is
/// no tray.
async fn serve_headless() -> Result<()> {
    let addr = bind_addr()?;
    let db_path = db_path()?;

    tracing::info!(db_path = %db_path.display(), %addr, "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    let service = build_node_service(&db_path).await?;

    tracing::info!(%addr, "gRPC server listening");
    Server::builder()
        .add_service(NodeServiceServer::new(service))
        .serve_with_shutdown(addr, shutdown)
        .await
        .context("gRPC server terminated with error")?;
    Ok(())
}

/// Tray-driven server loop. Shutdown is owned by [`tray::TrayController`];
/// signal handlers still apply so packaged installs can `kill -TERM` the
/// daemon without going through the menu.
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    let addr = bind_addr()?;
    let db_path = db_path()?;

    tracing::info!(db_path = %db_path.display(), %addr, "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    let service = build_node_service(&db_path).await?;

    // `TrayController` is `Clone`; one copy goes to the metrics layer, the
    // other drives the shutdown future.
    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(%addr, "gRPC server listening");
    Server::builder()
        .layer(TrayMetricsLayer::new(controller))
        .add_service(NodeServiceServer::new(service))
        .serve_with_shutdown(addr, combined_shutdown)
        .await
        .context("gRPC server terminated with error")?;
    Ok(())
}

/// Open the database and assemble the gRPC service implementation.
async fn build_node_service(db_path: &std::path::Path) -> Result<NodeServiceImpl> {
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!("Failed to create database parent dir: {}", parent.display())
        })?;
    }

    let mut store = Arc::new(
        SurrealStore::new(db_path.to_path_buf())
            .await
            .context("Failed to initialize SurrealStore")?,
    );

    let node_service = Arc::new(
        CoreNodeService::new(&mut store)
            .await
            .context("Failed to initialize NodeService")?,
    );

    // Embedding service is wired by a follow-up issue. The gRPC `SearchNodes`
    // handler returns `Unavailable` until it is provided.
    Ok(NodeServiceImpl::new(node_service, None))
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
