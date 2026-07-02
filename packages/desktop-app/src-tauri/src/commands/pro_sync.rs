//! Pro-tier sync commands invoked from the Svelte frontend.
//!
//! All commands no-op (return early with `Ok`) when the Tauri app is
//! running in community mode — i.e. there is no `ProClient` in
//! managed state. That keeps the frontend's invoke calls
//! side-effect-free when probing for sync UI.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::services::pro_client::pb::cloud_sync_service_client::CloudSyncServiceClient;
use crate::services::pro_client::pb::sync_status_event::State as PbState;
use crate::services::pro_client::pb::{
    AcceptInviteRequest, ApproveRequestRequest, CreateInviteRequest, GetIdentityRequest,
    InitiateOAuthRequest, LeaveCollectionRequest, ListInvitesRequest, ListMembersRequest,
    ListRequestsRequest, RemoveMemberRequest, RequestJoinRequest, RevokeInviteRequest,
    SetMemberRequest, SignOutRequest, WatchSyncStatusRequest,
};
use crate::services::{ProClient, ProTier};
use tonic::transport::Channel;

/// Default auth-Worker URL when the frontend doesn't supply one. Release builds
/// hit the deployed canonical domain (`pro.nodespace.ai`, nodespace-cloud#21);
/// debug builds hit the local `wrangler dev` worker (`127.0.0.1:8787`, what
/// `device-sync.sh` runs). Either is overridable via the optional `worker_url` arg.
#[cfg(debug_assertions)]
const DEFAULT_WORKER_URL: &str = "http://127.0.0.1:8787";
#[cfg(not(debug_assertions))]
const DEFAULT_WORKER_URL: &str = "https://pro.nodespace.ai";

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

    // `client` is moved into the spawned task by the closure; the
    // ProClient it was cloned from is `Arc`-backed, so the clone
    // implicit in `pro.client().await` keeps the underlying
    // connection alive for the stream's lifetime — no explicit
    // keep-alive binding is required.
    let mut client = pro.client().await;
    let app_handle = app.clone();

    tokio::spawn(async move {
        let stream = match client.watch_sync_status(WatchSyncStatusRequest {}).await {
            Ok(resp) => resp.into_inner(),
            Err(e) => {
                tracing::debug!(error = %e, "sync-status subscribe failed");
                STREAM_SPAWNED.store(false, Ordering::SeqCst);
                emit_disconnected(&app_handle, format!("sync-status subscribe failed: {e}"));
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
                        "user_email": evt.user_email,
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
        // Tell the frontend the stream is gone so the pill goes grey
        // instead of stuck on the last status the daemon emitted.
        // Without this the Svelte side has no way to distinguish
        // "still connected, just idle" from "stream dropped"; the
        // pill would lie about state until the window is reloaded.
        emit_disconnected(&app_handle, "sync-status stream ended".into());
    });

    Ok(())
}

/// Emit a synthetic `sync:status` event with `state =
/// STATE_DISCONNECTED` so the frontend can return the pill to its
/// grey "Sign in" baseline after the WatchSyncStatus stream ends
/// (subscription failure, daemon stream-close, item error). Without
/// this the UI keeps showing whatever state the daemon last emitted
/// and there's no signal that the stream is gone.
fn emit_disconnected(app: &AppHandle, reason: String) {
    let payload = serde_json::json!({
        "state": PbState::Disconnected as i32,
        "detail": reason,
        "user_email": "",
    });
    if let Err(e) = app.emit("sync:status", payload) {
        tracing::warn!(error = %e, "failed to emit synthetic sync:status DISCONNECTED");
    }
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

/// Sign out of Pro (#199 S6). Tells the daemon to drop its session and wipe the
/// persisted refresh token from the OS keychain, so a restart won't auto-resume.
/// The resulting AUTH_REQUIRED transition flows back through the `sync:status`
/// stream. No-ops in community mode (no `ProClient`), matching the other Pro
/// commands' side-effect-free contract.
#[tauri::command]
pub async fn pro_signout(app: AppHandle) -> Result<(), String> {
    let Some(pro) = app.try_state::<ProClient>() else {
        return Ok(());
    };
    let mut client = pro.client().await;
    client
        .sign_out(SignOutRequest {})
        .await
        .map_err(|e| format!("SignOut failed: {e}"))?;
    tracing::info!("Pro: SignOut");
    Ok(())
}

// --- Team membership commands (M5, #147) ----------------------------------
//
// Thin pass-throughs over the daemon's `CloudSyncService` membership RPCs. The
// daemon forwards the signed-in user's JWT to the matching cloud RPC, so the
// admin / last-admin / open-vs-restricted gates are enforced server-side — these
// commands carry no authority of their own. No collaboration UI is wired yet;
// they exist for tests and a future UI, mirroring the `pro_initiate_oauth`
// shape (resolve the Pro client or fail in community mode, then one RPC).

/// One roster entry returned by [`pro_list_members`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemberDto {
    pub person_id: String,
    /// "admin" | "modify" | "readOnly".
    pub permission: String,
}

/// One pending invite returned by [`pro_list_invites`] (S2, #239).
#[derive(Debug, Clone, serde::Serialize)]
pub struct InviteDto {
    /// uuid — the handle [`pro_revoke_invite`] takes.
    pub id: String,
    /// 64-hex share code (bearer); may be surfaced for the admin to copy.
    pub code: String,
    /// Bound invitee email; empty for a bearer share-code.
    pub email: String,
    /// "admin" | "modify" | "readOnly".
    pub permission: String,
    /// RFC3339; empty when the invite never expires.
    pub expires_at: String,
}

/// One pending join request returned by [`pro_list_requests`] (S2, #239).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RequestDto {
    /// uuid — the handle [`pro_approve_request`] / [`pro_revoke_invite`] take.
    pub id: String,
    /// The requester's person_node_id.
    pub requested_by: String,
    /// RFC3339.
    pub created_at: String,
}

