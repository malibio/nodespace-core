//! tonic `LocalAgentService` implementation — node-as-message-queue architecture.
//!
//! The daemon watches for `NodeUpdated`/`NodeCreated` events on `ai-chat` nodes.
//! When the last message in `properties['ai-chat']['messages']` has `role: 'user'`
//! and `status != 'processing'`, it triggers an inference turn in-process.
//!
//! Streaming tokens are broadcast to any connected `SubscribeTokenStream` client
//! (the Tauri process), which translates them to Tauri events for the frontend.
//!
//! Session IPC (StartSession, SendMessage, EndSession) is removed. The node
//! is the sole source of truth for conversation state.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nodespace_agent::agent_types::{
    AgentToolExecutor, ChatInferenceEngine, ChatMessage, InferenceError, InferenceUsage,
    LocalAgentStatus, ModelManager, ModelStatus, Role, StreamingChunk,
};
use nodespace_agent::local_agent::agent_loop::LocalAgentService;
use nodespace_agent::local_agent::composite_model_manager::CompositeModelManager;
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::ollama_model_manager::OllamaModelManager;
use nodespace_agent::local_agent::tools::GraphToolExecutor;
use nodespace_core::models::{NodeFilter, NodeUpdate};
use nodespace_core::services::{NodeEmbeddingService, NodeService};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    local_agent_service_server::LocalAgentService as GrpcLocalAgentService, AgentChunk,
    CancelModelDownloadRequest, CancelModelDownloadResponse, CancelTurnRequest, CancelTurnResponse,
    DeleteModelRequest, DeleteModelResponse, DownloadModelRequest, EnsureModelReadyRequest,
    GetLocalStatusRequest, GetSystemRamRequest, GetSystemRamResponse, ListModelsRequest,
    ListModelsResponse, LoadModelRequest, LoadModelResponse, LocalAgentStatusResponse,
    ModelEntry, ModelLoadProgressEvent, OllamaAvailableRequest, OllamaAvailableResponse,
    RecommendedModelRequest, RecommendedModelResponse, SubscribeTokenStreamRequest,
    UnloadModelRequest, UnloadModelResponse,
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

    async fn model_info(
        &self,
    ) -> Result<Option<nodespace_agent::agent_types::ChatModelSpec>, InferenceError> {
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

/// Cancellation tokens keyed by node_id for in-progress turns.
type TurnTokens = Arc<Mutex<HashMap<String, CancellationToken>>>;

struct LocalAgentServiceInner {
    service: RwLock<AgentService>,
    model_manager: Arc<CompositeModelManager>,
    node_service: Arc<NodeService>,
    active_model_id: Mutex<Option<String>>,
    embedding_service: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>>,
    /// Broadcast channel for streaming tokens → all SubscribeTokenStream clients.
    token_tx: broadcast::Sender<AgentChunk>,
    /// Cancellation tokens keyed by node_id.
    turn_tokens: TurnTokens,
}

/// tonic-compatible handle. `Clone` (cheap Arc clone) so tonic can hand
/// copies to concurrent request handlers.
#[derive(Clone)]
pub struct LocalAgentServiceImpl {
    inner: Arc<LocalAgentServiceInner>,
}

impl LocalAgentServiceImpl {
    pub fn new(
        node_service: Arc<NodeService>,
        embedding_service: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>>,
    ) -> Self {
        let gguf =
            Arc::new(GgufModelManager::new().expect("GgufModelManager initialization failed"));
        let ollama = Arc::new(OllamaModelManager::new());
        let model_manager = Arc::new(CompositeModelManager::new(gguf, ollama));

        // Channel capacity: enough headroom for burst token output (~256 tokens per broadcast).
        let (token_tx, _) = broadcast::channel(512);

        Self {
            inner: Arc::new(LocalAgentServiceInner {
                service: RwLock::new(Arc::new(Self::build_noop_service(node_service.clone()))),
                model_manager,
                node_service,
                active_model_id: Mutex::new(None),
                embedding_service,
                token_tx,
                turn_tokens: Arc::new(Mutex::new(HashMap::new())),
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

        let new_service = Arc::new(LocalAgentService::new_with_assembler(
            engine,
            executor,
            prompt_assembler,
        ));

        let mut guard = self.inner.service.write().await;
        *guard = new_service;
    }

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

    pub async fn reset_to_noop_engine(&self) {
        let mut guard = self.inner.service.write().await;
        *guard = Arc::new(Self::build_noop_service(self.inner.node_service.clone()));
        drop(guard);
        *self.inner.active_model_id.lock().await = None;
        tracing::debug!("LocalAgentServiceImpl: inference engine reset to NoOp");
    }

    // ---------------------------------------------------------------------------
    // Event watcher — subscribes to NodeService and reacts to ai-chat changes
    // ---------------------------------------------------------------------------

    /// Spawn a background task that subscribes to the NodeService event bus and
    /// handles ai-chat node changes. Call once from the daemon startup after
    /// `LocalAgentServiceImpl::new`.
    ///
    /// Also scans for nodes stuck in `status: 'processing'` at startup (daemon
    /// restart recovery) and retries those turns.
    pub fn start_event_watcher(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            // Recovery pass: find any ai-chat nodes stuck in processing.
            this.recover_stuck_turns().await;

            let mut rx = this.inner.node_service.subscribe_to_events();
            loop {
                match rx.recv().await {
                    Ok(envelope) => {
                        let (node_id, node_type) = match &envelope.event {
                            nodespace_core::db::events::DomainEvent::NodeCreated {
                                node_id,
                                node_type,
                            } => (node_id.clone(), node_type.clone()),
                            nodespace_core::db::events::DomainEvent::NodeUpdated {
                                node_id,
                                node_type,
                                ..
                            } => (node_id.clone(), node_type.clone()),
                            _ => continue,
                        };

                        if node_type != "ai-chat" {
                            continue;
                        }

                        let this2 = this.clone();
                        tokio::spawn(async move {
                            this2.maybe_handle_ai_chat_node(&node_id).await;
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped,
                            "LocalAgentService event watcher lagged; some ai-chat events dropped"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("LocalAgentService event watcher: channel closed, stopping");
                        break;
                    }
                }
            }
        });
    }

    /// Check the node and start an inference turn if appropriate.
    async fn maybe_handle_ai_chat_node(&self, node_id: &str) {
        let node = match self.inner.node_service.get_node(node_id).await {
            Ok(Some(n)) => n,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(node_id, error = %e, "failed to fetch ai-chat node");
                return;
            }
        };

        let ai_chat = match node.properties.get("ai-chat") {
            Some(v) => v,
            None => return,
        };

        // Already processing — skip.
        if ai_chat.get("status").and_then(|v| v.as_str()) == Some("processing") {
            return;
        }

        // Check that the last message is from the user.
        let messages = match ai_chat.get("messages").and_then(|v| v.as_array()) {
            Some(m) if !m.is_empty() => m,
            _ => return,
        };
        let last = &messages[messages.len() - 1];
        if last.get("role").and_then(|v| v.as_str()) != Some("user") {
            return;
        }

        // Check if a turn is already running for this node (race between event
        // delivery and turn start for the same node).
        {
            let tokens = self.inner.turn_tokens.lock().await;
            if tokens.contains_key(node_id) {
                return;
            }
        }

        tracing::info!(node_id, "ai-chat turn triggered");
        self.run_ai_chat_turn(node_id.to_string()).await;
    }

    /// Execute a full inference turn for the given ai-chat node.
    async fn run_ai_chat_turn(&self, node_id: String) {
        let cancel = CancellationToken::new();
        self.inner
            .turn_tokens
            .lock()
            .await
            .insert(node_id.clone(), cancel.clone());

        // Write status: 'processing'
        if let Err(e) = self
            .write_ai_chat_status(&node_id, "processing", None)
            .await
        {
            tracing::warn!(node_id, error = %e, "failed to set ai-chat status to processing");
            self.inner.turn_tokens.lock().await.remove(&node_id);
            return;
        }

        // Load history from node.
        let history = load_node_history(&self.inner.node_service, &node_id).await;
        if history.is_empty() {
            tracing::warn!(node_id, "ai-chat history empty — skipping turn");
            let _ = self.write_ai_chat_status(&node_id, "idle", None).await;
            self.inner.turn_tokens.lock().await.remove(&node_id);
            return;
        }

        // Separate the user message (last) from the prior history.
        let user_message = match history.last() {
            Some(m) if m.role == Role::User => m.content.clone(),
            _ => {
                tracing::warn!(node_id, "ai-chat last message is not from user — skipping");
                let _ = self.write_ai_chat_status(&node_id, "idle", None).await;
                self.inner.turn_tokens.lock().await.remove(&node_id);
                return;
            }
        };
        let prior_history: Vec<ChatMessage> = history[..history.len() - 1].to_vec();

        let service = self.get_service().await;

        // Create an ephemeral session seeded with prior history.
        let session_id = service.create_session(None, prior_history).await;

        // Refresh workspace context.
        let emb = self.inner.embedding_service.read().await.clone();
        if let Ok(ctx) =
            build_workspace_context(&self.inner.node_service, emb, Some(&user_message)).await
        {
            service.set_session_context(&session_id, ctx).await;
        }

        let token_tx = self.inner.token_tx.clone();
        let node_id2 = node_id.clone();

        let send_result = service
            .send_message(
                &session_id,
                &user_message,
                move |_status: LocalAgentStatus| {},
                move |chunk: StreamingChunk| {
                    if !matches!(chunk, StreamingChunk::Done { .. }) {
                        let mut proto = streaming_chunk_to_proto(chunk);
                        proto.node_id = Some(node_id2.clone());
                        // Ignore send errors — no subscribers connected is fine.
                        let _ = token_tx.send(proto);
                    }
                },
            )
            .await;

        // End the ephemeral session.
        service.end_session(&session_id).await;

        match send_result {
            Ok(turn_result) => {
                // Emit done chunk to subscribers.
                let _ = self.inner.token_tx.send(AgentChunk {
                    chunk_type: "done".to_string(),
                    prompt_tokens: Some(turn_result.usage.prompt_tokens as i32),
                    completion_tokens: Some(turn_result.usage.completion_tokens as i32),
                    node_id: Some(node_id.clone()),
                    ..Default::default()
                });

                // Append assistant message to node.
                let assistant_content = service
                    .get_session(&session_id)
                    .await
                    .and_then(|s| {
                        s.messages
                            .into_iter()
                            .rev()
                            .find(|m| m.role == Role::Assistant)
                            .map(|m| m.content)
                    })
                    .unwrap_or_else(|| turn_result.response.clone());

                if let Err(e) = self
                    .append_assistant_message(&node_id, &assistant_content)
                    .await
                {
                    tracing::warn!(node_id, error = %e, "failed to append assistant message");
                }
            }
            Err(e) => {
                tracing::warn!(node_id, error = %e, "inference turn failed");
                let _ = self.inner.token_tx.send(AgentChunk {
                    chunk_type: "error".to_string(),
                    error_message: Some(e.to_string()),
                    node_id: Some(node_id.clone()),
                    ..Default::default()
                });
            }
        }

        // Write status: 'idle'
        if let Err(e) = self.write_ai_chat_status(&node_id, "idle", None).await {
            tracing::warn!(node_id, error = %e, "failed to set ai-chat status to idle");
        }

        self.inner.turn_tokens.lock().await.remove(&node_id);
        tracing::info!(node_id, "ai-chat turn complete");
    }

    /// Scan for ai-chat nodes stuck in `status: 'processing'` at daemon startup
    /// and retry their turns (handles daemon restart mid-turn).
    async fn recover_stuck_turns(&self) {
        let filter = NodeFilter::new().with_node_type("ai-chat".to_string());

        let nodes = match self.inner.node_service.query_nodes(filter).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "recovery scan failed");
                return;
            }
        };

        for node in nodes {
            let ai_chat = match node.properties.get("ai-chat") {
                Some(v) => v,
                None => continue,
            };
            if ai_chat.get("status").and_then(|v| v.as_str()) != Some("processing") {
                continue;
            }
            // Verify last message is from user before retrying.
            let is_trailing_user = ai_chat
                .get("messages")
                .and_then(|v| v.as_array())
                .and_then(|msgs| msgs.last())
                .and_then(|m| m.get("role"))
                .and_then(|v| v.as_str())
                == Some("user");

            if is_trailing_user {
                tracing::info!(node_id = %node.id, "recovering stuck ai-chat turn");
                let this = self.clone();
                let nid = node.id.clone();
                tokio::spawn(async move {
                    this.run_ai_chat_turn(nid).await;
                });
            } else {
                // Stuck in processing but no trailing user message — reset to idle.
                let _ = self.write_ai_chat_status(&node.id, "idle", None).await;
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Node write helpers
    // ---------------------------------------------------------------------------

    /// Write `properties['ai-chat']['status']` to the node.
    /// Retries once on version conflict (optimistic concurrency).
    async fn write_ai_chat_status(
        &self,
        node_id: &str,
        status: &str,
        model: Option<&str>,
    ) -> Result<(), String> {
        for attempt in 0..2 {
            let node = self
                .inner
                .node_service
                .get_node(node_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("node {node_id} not found"))?;

            let mut ai_chat = node
                .properties
                .get("ai-chat")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            ai_chat["status"] = serde_json::Value::String(status.to_string());
            if let Some(m) = model {
                ai_chat["model"] = serde_json::Value::String(m.to_string());
            }

            let mut props = node.properties.clone();
            props["ai-chat"] = ai_chat;

            let update = NodeUpdate::new().with_properties(props);
            match self
                .inner
                .node_service
                .update_node(node_id, node.version, update)
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) if attempt == 0 => {
                    tracing::debug!(
                        node_id,
                        error = %e,
                        "version conflict writing ai-chat status, retrying"
                    );
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        unreachable!()
    }

    /// Append an assistant message to `properties['ai-chat']['messages']`.
    /// Retries once on version conflict.
    async fn append_assistant_message(
        &self,
        node_id: &str,
        content: &str,
    ) -> Result<(), String> {
        for attempt in 0..2 {
            let node = self
                .inner
                .node_service
                .get_node(node_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("node {node_id} not found"))?;

            let mut ai_chat = node
                .properties
                .get("ai-chat")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            let messages = ai_chat
                .get_mut("messages")
                .and_then(|v| v.as_array_mut());

            let timestamp = chrono::Utc::now().to_rfc3339();
            let new_msg = serde_json::json!({
                "role": "assistant",
                "content": content,
                "timestamp": timestamp
            });

            if let Some(msgs) = messages {
                msgs.push(new_msg);
            } else {
                ai_chat["messages"] = serde_json::json!([new_msg]);
            }

            // Set status to idle here too (atomic with message append).
            ai_chat["status"] = serde_json::Value::String("idle".to_string());

            let mut props = node.properties.clone();
            props["ai-chat"] = ai_chat;

            let update = NodeUpdate::new().with_properties(props);
            match self
                .inner
                .node_service
                .update_node(node_id, node.version, update)
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) if attempt == 0 => {
                    tracing::debug!(
                        node_id,
                        error = %e,
                        "version conflict appending assistant message, retrying"
                    );
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        unreachable!()
    }
}

// ---------------------------------------------------------------------------
// gRPC trait implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl GrpcLocalAgentService for LocalAgentServiceImpl {
    type SubscribeTokenStreamStream = ReceiverStream<Result<AgentChunk, Status>>;

    async fn subscribe_token_stream(
        &self,
        _request: Request<SubscribeTokenStreamRequest>,
    ) -> Result<Response<Self::SubscribeTokenStreamStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<AgentChunk, Status>>(128);
        let mut broadcast_rx = self.inner.token_tx.subscribe();

        tokio::spawn(async move {
            loop {
                match broadcast_rx.recv().await {
                    Ok(chunk) => {
                        if tx.send(Ok(chunk)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "SubscribeTokenStream subscriber lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            tracing::debug!("SubscribeTokenStream: client disconnected");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn cancel_turn(
        &self,
        request: Request<CancelTurnRequest>,
    ) -> Result<Response<CancelTurnResponse>, Status> {
        let node_id = request.into_inner().node_id;
        let tokens = self.inner.turn_tokens.lock().await;
        if let Some(token) = tokens.get(&node_id) {
            token.cancel();
            tracing::info!(node_id, "ai-chat turn cancelled");
        }
        Ok(Response::new(CancelTurnResponse {}))
    }

    async fn get_status(
        &self,
        _request: Request<GetLocalStatusRequest>,
    ) -> Result<Response<LocalAgentStatusResponse>, Status> {
        let tokens = self.inner.turn_tokens.lock().await;
        let status = if tokens.is_empty() {
            LocalAgentStatus::Idle
        } else {
            LocalAgentStatus::Streaming
        };
        let status_json = serde_json::to_string(&status)
            .map_err(|e| Status::internal(format!("Failed to serialize status: {e}")))?;
        Ok(Response::new(LocalAgentStatusResponse { status_json }))
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
            .inner
            .model_manager
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
                    min_memory_gb: m.min_memory_gb as u32,
                }
            })
            .collect();

        Ok(Response::new(ListModelsResponse { models: entries }))
    }

    type DownloadModelStream = ReceiverStream<Result<ModelLoadProgressEvent, Status>>;

    async fn download_model(
        &self,
        request: Request<DownloadModelRequest>,
    ) -> Result<Response<Self::DownloadModelStream>, Status> {
        let model_id = request.into_inner().model_id;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<ModelLoadProgressEvent, Status>>(16);

        let model_id_clone = model_id.clone();
        let manager = self.inner.model_manager.clone();

        let tx_gguf = tx.clone();
        let mid_gguf = model_id.clone();
        manager
            .set_gguf_progress_callback(Box::new(move |evt| {
                let event = ModelLoadProgressEvent {
                    event_type: "downloading".to_string(),
                    model_id: mid_gguf.clone(),
                    bytes_downloaded: Some(evt.bytes_downloaded as i64),
                    bytes_total: Some(evt.bytes_total as i64),
                    ..Default::default()
                };
                let _ = tx_gguf.try_send(Ok(event));
            }))
            .await;

        let tx_ollama = tx.clone();
        let mid_ollama = model_id.clone();
        manager
            .set_ollama_progress_callback(Box::new(move |evt| {
                let event = ModelLoadProgressEvent {
                    event_type: "downloading".to_string(),
                    model_id: mid_ollama.clone(),
                    bytes_downloaded: Some(evt.bytes_downloaded as i64),
                    bytes_total: Some(evt.bytes_total as i64),
                    ..Default::default()
                };
                let _ = tx_ollama.try_send(Ok(event));
            }))
            .await;

        tokio::spawn(async move {
            match manager.download(&model_id_clone).await {
                Ok(()) => {
                    let _ = tx
                        .send(Ok(ModelLoadProgressEvent {
                            event_type: "ready".to_string(),
                            model_id: model_id_clone,
                            ..Default::default()
                        }))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(ModelLoadProgressEvent {
                            event_type: "error".to_string(),
                            model_id: model_id_clone,
                            error_message: Some(e.to_string()),
                            ..Default::default()
                        }))
                        .await;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn delete_model(
        &self,
        request: Request<DeleteModelRequest>,
    ) -> Result<Response<DeleteModelResponse>, Status> {
        let model_id = request.into_inner().model_id;
        self.inner
            .model_manager
            .delete(&model_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete model: {e}")))?;
        Ok(Response::new(DeleteModelResponse {}))
    }

    async fn load_model(
        &self,
        request: Request<LoadModelRequest>,
    ) -> Result<Response<LoadModelResponse>, Status> {
        let model_id = request.into_inner().model_id;
        self.inner
            .model_manager
            .load(&model_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to load model: {e}")))?;
        Ok(Response::new(LoadModelResponse {}))
    }

    async fn unload_model(
        &self,
        _request: Request<UnloadModelRequest>,
    ) -> Result<Response<UnloadModelResponse>, Status> {
        self.inner
            .model_manager
            .unload()
            .await
            .map_err(|e| Status::internal(format!("Failed to unload model: {e}")))?;
        Ok(Response::new(UnloadModelResponse {}))
    }

    async fn cancel_model_download(
        &self,
        request: Request<CancelModelDownloadRequest>,
    ) -> Result<Response<CancelModelDownloadResponse>, Status> {
        let model_id = request.into_inner().model_id;
        self.inner
            .model_manager
            .cancel_download(&model_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to cancel download: {e}")))?;
        Ok(Response::new(CancelModelDownloadResponse {}))
    }

    async fn ollama_available(
        &self,
        _request: Request<OllamaAvailableRequest>,
    ) -> Result<Response<OllamaAvailableResponse>, Status> {
        let available = self.inner.model_manager.ollama_available().await;
        Ok(Response::new(OllamaAvailableResponse { available }))
    }

    async fn recommended_model(
        &self,
        _request: Request<RecommendedModelRequest>,
    ) -> Result<Response<RecommendedModelResponse>, Status> {
        let model_id = self
            .inner
            .model_manager
            .recommended_model()
            .await
            .map_err(|e| Status::internal(format!("Failed to get recommended model: {e}")))?;
        Ok(Response::new(RecommendedModelResponse { model_id }))
    }

    async fn get_system_ram(
        &self,
        _request: Request<GetSystemRamRequest>,
    ) -> Result<Response<GetSystemRamResponse>, Status> {
        let ram_bytes = nodespace_agent::local_agent::model_manager::detect_system_ram();
        Ok(Response::new(GetSystemRamResponse { ram_bytes }))
    }
}

impl LocalAgentServiceImpl {
    async fn load_model_and_collect_events(&self, model_id: &str) -> Vec<ModelLoadProgressEvent> {
        use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
        use nodespace_agent::local_agent::ollama_inference::OllamaInferenceEngine;
        use nodespace_nlp_engine::chat::ChatConfig;

        let mut events = Vec::new();

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

        if CompositeModelManager::is_ollama(model_id) {
            let ollama_name = CompositeModelManager::strip_ollama_prefix(model_id).to_string();

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
                .inner
                .model_manager
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

        let model_path = match self.inner.model_manager.gguf_manager().model_path(model_id) {
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

async fn load_node_history(node_service: &Arc<NodeService>, node_id: &str) -> Vec<ChatMessage> {
    let node = match node_service.get_node(node_id).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            tracing::warn!(node_id, "ai-chat node not found for history");
            return vec![];
        }
        Err(e) => {
            tracing::error!(node_id, error = %e, "failed to load ai-chat node for history");
            return vec![];
        }
    };

    let messages = node
        .properties
        .get("ai-chat")
        .and_then(|ns| ns.get("messages"))
        .and_then(|v| v.as_array());

    let Some(messages) = messages else {
        return vec![];
    };

    messages
        .iter()
        .filter_map(|m| {
            let role_str = m.get("role")?.as_str()?;
            let role = match role_str {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => return None,
            };
            let content = m.get("content")?.as_str()?.to_string();
            Some(ChatMessage {
                role,
                content,
                tool_call_id: None,
                name: None,
            })
        })
        .collect()
}

async fn build_workspace_context(
    node_service: &Arc<NodeService>,
    embedding_service: Option<Arc<NodeEmbeddingService>>,
    query: Option<&str>,
) -> Result<String, ()> {
    let context = nodespace_core::ops::context_ops::build_workspace_context(
        node_service,
        embedding_service.as_ref(),
        query,
    )
    .await
    .map_err(|_| ())?;
    Ok(context.format_for_prompt(4000))
}
