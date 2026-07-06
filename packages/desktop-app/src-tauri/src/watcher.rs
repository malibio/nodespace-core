//! gRPC-backed watcher that bridges `nodespaced`'s `WatchNodes` stream to the
//! Tauri frontend via `app.emit("node:*", ...)`.
//!
//! # Status
//!
//! This is the sole, currently-active source of `node:created` /
//! `node:updated` / `node:deleted` / `relationship:*` Tauri events — started
//! unconditionally from `lib.rs`'s setup closure via `watcher::spawn(...)`.
//! There is no in-process forwarder alongside it; the Tauri process talks to
//! `nodespaced` exclusively over gRPC, and this watcher is that seam's event
//! path.
//!
//! # Behavior
//!
//! - Opens a `WatchNodes` stream against the build-variant-scoped socket
//!   (see `daemon_setup::daemon_socket_relative`), or the path from `NODESPACED_SOCKET`.
//! - Translates each proto `NodeEvent` to a Tauri event (id + optional node_type).
//! - On stream error or disconnection, reconnects with exponential backoff
//!   starting at 1 second and capped at 30 seconds.
//! - Exits cleanly when the supplied cancellation token is cancelled.

use std::time::Duration;

use anyhow::{Context, Result};
use nodespace_proto::nodespace::{node_event::Event as NodeEventKind, WatchRequest};
use nodespace_proto::NodeServiceClient;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
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

/// Resolve the daemon socket path. Honors `NODESPACED_SOCKET`, then falls back
/// to the build-variant-scoped default (see daemon_setup::daemon_socket_relative).
#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return std::path::PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(crate::daemon_setup::daemon_socket_relative())
}

/// Spawn the watcher as a Tokio task. Returns immediately; the task runs
/// until `cancel_token` is cancelled or the process exits.
#[cfg(not(unix))]
pub fn spawn(_app: AppHandle, _cancel_token: tokio_util::sync::CancellationToken) {
    // No-op on Windows — watcher uses Unix Domain Socket transport.
}

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
///
/// Generic over `Runtime` (rather than hardcoded to the real `Wry` runtime,
/// like the rest of this module's public API) SOLELY so the ADR-048
/// integration test can drive it against `tauri::test`'s `MockRuntime` — a
/// real event bus with no webview. The production `spawn` entry point above
/// still only ever instantiates this with the real runtime.
#[cfg(unix)]
pub async fn run<R: Runtime>(
    app: AppHandle<R>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<()> {
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
async fn stream_once<R: Runtime>(app: &AppHandle<R>, sock: &std::path::Path) -> Result<()> {
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
fn forward<R: Runtime>(app: &AppHandle<R>, event: nodespace_proto::nodespace::NodeEvent) {
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
            // node:updated payload omits node_type because the frontend
            // already knows the type from its cached node.
            debug!(
                node_id = %data.id,
                node_type = %data.node_type,
                "WatchNodes → emitting node:updated"
            );
            let payload = NodeIdPayload {
                id: data.id,
                node_type: None,
            };
            if let Err(e) = app.emit("node:updated", &payload) {
                error!("Failed to emit node:updated for {}: {e}", payload.id);
            }
        }
        NodeEventKind::Deleted(d) => {
            // node_type is required — consumers (e.g. collections sidebar)
            // apply type-aware cleanup logic for schema/collection deletions
            // without fetching the already-deleted node.
            let payload = NodeIdPayload {
                id: d.node_id.clone(),
                node_type: Some(d.node_type),
            };
            if let Err(e) = app.emit("node:deleted", &payload) {
                error!("Failed to emit node:deleted for {}: {e}", d.node_id);
            }
        }
        // Relationship variants — so cloud-sync / cross-window hierarchy
        // changes reach the frontend's reactiveStructureTree.
        // `properties` arrives JSON-encoded on the wire (proto schema is
        // stable); re-parse it here before emitting so the frontend gets a
        // real object (the `has_child` listener reads `properties.order`).
        NodeEventKind::RelationshipCreated(r) => emit_relationship(app, "relationship:created", r),
        NodeEventKind::RelationshipUpdated(r) => emit_relationship(app, "relationship:updated", r),
        NodeEventKind::RelationshipDeleted(r) => {
            let payload = RelationshipDeletedOut {
                id: r.id.clone(),
                from_id: r.from_id,
                to_id: r.to_id,
                relationship_type: r.relationship_type,
            };
            if let Err(e) = app.emit("relationship:deleted", &payload) {
                error!("Failed to emit relationship:deleted for {}: {e}", r.id);
            }
        }
    }
}

/// Frontend payload for `relationship:created` / `relationship:updated`
/// (camelCase via serde rename) — the shape `tauri-sync-listener.ts` expects.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RelationshipPayloadOut {
    id: String,
    from_id: String,
    to_id: String,
    relationship_type: String,
    properties: serde_json::Value,
}

/// Frontend payload for `relationship:deleted` (no `properties` field).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RelationshipDeletedOut {
    id: String,
    from_id: String,
    to_id: String,
    relationship_type: String,
}

fn emit_relationship<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    r: nodespace_proto::nodespace::RelationshipPayload,
) {
    // `r.properties` arrives JSON-encoded as a string on the wire so
    // the proto schema stays stable across additions to the
    // underlying `serde_json::Value`. If parsing fails, we still
    // emit the event with an empty `{}` so the frontend's
    // `has_child` listener can fall back via `Date.now()` — but
    // surface the parse failure as a warning so the silent
    // ordering-corruption case is visible in logs instead of just
    // showing up as nodes sorted to the tail of their parent.
    let props = match serde_json::from_str(&r.properties) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                rel_id = %r.id,
                rel_type = %r.relationship_type,
                error = %e,
                "Failed to parse relationship properties JSON; emitting empty object"
            );
            serde_json::Value::Object(Default::default())
        }
    };
    let payload = RelationshipPayloadOut {
        id: r.id.clone(),
        from_id: r.from_id,
        to_id: r.to_id,
        relationship_type: r.relationship_type,
        properties: props,
    };
    if let Err(e) = app.emit(name, &payload) {
        error!("Failed to emit {name} for {}: {e}", r.id);
    }
}
