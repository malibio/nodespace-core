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
//! - Opens a `WatchNodes` stream against `localhost:50051` (or the address
//!   from `NODESPACED_ADDR`).
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

/// Default gRPC endpoint for `nodespaced`. Matches the daemon's
/// `DEFAULT_ADDR` (ADR-031).
const DEFAULT_ENDPOINT: &str = "http://[::1]:50051";

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

/// Resolve the daemon endpoint. Honors `NODESPACED_ADDR` so test setups can
/// redirect to an ephemeral port.
fn endpoint() -> String {
    match std::env::var("NODESPACED_ADDR") {
        Ok(addr) if !addr.is_empty() => format!("http://{}", addr),
        _ => DEFAULT_ENDPOINT.to_string(),
    }
}

/// Spawn the watcher as a Tokio task. Returns immediately; the task runs
/// until `cancel_token` is cancelled or the process exits.
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
async fn run(app: AppHandle, cancel_token: tokio_util::sync::CancellationToken) -> Result<()> {
    let endpoint = endpoint();
    info!("Node watcher starting (endpoint={endpoint})");

    let mut backoff = BACKOFF_START;
    loop {
        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                info!("Node watcher received shutdown signal, exiting");
                return Ok(());
            }
            outcome = stream_once(&app, &endpoint) => {
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
async fn stream_once(app: &AppHandle, endpoint: &str) -> Result<()> {
    let mut client = NodeServiceClient::connect(endpoint.to_string())
        .await
        .with_context(|| format!("failed to connect to nodespaced at {endpoint}"))?;

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
