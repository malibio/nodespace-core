//! `nodespaced` — background daemon that owns the RocksDB lock and serves
//! NodeSpace operations over gRPC on `localhost:50051`.
//!
//! Lifecycle:
//!   1. Initialize tracing.
//!   2. Open `SurrealStore` (embedded RocksDB) at the configured path.
//!   3. Build `NodeService` from `nodespace-core`.
//!   4. Register the tonic `NodeService` handler and `serve_with_shutdown`.
//!   5. Tear down cleanly on `SIGTERM` or `SIGINT`.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_core::{NodeService as CoreNodeService, SurrealStore};
use nodespace_daemon::{NodeServiceImpl, NodeServiceServer};
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
        .with_context(|| format!("Invalid NODESPACED_ADDR: {}", raw))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let addr = bind_addr()?;
    let db_path = db_path()?;

    tracing::info!(?db_path, %addr, "Starting nodespaced");

    // Ensure parent directory exists before opening the store
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create database parent dir: {:?}", parent))?;
    }

    let mut store = Arc::new(
        SurrealStore::new(db_path.clone())
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
    let service = NodeServiceImpl::new(node_service, None);

    tracing::info!(%addr, "gRPC server listening");

    Server::builder()
        .add_service(NodeServiceServer::new(service))
        .serve_with_shutdown(addr, shutdown_signal())
        .await
        .context("gRPC server terminated with error")?;

    tracing::info!("nodespaced shutdown complete");
    Ok(())
}

/// Future that resolves on the first `SIGTERM` or `SIGINT`. The tonic server's
/// `serve_with_shutdown` then drains in-flight requests, stops accepting new
/// connections, and drops the service handle — which releases the RocksDB lock.
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "Failed to install SIGTERM handler");
            return;
        }
    };
    let mut sigint = match signal(SignalKind::interrupt()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "Failed to install SIGINT handler");
            return;
        }
    };

    tokio::select! {
        _ = sigterm.recv() => tracing::info!("SIGTERM received — initiating graceful shutdown"),
        _ = sigint.recv()  => tracing::info!("SIGINT received — initiating graceful shutdown"),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %e, "Failed to wait for ctrl-c");
    } else {
        tracing::info!("Ctrl-C received — initiating graceful shutdown");
    }
}
