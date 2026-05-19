//! gRPC-backed watcher that bridges `nodespaced`'s `WatchNodes` stream to the
//! Tauri frontend via `app.emit("node:*", ...)`.
//!
//! # Status (issue #1114)
//!
//! This module is **ready-to-use but inert** in the current codebase. It is
//! not started by `lib.rs` because the in-process `DomainEventForwarder` is
//! still the live source of `node:created` / `node:updated` / `node:deleted`
//! Tauri events. Running both simultaneously would double-emit every node
//! event to the frontend.
//!
//! Activation belongs to issue #1113 (Migrate Tauri command handlers to thin
//! gRPC proxy wrappers): once the Tauri process stops holding `NodeService`
//! in-process and talks to `nodespaced` exclusively over gRPC,
//! `DomainEventForwarder` becomes the dead code path and this watcher takes
//! over. The frontend event contract (`node:created` / `node:updated` /
//! `node:deleted` with `NodeIdPayload`) is intentionally identical so that
//! the swap is invisible to Svelte stores.
//!
//! # Behavior
//!
//! - Opens a `WatchNodes` stream against `~/.nodespace/daemon.sock` (or the path
//!   from `NODESPACED_SOCKET`).
//! - Translates each proto `NodeEvent` to a Tauri event with the same payload
//!   shape as `DomainEventForwarder` (id + optional node_type).
//! - On stream error or disconnection, reconnects with exponential backoff
//!   starting at 1 second and capped at 30 seconds.
//! - Exits cleanly when the supplied cancellation token is cancelled.

use std::time::Duration;

use anyhow::{Context, Result};
use nodespace_daemon::nodespace::{node_event::Event as NodeEventKind, WatchRequest};
use nodespace_daemon::NodeServiceClient;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

/// Exponential backoff bounds for reconnection attempts.
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Frontend payload — must stay structurally identical to
/// `services::domain_event_forwarder::NodeIdPayload` so Svelte stores see no
/// difference between the in-process and gRPC event paths.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeIdPayload {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_type: Option<String>,
}

/// Resolve the daemon socket path. Honors `NODESPACED_SOCKET`.
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

/// Spawn the watcher as a Tokio task. Returns immediately; the task runs
/// until `cancel_token` is cancelled or the process exits.
#[cfg(unix)]
pub fn spawn(app: AppHandle, cancel_token: tokio_util::sync::CancellationToken) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app, cancel_token).await {
            error!("Node watcher exited with error: {e:#}");
        } else {
            info!("Node watcher exited cleanly");
        }
    });
}

/// Watcher loop. Connects, streams events, and reconnects with exponential
/// backoff on any failure. Exits when `cancel_token` fires.
#[cfg(unix)]
async fn run(app: AppHandle, cancel_token: tokio_util::sync::CancellationToken) -> Result<()> {
    let sock = socket_path();
    info!("Node watcher starting (sock={})", sock.display());

    let mut backoff = BACKOFF_START;
    loop {
        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("Node watcher received shutdown signal, exiting");
                return Ok(());
            }
            outcome = stream_once(&app, &sock) => {
                match outcome {
                    Ok(()) => {
                        // Server closed the stream cleanly — reconnect immediately
                        // with the backoff reset, since this isn't an error condition.
                        debug!("WatchNodes stream ended; reconnecting");
                        backoff = BACKOFF_START;
                    }
                    Err(e) => {
                        warn!("WatchNodes stream failed: {e:#}; reconnecting in {:?}", backoff);
                    }
                }
            }
        }

        // Wait for backoff or shutdown, whichever comes first.
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("Node watcher cancelled during backoff");
                return Ok(());
            }
            _ = tokio::time::sleep(backoff) => {
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

/// Open a single WatchNodes stream and forward events until the stream ends
/// or errors. Returns `Ok(())` on clean stream end, `Err` on transport or
/// stream error.
#[cfg(unix)]
async fn stream_once(app: &AppHandle, sock: &std::path::Path) -> Result<()> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let sock = sock.to_path_buf();
    let channel = Endpoint::from_static("http://localhost")
        .connect_with_connector(service_fn(move |_: Uri| {
            let sock = sock.clone();
            async move { UnixStream::connect(&sock).await.map(TokioIo::new) }
        }))
        .await
        .with_context(|| "failed to connect to nodespaced")?;
    let mut client = NodeServiceClient::new(channel);

    let mut stream = client
        .watch_nodes(WatchRequest::default())
        .await
        .context("failed to open WatchNodes stream")?
        .into_inner();

    info!("WatchNodes stream open");

    while let Some(item) = stream.next().await {
        let event = item.context("WatchNodes stream returned an error item")?;
        forward(app, event);
    }

    Ok(())
}

/// Translate a proto `NodeEvent` into the corresponding Tauri event.
fn forward(app: &AppHandle, event: nodespace_daemon::nodespace::NodeEvent) {
    let Some(kind) = event.event else {
        debug!("Received NodeEvent with no event variant; ignoring");
        return;
    };

    match kind {
        NodeEventKind::Created(data) => {
            let payload = NodeIdPayload {
                id: data.id.clone(),
                node_type: Some(data.node_type),
            };
            if let Err(e) = app.emit("node:created", &payload) {
                error!("Failed to emit node:created for {}: {e}", data.id);
            }
        }
        NodeEventKind::Updated(data) => {
            // Match DomainEventForwarder: node:updated payload omits node_type
            // because the frontend already knows the type from its cached node.
            let payload = NodeIdPayload {
                id: data.id,
                node_type: None,
            };
            if let Err(e) = app.emit("node:updated", &payload) {
                error!("Failed to emit node:updated for {}: {e}", payload.id);
            }
        }
        NodeEventKind::Deleted(d) => {
            // node_type is required to match the in-process DomainEventForwarder
            // contract — consumers (e.g. collections sidebar) apply type-aware
            // cleanup logic for schema/collection deletions without fetching
            // the already-deleted node.
            let payload = NodeIdPayload {
                id: d.node_id.clone(),
                node_type: Some(d.node_type),
            };
            if let Err(e) = app.emit("node:deleted", &payload) {
                error!("Failed to emit node:deleted for {}: {e}", d.node_id);
            }
        }
    }
}