/// The caller's own identity, returned by [`pro_current_person`] (#238/#239).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonDto {
    /// Bound PersonNode id; empty on an un-bound device ("role unknown").
    pub person_id: String,
    /// Signed-in email; empty when signed out.
    pub email: String,
}

/// Resolve the Pro gRPC client, or fail when running in community mode (no
/// `ProClient` in managed state). Mirrors the guard in `pro_initiate_oauth`.
async fn membership_client(app: &AppHandle) -> Result<CloudSyncServiceClient<Channel>, String> {
    match app.try_state::<ProClient>() {
        Some(pro) => Ok(pro.client().await),
        None => Err("community tier — Pro membership operations unavailable".into()),
    }
}

/// Add a member or change their role on a collection (admin only, server-gated).
/// `permission` is "admin" | "modify" | "readOnly".
#[tauri::command]
pub async fn pro_set_member(
    app: AppHandle,
    collection_id: String,
    person_id: String,
    permission: String,
) -> Result<(), String> {
    let mut client = membership_client(&app).await?;
    client
        .set_member(SetMemberRequest {
            collection_id,
            person_id,
            permission,
        })
        .await
        .map_err(|e| format!("SetMember failed: {e}"))?;
    Ok(())
}

/// Remove a member from a collection (admin only; last-admin protected server-side).
#[tauri::command]
pub async fn pro_remove_member(
    app: AppHandle,
    collection_id: String,
    person_id: String,
) -> Result<(), String> {
    let mut client = membership_client(&app).await?;
    client
        .remove_member(RemoveMemberRequest {
            collection_id,
            person_id,
        })
        .await
        .map_err(|e| format!("RemoveMember failed: {e}"))?;
    Ok(())
}

/// Leave a collection the signed-in user belongs to (resolved from their JWT).
#[tauri::command]
pub async fn pro_leave_collection(app: AppHandle, collection_id: String) -> Result<(), String> {
    let mut client = membership_client(&app).await?;
    client
        .leave_collection(LeaveCollectionRequest { collection_id })
        .await
        .map_err(|e| format!("LeaveCollection failed: {e}"))?;
    Ok(())
}

/// List the roster of a collection (member/admin).
#[tauri::command]
pub async fn pro_list_members(
    app: AppHandle,
    collection_id: String,
) -> Result<Vec<MemberDto>, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .list_members(ListMembersRequest { collection_id })
        .await
        .map_err(|e| format!("ListMembers failed: {e}"))?
        .into_inner();
    Ok(resp
        .members
        .into_iter()
        .map(|m| MemberDto {
            person_id: m.person_id,
            permission: m.permission,
        })
        .collect())
}

