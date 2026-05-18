//! tonic `AgentSessionService` implementation backed by `PtySessionManager`.
//!
//! Each RPC adapts a proto request into the corresponding `PtySessionManager`
//! call and converts results back into proto messages. `StreamOutput` is the
//! only server-streaming RPC: it subscribes to the session's broadcast channel
//! and forwards [`OutputChunk`](crate::nodespace::OutputChunk) messages until
//! the session closes or the client disconnects.
//!
//! The handler owns `Arc` handles to the shared engine state so it stays cheap
//! to construct and clone, and so multiple concurrent RPCs see the same set of
//! sessions.

use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use nodespace_agent::acp::context_assembly::GraphContextAssembler;
use nodespace_agent::agent_types::AgentType;
use nodespace_agent::pty::PtySessionManager;
use tokio::sync::broadcast::error::RecvError;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::nodespace::{
    agent_session_service_server::AgentSessionService, LaunchSessionRequest, LaunchSessionResponse,
    ListSessionsRequest, ListSessionsResponse, OutputChunk, ResizeRequest, ResizeResponse,
    SessionInfo, StreamOutputRequest, TerminateSessionRequest, TerminateSessionResponse,
    WriteInputRequest, WriteInputResponse,
};

/// gRPC adapter that owns shared handles to the PTY engine.
pub struct AgentSessionHandler {
    manager: Arc<PtySessionManager>,
    assembler: Arc<GraphContextAssembler>,
}

impl AgentSessionHandler {
    pub fn new(manager: Arc<PtySessionManager>, assembler: Arc<GraphContextAssembler>) -> Self {
        Self { manager, assembler }
    }
}

#[tonic::async_trait]
impl AgentSessionService for AgentSessionHandler {
    async fn launch_session(
        &self,
        request: Request<LaunchSessionRequest>,
    ) -> Result<Response<LaunchSessionResponse>, Status> {
        let req = request.into_inner();

        let agent_type = parse_agent_type(&req.agent_type).map_err(Status::invalid_argument)?;
        let id = self
            .manager
            .launch(agent_type, req.prompt, &self.assembler)
            .await
            .map_err(|e| Status::internal(format!("launch session failed: {e}")))?;

        // Apply requested dimensions if the caller passed non-zero values.
        // The engine defaults to 80x24 at spawn — callers resize as soon as
        // they know the real terminal geometry.
        if req.cols != 0 && req.rows != 0 {
            apply_resize(&self.manager, &id, req.cols, req.rows).await?;
        }

        let created_at = current_unix_secs();
        Ok(Response::new(LaunchSessionResponse {
            session_id: id.to_string(),
            created_at,
        }))
    }

    type StreamOutputStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<OutputChunk, Status>> + Send + 'static>>;

