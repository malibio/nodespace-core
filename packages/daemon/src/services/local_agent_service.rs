//! tonic `LocalAgentService` implementation.
//!
//! Wraps the agent-crate `LocalAgentService` (ReAct loop + llama.cpp
//! inference) and `CompositeModelManager` so they run in the daemon process
//! rather than the Tauri process. GPU context, model lifecycle, and session
//! state all live here; the Tauri app becomes a thin gRPC proxy.

use std::sync::Arc;

use async_trait::async_trait;
use nodespace_agent::agent_types::{
    AgentToolExecutor, ChatInferenceEngine, InferenceError, InferenceUsage, LocalAgentStatus,
    ModelManager, ModelStatus, StreamingChunk,
};
use nodespace_agent::local_agent::agent_loop::LocalAgentService;
use nodespace_agent::local_agent::composite_model_manager::CompositeModelManager;
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::ollama_model_manager::OllamaModelManager;
use nodespace_agent::local_agent::tools::GraphToolExecutor;
use nodespace_core::services::NodeService;
use tokio::sync::{Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    local_agent_service_server::LocalAgentService as GrpcLocalAgentService, AgentChunk,
    CancelGenerationRequest, CancelGenerationResponse, EndLocalSessionRequest,
    EndLocalSessionResponse, EnsureModelReadyRequest, GetLocalStatusRequest,
    GetSessionsRequest, GetSessionsResponse, ListModelsRequest, ListModelsResponse,
    LocalAgentStatusResponse, LocalSessionInfo, ModelEntry, ModelLoadProgressEvent,
    SendLocalMessageRequest, StartLocalSessionRequest, StartLocalSessionResponse,
};

// ---------------------------------------------------------------------------
// Stub inference engine
// ---------------------------------------------------------------------------

struct NoOpInferenceEngine;

#[async_trait]
impl ChatInferenceEngine for NoOpInferenceEngine {
    async fn generate(
        &self,
        _request: nodespace_agent::agent_types::InferenceRequest,
        _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
    ) -> Result<InferenceUsage, InferenceError> {
        Err(InferenceError::NoModelLoaded)
    }

    async fn model_info(&self) -> Result<Option<nodespace_agent::agent_types::ChatModelSpec>, InferenceError> {
        Ok(None)
    }

    async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
        Ok((text.len() as f32 / 4.0).ceil() as u32)
    }
}

// ---------------------------------------------------------------------------
// LocalAgentServiceImpl
// ---------------------------------------------------------------------------

type AgentService = Arc<LocalAgentService<dyn ChatInferenceEngine, dyn AgentToolExecutor>>;

struct LocalAgentServiceInner {
    /// Wrapped in `RwLock<Arc<...>>` so we can swap the service on engine
    /// replacement while `send_message` callers hold only an `Arc` clone.
    service: RwLock<AgentService>,
    model_manager: Arc<CompositeModelManager>,
    node_service: Arc<NodeService>,
    active_model_id: Mutex<Option<String>>,
}

/// tonic-compatible handle. `Clone` (cheap Arc clone) so tonic can hand
/// copies to concurrent request handlers.
#[derive(Clone)]
pub struct LocalAgentServiceImpl {
    inner: Arc<LocalAgentServiceInner>,
}

impl LocalAgentServiceImpl {
    pub fn new(node_service: Arc<NodeService>) -> Self {
        let gguf = Arc::new(
            GgufModelManager::new().expect("GgufModelManager initialization failed"),
        );
        let ollama = Arc::new(OllamaModelManager::new());
        let model_manager = Arc::new(CompositeModelManager::new(gguf, ollama));

        Self {
            inner: Arc::new(LocalAgentServiceInner {
                service: RwLock::new(Arc::new(Self::build_noop_service(node_service.clone()))),
                model_manager,
                node_service,
                active_model_id: Mutex::new(None),
            }),
        }
    }