/// Create an invite (admin only). Returns the invite code. When `email` is set
/// the invite is bound to it; otherwise it's a bearer share-code. `ttl_secs` of
/// `None`/`0` uses the server default (7 days).
#[tauri::command]
pub async fn pro_create_invite(
    app: AppHandle,
    collection_id: String,
    permission: String,
    email: Option<String>,
    ttl_secs: Option<u64>,
) -> Result<String, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .create_invite(CreateInviteRequest {
            collection_id,
            permission,
            email: email.unwrap_or_default(),
            ttl_secs: ttl_secs.unwrap_or(0),
        })
        .await
        .map_err(|e| format!("CreateInvite failed: {e}"))?
        .into_inner();
    Ok(resp.code)
}

/// Redeem an invite code (invitee's own JWT). Returns the joined collection id
/// so the caller can pull it.
#[tauri::command]
pub async fn pro_accept_invite(app: AppHandle, code: String) -> Result<String, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .accept_invite(AcceptInviteRequest { code })
        .await
        .map_err(|e| format!("AcceptInvite failed: {e}"))?
        .into_inner();
    Ok(resp.detail)
}

/// Request to join a restricted collection. Returns the request id.
#[tauri::command]
pub async fn pro_request_join(app: AppHandle, collection_id: String) -> Result<String, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .request_join(RequestJoinRequest { collection_id })
        .await
        .map_err(|e| format!("RequestJoin failed: {e}"))?
        .into_inner();
    Ok(resp.request_id)
}

/// Approve a pending join request (admin only). `permission` of `None`/empty
/// grants the originally-requested tier.
#[tauri::command]
pub async fn pro_approve_request(
    app: AppHandle,
    request_id: String,
    permission: Option<String>,
) -> Result<(), String> {
    let mut client = membership_client(&app).await?;
    client
        .approve_request(ApproveRequestRequest {
            request_id,
            permission: permission.unwrap_or_default(),
        })
        .await
        .map_err(|e| format!("ApproveRequest failed: {e}"))?;
    Ok(())
}

/// List a collection's pending invites (admin only, server-gated). #239.
#[tauri::command]
pub async fn pro_list_invites(
    app: AppHandle,
    collection_id: String,
) -> Result<Vec<InviteDto>, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .list_invites(ListInvitesRequest { collection_id })
        .await
        .map_err(|e| format!("ListInvites failed: {e}"))?
        .into_inner();
    Ok(resp
        .invites
        .into_iter()
        .map(|i| InviteDto {
            id: i.id,
            code: i.code,
            email: i.email,
            permission: i.permission,
            expires_at: i.expires_at,
        })
        .collect())
}

/// List a collection's pending join requests (admin only, server-gated). #239.
#[tauri::command]
pub async fn pro_list_requests(
    app: AppHandle,
    collection_id: String,
) -> Result<Vec<RequestDto>, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .list_requests(ListRequestsRequest { collection_id })
        .await
        .map_err(|e| format!("ListRequests failed: {e}"))?
        .into_inner();
    Ok(resp
        .requests
        .into_iter()
        .map(|r| RequestDto {
            id: r.id,
            requested_by: r.requested_by,
            created_at: r.created_at,
        })
        .collect())
}

/// Revoke a pending invite or join request by id (admin only, server-gated). #239.
#[tauri::command]
pub async fn pro_revoke_invite(app: AppHandle, invite_id: String) -> Result<(), String> {
    let mut client = membership_client(&app).await?;
    client
        .revoke_invite(RevokeInviteRequest { invite_id })
        .await
        .map_err(|e| format!("RevokeInvite failed: {e}"))?;
    Ok(())
}

/// The caller's own identity — bound PersonNode id + signed-in email (#238/#239).
/// Lets the UI tell which roster row is "me" and gate admin controls on the
/// caller's own per-collection role. `person_id` is empty on an un-bound device;
/// the UI treats that as "role unknown" and hides admin controls.
#[tauri::command]
pub async fn pro_current_person(app: AppHandle) -> Result<PersonDto, String> {
    let mut client = membership_client(&app).await?;
    let resp = client
        .get_identity(GetIdentityRequest {})
        .await
        .map_err(|e| format!("GetIdentity failed: {e}"))?
        .into_inner();
    Ok(PersonDto {
        person_id: resp.person_node_id,
        email: resp.email,
    })
}
