//! tonic `LocalAgentService` implementation — node-as-message-queue architecture.
//!
//! The daemon watches for `NodeUpdated`/`NodeCreated` events on `ai-chat` nodes.
//! When the last message in `properties['ai-chat']['messages']` has `role: 'user'`
//! and `status == 'processing'`, it triggers an inference turn in-process.
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
    AgentToolExecutor, ChatInferenceEngine, ChatMessage, ChatModelSpec, InferenceError,
    InferenceUsage, LocalAgentStatus, ModelManager, ModelStatus, PriorWrite, Role, StreamingChunk,
    ToolExecutionRecord,
};
use nodespace_agent::local_agent::agent_loop::{
    canonical_args, LocalAgentService, CANONICAL_ARGS_MAX_CHARS,
};
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::tools::{
    is_cross_turn_guarded_tool, is_write_tool, GraphToolExecutor, SharedEmbeddingService,
};
use nodespace_core::models::{
    AiChatCompletedWrite, AiChatMessage, AiChatNode, NodeFilter, NodeUpdate,
};
use nodespace_core::services::{NodeEmbeddingService, NodeService};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    local_agent_service_server::LocalAgentService as GrpcLocalAgentService, AgentChunk,
    CancelModelDownloadRequest, CancelModelDownloadResponse, CancelTurnRequest, CancelTurnResponse,
    DeleteModelRequest, DeleteModelResponse, DownloadModelRequest, EnsureModelReadyRequest,
    GetLocalStatusRequest, GetSystemRamRequest, GetSystemRamResponse, ListModelsRequest,
    ListModelsResponse, LoadModelRequest, LoadModelResponse, LocalAgentStatusResponse, ModelEntry,
    ModelLoadProgressEvent, RecommendedModelRequest, RecommendedModelResponse,
    SubscribeTokenStreamRequest, UnloadModelRequest, UnloadModelResponse,
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

/// How long the engine-swap geometry snapshot may take before it is abandoned.
/// Generous for a local read; the bound exists only so a remote engine that
/// stalls cannot hold up the model load that awaits it.
const MODEL_SPEC_SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