    fn build_noop_service(
        node_service: Arc<NodeService>,
    ) -> LocalAgentService<dyn ChatInferenceEngine, dyn AgentToolExecutor> {
        let engine: Arc<dyn ChatInferenceEngine> = Arc::new(NoOpInferenceEngine);
        let executor: Arc<dyn AgentToolExecutor> = Arc::new(GraphToolExecutor {
            node_service: Some(node_service),
            embedding_service: None,
        });
        LocalAgentService::new(engine, executor)
    }

    /// Clone the service Arc so callers can release the lock before awaiting.
    async fn get_service(&self) -> AgentService {
        self.inner.service.read().await.clone()
    }

    async fn replace_engine(&self, engine: Arc<dyn ChatInferenceEngine>) {
        let executor: Arc<dyn AgentToolExecutor> = Arc::new(GraphToolExecutor {
            node_service: Some(self.inner.node_service.clone()),
            embedding_service: None,
        });

        let prompt_assembler = Some(Arc::new(
            nodespace_agent::prompt_assembler::PromptAssembler::new(
                self.inner.node_service.clone(),
            ),
        ));

        let new_service =
            Arc::new(LocalAgentService::new_with_assembler(engine, executor, prompt_assembler));

        let mut guard = self.inner.service.write().await;
        *guard = new_service;
    }

    /// Replace the engine only if the given model_id differs from the active one.
    async fn replace_engine_if_changed(
        &self,
        model_id: &str,
        engine: Arc<dyn ChatInferenceEngine>,
    ) -> bool {
        {
            let active = self.inner.active_model_id.lock().await;
            if active.as_deref() == Some(model_id) {
                return false;
            }
        }
        self.replace_engine(engine).await;
        *self.inner.active_model_id.lock().await = Some(model_id.to_string());
        true
    }

    /// Reset the inference engine to NoOp. Called during daemon shutdown.
    pub async fn reset_to_noop_engine(&self) {
        let mut guard = self.inner.service.write().await;
        *guard = Arc::new(Self::build_noop_service(self.inner.node_service.clone()));
        drop(guard);
        *self.inner.active_model_id.lock().await = None;
        tracing::debug!("LocalAgentServiceImpl: inference engine reset to NoOp");
    }
}

