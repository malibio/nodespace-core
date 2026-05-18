//! Tauri commands for the local agent — thin gRPC proxy to LocalAgentService.
//!
//! All session management, model loading, and inference now run in the
//! `nodespaced` daemon. These handlers forward Tauri IPC calls over gRPC and
//! re-emit streaming events to the frontend via Tauri channels.
//!
//! Issue #1137

use crate::agent_events;
use crate::commands::nodes::CommandError;
use crate::services::GrpcClient;
use nodespace_agent::agent_types::{AgentSession, AgentTurnResult, LocalAgentStatus, ModelInfo};
use nodespace_daemon::nodespace::{
    CancelGenerationRequest, EndLocalSessionRequest, EnsureModelReadyRequest,
    GetLocalStatusRequest, GetSessionsRequest, ListModelsRequest, SendLocalMessageRequest,
    StartLocalSessionRequest,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio_stream::StreamExt;

fn grpc_err(msg: impl std::fmt::Display) -> CommandError {
    CommandError {
        message: msg.to_string(),
        code: "GRPC_ERROR".to_string(),
        details: None,
    }
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

/// Get the current status of the local agent.
#[tauri::command]
pub async fn local_agent_status(
    grpc: State<'_, GrpcClient>,
) -> Result<LocalAgentStatus, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let resp = client
        .get_status(GetLocalStatusRequest { session_id: None })
        .await
        .map_err(|e| grpc_err(e.message()))?;
    serde_json::from_str(&resp.into_inner().status_json)
        .map_err(|e| grpc_err(format!("Failed to deserialize status: {e}")))
}

/// Create a new local agent conversation session.
#[tauri::command]
pub async fn local_agent_new_session(
    model_id: String,
    grpc: State<'_, GrpcClient>,
) -> Result<String, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let resp = client
        .start_session(StartLocalSessionRequest { model_id })
        .await
        .map_err(|e| grpc_err(e.message()))?;
    Ok(resp.into_inner().session_id)
}

/// Send a user message to a local agent session.
///
/// Streams `AgentChunk` events from the daemon, translating them into
/// Tauri events on `local-agent://chunk`, `local-agent://status`, and
/// `local-agent://tool` so the frontend receives the same events as before.
///
/// Returns the final `AgentTurnResult` when the turn completes.
#[tauri::command]
pub async fn local_agent_send(
    session_id: String,
    message: String,
    app: AppHandle,
    grpc: State<'_, GrpcClient>,
) -> Result<AgentTurnResult, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let mut stream = client
        .send_message(SendLocalMessageRequest {
            session_id: session_id.clone(),
            message,
        })
        .await
        .map_err(|e| grpc_err(e.message()))?
        .into_inner();

    let mut response_text = String::new();
    let mut prompt_tokens = 0u32;
    let mut completion_tokens = 0u32;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| {
            let msg = e.message().to_string();
            let _ = app.emit(agent_events::LOCAL_AGENT_ERROR, &msg);
            grpc_err(msg)
        })?;

        match chunk.chunk_type.as_str() {
            "token" => {
                if let Some(text) = chunk.token_text {
                    response_text.push_str(&text);
                    #[derive(Serialize)]
                    struct TokenChunk {
                        #[serde(rename = "Token")]
                        token: TokenInner,
                    }
                    #[derive(Serialize)]
                    struct TokenInner {
                        text: String,
                    }
                    let _ = app.emit(
                        agent_events::LOCAL_AGENT_CHUNK,
                        &TokenChunk {
                            token: TokenInner { text },
                        },
                    );
                }
            }
            "tool_call_start" => {
                if let (Some(id), Some(name)) = (chunk.tool_call_id, chunk.tool_name) {
                    #[derive(Serialize)]
                    struct ToolEvent {
                        id: String,
                        name: String,
                    }
                    let _ = app.emit(
                        agent_events::LOCAL_AGENT_TOOL,
                        &ToolEvent {
                            id: id.clone(),
                            name: name.clone(),
                        },
                    );
                    // Also emit as chunk for compatibility
                    #[derive(Serialize)]
                    struct ToolStartChunk {
                        #[serde(rename = "ToolCallStart")]
                        tool_call_start: ToolStartInner,
                    }
                    #[derive(Serialize)]
                    struct ToolStartInner {
                        id: String,
                        name: String,
                    }
                    let _ = app.emit(
                        agent_events::LOCAL_AGENT_CHUNK,
                        &ToolStartChunk {
                            tool_call_start: ToolStartInner { id, name },
                        },
                    );
                }
            }
            "tool_call_args" => {
                if let (Some(id), Some(args_json)) = (chunk.tool_call_id, chunk.tool_args_json) {
                    #[derive(Serialize)]
                    struct ToolArgsChunk {
                        #[serde(rename = "ToolCallArgs")]
                        tool_call_args: ToolArgsInner,
                    }
                    #[derive(Serialize)]
                    struct ToolArgsInner {
                        id: String,
                        args_json: String,
                    }
                    let _ = app.emit(
                        agent_events::LOCAL_AGENT_CHUNK,
                        &ToolArgsChunk {
                            tool_call_args: ToolArgsInner { id, args_json },
                        },
                    );
                }
            }
            "done" => {
                prompt_tokens = chunk.prompt_tokens.unwrap_or(0) as u32;
                completion_tokens = chunk.completion_tokens.unwrap_or(0) as u32;
            }
            "error" => {
                let msg = chunk
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string());
                let _ = app.emit(agent_events::LOCAL_AGENT_ERROR, &msg);
                return Err(grpc_err(msg));
            }
            _ => {}
        }
    }

    Ok(AgentTurnResult {
        response: response_text,
        tool_calls_made: vec![],
        usage: nodespace_agent::agent_types::InferenceUsage {
            prompt_tokens,
            completion_tokens,
        },
    })
}

