//! Opt-in session capture: creates an `ai-chat` node at PTY session end.
//!
//! [`CaptureService::finalize`] is called by the agent session handler after
//! the PTY process exits. It reads capture settings from the daemon config and,
//! when `capture.enabled = true`, assembles an `ai-chat` node payload and
//! writes it via `NodeService`.
//!
//! The call is fire-and-forget from the session lifecycle perspective: any
//! error is logged but does not surface to the user or block teardown.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use nodespace_agent::pty::{ExitStatus, SessionCapture};
use nodespace_core::services::{CreateNodeParams, NodeService as CoreNodeService};
use serde_json::json;
use uuid::Uuid;

use crate::services::settings_service::{read_capture_settings, CaptureContentSetting};

/// Parameters describing a completed PTY session.
pub struct CompletedSession {
    pub id: Uuid,
    pub agent_type: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub exit_status: ExitStatus,
}

/// Attempt to create an `ai-chat` node for a completed session.
///
/// Returns `Ok(Some(node_id))` if a node was created, `Ok(None)` if capture is
/// disabled, or `Err` on a config/ops failure. Callers should log errors and
/// continue — failed capture must not affect session teardown.
pub async fn finalize_capture(
    session: &CompletedSession,
    capture: &SessionCapture,
    node_service: &Arc<CoreNodeService>,
    config_path: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    let settings = read_capture_settings(config_path).await?;

    if !settings.enabled {
        return Ok(None);
    }

    let content_level: CaptureContentSetting = settings.content;

    let content = format!(
        "{} session — {}",
        session.agent_type,
        session.started_at.format("%Y-%m-%d %H:%M UTC")
    );

    let mut properties = json!({
        "agent_type": session.agent_type,
        "started_at": session.started_at.to_rfc3339(),
        "ended_at": session.ended_at.to_rfc3339(),
        "exit_code": session.exit_status.code,
        "agent_session_id": session.id.to_string(),
        "provider": "native",
        "model": session.agent_type,
        "status": "completed",
        "last_active": session.ended_at.to_rfc3339(),
        "context_tokens": 0,
        "created_nodes": [],
        "messages": [],
    });

    if matches!(
        content_level,
        CaptureContentSetting::Summary | CaptureContentSetting::Full
    ) {
        let summary = capture.summary();
        properties["summary"] = json!(summary);
    }

    if content_level == CaptureContentSetting::Full {
        let transcript = capture.transcript();
        properties["transcript"] = json!(transcript);
    }

    let node_id = node_service
        .create_node_with_parent(CreateNodeParams {
            id: None,
            node_type: "ai-chat".to_string(),
            content,
            parent_id: None,
            insert_after_node_id: None,
            properties,
        })
        .await
        .map_err(|e| anyhow::anyhow!("capture: failed to create ai-chat node: {}", e))?;

    tracing::info!(
        session_id = %session.id,
        node_id = %node_id,
        "session capture: created ai-chat node"
    );

    Ok(Some(node_id))
}