// ---------------------------------------------------------------------------
// gRPC trait implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl GrpcLocalAgentService for LocalAgentServiceImpl {
    async fn start_session(
        &self,
        request: Request<StartLocalSessionRequest>,
    ) -> Result<Response<StartLocalSessionResponse>, Status> {
        let req = request.into_inner();
        let service = self.get_service().await;
        let session_id = service
            .create_session(if req.model_id.is_empty() {
                None
            } else {
                Some(req.model_id)
            })
            .await;

        if let Ok(ctx) = build_workspace_context(&self.inner.node_service).await {
            service.set_session_context(&session_id, ctx).await;
        }

        Ok(Response::new(StartLocalSessionResponse { session_id }))
    }

    type SendMessageStream = ReceiverStream<Result<AgentChunk, Status>>;

    async fn send_message(
        &self,
        request: Request<SendLocalMessageRequest>,
    ) -> Result<Response<Self::SendMessageStream>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id.clone();
        let message = req.message;

        // Clone Arc so we can release the RwLock before awaiting.
        let service = self.get_service().await;

        // Refresh workspace context before the turn.
        if let Ok(ctx) = build_workspace_context(&self.inner.node_service).await {
            service.set_session_context(&session_id, ctx).await;
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentChunk, Status>>(64);

        // Drive the agent turn inline (not spawned) so the gRPC stream starts
        // filling before this handler returns. The callbacks push into an
        // unbounded channel; a forward task drains it into the bounded `tx`.
        let (chunk_tx, mut chunk_rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<AgentChunk, Status>>();
        let chunk_tx2 = chunk_tx.clone();

        let session_id2 = session_id.clone();
        let service_result = service
            .send_message(
                &session_id,
                &message,
                move |_status: LocalAgentStatus| {
                    let _ = _status;
                },
                move |chunk: StreamingChunk| {
                    // Filter Done — the agent loop emits it, but we send our own
                    // done chunk below with authoritative token counts from the
                    // turn result, avoiding a duplicate done event on the stream.
                    if !matches!(chunk, StreamingChunk::Done { .. }) {
                        let _ = chunk_tx.send(Ok(streaming_chunk_to_proto(chunk)));
                    }
                },
            )
            .await;

        match service_result {
            Ok(turn_result) => {
                let _ = chunk_tx2.send(Ok(AgentChunk {
                    chunk_type: "done".to_string(),
                    prompt_tokens: Some(turn_result.usage.prompt_tokens as i32),
                    completion_tokens: Some(turn_result.usage.completion_tokens as i32),
                    ..Default::default()
                }));
            }
            Err(e) => {
                let _ = chunk_tx2.send(Ok(AgentChunk {
                    chunk_type: "error".to_string(),
                    error_message: Some(e.to_string()),
                    ..Default::default()
                }));
            }
        }

        tokio::spawn(async move {
            while let Some(item) = chunk_rx.recv().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
            tracing::debug!(session_id = %session_id2, "Agent chunk stream closed");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_status(
        &self,
        request: Request<GetLocalStatusRequest>,
    ) -> Result<Response<LocalAgentStatusResponse>, Status> {
        let req = request.into_inner();
        let service = self.get_service().await;

        let sessions = service.get_sessions().await;
        let status = if let Some(session_id) = req.session_id {
            sessions
                .into_iter()
                .find(|(id, _)| *id == session_id)
                .map(|(_, s)| s)
                .unwrap_or(LocalAgentStatus::Idle)
        } else {
            sessions
                .into_iter()
                .last()
                .map(|(_, s)| s)
                .unwrap_or(LocalAgentStatus::Idle)
        };

        let status_json = serde_json::to_string(&status)
            .map_err(|e| Status::internal(format!("Failed to serialize status: {e}")))?;

        Ok(Response::new(LocalAgentStatusResponse { status_json }))
    }

    async fn cancel_generation(
        &self,
        request: Request<CancelGenerationRequest>,
    ) -> Result<Response<CancelGenerationResponse>, Status> {
        let req = request.into_inner();
        self.get_service().await.cancel(&req.session_id).await;
        tracing::info!(session_id = %req.session_id, "Local agent generation cancelled");
        Ok(Response::new(CancelGenerationResponse {}))
    }

    async fn end_session(
        &self,
        request: Request<EndLocalSessionRequest>,
    ) -> Result<Response<EndLocalSessionResponse>, Status> {
        let req = request.into_inner();
        self.get_service().await.end_session(&req.session_id).await;
        tracing::info!(session_id = %req.session_id, "Local agent session ended");
        Ok(Response::new(EndLocalSessionResponse {}))
    }

    async fn get_sessions(
        &self,
        _request: Request<GetSessionsRequest>,
    ) -> Result<Response<GetSessionsResponse>, Status> {
        let service = self.get_service().await;
        let pairs = service.get_sessions().await;

        let mut sessions = Vec::with_capacity(pairs.len());
        for (id, status) in &pairs {
            let status_json = serde_json::to_string(status)
                .map_err(|e| Status::internal(format!("Failed to serialize status: {e}")))?;

            let (model_id, created_at) = if let Some(sess) = service.get_session(id).await {
                (sess.model_id, sess.created_at.to_rfc3339())
            } else {
                (None, String::new())
            };

            sessions.push(LocalSessionInfo {
                session_id: id.clone(),
                model_id,
                created_at,
                status_json,
            });
        }

        Ok(Response::new(GetSessionsResponse { sessions }))
    }

    type EnsureModelReadyStream = ReceiverStream<Result<ModelLoadProgressEvent, Status>>;

    async fn ensure_model_ready(
        &self,
        request: Request<EnsureModelReadyRequest>,
    ) -> Result<Response<Self::EnsureModelReadyStream>, Status> {
        let model_id = request.into_inner().model_id;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<ModelLoadProgressEvent, Status>>(16);

        let events = self.load_model_and_collect_events(&model_id).await;

        tokio::spawn(async move {
            for event in events {
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn list_models(
        &self,
        _request: Request<ListModelsRequest>,
    ) -> Result<Response<ListModelsResponse>, Status> {
        let models = self
            .inner.model_manager
            .list()
            .await
            .map_err(|e| Status::internal(format!("Failed to list models: {e}")))?;

        let entries = models
            .into_iter()
            .map(|m| {
                let status_json = serde_json::to_string(&m.status).unwrap_or_default();
                let backend = format!("{:?}", m.backend).to_lowercase();
                ModelEntry {
                    id: m.id,
                    name: m.name,
                    backend,
                    status_json,
                    size_bytes: m.size_bytes as i64,
                    quantization: m.quantization,
                }
            })
            .collect();

        Ok(Response::new(ListModelsResponse { models: entries }))
    }
}

impl LocalAgentServiceImpl {
    /// Drive the full model-load sequence and return the resulting events.
    async fn load_model_and_collect_events(&self, model_id: &str) -> Vec<ModelLoadProgressEvent> {
        use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
        use nodespace_agent::local_agent::ollama_inference::OllamaInferenceEngine;
        use nodespace_nlp_engine::chat::ChatConfig;

        let mut events = Vec::new();

        // --- Check model catalog ---
        let models = match self.inner.model_manager.list().await {
            Ok(m) => m,
            Err(e) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(e.to_string()),
                    ..Default::default()
                });
                return events;
            }
        };

        let model = match models.iter().find(|m| m.id == model_id) {
            Some(m) => m,
            None => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Unknown model: {model_id}")),
                    ..Default::default()
                });
                return events;
            }
        };

        // --- Ollama fast path ---
        if CompositeModelManager::is_ollama(model_id) {
            let ollama_name =
                CompositeModelManager::strip_ollama_prefix(model_id).to_string();

            events.push(ModelLoadProgressEvent {
                event_type: "loading".to_string(),
                model_id: model_id.to_string(),
                message: Some(format!("Connecting to Ollama model {ollama_name}...")),
                ..Default::default()
            });

            if let Err(e) = self.inner.model_manager.load(model_id).await {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(e.to_string()),
                    ..Default::default()
                });
                return events;
            }

            let ollama_base_url = self
                .inner.model_manager
                .ollama_manager()
                .base_url()
                .to_string();
            let engine = OllamaInferenceEngine::with_base_url(ollama_name.clone(), ollama_base_url);
            let swapped = self
                .replace_engine_if_changed(model_id, Arc::new(engine))
                .await;

            events.push(ModelLoadProgressEvent {
                event_type: "ready".to_string(),
                model_id: model_id.to_string(),
                message: Some(format!("Ollama model {ollama_name} ready")),
                engine_swapped: Some(swapped),
                ..Default::default()
            });

            return events;
        }

        // --- GGUF path ---

        // Check in-memory active engine first: catalog `Loaded` status is
        // persisted and may survive a daemon restart without the engine being
        // in memory. `active_model_id` reflects the running engine.
        {
            let active = self.inner.active_model_id.lock().await;
            if active.as_deref() == Some(model_id) {
                events.push(ModelLoadProgressEvent {
                    event_type: "ready".to_string(),
                    model_id: model_id.to_string(),
                    message: Some(format!("{model_id} already loaded")),
                    engine_swapped: Some(false),
                    ..Default::default()
                });
                return events;
            }
        }

        match &model.status {
            // Catalog says loaded but engine is not active (e.g. after restart):
            // fall through to re-load.
            ModelStatus::Loaded | ModelStatus::Ready => {}
            ModelStatus::NotDownloaded | ModelStatus::Error { .. } => {
                events.push(ModelLoadProgressEvent {
                    event_type: "downloading".to_string(),
                    model_id: model_id.to_string(),
                    message: Some(format!("Downloading {model_id}...")),
                    ..Default::default()
                });

                if let Err(e) = self.inner.model_manager.download(model_id).await {
                    events.push(ModelLoadProgressEvent {
                        event_type: "error".to_string(),
                        model_id: model_id.to_string(),
                        error_message: Some(format!("Download failed: {e}")),
                        ..Default::default()
                    });
                    return events;
                }
            }
            ModelStatus::Downloading { .. } | ModelStatus::Verifying => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!(
                        "Model '{model_id}' is currently being downloaded"
                    )),
                    ..Default::default()
                });
                return events;
            }
        }

        events.push(ModelLoadProgressEvent {
            event_type: "loading".to_string(),
            model_id: model_id.to_string(),
            message: Some(format!("Loading {model_id}...")),
            ..Default::default()
        });

        let model_path = match self
            .inner.model_manager
            .gguf_manager()
            .model_path(model_id)
        {
            Ok(p) => p,
            Err(e) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Failed to resolve model path: {e}")),
                    ..Default::default()
                });
                return events;
            }
        };

        let family = match self.inner.model_manager.gguf_manager().family_for(model_id) {
            Ok(f) => f,
            Err(e) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Failed to look up model family: {e}")),
                    ..Default::default()
                });
                return events;
            }
        };

        let model_path_str = model_path.to_string_lossy().to_string();
        let engine_result = tokio::task::spawn_blocking(move || {
            LlamaChatInferenceEngine::load(&model_path_str, family, ChatConfig::default())
        })
        .await;

        let engine = match engine_result {
            Ok(Ok(e)) => e,
            Ok(Err(e)) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Failed to load inference engine: {e}")),
                    ..Default::default()
                });
                return events;
            }
            Err(e) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Task join error: {e}")),
                    ..Default::default()
                });
                return events;
            }
        };

        // Update catalog status only after llama.cpp context is live.
        if let Err(e) = self.inner.model_manager.load(model_id).await {
            events.push(ModelLoadProgressEvent {
                event_type: "error".to_string(),
                model_id: model_id.to_string(),
                error_message: Some(format!("Failed to mark model as loaded: {e}")),
                ..Default::default()
            });
            return events;
        }

        self.replace_engine(Arc::new(engine)).await;
        *self.inner.active_model_id.lock().await = Some(model_id.to_string());

        events.push(ModelLoadProgressEvent {
            event_type: "ready".to_string(),
            model_id: model_id.to_string(),
            message: Some(format!("{model_id} ready")),
            engine_swapped: Some(true),
            ..Default::default()
        });

        events
    }

}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn streaming_chunk_to_proto(chunk: StreamingChunk) -> AgentChunk {
    match chunk {
        StreamingChunk::Token { text } => AgentChunk {
            chunk_type: "token".to_string(),
            token_text: Some(text),
            ..Default::default()
        },
        StreamingChunk::ToolCallStart { id, name } => AgentChunk {
            chunk_type: "tool_call_start".to_string(),
            tool_call_id: Some(id),
            tool_name: Some(name),
            ..Default::default()
        },
        StreamingChunk::ToolCallArgs { id, args_json } => AgentChunk {
            chunk_type: "tool_call_args".to_string(),
            tool_call_id: Some(id),
            tool_args_json: Some(args_json),
            ..Default::default()
        },
        StreamingChunk::Done { usage } => AgentChunk {
            chunk_type: "done".to_string(),
            prompt_tokens: Some(usage.prompt_tokens as i32),
            completion_tokens: Some(usage.completion_tokens as i32),
            ..Default::default()
        },
        StreamingChunk::Error { message } => AgentChunk {
            chunk_type: "error".to_string(),
            error_message: Some(message),
            ..Default::default()
        },
    }
}

async fn build_workspace_context(node_service: &Arc<NodeService>) -> Result<String, ()> {
    let context = nodespace_core::ops::context_ops::build_workspace_context(node_service)
        .await
        .map_err(|_| ())?;
    Ok(context.format_for_prompt(1500))
}
