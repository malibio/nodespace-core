//! Pro-tier sync commands invoked from the Svelte frontend.
//!
//! All commands no-op (return early with `Ok`) when the Tauri app is
//! running in community mode — i.e. there is no `ProClient` in
//! managed state. That keeps the frontend's invoke calls
//! side-effect-free when probing for sync UI.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::services::pro_client::pb::{InitiateOAuthRequest, WatchSyncStatusRequest};
use crate::services::{ProClient, ProTier};

/// Default cloud-worker URL when the frontend doesn't supply one.
/// Matches `nodespace-sync/cloud-worker`'s default bind
/// (`127.0.0.1:8787`); override via the optional `worker_url` arg.
const DEFAULT_WORKER_URL: &str = "http://127.0.0.1:8787";

/// Flag tracking whether the status-stream task is already running.
/// Module-level so repeated calls to `pro_subscribe_sync_status` from
/// the frontend (e.g. across hot-reloads) don't pile up tasks.
static STREAM_SPAWNED: AtomicBool = AtomicBool::new(false);

/// Snapshot of the most recent tier-detection result. Returned to
/// the frontend on demand so the UI doesn't have to wait for the
/// `pro:tier-detected` Tauri event when re-mounting.
#[tauri::command]
pub async fn pro_tier(app: AppHandle) -> Result<ProTier, String> {
    match app.try_state::<ProClient>() {
        Some(pro) => Ok(pro.tier().await),
        None => Ok(ProTier::Community),
    }
}

/// Start a long-lived `WatchSyncStatus` subscription on the daemon
/// and forward each event to the frontend as a Tauri event named
/// `sync:status`.
///
/// Idempotent: only the first call spawns the task. Subsequent calls
/// return immediately.
#[tauri::command]
pub async fn pro_subscribe_sync_status(app: AppHandle) -> Result<(), String> {
    let Some(pro) = app.try_state::<ProClient>() else {
        // Community mode — nothing to subscribe to.
        return Ok(());
    };
    if STREAM_SPAWNED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let pro: ProClient = (*pro).clone();
    let mut client = pro.client().await;
    let app_handle = app.clone();

    tokio::spawn(async move {
        let stream = match client.watch_sync_status(WatchSyncStatusRequest {}).await {
            Ok(resp) => resp.into_inner(),
            Err(e) => {
                tracing::warn!(error = %e, "sync-status subscribe failed");
                STREAM_SPAWNED.store(false, Ordering::SeqCst);
                return;
            }
        };

        use tokio_stream::StreamExt;
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            match item {
                Ok(evt) => {
                    let payload = serde_json::json!({
                        "state": evt.state,
                        "detail": evt.detail,
                    });
                    if let Err(e) = app_handle.emit("sync:status", payload) {
                        tracing::warn!(error = %e, "failed to emit sync:status");
                        break;
                    }
                }
                Err(status) => {
                    tracing::warn!(error = %status, "sync-status stream item error");
                    break;
                }
            }
        }
        STREAM_SPAWNED.store(false, Ordering::SeqCst);
        tracing::info!("sync-status stream ended");
        // Keep the ProClient alive for the task's lifetime — no
        // explicit drop needed; the move semantics handle it.
        let _ = pro;
    });

    Ok(())
}

/// Kick off the daemon's OAuth PKCE flow. The daemon opens the
/// system browser and listens on a localhost callback; this command
/// returns the attempt ID synchronously. UI tracks progress via the
/// `sync:status` stream wired in `pro_subscribe_sync_status`.
///
/// `worker_url` defaults to `http://127.0.0.1:8787` (the
/// `nodespace-sync/cloud-worker` default). `user_hint` is shown in
/// the worker's login form so users see which account they're
/// signing into; empty string is fine.
#[tauri::command]
pub async fn pro_initiate_oauth(
    app: AppHandle,
    worker_url: Option<String>,
    user_hint: Option<String>,
) -> Result<String, String> {
    let Some(pro) = app.try_state::<ProClient>() else {
        return Err("community tier — Pro sign-in unavailable".into());
    };
    let mut client = pro.client().await;
    let req = InitiateOAuthRequest {
        worker_url: worker_url.unwrap_or_else(|| DEFAULT_WORKER_URL.to_string()),
        user_hint: user_hint.unwrap_or_default(),
    };
    tracing::info!(worker = %req.worker_url, user_hint = %req.user_hint, "Pro: InitiateOAuth");
    let resp = client
        .initiate_o_auth(req)
        .await
        .map_err(|e| format!("InitiateOAuth failed: {e}"))?
        .into_inner();
    Ok(resp.attempt_id)
}