struct LocalAgentServiceInner {
    service: RwLock<AgentService>,
    model_manager: Arc<GgufModelManager>,
    node_service: Arc<NodeService>,
    active_model_id: Mutex<Option<String>>,
    /// The loaded model's geometry, captured at engine-swap time.
    ///
    /// Read by `get_status` instead of calling `model_spec()`, which reaches a
    /// `std::sync::Mutex` that the native engine holds for the *entire*
    /// duration of a generation. Querying it live would make a status RPC
    /// block a tokio worker for the length of a turn (60-180s). The geometry
    /// only changes on engine swap, and `replace_engine` is the single choke
    /// point for that, so a value cached there is always current.
    loaded_model_spec: Mutex<Option<ChatModelSpec>>,
    /// Bound on the engine-swap geometry snapshot. A field rather than a
    /// constant so tests can drive the timeout path without paying it in
    /// wall-clock time.
    model_spec_snapshot_timeout: std::time::Duration,
    embedding_service: SharedEmbeddingService,
    /// Broadcast channel for streaming tokens → all SubscribeTokenStream clients.
    token_tx: broadcast::Sender<AgentChunk>,
    /// Cancellation tokens keyed by node_id.
    turn_tokens: TurnTokens,
    /// Cancels this database's background event watcher when the database is
    /// closed (ADR-053: per-database compute scoping). Shared across the cheap
    /// `Arc` clones tonic hands to request handlers, so a single `shutdown()`
    /// stops the watcher spawned from any clone.
    shutdown_token: CancellationToken,
    /// Path to `~/.nodespace/daemon.toml`, read to resolve OpenAI-compatible
    /// provider configs by UUID when loading an `openai-compat:<uuid>` model.
    daemon_config_path: std::path::PathBuf,
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
        embedding_service: SharedEmbeddingService,
        daemon_config_path: std::path::PathBuf,
    ) -> Self {
        let model_manager =
            Arc::new(GgufModelManager::new().expect("GgufModelManager initialization failed"));

        // Channel capacity: enough headroom for burst token output (~256 tokens per broadcast).
        let (token_tx, _) = broadcast::channel(512);

        Self {
            inner: Arc::new(LocalAgentServiceInner {
                service: RwLock::new(Arc::new(Self::build_noop_service(
                    node_service.clone(),
                    embedding_service.clone(),
                ))),
                model_manager,
                node_service,
                active_model_id: Mutex::new(None),
                loaded_model_spec: Mutex::new(None),
                model_spec_snapshot_timeout: MODEL_SPEC_SNAPSHOT_TIMEOUT,
                embedding_service,
                token_tx,
                turn_tokens: Arc::new(Mutex::new(HashMap::new())),
                shutdown_token: CancellationToken::new(),
                daemon_config_path,
            }),
        }
    }

    /// Stop this database's background event watcher (ADR-053: per-database
    /// compute scoping). Called when the owning database is closed or evicted so
    /// its watcher does not keep subscribing to a now-detached event bus.
    /// Idempotent and cheap — cancelling an already-cancelled token is a no-op.
    pub fn shutdown(&self) {
        self.inner.shutdown_token.cancel();
    }

    fn build_noop_service(
        node_service: Arc<NodeService>,
        embedding_service: SharedEmbeddingService,
    ) -> LocalAgentService<dyn ChatInferenceEngine, dyn AgentToolExecutor> {
        let engine: Arc<dyn ChatInferenceEngine> = Arc::new(NoOpInferenceEngine);
        let executor: Arc<dyn AgentToolExecutor> = Arc::new(GraphToolExecutor {
            node_service: Some(node_service),
            embedding_service,
            inference_engine: Some(engine.clone()),
        });
        LocalAgentService::new(engine, executor)
    }

    async fn get_service(&self) -> AgentService {
        self.inner.service.read().await.clone()
    }

    async fn replace_engine(&self, engine: Arc<dyn ChatInferenceEngine>) {
        // Hand the executor the *shared* embedding handle, not a snapshot. The
        // executor reads the current value per call, so search_semantic and
        // search_skills work as soon as the embedding model finishes loading
        // in the background — no engine swap required, and no construction
        // site can wire a stale or `None` service.
        //
        // `inference_engine` is different: it's not a shared handle updated in
        // place, just the plain engine this rebuilt executor should use for
        // `resolve_query`'s nested decomposition call. Every engine swap already
        // rebuilds the whole executor here, so there is no separate "wire once,
        // update later" path to support for it.
        let executor: Arc<dyn AgentToolExecutor> = Arc::new(GraphToolExecutor {
            node_service: Some(self.inner.node_service.clone()),
            embedding_service: self.inner.embedding_service.clone(),
            inference_engine: Some(engine.clone()),
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

        // Snapshot the model geometry here, the one place an engine can change.
        // Safe to query now: a swap happens between turns, so the engine mutex
        // this reaches is uncontended, unlike the same call from `get_status`.
        //
        // Bounded because this is not always a local read: a remote engine
        // answers `model_info` with an HTTP round-trip on a client with no
        // default timeout, so an endpoint that accepts the connection and then
        // stalls would hang the model-load RPC that awaits this. Losing the
        // geometry only costs a degraded status report; blocking the swap
        // would cost the load itself.
        let spec = match tokio::time::timeout(
            self.inner.model_spec_snapshot_timeout,
            new_service.model_spec(),
        )
        .await
        {
            Ok(Ok(spec)) => spec,
            Ok(Err(e)) => {
                tracing::warn!(
                    error = %e,
                    "model_spec failed during engine swap; status will report no model loaded"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    timeout = ?self.inner.model_spec_snapshot_timeout,
                    "model_spec timed out during engine swap; status will report no model loaded"
                );
                None
            }
        };

        let mut guard = self.inner.service.write().await;
        *guard = new_service;
        // Released before taking `loaded_model_spec`: no site holds two of
        // these locks at once, which is what keeps the ordering acyclic.
        drop(guard);
        *self.inner.loaded_model_spec.lock().await = spec;
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
        *guard = Arc::new(Self::build_noop_service(
            self.inner.node_service.clone(),
            self.inner.embedding_service.clone(),
        ));
        drop(guard);
        *self.inner.active_model_id.lock().await = None;
        *self.inner.loaded_model_spec.lock().await = None;
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
                let event = tokio::select! {
                    // Stop promptly when the owning database is closed (ADR-053),
                    // even if no further events arrive on the bus.
                    _ = this.inner.shutdown_token.cancelled() => {
                        tracing::info!(
                            "LocalAgentService event watcher: database closed, stopping"
                        );
                        break;
                    }
                    event = rx.recv() => event,
                };

                match event {
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

        let ai_chat = match AiChatNode::from_node(node) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Only trigger when the frontend has set status: processing, signalling
        // it wants an inference turn. Any other status (idle, error) is not
        // actionable here.
        if ai_chat.status != "processing" {
            return;
        }

        // Check that the last message is from the user.
        match ai_chat.messages.last() {
            Some(last) if last.role == "user" => {}
            _ => return,
        }

        // Atomically check-and-insert the cancellation token to prevent duplicate
        // turns when NodeCreated and NodeUpdated arrive in close succession.
        let cancel = {
            let mut tokens = self.inner.turn_tokens.lock().await;
            if tokens.contains_key(node_id) {
                return;
            }
            let cancel = CancellationToken::new();
            tokens.insert(node_id.to_string(), cancel.clone());
            cancel
        };

        tracing::info!(node_id, "ai-chat turn triggered");
        self.run_ai_chat_turn(node_id.to_string(), cancel).await;
    }

    /// Execute a full inference turn for the given ai-chat node.
    /// `cancel` is already stored in `turn_tokens` by the caller.
    /// The caller is responsible for having already set status: processing
    /// before triggering this turn — writing it again here would re-emit a
    /// nodeUpdated event and cause a re-entry loop.
    async fn run_ai_chat_turn(&self, node_id: String, cancel: CancellationToken) {
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

        // Refresh workspace context before creating the session.
        let emb = self.inner.embedding_service.read().await.clone();
        let ctx = build_workspace_context(&self.inner.node_service, emb, Some(&user_message)).await;

        // Create an ephemeral session seeded with prior history.
        let session_id = service.create_session(None, prior_history).await;

        if let Ok(ctx_str) = ctx {
            service.set_session_context(&session_id, ctx_str).await;
        }

        // Seed the deterministic duplicate guard with what earlier turns wrote.
        // The prompt note built from the same record tells the model the work is
        // done; this makes the tool-execution path enforce it regardless of
        // whether the model heeds that note.
        let prior_writes = load_prior_writes(&self.inner.node_service, &node_id).await;
        if !prior_writes.is_empty() {
            service
                .set_session_prior_writes(&session_id, prior_writes)
                .await;
        }

        let token_tx = self.inner.token_tx.clone();
        let node_id2 = node_id.clone();

        let send_fut = service.send_message(
            &session_id,
            &user_message,
            move |_status: LocalAgentStatus| {},
            move |chunk: StreamingChunk| {
                // `Done` is conveyed via the turn result, not a wire chunk.
                // `Reasoning` is intentionally not streamed live (it is captured
                // and surfaced via the persisted message; see issue design):
                // skip it here so the live answer stream stays clean.
                if !matches!(
                    chunk,
                    StreamingChunk::Done { .. } | StreamingChunk::Reasoning { .. }
                ) {
                    let mut proto = streaming_chunk_to_proto(chunk);
                    proto.node_id = Some(node_id2.clone());
                    // Ignore send errors — no subscribers connected is fine.
                    let _ = token_tx.send(proto);
                }
            },
        );

        // Race inference against the cancellation token.
        // `needs_idle_reset`: append_assistant_message sets status: idle on success,
        // so only cancelled/failed paths need an explicit reset.
        let needs_idle_reset;

        let turn_result = tokio::select! {
            result = send_fut => {
                match result {
                    Ok(r) => Some(r),
                    Err(e) => {
                        tracing::warn!(node_id, error = %e, "inference turn failed");
                        let _ = self.inner.token_tx.send(AgentChunk {
                            chunk_type: "error".to_string(),
                            error_message: Some(e.to_string()),
                            node_id: Some(node_id.clone()),
                            ..Default::default()
                        });
                        None
                    }
                }
            },
            () = cancel.cancelled() => {
                tracing::info!(node_id, "ai-chat turn cancelled");
                let _ = self.inner.token_tx.send(AgentChunk {
                    chunk_type: "cancelled".to_string(),
                    node_id: Some(node_id.clone()),
                    ..Default::default()
                });
                None
            }
        };

        // End the ephemeral session.
        service.end_session(&session_id).await;

        match turn_result {
            Some(result) => {
                // Emit done chunk to subscribers.
                let _ = self.inner.token_tx.send(AgentChunk {
                    chunk_type: "done".to_string(),
                    prompt_tokens: Some(result.usage.prompt_tokens as i32),
                    completion_tokens: Some(result.usage.completion_tokens as i32),
                    node_id: Some(node_id.clone()),
                    ..Default::default()
                });

                // Append assistant message; also atomically sets status: idle.
                // `AgentTurnResult` is the authoritative current-turn output: every
                // return path (normal, no-tools-final, synthesized fallback, empty)
                // sets `response`/`reasoning` for *this* turn. Using it directly avoids
                // a session-history scan that, on the fallback branches (which do not
                // push an assistant message), could surface a *previous* turn's
                // content/reasoning instead.
                match self
                    .append_assistant_message(
                        &node_id,
                        &result.response,
                        result.reasoning.as_deref(),
                        completed_writes_from(&result.tool_calls_made),
                    )
                    .await
                {
                    Ok(()) => needs_idle_reset = false,
                    Err(e) => {
                        tracing::warn!(node_id, error = %e, "failed to append assistant message");
                        needs_idle_reset = true;
                    }
                }
            }
            None => {
                // Cancelled — or inference error (send_message returned Err).
                needs_idle_reset = true;
            }
        }

        if needs_idle_reset {
            if let Err(e) = self.write_ai_chat_status(&node_id, "idle", None).await {
                tracing::warn!(node_id, error = %e, "failed to reset ai-chat status to idle");
            }
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
            let node_id = node.id.clone();
            let ai_chat = match AiChatNode::from_node(node) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if ai_chat.status != "processing" {
                continue;
            }
            // Verify last message is from user before retrying.
            let is_trailing_user = ai_chat
                .messages
                .last()
                .map(|m| m.role == "user")
                .unwrap_or(false);

            if is_trailing_user {
                tracing::info!(node_id = %node_id, "recovering stuck ai-chat turn");
                let cancel = {
                    let mut tokens = self.inner.turn_tokens.lock().await;
                    if tokens.contains_key(&node_id) {
                        continue;
                    }
                    let cancel = CancellationToken::new();
                    tokens.insert(node_id.clone(), cancel.clone());
                    cancel
                };
                let this = self.clone();
                tokio::spawn(async move {
                    this.run_ai_chat_turn(node_id, cancel).await;
                });
            } else {
                // Stuck in processing but no trailing user message — reset to idle.
                let _ = self.write_ai_chat_status(&node_id, "idle", None).await;
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Node write helpers
    // ---------------------------------------------------------------------------

    /// Write `properties['ai-chat']['status']` to the node.
    /// Retries up to 5 times on version conflict (optimistic concurrency).
    async fn write_ai_chat_status(
        &self,
        node_id: &str,
        status: &str,
        model: Option<&str>,
    ) -> Result<(), String> {
        for attempt in 0..5 {
            let node = self
                .inner
                .node_service
                .get_node(node_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("node {node_id} not found"))?;

            let version = node.version;
            let mut props = node.properties.clone();
            let mut ai_chat = AiChatNode::from_node(node).map_err(|e| e.to_string())?;

            ai_chat.status = status.to_string();
            if let Some(m) = model {
                ai_chat.model = Some(m.to_string());
            }

            // Splice the updated namespace back, preserving sibling namespaces.
            props["ai-chat"] = ai_chat.to_properties_value();

            let update = NodeUpdate::new().with_properties(props);
            match self
                .inner
                .node_service
                .update_node(node_id, version, update)
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
    /// Retries up to 5 times on version conflict. `reasoning` is the model's captured
    /// chain-of-thought, persisted alongside the answer when present.
    ///
    /// `completed_writes` records the graph writes this turn performed. The agent
    /// session is rebuilt from these persisted messages on every turn, so this is
    /// the only durable evidence that the turn's write actually happened; without
    /// it the next turn can re-execute an instruction it already satisfied.
    async fn append_assistant_message(
        &self,
        node_id: &str,
        content: &str,
        reasoning: Option<&str>,
        completed_writes: Vec<AiChatCompletedWrite>,
    ) -> Result<(), String> {
        for attempt in 0..5 {
            let node = self
                .inner
                .node_service
                .get_node(node_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("node {node_id} not found"))?;

            let version = node.version;
            let mut props = node.properties.clone();
            let mut ai_chat = AiChatNode::from_node(node).map_err(|e| e.to_string())?;

            // Persist reasoning only when the model produced some, keeping the
            // message shape minimal for plain answers.
            let reasoning = reasoning
                .filter(|r| !r.trim().is_empty())
                .map(|r| r.to_string());
            ai_chat.messages.push(AiChatMessage {
                role: "assistant".to_string(),
                content: content.to_string(),
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                reasoning,
                completed_writes: completed_writes.clone(),
            });

            // Set status to idle here too (atomic with message append).
            ai_chat.status = "idle".to_string();

            // Splice the updated namespace back, preserving sibling namespaces.
            props["ai-chat"] = ai_chat.to_properties_value();

            let update = NodeUpdate::new().with_properties(props);
            match self
                .inner
                .node_service
                .update_node(node_id, version, update)
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

        // Report the loaded model's real geometry alongside the activity status,
        // from the snapshot taken at engine-swap time. Deliberately NOT queried
        // live: `model_spec()` reaches a `std::sync::Mutex` held for the whole
        // of a generation, so a live call would block a tokio worker for the
        // length of a turn — the exact hang a status poller would trip over.
        let spec = self.inner.loaded_model_spec.lock().await.clone();
        // Report the catalog id the model was loaded BY, not the resolved GGUF
        // path the engine reports. Callers compare this against the id they
        // asked for ("gemma-4-e4b-q4km"), which no path substring matches.
        let active_model_id = self.inner.active_model_id.lock().await.clone();
        // The id and the window degrade independently. A snapshot that failed
        // or timed out costs the window, not the identity: `active_model_id` is
        // set by the same swap and is still authoritative, so a loaded model
        // keeps reporting itself with `granted_n_ctx == 0` (window unknown)
        // rather than vanishing into "nothing loaded".
        let (model_id, granted_n_ctx) = match spec {
            Some(spec) => (
                active_model_id.unwrap_or(spec.model_id),
                spec.context_window,
            ),
            None => (active_model_id.unwrap_or_default(), 0),
        };

        Ok(Response::new(LocalAgentStatusResponse {
            status_json,
            model_id,
            granted_n_ctx,
        }))
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
        let mut models = self
            .inner
            .model_manager
            .list()
            .await
            .map_err(|e| Status::internal(format!("Failed to list models: {e}")))?;

        models.extend(self.discover_openai_compat_models().await);

        let entries = models
            .into_iter()
            .map(|m| {
                let status_json = serde_json::to_string(&m.status).unwrap_or_default();
                let backend = m.backend.as_wire_str().to_string();
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
            .set_progress_callback(
                &model_id,
                Box::new(move |evt| {
                    let event = ModelLoadProgressEvent {
                        event_type: "downloading".to_string(),
                        model_id: mid_gguf.clone(),
                        bytes_downloaded: Some(evt.bytes_downloaded as i64),
                        bytes_total: Some(evt.bytes_total as i64),
                        ..Default::default()
                    };
                    let _ = tx_gguf.try_send(Ok(event));
                }),
            )
            .await;

        tokio::spawn(async move {
            match manager.download(&model_id_clone).await {
                Ok(()) => {
                    let _ = tx
                        .send(Ok(ModelLoadProgressEvent {
                            event_type: "ready".to_string(),
                            model_id: model_id_clone.clone(),
                            ..Default::default()
                        }))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(ModelLoadProgressEvent {
                            event_type: "error".to_string(),
                            model_id: model_id_clone.clone(),
                            error_message: Some(e.to_string()),
                            ..Default::default()
                        }))
                        .await;
                }
            }
            drop(tx);

            // Each progress callback holds a `Sender` clone, so the stream
            // above only closes (and the Tauri command awaiting it only
            // returns) once those clones are also dropped. Without this, a
            // completed download's channel is kept open indefinitely by its
            // own now-unused callback, hanging the frontend's await forever.
            // Cleared by model_id, not wholesale, so a concurrent download of
            // a different model is unaffected.
            manager.clear_progress_callback(&model_id_clone).await;
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
    /// Query every configured OpenAI-compatible endpoint for the models it
    /// serves, as catalog rows.
    ///
    /// Endpoints are queried concurrently: they are independent network calls,
    /// and the model selector awaits this whole listing before it can render.
    /// An endpoint that is unreachable or misconfigured contributes nothing
    /// rather than failing the catalog — a user with one dead provider must
    /// still see the models from every other one.
    async fn discover_openai_compat_models(&self) -> Vec<nodespace_agent::agent_types::ModelInfo> {
        use nodespace_agent::local_agent::openai_compat_discovery::{
            discover_models_or_empty, discovered_model_info,
        };

        let configs = match crate::services::settings_service::load_openai_compat_configs(
            &self.inner.daemon_config_path,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read OpenAI-compat configs for discovery");
                return Vec::new();
            }
        };

        let mut lookups = tokio::task::JoinSet::new();
        for config in configs {
            lookups.spawn(async move {
                let models = discover_models_or_empty(&config.base_url, &config.api_key).await;
                models
                    .into_iter()
                    .map(|model| discovered_model_info(&config.id, &config.name, &model))
                    .collect::<Vec<_>>()
            });
        }

        let mut discovered = Vec::new();
        while let Some(result) = lookups.join_next().await {
            match result {
                Ok(rows) => discovered.extend(rows),
                Err(e) => tracing::warn!(error = %e, "OpenAI-compat discovery task failed"),
            }
        }
        discovered
    }

    async fn load_model_and_collect_events(&self, model_id: &str) -> Vec<ModelLoadProgressEvent> {
        use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
        use nodespace_agent::local_agent::openai_compat_inference::{
            is_openai_compat, parse_openai_compat_id, OpenAiCompatInferenceEngine,
        };
        use nodespace_nlp_engine::chat::ChatConfig;

        let mut events = Vec::new();

        // OpenAI-compat configs are user-defined (stored in daemon.toml), not part
        // of the model catalog `list()` returns — resolve and branch on them first
        // so they never fall through to the "Unknown model" / GGUF path below.
        if is_openai_compat(model_id) {
            // A discovered model carries its own identifier after the config
            // UUID; without one, fall back to the config's pinned `model`.
            let (config_id, discovered_model) = parse_openai_compat_id(model_id);

            events.push(ModelLoadProgressEvent {
                event_type: "loading".to_string(),
                model_id: model_id.to_string(),
                message: Some("Connecting to OpenAI-compatible endpoint...".to_string()),
                ..Default::default()
            });

            let config = match crate::services::settings_service::find_openai_compat_config(
                &self.inner.daemon_config_path,
                config_id,
            )
            .await
            {
                Ok(Some(c)) => c,
                Ok(None) => {
                    events.push(ModelLoadProgressEvent {
                        event_type: "error".to_string(),
                        model_id: model_id.to_string(),
                        error_message: Some(format!(
                            "No OpenAI-compatible provider config found for '{config_id}'. Check Settings > Integrations."
                        )),
                        ..Default::default()
                    });
                    return events;
                }
                Err(e) => {
                    events.push(ModelLoadProgressEvent {
                        event_type: "error".to_string(),
                        model_id: model_id.to_string(),
                        error_message: Some(format!(
                            "Failed to read OpenAI-compatible provider config: {e}"
                        )),
                        ..Default::default()
                    });
                    return events;
                }
            };

            // `model` is the wire-protocol identifier (e.g. "gpt-4o" or
            // "mistral:7b"); `name` is only a cosmetic UI label and must never
            // be sent as the request's "model" field — real OpenAI-API and
            // multi-model servers reject or misroute an arbitrary display
            // string. A model discovered via /models wins over the config's
            // pinned value, since it names one of several models the same
            // endpoint serves.
            let model = match discovered_model {
                Some(m) => m.to_string(),
                None if !config.model.is_empty() => config.model.clone(),
                // Only for configs created before the field existed;
                // single-model local servers generally ignore it.
                None => "default".to_string(),
            };
            let engine = OpenAiCompatInferenceEngine::new(
                config.base_url.clone(),
                config.api_key.clone(),
                model,
            );
            let swapped = self
                .replace_engine_if_changed(model_id, Arc::new(engine))
                .await;

            events.push(ModelLoadProgressEvent {
                event_type: "ready".to_string(),
                model_id: model_id.to_string(),
                message: Some(format!("{} ready", config.name)),
                engine_swapped: Some(swapped),
                ..Default::default()
            });

            return events;
        }

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

        let model_path = match self.inner.model_manager.model_path(model_id) {
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

        let (family, chat_config) = match self.inner.model_manager.model_spec_for(model_id) {
            Ok(spec) => {
                let config = ChatConfig {
                    n_ctx: spec.context_window,
                    default_temperature: spec.default_temperature,
                    type_k: spec.type_k,
                    type_v: spec.type_v,
                    ..ChatConfig::default()
                };
                (spec.family, config)
            }
            Err(e) => {
                events.push(ModelLoadProgressEvent {
                    event_type: "error".to_string(),
                    model_id: model_id.to_string(),
                    error_message: Some(format!("Failed to look up model spec: {e}")),
                    ..Default::default()
                });
                return events;
            }
        };

        let model_path_str = model_path.to_string_lossy().to_string();
        let engine_result = tokio::task::spawn_blocking(move || {
            LlamaChatInferenceEngine::load(&model_path_str, family, chat_config)
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
        // Reasoning is not streamed live (filtered out before this function in the
        // send callback); it reaches the UI via the persisted message. This arm
        // exists only for exhaustiveness and tags the chunk so any future live
        // consumer can distinguish/ignore it rather than render it as the answer.
        StreamingChunk::Reasoning { text } => AgentChunk {
            chunk_type: "reasoning".to_string(),
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

/// Maximum length of an evidence summary before it is clipped.
const SUMMARY_MAX_CHARS: usize = 120;

/// Which argument identifies the thing a write acted on, per tool.
///
/// An explicit mapping rather than a probe across likely key names: the write
/// tools do not share one argument shape, and a probe silently degrades to a
/// bare tool name for the ones it does not happen to cover. That is worst
/// exactly where the evidence matters most — `create_nodes_from_markdown`
/// imports a whole subtree, so a repeat duplicates all of it.
///
/// Returning `None` here means "this tool changes graph state but is not
/// described by a single argument"; `create_relationship` is rendered from its
/// own fields instead (see `completed_writes_from`).
fn write_summary_arg(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "create_node" | "update_node" | "update_task_status" | "delete_node" => {
            Some(&["content", "title", "id", "node_id"])
        }
        // `create_schema` is keyed on the display name it declares; `update_schema`
        // identifies its target by `schema_id` and has no top-level `name` (the
        // only `name` keys sit nested inside `add_fields[]`, out of reach here).
        "create_schema" => Some(&["name"]),
        "update_schema" => Some(&["schema_id"]),
        "create_nodes_from_markdown" => Some(&["markdown"]),
        // `create_relationship` has no single describing argument; the call site
        // renders the edge from its own fields instead.
        _ => None,
    }
}

/// Clip an evidence label, marking it when clipped so a truncated summary is
/// not mistaken for a complete one.
fn clip_summary(s: &str) -> String {
    // Newlines would let user-supplied content shape the evidence block's
    // line structure; the label is a single line by construction.
    let flat = s.replace(['\n', '\r'], " ");
    if flat.chars().count() > SUMMARY_MAX_CHARS {
        let head: String = flat.chars().take(SUMMARY_MAX_CHARS).collect();
        format!("{head}…")
    } else {
        flat
    }
}

/// Pull the successful graph writes out of a turn's tool executions.
///
/// Failed calls are excluded: a write that errored did not happen, and recording
/// it would tell the next turn not to retry work that never landed.
fn completed_writes_from(executions: &[ToolExecutionRecord]) -> Vec<AiChatCompletedWrite> {
    executions
        .iter()
        .filter(|r| !r.is_error && is_write_tool(&r.name))
        .map(|r| {
            // Every write tool that reports an affected node does so under `id`
            // (as a `nodespace://` URI — the same form the model uses to refer to
            // nodes elsewhere, so the evidence matches what it already reads).
            // Schema and relationship writes report no node id at all.
            let node_id = r
                .result
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            // A label makes the evidence self-describing, so the model can match
            // it against the instruction still in history.
            let summary = match write_summary_arg(&r.name) {
                Some(keys) => keys
                    .iter()
                    .find_map(|k| r.args.get(*k))
                    .and_then(|v| v.as_str())
                    .map(clip_summary),
                // A relationship has no single describing argument; render the
                // edge itself, which is what identifies it.
                None if r.name == "create_relationship" => {
                    let field = |k: &str| {
                        r.args
                            .get(k)
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string()
                    };
                    Some(clip_summary(&format!(
                        "{} -[{}]-> {}",
                        field("from_id"),
                        field("relationship_type"),
                        field("to_id")
                    )))
                }
                None => None,
            };

            // Identity for the cross-turn duplicate guard. Canonicalised through
            // the same function the per-turn detector uses, so the two agree on
            // what "the same call" means. Dropped when oversized rather than
            // truncated — see `CANONICAL_ARGS_MAX_CHARS`.
            let canonical = canonical_args(&r.args.to_string());
            let canonical_args =
                (canonical.chars().count() <= CANONICAL_ARGS_MAX_CHARS).then_some(canonical);

            AiChatCompletedWrite {
                tool: r.name.clone(),
                node_id,
                summary,
                canonical_args,
            }
        })
        .collect()
}

/// Rebuild the duplicate-guard's view of earlier turns from persisted messages.
///
/// Only writes that carry a canonical-args identity can be matched against an
/// incoming call, so entries without one are skipped: they would match nothing
/// and only add noise. Filtering to the guarded tools here keeps the set small,
/// since the execution-path check applies the same restriction anyway.
fn prior_writes_from_history(messages: &[AiChatMessage]) -> Vec<PriorWrite> {
    messages
        .iter()
        .flat_map(|m| m.completed_writes.iter())
        .filter(|w| is_cross_turn_guarded_tool(&w.tool))
        .filter_map(|w| {
            w.canonical_args.as_ref().map(|args| PriorWrite {
                tool: w.tool.clone(),
                canonical_args: args.clone(),
                node_id: w.node_id.clone(),
                summary: w.summary.clone(),
            })
        })
        .collect()
}

/// Render persisted writes as a system-role note for the rebuilt history.
///
/// The assistant's prose ("I have added X") is an unverifiable claim; a model
/// weighing it against the user's still-present instruction may re-execute. This
/// restores the missing half of the record — concrete proof, stated as
/// fact-of-record rather than as the model's own narration.
///
/// `Role::System`, deliberately, not `Role::Tool`. A tool-role message is
/// rendered with a `tool_call_id` and must be preceded by the assistant
/// tool-call turn it answers (see `chat_message_to_oai_value` in
/// `nlp-engine/src/chat/mod.rs`). Those tool calls are not persisted, so a
/// tool-role message here would be an orphan tool result — the shape the
/// summarization back-off in `agent_loop.rs` exists specifically to avoid,
/// and which can abort a turn with llama.cpp `ffi error -3`.
fn completed_writes_message(writes: &[AiChatCompletedWrite]) -> Option<ChatMessage> {
    if writes.is_empty() {
        return None;
    }
    let mut lines = String::from(
        "Record of graph writes already completed in the previous turn. \
         These are done — do not repeat them:\n",
    );
    for w in writes {
        lines.push_str(&format!("- {}", w.tool));
        if let Some(ref s) = w.summary {
            lines.push_str(&format!(" \"{s}\""));
        }
        if let Some(ref id) = w.node_id {
            lines.push_str(&format!(" -> {id}"));
        }
        lines.push('\n');
    }
    Some(ChatMessage::text(
        Role::System,
        lines.trim_end().to_string(),
    ))
}

/// Load the writes completed by earlier turns of this chat.
///
/// Separate from `load_node_history` because that function's return type is
/// `ChatMessage`, which has no room for the per-message write record — the very
/// erasure that let the original duplicate through.
async fn load_prior_writes(node_service: &Arc<NodeService>, node_id: &str) -> Vec<PriorWrite> {
    let node = match node_service.get_node(node_id).await {
        Ok(Some(n)) => n,
        // A missing or unreadable node is already logged by `load_node_history`,
        // which runs first on the same node; staying quiet here avoids a
        // duplicate error line for one underlying failure.
        _ => return Vec::new(),
    };
    match AiChatNode::from_node(node) {
        Ok(c) => prior_writes_from_history(&c.messages),
        Err(_) => Vec::new(),
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

    let ai_chat = match AiChatNode::from_node(node) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(node_id, error = %e, "node is not an ai-chat node");
            return vec![];
        }
    };

    ai_chat
        .messages
        .into_iter()
        .flat_map(|m| {
            let role = match m.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => return Vec::new(),
            };
            let mut msg = ChatMessage::text(role, m.content);
            // Round-trip any persisted reasoning so reloaded history retains it.
            msg.reasoning = m.reasoning;
            // Follow the assistant turn with the record of what it actually wrote,
            // so the next turn can tell a completed instruction from a pending one.
            match completed_writes_message(&m.completed_writes) {
                Some(evidence) => vec![msg, evidence],
                None => vec![msg],
            }
        })
        .collect()
}

async fn build_workspace_context(
    node_service: &Arc<NodeService>,
    embedding_service: Option<Arc<NodeEmbeddingService>>,
    query: Option<&str>,
) -> Result<String, ()> {
    let mut context = nodespace_core::ops::context_ops::build_workspace_context(
        node_service,
        embedding_service.as_ref(),
        query,
    )
    .await
    .map_err(|_| ())?;

    // Inject schemas created in the last 5 minutes that may not yet be indexed
    // in the embedding store (30s debounce). This ensures the model sees custom
    // types it just created when composing the next turn in the same session.
    // Capped at MAX_SEMANTIC_SCHEMAS total to prevent unbounded context growth
    // during batch schema creation sessions.
    const MAX_SCHEMAS: usize = 5;
    if let Ok(all_schemas) = node_service.get_all_schemas().await {
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
        let existing_ids: std::collections::HashSet<String> = context
            .relevant_schemas
            .iter()
            .map(|s| s.id.clone())
            .collect();
        let remaining_slots = MAX_SCHEMAS.saturating_sub(context.relevant_schemas.len());
        let mut injected = 0;
        for schema in all_schemas {
            if injected >= remaining_slots {
                break;
            }
            if schema.is_core {
                continue; // skip built-in types
            }
            if existing_ids.contains(&schema.id) {
                continue; // already present from semantic search
            }
            if schema.created_at >= cutoff {
                tracing::debug!(
                    schema_id = %schema.id,
                    "workspace_context: injecting recently-created schema (debounce bypass)"
                );
                context.relevant_schemas.push(schema);
                injected += 1;
            }
        }
    }

    Ok(context.format_for_prompt(4000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_core::models::Node;
    use nodespace_core::{NodeService as CoreNodeService, SqliteStore};

    /// Build a `LocalAgentServiceImpl` backed by a temp-dir SqliteStore.
    /// Returns the `TempDir` so it outlives the test body.
    async fn test_service() -> (LocalAgentServiceImpl, Arc<NodeService>, tempfile::TempDir) {
        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let mut store = Arc::new(
            SqliteStore::new(tempdir.path().join("daemon-db"))
                .await
                .expect("SqliteStore"),
        );
        let node_service = Arc::new(CoreNodeService::new(&mut store).await.expect("NodeService"));
        let embedding: SharedEmbeddingService = Arc::new(RwLock::new(None));
        let daemon_config_path = tempdir.path().join("daemon.toml");
        let svc = LocalAgentServiceImpl::new(node_service.clone(), embedding, daemon_config_path);
        (svc, node_service, tempdir)
    }

    async fn create_ai_chat_node(node_service: &Arc<NodeService>) -> String {
        let node = Node::new(
            "ai-chat".to_string(),
            "Test chat".to_string(),
            serde_json::json!({ "ai-chat": { "messages": [] } }),
        );
        node_service
            .create_node(node)
            .await
            .expect("create ai-chat")
    }

    /// Create an ai-chat node already sitting in `status: "processing"` with a
    /// trailing user message — the exact shape the frontend produces via
    /// batch-update before triggering a turn (mirrors `scripts/aichat.ts`).
    async fn create_processing_node_with_user_message(
        node_service: &Arc<NodeService>,
        user_text: &str,
    ) -> String {
        let node_id = create_ai_chat_node(node_service).await;
        let node = node_service
            .get_node(&node_id)
            .await
            .expect("get node")
            .expect("node exists");
        let version = node.version;
        let mut ai_chat = AiChatNode::from_node(node).expect("from_node");
        ai_chat.status = "processing".to_string();
        ai_chat.messages.push(AiChatMessage {
            role: "user".to_string(),
            content: user_text.to_string(),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            reasoning: None,
            completed_writes: Vec::new(),
        });
        let mut props = serde_json::json!({});
        props["ai-chat"] = ai_chat.to_properties_value();
        node_service
            .update_node(&node_id, version, NodeUpdate::new().with_properties(props))
            .await
            .expect("set processing + user message");
        node_id
    }

    async fn get_ai_chat(node_service: &Arc<NodeService>, node_id: &str) -> AiChatNode {
        let node = node_service
            .get_node(node_id)
            .await
            .expect("get node")
            .expect("node exists");
        AiChatNode::from_node(node).expect("from_node")
    }

    // -- Stub inference engine -------------------------------------------

    /// Canned engine returning a fixed text reply with no tool calls — mirrors
    /// `agent_loop.rs`'s `MockEngine::single_text` pattern, one level up at the
    /// daemon-service seam. Counts constructions/generations so tests can
    /// assert engine-reuse ("second message reuses the loaded engine").
    struct StubEngine {
        reply: String,
        generate_count: std::sync::atomic::AtomicUsize,
    }

    impl StubEngine {
        fn new(reply: &str) -> Self {
            Self {
                reply: reply.to_string(),
                generate_count: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ChatInferenceEngine for StubEngine {
        async fn generate(
            &self,
            _request: nodespace_agent::agent_types::InferenceRequest,
            on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            self.generate_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            on_chunk(StreamingChunk::Token {
                text: self.reply.clone(),
            });
            on_chunk(StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                },
            });
            Ok(InferenceUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
            })
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

    /// An engine whose `generate` blocks until the test releases it, so a
    /// turn can be cancelled mid-flight deterministically.
    struct BlockingEngine {
        release: tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    }

    impl BlockingEngine {
        fn new(release: tokio::sync::oneshot::Receiver<()>) -> Self {
            Self {
                release: tokio::sync::Mutex::new(Some(release)),
            }
        }
    }

    #[async_trait]
    impl ChatInferenceEngine for BlockingEngine {
        async fn generate(
            &self,
            _request: nodespace_agent::agent_types::InferenceRequest,
            _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            let rx = self.release.lock().await.take();
            if let Some(rx) = rx {
                let _ = rx.await;
            }
            Err(InferenceError::Engine("should not complete".into()))
        }

        async fn model_info(
            &self,
        ) -> Result<Option<nodespace_agent::agent_types::ChatModelSpec>, InferenceError> {
            Ok(None)
        }

        async fn token_count(&self, _text: &str) -> Result<u32, InferenceError> {
            Ok(0)
        }
    }

    /// Engine reporting a fixed geometry, so status assertions have real
    /// values to check rather than the `None` the other stubs return.
    struct SpecEngine {
        model_id: String,
        context_window: u32,
    }

    #[async_trait]
    impl ChatInferenceEngine for SpecEngine {
        async fn generate(
            &self,
            _request: nodespace_agent::agent_types::InferenceRequest,
            _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            Err(InferenceError::Engine("not used".into()))
        }

        async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
            Ok(Some(ChatModelSpec {
                model_id: self.model_id.clone(),
                family: nodespace_agent::agent_types::ModelFamily::Gemma4,
                context_window: self.context_window,
                default_temperature: 0.7,
                type_k: None,
                type_v: None,
            }))
        }

        async fn token_count(&self, _text: &str) -> Result<u32, InferenceError> {
            Ok(0)
        }
    }

    /// Engine that models the native `LlamaChatEngine` locking discipline: a
    /// `std::sync::Mutex` held for the *whole* of a generation, which
    /// `model_info` must also take. This is the shape that makes a live
    /// `model_spec()` call from `get_status` block for the length of a turn.
    struct MutexHeldDuringGenerationEngine {
        /// Stands in for `LlamaChatEngine`'s state mutex.
        state: Arc<std::sync::Mutex<()>>,
        /// Signals that `generate` has taken the lock.
        started: tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        /// Released by the test to let `generate` finish.
        release: tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    }

    #[async_trait]
    impl ChatInferenceEngine for MutexHeldDuringGenerationEngine {
        async fn generate(
            &self,
            _request: nodespace_agent::agent_types::InferenceRequest,
            _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            let started = self.started.lock().await.take();
            let release = self.release.lock().await.take();
            let state = self.state.clone();
            // Take the lock on a blocking thread and hold it for the whole
            // "generation", as the native engine's `generate_blocking` does.
            // A `std::sync::MutexGuard` cannot be held across an await point,
            // which is exactly why the real lock lives off the async path.
            tokio::task::spawn_blocking(move || {
                let _guard = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(tx) = started {
                    let _ = tx.send(());
                }
                if let Some(rx) = release {
                    let _ = rx.blocking_recv();
                }
            })
            .await
            .expect("generation thread joins");
            Err(InferenceError::Engine("should not complete".into()))
        }

        async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
            let _guard = self.state.lock().unwrap_or_else(|p| p.into_inner());
            Ok(Some(ChatModelSpec {
                model_id: "held-model".to_string(),
                family: nodespace_agent::agent_types::ModelFamily::Gemma4,
                context_window: 8192,
                default_temperature: 0.7,
                type_k: None,
                type_v: None,
            }))
        }

        async fn token_count(&self, _text: &str) -> Result<u32, InferenceError> {
            Ok(0)
        }
    }

    // -- get_status model geometry -----------------------------------------

    #[tokio::test]
    async fn get_status_reports_geometry_from_swapped_engine() {
        let (svc, _node_service, _tempdir) = test_service().await;
        svc.replace_engine_if_changed(
            "gemma-4-e4b-q4km",
            Arc::new(SpecEngine {
                model_id: "/models/resolved-path.gguf".to_string(),
                context_window: 16384,
            }),
        )
        .await;

        let status = svc
            .get_status(Request::new(GetLocalStatusRequest { session_id: None }))
            .await
            .expect("get_status")
            .into_inner();

        // The catalog id the model was loaded BY, not the engine's GGUF path.
        assert_eq!(status.model_id, "gemma-4-e4b-q4km");
        assert_eq!(status.granted_n_ctx, 16384);
    }

    #[tokio::test]
    async fn get_status_reports_no_model_after_reset() {
        let (svc, _node_service, _tempdir) = test_service().await;
        svc.replace_engine_if_changed(
            "gemma-4-e4b-q4km",
            Arc::new(SpecEngine {
                model_id: "/models/resolved-path.gguf".to_string(),
                context_window: 16384,
            }),
        )
        .await;
        svc.reset_to_noop_engine().await;

        let status = svc
            .get_status(Request::new(GetLocalStatusRequest { session_id: None }))
            .await
            .expect("get_status")
            .into_inner();

        // The cached geometry must be cleared on reset, not left stale.
        assert_eq!(status.model_id, "");
        assert_eq!(status.granted_n_ctx, 0);
    }

    /// Engine whose `model_info` never returns — the shape of a remote
    /// endpoint that accepts the connection and then stalls.
    struct HangingModelInfoEngine;

    #[async_trait]
    impl ChatInferenceEngine for HangingModelInfoEngine {
        async fn generate(
            &self,
            _request: nodespace_agent::agent_types::InferenceRequest,
            _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            Err(InferenceError::Engine("not used".into()))
        }

        async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
            std::future::pending().await
        }

        async fn token_count(&self, _text: &str) -> Result<u32, InferenceError> {
            Ok(0)
        }
    }

    /// A remote engine that stalls on `model_info` must not hold up the swap.
    /// Losing the geometry degrades the status report; blocking here would
    /// block the model-load RPC that awaits `replace_engine`.
    #[tokio::test]
    async fn engine_swap_completes_when_model_info_hangs() {
        let (mut svc, _node_service, _tempdir) = test_service().await;
        // Drive the timeout path without paying the production bound in
        // wall-clock time on every run.
        let short = std::time::Duration::from_millis(50);
        Arc::get_mut(&mut svc.inner)
            .expect("sole owner before any clone")
            .model_spec_snapshot_timeout = short;

        // Both calls run on a runtime of their own, on a dedicated OS thread,
        // and the test waits on a channel. Neither may share a runtime with
        // this test: if the swap timeout regresses, `replace_engine` never
        // returns; and if `get_status` regresses to a live engine query, it
        // blocks its thread outright. Either one, awaited on a shared runtime,
        // hangs the whole suite rather than failing this test — which is what
        // it did before, but only under concurrent load, making it a latent
        // flake. Waiting on a channel cannot be starved by tokio scheduling.
        let swap_svc = svc.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("swap runtime");
            let result = rt.block_on(async move {
                swap_svc
                    .replace_engine_if_changed("hanging-model", Arc::new(HangingModelInfoEngine))
                    .await;
                swap_svc
                    .get_status(Request::new(GetLocalStatusRequest { session_id: None }))
                    .await
            });
            // A send failure means the test already timed out and moved on.
            let _ = done_tx.send(result);
        });

        let status = done_rx
            .recv_timeout(short + std::time::Duration::from_secs(5))
            .expect("engine swap must not block on a stalled model_info")
            .expect("get_status")
            .into_inner();

        // Swap completed rather than hanging the caller, and the model still
        // reports its identity — only the window is unknown. Reporting an
        // empty id here would make a loaded model look absent.
        assert_eq!(status.model_id, "hanging-model");
        assert_eq!(status.granted_n_ctx, 0);
    }

    /// The core regression guard: `get_status` must not touch the engine
    /// mutex, which a generation holds for its entire duration.
    ///
    /// Querying `model_spec()` live here would block until the turn finished.
    /// The generation is deliberately never released before the status call, so
    /// against the pre-fix code the status RPC blocks outright rather than
    /// merely returning wrong values, and the timeout turns that into a
    /// failure. Needs the multi-thread runtime: the blocking lock wedges a
    /// worker thread, and on the default single-threaded runtime there would be
    /// no thread left to fire the timeout — the test would hang instead of
    /// failing cleanly.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn get_status_does_not_block_while_a_generation_holds_the_engine_lock() {
        let (svc, node_service, _tempdir) = test_service().await;

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        svc.replace_engine_if_changed(
            "held-model",
            Arc::new(MutexHeldDuringGenerationEngine {
                state: Arc::new(std::sync::Mutex::new(())),
                started: tokio::sync::Mutex::new(Some(started_tx)),
                release: tokio::sync::Mutex::new(Some(release_rx)),
            }),
        )
        .await;

        // Kick off a turn and wait until it actually holds the engine lock.
        let node_id = create_processing_node_with_user_message(&node_service, "Hi").await;
        let turn_svc = svc.clone();
        let turn = tokio::spawn(async move {
            turn_svc.maybe_handle_ai_chat_node(&node_id).await;
        });
        started_rx.await.expect("generation should take the lock");

        // Run the status call on a runtime of its own, on a dedicated OS
        // thread, and wait on a channel rather than on the task.
        //
        // A pre-fix `get_status` blocks the thread it runs on outright. Any
        // construction that shares a runtime with it — a spawned task, a
        // timeout wrapped around the call — can have every worker consumed
        // before the timer is scheduled, which hangs the whole suite instead
        // of failing this test. Whether that happens depends on what else the
        // harness is running concurrently, so it is a latent flake in any
        // shared-runtime form. An isolated runtime cannot be starved by the
        // blocked call, so the failure stays clean and deterministic.
        let status_svc = svc.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("status runtime");
            let result = rt.block_on(
                status_svc.get_status(Request::new(GetLocalStatusRequest { session_id: None })),
            );
            // A send failure means the test already timed out and moved on.
            let _ = done_tx.send(result);
        });

        let status = done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("get_status must return while a generation holds the engine lock")
            .expect("get_status")
            .into_inner();

        // Served from the cache, so the real geometry is still reported.
        assert_eq!(status.model_id, "held-model");
        assert_eq!(status.granted_n_ctx, 8192);

        let _ = release_tx.send(());
        let _ = turn.await;
    }

    // -- Agent-flow (stuck-state) coverage with a stub model ----------------

    #[tokio::test]
    async fn send_message_reaches_idle_with_stub_engine() {
        let (svc, node_service, _tempdir) = test_service().await;
        svc.replace_engine(Arc::new(StubEngine::new("Hello back!")))
            .await;

        let node_id = create_processing_node_with_user_message(&node_service, "Hi there").await;

        svc.maybe_handle_ai_chat_node(&node_id).await;

        let ai_chat = get_ai_chat(&node_service, &node_id).await;
        assert_eq!(
            ai_chat.status, "idle",
            "turn must terminate, never stuck processing"
        );
        let assistant = ai_chat
            .messages
            .iter()
            .find(|m| m.role == "assistant")
            .expect("assistant reply appended");
        assert_eq!(assistant.content, "Hello back!");
        assert!(
            svc.inner.turn_tokens.lock().await.get(&node_id).is_none(),
            "no lingering turn token after completion"
        );
    }

    #[tokio::test]
    async fn stuck_processing_node_recovered_on_watcher_restart() {
        let (svc, node_service, _tempdir) = test_service().await;
        svc.replace_engine(Arc::new(StubEngine::new("Recovered reply.")))
            .await;

        // Simulate a daemon crash mid-turn: node left in `processing` with a
        // trailing user message, no in-memory turn_tokens entry (fresh process).
        let node_id =
            create_processing_node_with_user_message(&node_service, "Are you there?").await;

        svc.recover_stuck_turns().await;

        // recover_stuck_turns spawns the retry; poll briefly for completion.
        let mut ai_chat = get_ai_chat(&node_service, &node_id).await;
        for _ in 0..50 {
            if ai_chat.status == "idle" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            ai_chat = get_ai_chat(&node_service, &node_id).await;
        }

        assert_eq!(
            ai_chat.status, "idle",
            "a node stuck in processing at startup must recover to idle, not stay stuck"
        );
        assert!(ai_chat.messages.iter().any(|m| m.role == "assistant"));
    }

    #[tokio::test]
    async fn turn_cancelled_via_cancel_turn_resets_to_idle() {
        let (svc, node_service, _tempdir) = test_service().await;
        let (_release_tx, release_rx) = tokio::sync::oneshot::channel();
        svc.replace_engine(Arc::new(BlockingEngine::new(release_rx)))
            .await;

        let node_id =
            create_processing_node_with_user_message(&node_service, "Take your time").await;

        // Trigger the turn in the background (it will block inside BlockingEngine).
        let svc2 = svc.clone();
        let node_id2 = node_id.clone();
        let handle = tokio::spawn(async move {
            svc2.maybe_handle_ai_chat_node(&node_id2).await;
        });

        // Wait until the turn has actually registered its cancellation token.
        for _ in 0..50 {
            if svc.inner.turn_tokens.lock().await.contains_key(&node_id) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            svc.inner.turn_tokens.lock().await.contains_key(&node_id),
            "turn should be registered before cancelling"
        );

        // Cancel via the same path the gRPC CancelTurn handler uses.
        {
            let tokens = svc.inner.turn_tokens.lock().await;
            if let Some(token) = tokens.get(&node_id) {
                token.cancel();
            }
        }
        handle.await.expect("turn task joins");

        let ai_chat = get_ai_chat(&node_service, &node_id).await;
        assert_eq!(
            ai_chat.status, "idle",
            "cancelled turn must reset to idle, not stay stuck"
        );
        assert!(
            !ai_chat.messages.iter().any(|m| m.role == "assistant"),
            "a cancelled turn must not append an assistant message"
        );
        assert!(
            svc.inner.turn_tokens.lock().await.get(&node_id).is_none(),
            "no stray cancellation-token entry left behind after cancel"
        );
    }

    #[tokio::test]
    async fn second_message_reuses_loaded_engine() {
        let (svc, node_service, _tempdir) = test_service().await;
        let swapped_first = svc
            .replace_engine_if_changed("stub-model", Arc::new(StubEngine::new("first reply")))
            .await;
        assert!(swapped_first, "first load must swap the engine in");

        let node_id_1 =
            create_processing_node_with_user_message(&node_service, "First message").await;
        svc.maybe_handle_ai_chat_node(&node_id_1).await;
        assert_eq!(get_ai_chat(&node_service, &node_id_1).await.status, "idle");

        // A second "load" of the same model_id must be a no-op swap — this is
        // the literal "second message reuses the loaded engine" criterion.
        let swapped_second = svc
            .replace_engine_if_changed("stub-model", Arc::new(StubEngine::new("second reply")))
            .await;
        assert!(
            !swapped_second,
            "loading the same model_id again must not swap the engine"
        );

        let node_id_2 =
            create_processing_node_with_user_message(&node_service, "Second message").await;
        svc.maybe_handle_ai_chat_node(&node_id_2).await;

        let ai_chat_2 = get_ai_chat(&node_service, &node_id_2).await;
        assert_eq!(ai_chat_2.status, "idle");
        // Because the swap was skipped, the ORIGINAL engine (first reply) is
        // still the one wired in and answers the second turn too.
        let assistant = ai_chat_2
            .messages
            .iter()
            .find(|m| m.role == "assistant")
            .expect("assistant reply appended");
        assert_eq!(assistant.content, "first reply");
    }

    #[tokio::test]
    async fn reasoning_round_trips_through_persist_and_reload() {
        let (svc, node_service, _tempdir) = test_service().await;
        let node_id = create_ai_chat_node(&node_service).await;

        svc.append_assistant_message(
            &node_id,
            "The answer.",
            Some("I reasoned about it."),
            Vec::new(),
        )
        .await
        .expect("append");

        let history = load_node_history(&node_service, &node_id).await;
        let assistant = history
            .iter()
            .find(|m| matches!(m.role, Role::Assistant))
            .expect("assistant message present");
        assert_eq!(assistant.content, "The answer.");
        assert_eq!(assistant.reasoning.as_deref(), Some("I reasoned about it."));
    }

    /// Build a successful tool execution record.
    fn exec(name: &str, args: serde_json::Value, result: serde_json::Value) -> ToolExecutionRecord {
        ToolExecutionRecord {
            tool_call_id: format!("tc_{name}"),
            name: name.to_string(),
            args,
            result,
            is_error: false,
            duration_ms: 1,
        }
    }

    /// The cross-turn case. The per-turn `seen_calls` guard cannot cover this:
    /// the agent session is destroyed at the end of every turn, so turn N+1 has
    /// no in-memory record of turn N at all.
    ///
    /// This is the regression that matters — before the fix, the rebuilt history
    /// contained the user's instruction and the assistant's prose claim but no
    /// trace of the write, so the model could satisfy the same instruction twice.
    #[tokio::test]
    async fn completed_write_is_visible_in_the_next_turns_history() {
        let (svc, node_service, _tempdir) = test_service().await;
        let node_id = create_ai_chat_node(&node_service).await;

        let writes = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"content": "Kind of Blue by Miles Davis", "node_type": "album_to_listen"}),
            serde_json::json!({"id": "nodespace://f1f25564"}),
        )]);

        svc.append_assistant_message(&node_id, "I have added \"Kind of Blue\".", None, writes)
            .await
            .expect("append");

        // What the NEXT turn actually sees.
        let history = load_node_history(&node_service, &node_id).await;

        let evidence = history
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .expect(
                "next turn must see durable evidence of the completed write; without it the \
                 model re-executes the instruction still present in history",
            );
        assert!(evidence.content.contains("create_node"));
        assert!(evidence.content.contains("Kind of Blue by Miles Davis"));
        assert!(evidence.content.contains("nodespace://f1f25564"));
        // The evidence must follow the assistant turn it describes.
        let assistant_idx = history
            .iter()
            .position(|m| matches!(m.role, Role::Assistant))
            .expect("assistant message present");
        let evidence_idx = history
            .iter()
            .position(|m| matches!(m.role, Role::System))
            .expect("evidence present");
        assert!(evidence_idx > assistant_idx);

        // The evidence must never be a tool-role message: those require a
        // preceding assistant tool-call turn to pair with, and none is
        // persisted. An orphan tool result can abort the turn outright.
        assert!(
            !history.iter().any(|m| matches!(m.role, Role::Tool)),
            "evidence must not be injected as an orphan tool result"
        );
    }

    /// A write that failed did not happen. Recording it would tell the next turn
    /// not to retry work that never landed.
    #[tokio::test]
    async fn failed_writes_are_not_recorded_as_completed() {
        let failed = ToolExecutionRecord {
            tool_call_id: "tc_1".into(),
            name: "create_node".into(),
            args: serde_json::json!({"content": "never landed"}),
            result: serde_json::json!({"error": "validation failed"}),
            is_error: true,
            duration_ms: 1,
        };
        assert!(completed_writes_from(&[failed]).is_empty());
    }

    /// Reads are not writes: repeating a search is wasteful, not a duplicate,
    /// and recording them would bloat every subsequent prompt.
    #[tokio::test]
    async fn reads_are_not_recorded_as_completed_writes() {
        let writes = completed_writes_from(&[
            exec(
                "search_nodes",
                serde_json::json!({"query": "x"}),
                serde_json::json!({"count": 3}),
            ),
            exec(
                "get_node",
                serde_json::json!({"id": "n1"}),
                serde_json::json!({"content": "y"}),
            ),
        ]);
        assert!(writes.is_empty());
    }

    /// A turn that only read must not emit an evidence message at all — an empty
    /// "writes completed" block would be noise in every prompt.
    #[tokio::test]
    async fn read_only_turn_adds_no_evidence_message() {
        let (svc, node_service, _tempdir) = test_service().await;
        let node_id = create_ai_chat_node(&node_service).await;

        svc.append_assistant_message(&node_id, "I found 3 tasks.", None, Vec::new())
            .await
            .expect("append");

        let history = load_node_history(&node_service, &node_id).await;
        assert!(!history.iter().any(|m| matches!(m.role, Role::System)));
    }

    /// Evidence must be self-describing for every write tool, not just the ones
    /// that happen to take a `content` argument. A bare tool name cannot be
    /// matched against the instruction it satisfied — which is the whole point.
    #[tokio::test]
    async fn every_write_tool_produces_a_usable_label() {
        // A relationship has no describing argument and returns no node id; it
        // must still render as something identifiable.
        let rel = completed_writes_from(&[exec(
            "create_relationship",
            serde_json::json!({
                "from_id": "nodespace://a",
                "to_id": "nodespace://b",
                "relationship_type": "mentions"
            }),
            serde_json::json!({"from_id": "nodespace://a", "to_id": "nodespace://b", "created": true}),
        )]);
        assert_eq!(rel.len(), 1);
        assert_eq!(rel[0].node_id, None);
        let label = rel[0].summary.as_deref().expect("relationship label");
        assert!(label.contains("nodespace://a"), "got {label:?}");
        assert!(label.contains("mentions"), "got {label:?}");
        assert!(label.contains("nodespace://b"), "got {label:?}");

        // A markdown import is the highest-stakes duplicate in the set: its
        // argument key is `markdown`, not `content`.
        let md = completed_writes_from(&[exec(
            "create_nodes_from_markdown",
            serde_json::json!({"markdown": "# Trip plan\n- flights"}),
            serde_json::json!({"created": 4}),
        )]);
        assert_eq!(md.len(), 1);
        assert!(
            md[0].summary.as_deref().unwrap_or("").contains("Trip plan"),
            "markdown import must be identifiable, got {:?}",
            md[0].summary
        );

        // Schemas are named, not contented.
        let schema = completed_writes_from(&[exec(
            "create_schema",
            serde_json::json!({"name": "album_to_listen"}),
            serde_json::json!({"created": true}),
        )]);
        assert_eq!(schema[0].summary.as_deref(), Some("album_to_listen"));

        // `update_schema` identifies its target by `schema_id`, not `name`. A
        // repeat here is destructive — `rename_fields` rekeys property data on
        // every existing node of the type — so it must not degrade to a bare
        // tool name.
        let updated = completed_writes_from(&[exec(
            "update_schema",
            serde_json::json!({
                "schema_id": "album_to_listen",
                "add_fields": [{"name": "rating", "field_type": "number"}]
            }),
            serde_json::json!({"updated": true}),
        )]);
        assert_eq!(updated[0].summary.as_deref(), Some("album_to_listen"));
    }

    /// The node id is stored as the `nodespace://` URI the tools actually
    /// return, matching the form the model uses to refer to nodes elsewhere.
    #[tokio::test]
    async fn node_id_is_recorded_as_the_uri_the_tool_returned() {
        let writes = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"content": "a"}),
            serde_json::json!({"id": "nodespace://aaa"}),
        )]);
        assert_eq!(writes[0].node_id.as_deref(), Some("nodespace://aaa"));
    }

    /// A clipped label must be distinguishable from a complete one, and must not
    /// carry newlines that would reshape the evidence block.
    #[tokio::test]
    async fn long_and_multiline_summaries_are_clipped_and_flattened() {
        let writes = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"content": format!("{}\nsecond line", "x".repeat(200))}),
            serde_json::json!({"id": "nodespace://a"}),
        )]);
        let s = writes[0].summary.as_deref().expect("summary");
        assert!(s.ends_with('…'), "clipped summary must be marked: {s:?}");
        assert!(!s.contains('\n'));
        assert_eq!(s.chars().count(), SUMMARY_MAX_CHARS + 1);
    }

    #[tokio::test]
    async fn empty_or_whitespace_reasoning_is_omitted() {
        let (svc, node_service, _tempdir) = test_service().await;
        let node_id = create_ai_chat_node(&node_service).await;

        // None and whitespace-only both persist no reasoning field.
        svc.append_assistant_message(&node_id, "Plain answer.", None, Vec::new())
            .await
            .expect("append none");
        svc.append_assistant_message(&node_id, "Another answer.", Some("   "), Vec::new())
            .await
            .expect("append whitespace");

        let history = load_node_history(&node_service, &node_id).await;
        let assistants: Vec<_> = history
            .iter()
            .filter(|m| matches!(m.role, Role::Assistant))
            .collect();
        assert_eq!(assistants.len(), 2);
        assert!(assistants.iter().all(|m| m.reasoning.is_none()));
    }

    #[tokio::test]
    async fn load_openai_compat_model_without_config_returns_clear_error() {
        let (svc, _node_service, _tempdir) = test_service().await;

        // No daemon.toml exists yet, so the config lookup returns None. This
        // must surface a specific "no config found" error, not fall through to
        // the GGUF path-resolution failure the bug report described.
        let events = svc
            .load_model_and_collect_events("openai-compat:missing-uuid")
            .await;

        let error_event = events
            .iter()
            .find(|e| e.event_type == "error")
            .expect("an error event should be emitted");
        let message = error_event
            .error_message
            .as_deref()
            .expect("error_message should be set");
        assert!(
            message.contains("No OpenAI-compatible provider config found"),
            "unexpected error message: {message}"
        );
        assert!(
            !message.to_lowercase().contains("gguf"),
            "error must not leak the GGUF path-resolution failure: {message}"
        );
    }

    #[tokio::test]
    async fn load_openai_compat_model_with_config_swaps_engine() {
        let (svc, _node_service, tempdir) = test_service().await;
        let config_path = tempdir.path().join("daemon.toml");
        let toml = r#"
[[openai_compat.configs]]
id = "abc-123"
name = "My Endpoint"
base_url = "http://127.0.0.1:9999/v1"
api_key = "sk-test"
"#;
        tokio::fs::write(&config_path, toml)
            .await
            .expect("write daemon.toml");

        let events = svc
            .load_model_and_collect_events("openai-compat:abc-123")
            .await;

        let ready_event = events
            .iter()
            .find(|e| e.event_type == "ready")
            .expect("a ready event should be emitted");
        assert_eq!(ready_event.engine_swapped, Some(true));
        assert!(events.iter().all(|e| e.event_type != "error"));
    }

    // -- Cross-turn duplicate-write guard --------------------------------

    /// The guard's identity is `(tool, canonical_args)`, so a write is only
    /// usable by it if the canonical args survive persistence. Without this the
    /// whole mechanism degrades to a no-op that still looks wired up.
    #[tokio::test]
    async fn completed_writes_record_canonical_args_for_the_guard() {
        let writes = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"content": "Buy milk", "node_type": "task"}),
            serde_json::json!({"id": "nodespace://n1"}),
        )]);
        assert_eq!(writes.len(), 1);
        let canonical = writes[0]
            .canonical_args
            .as_deref()
            .expect("a create must carry its canonical args");
        assert!(canonical.contains("Buy milk"), "got {canonical:?}");
    }

    /// Key order is a serialisation artefact, not a difference in intent. If it
    /// leaked into the identity, the same call re-emitted with reordered keys
    /// would slip past the guard.
    #[tokio::test]
    async fn canonical_args_ignore_key_order() {
        let a = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"content": "x", "node_type": "task"}),
            serde_json::json!({"id": "nodespace://n1"}),
        )]);
        let b = completed_writes_from(&[exec(
            "create_node",
            serde_json::json!({"node_type": "task", "content": "x"}),
            serde_json::json!({"id": "nodespace://n1"}),
        )]);
        assert_eq!(a[0].canonical_args, b[0].canonical_args);
    }

    /// Oversized args are dropped, not truncated. A truncated string could
    /// compare equal to a different call sharing a long prefix, which would turn
    /// a size limit into a wrongly-blocked write.
    #[tokio::test]
    async fn oversized_args_drop_the_identity_rather_than_truncating() {
        let huge = "#".repeat(CANONICAL_ARGS_MAX_CHARS + 100);
        let writes = completed_writes_from(&[exec(
            "create_nodes_from_markdown",
            serde_json::json!({"markdown": huge}),
            serde_json::json!({"created": 3}),
        )]);
        assert_eq!(writes.len(), 1);
        assert_eq!(
            writes[0].canonical_args, None,
            "oversized args must be dropped so they can never produce a false match"
        );
        // The evidence label is unaffected: the write is still reported.
        assert!(writes[0].summary.is_some());
    }

    /// The guard reads its state back out of persisted messages. Entries with no
    /// canonical args cannot match anything and are dropped rather than carried.
    #[tokio::test]
    async fn prior_writes_are_rebuilt_from_persisted_messages() {
        let msgs = vec![AiChatMessage {
            role: "assistant".to_string(),
            content: "Added it.".to_string(),
            timestamp: None,
            reasoning: None,
            completed_writes: vec![
                AiChatCompletedWrite {
                    tool: "create_node".to_string(),
                    node_id: Some("nodespace://n1".to_string()),
                    summary: Some("Buy milk".to_string()),
                    canonical_args: Some(r#"{"content":"Buy milk"}"#.to_string()),
                },
                AiChatCompletedWrite {
                    tool: "create_node".to_string(),
                    node_id: Some("nodespace://n2".to_string()),
                    summary: Some("no identity".to_string()),
                    canonical_args: None,
                },
            ],
        }];

        let prior = prior_writes_from_history(&msgs);
        assert_eq!(prior.len(), 1, "the identity-less write must be skipped");
        assert_eq!(prior[0].tool, "create_node");
        assert_eq!(prior[0].node_id.as_deref(), Some("nodespace://n1"));
    }

    /// Updates are idempotent: re-setting the same status or content is a no-op,
    /// not a duplicate. Carrying them into the guard would block a user
    /// legitimately re-asserting a value.
    #[tokio::test]
    async fn idempotent_updates_are_not_carried_into_the_guard() {
        let msgs = vec![AiChatMessage {
            role: "assistant".to_string(),
            content: "Done.".to_string(),
            timestamp: None,
            reasoning: None,
            completed_writes: vec![
                AiChatCompletedWrite {
                    tool: "update_task_status".to_string(),
                    node_id: Some("nodespace://t1".to_string()),
                    summary: Some("t1".to_string()),
                    canonical_args: Some(r#"{"status":"done"}"#.to_string()),
                },
                AiChatCompletedWrite {
                    tool: "update_node".to_string(),
                    node_id: Some("nodespace://t2".to_string()),
                    summary: Some("t2".to_string()),
                    canonical_args: Some(r#"{"content":"x"}"#.to_string()),
                },
                AiChatCompletedWrite {
                    tool: "update_schema".to_string(),
                    node_id: None,
                    summary: Some("s1".to_string()),
                    canonical_args: Some(r#"{"schema_id":"s1"}"#.to_string()),
                },
            ],
        }];

        assert!(
            prior_writes_from_history(&msgs).is_empty(),
            "no update tool may be guarded across turns"
        );
    }

    /// The guarded set must stay a subset of the tools recognised as writes:
    /// a guarded tool absent from `is_write_tool` would never have its args
    /// recorded, so the guard could never fire for it.
    ///
    /// Iterates the registry itself rather than a hardcoded list. A literal
    /// list would pass vacuously on exactly the change this is meant to catch —
    /// a newly added tool simply would not appear in it.
    #[tokio::test]
    async fn every_guarded_tool_is_also_recorded_as_a_write() {
        use nodespace_agent::local_agent::tools::Tool;

        for tool in Tool::ALL {
            let name = tool.name();
            if is_cross_turn_guarded_tool(name) {
                assert!(
                    is_write_tool(name),
                    "{name} is guarded across turns but not recorded as a write, \
                     so its canonical args are never persisted and the guard cannot fire"
                );
            }
        }
    }

    /// Every guarded tool must also produce an evidence label, otherwise a
    /// refusal can only say "duplicate" without naming what already exists.
    #[tokio::test]
    async fn every_guarded_tool_can_describe_what_it_wrote() {
        use nodespace_agent::local_agent::tools::Tool;

        for tool in Tool::ALL.iter().filter(|t| t.duplicate_is_destructive()) {
            let name = tool.name();
            assert!(
                write_summary_arg(name).is_some() || name == "create_relationship",
                "{name} is guarded but has no way to describe its write, so a \
                 refusal could not name the existing node"
            );
        }
    }
}