/// Cancel an in-progress generation for the given session.
#[tauri::command]
pub async fn local_agent_cancel(
    session_id: String,
    grpc: State<'_, GrpcClient>,
) -> Result<(), CommandError> {
    let mut client = grpc.local_agent_client().await;
    client
        .cancel_generation(CancelGenerationRequest { session_id })
        .await
        .map_err(|e| grpc_err(e.message()))?;
    Ok(())
}

/// End and remove a session.
#[tauri::command]
pub async fn local_agent_end_session(
    session_id: String,
    grpc: State<'_, GrpcClient>,
) -> Result<(), CommandError> {
    let mut client = grpc.local_agent_client().await;
    client
        .end_session(EndLocalSessionRequest { session_id })
        .await
        .map_err(|e| grpc_err(e.message()))?;
    Ok(())
}

/// Get all active agent sessions.
#[tauri::command]
pub async fn local_agent_get_sessions(
    grpc: State<'_, GrpcClient>,
) -> Result<Vec<AgentSession>, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let resp = client
        .get_sessions(GetSessionsRequest {})
        .await
        .map_err(|e| grpc_err(e.message()))?
        .into_inner();

    let mut sessions = Vec::new();
    for info in resp.sessions {
        let status: LocalAgentStatus =
            serde_json::from_str(&info.status_json).unwrap_or(LocalAgentStatus::Idle);
        let created_at = info
            .created_at
            .parse()
            .unwrap_or_else(|_| chrono::Utc::now());

        sessions.push(AgentSession {
            id: info.session_id,
            model_id: info.model_id,
            messages: vec![],
            status,
            created_at,
            tool_executions: vec![],
            dynamic_context: None,
        });
    }
    Ok(sessions)
}

// ---------------------------------------------------------------------------
// Model loading
// ---------------------------------------------------------------------------

/// Payload for `model://status` events.
#[derive(Debug, Clone, Serialize)]
struct ModelStatusEvent {
    model_id: String,
    status: String,
    message: Option<String>,
}

/// Payload for `model://download-progress` events.
#[derive(Debug, Clone, Serialize)]
struct DownloadProgressEvent {
    model_id: String,
    bytes_downloaded: i64,
    bytes_total: i64,
}

/// Ensure a model is downloaded, loaded, and the inference engine is ready.
///
/// Streams `ModelLoadProgressEvent` from the daemon and translates them into
/// Tauri events so the frontend sees the same status updates as before.
///
/// Returns `true` if the inference engine was (re-)installed.
#[tauri::command]
pub async fn ensure_model_ready(
    model_id: String,
    app: AppHandle,
    grpc: State<'_, GrpcClient>,
) -> Result<bool, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let mut stream = client
        .ensure_model_ready(EnsureModelReadyRequest {
            model_id: model_id.clone(),
        })
        .await
        .map_err(|e| grpc_err(e.message()))?
        .into_inner();

    let mut engine_swapped = false;

    while let Some(event_result) = stream.next().await {
        let event = event_result.map_err(|e| grpc_err(e.message()))?;

        match event.event_type.as_str() {
            "downloading" => {
                let _ = app.emit(
                    agent_events::MODEL_STATUS,
                    &ModelStatusEvent {
                        model_id: event.model_id.clone(),
                        status: "downloading".to_string(),
                        message: event.message.clone(),
                    },
                );
                if let (Some(dl), Some(tot)) = (event.bytes_downloaded, event.bytes_total) {
                    let _ = app.emit(
                        agent_events::MODEL_DOWNLOAD_PROGRESS,
                        &DownloadProgressEvent {
                            model_id: event.model_id,
                            bytes_downloaded: dl,
                            bytes_total: tot,
                        },
                    );
                }
            }
            "loading" => {
                let _ = app.emit(
                    agent_events::MODEL_STATUS,
                    &ModelStatusEvent {
                        model_id: event.model_id,
                        status: "loading".to_string(),
                        message: event.message,
                    },
                );
            }
            "ready" => {
                engine_swapped = event.engine_swapped.unwrap_or(false);
                let _ = app.emit(
                    agent_events::MODEL_STATUS,
                    &ModelStatusEvent {
                        model_id: event.model_id,
                        status: "ready".to_string(),
                        message: event.message,
                    },
                );
            }
            "error" => {
                let msg = event
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Err(grpc_err(msg));
            }
            _ => {}
        }
    }

    Ok(engine_swapped)
}

/// List all models available in the local catalog.
#[tauri::command]
pub async fn list_local_models(
    grpc: State<'_, GrpcClient>,
) -> Result<Vec<ModelInfo>, CommandError> {
    let mut client = grpc.local_agent_client().await;
    let resp = client
        .list_models(ListModelsRequest {})
        .await
        .map_err(|e| grpc_err(e.message()))?
        .into_inner();

    let models = resp
        .models
        .into_iter()
        .filter_map(|entry| {
            let status = serde_json::from_str(&entry.status_json).ok()?;
            let backend =
                serde_json::from_str(&format!("\"{}\"", entry.backend)).unwrap_or_default();
            Some(ModelInfo {
                id: entry.id,
                name: entry.name,
                family: nodespace_agent::agent_types::ModelFamily::Ollama, // placeholder; daemon owns family
                filename: None,
                size_bytes: entry.size_bytes as u64,
                quantization: entry.quantization,
                url: None,
                sha256: None,
                backend,
                status,
            })
        })
        .collect();

    Ok(models)
}