    async fn stream_output(
        &self,
        request: Request<StreamOutputRequest>,
    ) -> Result<Response<Self::StreamOutputStream>, Status> {
        let id =
            parse_session_id(&request.into_inner().session_id).map_err(Status::invalid_argument)?;
        let session = self
            .manager
            .get(&id)
            .await
            .ok_or_else(|| Status::not_found(format!("session not found: {id}")))?;

        let mut rx = session.subscribe_output();

        let stream = async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(chunk) => {
                        let timestamp_ms = chunk.timestamp.timestamp_millis();
                        yield Ok(OutputChunk {
                            data: chunk.data,
                            timestamp_ms,
                        });
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        // Client (or this handler) fell behind the broadcast
                        // buffer. Continue draining rather than tearing the
                        // stream down — losing a render frame is preferable
                        // to losing the whole session view.
                        tracing::warn!(
                            session_id = %id,
                            skipped,
                            "StreamOutput subscriber lagged; some chunks dropped"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn write_input(
        &self,
        request: Request<WriteInputRequest>,
    ) -> Result<Response<WriteInputResponse>, Status> {
        let req = request.into_inner();
        let id = parse_session_id(&req.session_id).map_err(Status::invalid_argument)?;
        let session = self
            .manager
            .get(&id)
            .await
            .ok_or_else(|| Status::not_found(format!("session not found: {id}")))?;

        let len = req.data.len();
        session
            .write_input(&req.data)
            .await
            .map_err(|e| Status::internal(format!("write_input failed: {e}")))?;

        // PtySession::write_input writes the entire buffer atomically and
        // flushes, so on success bytes_written always equals the input length.
        Ok(Response::new(WriteInputResponse {
            bytes_written: len as i32,
        }))
    }

    async fn resize_terminal(
        &self,
        request: Request<ResizeRequest>,
    ) -> Result<Response<ResizeResponse>, Status> {
        let req = request.into_inner();
        let id = parse_session_id(&req.session_id).map_err(Status::invalid_argument)?;
        apply_resize(&self.manager, &id, req.cols, req.rows).await?;
        Ok(Response::new(ResizeResponse {}))
    }

    async fn terminate_session(
        &self,
        request: Request<TerminateSessionRequest>,
    ) -> Result<Response<TerminateSessionResponse>, Status> {
        let req = request.into_inner();
        let id = parse_session_id(&req.session_id).map_err(Status::invalid_argument)?;

        let was_running = self
            .manager
            .terminate(&id)
            .await
            .map_err(|e| Status::internal(format!("terminate failed: {e}")))?;

        Ok(Response::new(TerminateSessionResponse {
            session_id: id.to_string(),
            was_running,
        }))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let metas = self.manager.list().await;
        let sessions: Vec<SessionInfo> = metas
            .into_iter()
            .map(|m| SessionInfo {
                session_id: m.id.to_string(),
                agent_type: agent_type_to_string(m.agent_type),
                started_at: m.started_at.timestamp(),
            })
            .collect();

        let count = sessions.len() as i32;
        Ok(Response::new(ListSessionsResponse { sessions, count }))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
//
// The parsers below return `Result<T, String>` rather than `Result<T, Status>`
// so the small-Ok variants don't trip `clippy::result_large_err` (tonic::Status
// is ~176 bytes and dwarfs `Uuid` / `AgentType`). Call sites map the string
// into the appropriate gRPC status code in one line, keeping the parser logic
// status-agnostic and trivially unit-testable.

fn parse_session_id(raw: &str) -> Result<Uuid, String> {
    Uuid::from_str(raw).map_err(|e| format!("invalid session_id '{raw}': {e}"))
}

/// Convert the proto's `agent_type` string into the canonical [`AgentType`].
///
/// The proto field is the kebab-case serde form of [`AgentType`]
/// (`"claude-code"`, `"codex"`, `"gemini-cli"`, `"pi"`, `"open-code"`). For
/// historical/UX reasons we also accept the snake_case forms named in the
/// proto comment (`"claude_code"`, `"gemini_cli"`, `"open_code"`).
fn parse_agent_type(raw: &str) -> Result<AgentType, String> {
    let normalized = raw.replace('_', "-");
    serde_json::from_value::<AgentType>(serde_json::Value::String(normalized)).map_err(|_| {
        format!(
            "unknown agent_type '{raw}'; expected one of: claude-code, codex, gemini-cli, pi, open-code"
        )
    })
}

fn agent_type_to_string(agent_type: AgentType) -> String {
    // serde serialization mirrors the kebab-case form parse_agent_type accepts.
    // Falling back to the debug form is dead code today (the enum is closed)
    // but keeps the function infallible if serde_json ever fails for a new
    // variant.
    serde_json::to_value(agent_type)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{agent_type:?}"))
}

async fn apply_resize(
    manager: &PtySessionManager,
    id: &Uuid,
    cols: u32,
    rows: u32,
) -> Result<(), Status> {
    let session = manager
        .get(id)
        .await
        .ok_or_else(|| Status::not_found(format!("session not found: {id}")))?;

    let cols = u16::try_from(cols)
        .map_err(|_| Status::invalid_argument(format!("cols {cols} exceeds u16 range")))?;
    let rows = u16::try_from(rows)
        .map_err(|_| Status::invalid_argument(format!("rows {rows} exceeds u16 range")))?;

    session
        .resize(cols, rows)
        .await
        .map_err(|e| Status::internal(format!("resize failed: {e}")))
}

fn current_unix_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_type_accepts_kebab_and_snake_case() {
        assert_eq!(
            parse_agent_type("claude-code").unwrap(),
            AgentType::ClaudeCode
        );
        assert_eq!(
            parse_agent_type("claude_code").unwrap(),
            AgentType::ClaudeCode
        );
        assert_eq!(parse_agent_type("codex").unwrap(), AgentType::Codex);
        assert_eq!(
            parse_agent_type("gemini-cli").unwrap(),
            AgentType::GeminiCli
        );
        assert_eq!(
            parse_agent_type("gemini_cli").unwrap(),
            AgentType::GeminiCli
        );
        assert_eq!(parse_agent_type("pi").unwrap(), AgentType::Pi);
        assert_eq!(parse_agent_type("open-code").unwrap(), AgentType::OpenCode);
        assert_eq!(parse_agent_type("open_code").unwrap(), AgentType::OpenCode);
    }

    #[test]
    fn parse_agent_type_rejects_unknown() {
        let err = parse_agent_type("not-a-real-agent").unwrap_err();
        assert!(
            err.contains("not-a-real-agent"),
            "error should echo offending input: {err}"
        );
    }

    #[test]
    fn agent_type_round_trips_through_string() {
        for t in [
            AgentType::ClaudeCode,
            AgentType::Codex,
            AgentType::GeminiCli,
            AgentType::Pi,
            AgentType::OpenCode,
        ] {
            let s = agent_type_to_string(t);
            assert_eq!(parse_agent_type(&s).unwrap(), t);
        }
    }

    #[test]
    fn parse_session_id_rejects_garbage() {
        let err = parse_session_id("not-a-uuid").unwrap_err();
        assert!(
            err.contains("not-a-uuid"),
            "error should echo offending input: {err}"
        );
    }
}
