//! Daemon service assembly (ADR-053: one daemon, multiple local databases).
//!
//! Splits startup into the process-global [`SharedServices`] (built once) and
//! the per-database [`DatabaseServices`] (one per open database). Lives in the
//! library so the [`crate::services::database_manager::DatabaseManager`] can
//! build and cache per-database service sets, and the `nodespaced` binary just
//! calls these entry points.

use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_agent::acp::context_assembly::GraphContextAssembler;
use nodespace_agent::prompt_assembler::PromptAssembler;
use nodespace_agent::pty::PtySessionManager;
use nodespace_agent::skill_pipeline::{seed_skill_nodes, seed_tool_nodes};
use nodespace_core::markdown::prepare_nodes_from_template;
use nodespace_core::services::{
    EmbeddingProcessor, EmbeddingScheduler, NodeAccessor, NodeEmbeddingService,
};
use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_nlp_engine::EmbeddingService;
use tokio::sync::{watch, RwLock};

use super::{
    AgentSessionHandler, EmbeddingReady, EmbeddingsServiceImpl, ImportServiceImpl,
    LocalAgentServiceImpl, NodeServiceImpl, SettingsServiceImpl,
};

/// The process-global build context every per-database service set needs
/// (ADR-053: one daemon, multiple local databases): the shared PTY manager and
/// the single embedding model. Cloneable and cheap to hold, so the
/// [`crate::services::database_manager::DatabaseManager`] keeps a copy and
/// builds databases on demand via [`build_database_services`].
#[derive(Clone)]
pub struct SharedContext {
    /// PTY sessions are process-global — one manager backs all databases.
    pub pty_manager: Arc<PtySessionManager>,
    /// The embedding model, loaded once for the whole process and published over
    /// a watch channel so each database's embedding wiring can await it. Holds
    /// `None` until the background load completes; a closed channel means the
    /// load failed or no model file exists.
    pub model: watch::Receiver<Option<Arc<EmbeddingService>>>,
    /// Whether an NLP model file was found at startup. Gates both the
    /// per-database embedding wiring and the `EmbeddingsService` registration.
    pub has_model: bool,
    /// Process-global embedding scheduler (ADR-053: per-database compute
    /// scoping). Grants the active database's embedding batches priority over
    /// other open databases so foreground work is not blocked by another
    /// database's backlog. Shared by every database's `EmbeddingProcessor`.
    pub scheduler: Arc<EmbeddingScheduler>,
}

/// Process-global services shared across every database the daemon serves
/// (ADR-053: one daemon, multiple local databases). Built once by
/// [`build_shared_services`]. `settings` is registered directly on the router;
/// `context` is what each per-database service set is built from and what the
/// [`crate::services::database_manager::DatabaseManager`] caches.
pub struct SharedServices {
    /// Daemon-wide settings (`daemon.toml`); registered once on the router.
    pub settings: SettingsServiceImpl,
    /// The build context handed to every per-database service set.
    pub context: SharedContext,
}

/// The service set backing a single database. One of these is assembled per
/// open database by [`build_database_services`]; the shared model and PTY
/// manager come from [`SharedServices`].
pub struct DatabaseServices {
    pub node_service_grpc: NodeServiceImpl,
    pub agent_session: AgentSessionHandler,
    pub import: ImportServiceImpl,
    pub local_agent: LocalAgentServiceImpl,
    /// Always registered when a model exists — returns `UNAVAILABLE` while the
    /// model loads, then serves normally. `None` only when no NLP model file
    /// exists at all.
    pub embeddings_service_grpc: Option<EmbeddingsServiceImpl>,
    /// Held so we can drain GPU resources after the server shuts down.
    /// Populated by the background embedding-wiring task.
    pub embedding_state: Arc<RwLock<Option<EmbeddingReady>>>,
}

/// Build the process-global services shared across every database (ADR-053):
/// the PTY manager, daemon settings, and the single embedding model. The model
/// is loaded once in the background and published over a watch channel so each
/// database's embedding wiring can await it. Returns the shared set plus the
/// model-load task handle (`None` when no model file exists).
pub async fn build_shared_services() -> Result<(SharedServices, Option<tokio::task::JoinHandle<()>>)>
{
    let pty_manager = Arc::new(PtySessionManager::new());
    let settings = SettingsServiceImpl::with_default_path()
        .map_err(|e| anyhow::anyhow!("Failed to initialize SettingsService: {}", e))?;

    // One embedding model backs every database. Determine the path now (cheap);
    // if absent, no task spawns and the channel stays closed so per-database
    // wiring exits quietly with semantic search disabled.
    let model_path = resolve_model_path();
    let has_model = model_path.is_some();
    let (model_tx, model_rx) = watch::channel::<Option<Arc<EmbeddingService>>>(None);
    let model_task = model_path.map(|path| {
        tokio::spawn(async move {
            load_shared_embedding_model_bg(path, model_tx).await;
        })
    });

    // One scheduler backs every database's embedding processor so the active
    // database's batches take priority on the single shared model (ADR-053).
    let scheduler = Arc::new(EmbeddingScheduler::new());

    Ok((
        SharedServices {
            settings,
            context: SharedContext {
                pty_manager,
                model: model_rx,
                has_model,
                scheduler,
            },
        },
        model_task,
    ))
}

/// Open one database and assemble its gRPC service implementations, sharing the
/// process-global services from `shared` (ADR-053).
///
/// Fast (~100ms): initialize `NodeService`, seed schemas, build all gRPC
/// handlers. The embedding model is NOT loaded here — a background task (the
/// returned handle) wires this database's `NodeEmbeddingService` +
/// `EmbeddingProcessor` and populates `embedding_state` once the shared model is
/// ready.
pub async fn build_database_services(
    db_path: &std::path::Path,
    shared: &SharedContext,
    database_id: &str,
) -> Result<(DatabaseServices, Option<tokio::task::JoinHandle<()>>)> {
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!("Failed to create database parent dir: {}", parent.display())
        })?;
    }

    let mut store = Arc::new(
        SqliteStore::new(db_path.to_path_buf())
            .await
            .context("Failed to initialize SqliteStore")?,
    );

    let mut node_service = CoreNodeService::new(&mut store)
        .await
        .context("Failed to initialize NodeService")?;

    seed_agent_nodes(&mut node_service).await;

    let embedding_state: Arc<RwLock<Option<EmbeddingReady>>> = Arc::new(RwLock::new(None));
    // Separate handle for consumers that only need Arc<NodeEmbeddingService> (assembler, etc.)
    let embedding_svc_state: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>> =
        Arc::new(RwLock::new(None));

    let node_service = Arc::new(node_service);

    let node_service_grpc = NodeServiceImpl::new(
        node_service.clone(),
        embedding_state.clone(),
        shared.scheduler.clone(),
    )
    .with_database_id(database_id.to_string());

    // EmbeddingsService is only registered when a model file exists at startup
    // (the shared model). If the model appears later, the endpoint is absent
    // until daemon restart — intentional, not a regression from prior behavior.
    let embeddings_service_grpc = shared
        .has_model
        .then(|| EmbeddingsServiceImpl::new(node_service.clone(), embedding_state.clone()));

    let assembler = Arc::new(GraphContextAssembler::new(
        node_service.clone(),
        embedding_svc_state.clone(),
    ));
    let capture_config_path = {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        home.join(".nodespace").join("daemon.toml")
    };
    let agent_session = AgentSessionHandler::new(
        shared.pty_manager.clone(),
        assembler,
        node_service.clone(),
        capture_config_path.clone(),
    );

    let import = ImportServiceImpl::new(node_service.clone());
    let local_agent = LocalAgentServiceImpl::new(
        node_service.clone(),
        embedding_svc_state.clone(),
        capture_config_path,
    );
    local_agent.start_event_watcher();

    // Wire this database's embedding processor from the shared model once it
    // loads. Spawned only when a model file exists; otherwise embeddings stay
    // disabled for this database.
    let embedding_task = shared.has_model.then(|| {
        let model = shared.model.clone();
        let store = store.clone();
        let ns = node_service.clone();
        let state = embedding_state.clone();
        let svc_state = embedding_svc_state.clone();
        let scheduler = shared.scheduler.clone();
        let db_id = database_id.to_string();
        tokio::spawn(async move {
            wire_database_embeddings_bg(model, store, ns, state, svc_state, scheduler, db_id).await;
        })
    });

    Ok((
        DatabaseServices {
            node_service_grpc,
            agent_session,
            import,
            local_agent,
            embeddings_service_grpc,
            embedding_state,
        },
        embedding_task,
    ))
}

/// Seed prompt, skill, and tool nodes on first launch. Idempotent — existing nodes are skipped.
async fn seed_agent_nodes(node_service: &mut CoreNodeService) {
    let prompt_templates = PromptAssembler::seed_prompt_nodes();
    let skill_templates = seed_skill_nodes();
    let tool_templates = seed_tool_nodes();

    let mut all_template_nodes = Vec::new();
    for tmpl in prompt_templates
        .iter()
        .chain(skill_templates.iter())
        .chain(tool_templates.iter())
    {
        match prepare_nodes_from_template(tmpl) {
            Ok(nodes) => all_template_nodes.push(nodes),
            Err(e) => {
                tracing::warn!(error = ?e, title = %tmpl.title, "Failed to expand seed template")
            }
        }
    }

    if let Err(e) = node_service
        .seed_nodes_from_templates(all_template_nodes)
        .await
    {
        tracing::warn!(error = %e, "Failed to seed agent nodes (non-fatal)");
    }
}

/// Resolve the NLP model path without loading it. Returns `None` when absent.
fn resolve_model_path() -> Option<std::path::PathBuf> {
    let p = if let Ok(custom) = std::env::var("NODESPACED_MODEL_PATH") {
        std::path::PathBuf::from(custom)
    } else {
        let home = dirs::home_dir()?;
        home.join(".nodespace")
            .join("models")
            .join("nomic-embed-text-v1.5.Q8_0.gguf")
    };
    if !p.exists() {
        tracing::warn!(path = %p.display(), "NLP model not found — semantic search disabled");
        return None;
    }
    Some(p)
}

/// Background task: load the NLP embedding model once for the whole process and
/// publish it over `model_tx`. Non-fatal — on failure the channel simply never
/// yields a model and embeddings stay disabled everywhere.
async fn load_shared_embedding_model_bg(
    model_path: std::path::PathBuf,
    model_tx: watch::Sender<Option<Arc<EmbeddingService>>>,
) {
    tracing::info!(path = %model_path.display(), "Loading shared embedding model in background");

    let config = nodespace_nlp_engine::EmbeddingConfig {
        model_path: Some(model_path),
        ..Default::default()
    };

    // `EmbeddingService::new` + `initialize` are synchronous CPU/IO-bound operations
    // (~6-8s). Use spawn_blocking so they don't park a tokio worker thread.
    let nlp = match tokio::task::spawn_blocking(move || {
        let mut svc = EmbeddingService::new(config).map_err(|e| {
            tracing::warn!(error = %e, "Failed to create NLP engine — semantic search disabled");
            e
        })?;
        svc.initialize().map_err(|e| {
            tracing::warn!(error = %e, "Failed to load NLP model — semantic search disabled");
            e
        })?;
        Ok::<_, nodespace_nlp_engine::EmbeddingError>(svc)
    })
    .await
    {
        Ok(Ok(svc)) => Arc::new(svc),
        Ok(Err(_)) | Err(_) => return,
    };

    // A send error only means every database's wiring task has already gone away
    // (daemon shutting down) — nothing left to wire.
    let _ = model_tx.send(Some(nlp));
    tracing::info!("Shared embedding model loaded — semantic search now available");
}

/// Background task: once the shared embedding model is ready, wire this
/// database's `NodeEmbeddingService` + `EmbeddingProcessor` and publish them to
/// the per-database state the gRPC handlers read. Awaits the model over the
/// shared watch channel; a closed channel (no model / load failed) exits quietly
/// with embeddings disabled for this database. Non-fatal throughout.
async fn wire_database_embeddings_bg(
    mut model: watch::Receiver<Option<Arc<EmbeddingService>>>,
    store: Arc<SqliteStore>,
    node_service: Arc<CoreNodeService>,
    state: Arc<RwLock<Option<EmbeddingReady>>>,
    svc_state: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>>,
    scheduler: Arc<EmbeddingScheduler>,
    db_id: String,
) {
    // Wait for the shared model to be published (or the sender to drop).
    let nlp = loop {
        if let Some(nlp) = model.borrow_and_update().clone() {
            break nlp;
        }
        if model.changed().await.is_err() {
            return; // sender dropped — no model will ever arrive
        }
    };

    let node_accessor: Arc<dyn NodeAccessor> = Arc::new((*node_service).clone());
    let behaviors = node_service.behaviors().clone();
    let embedding_service = Arc::new(NodeEmbeddingService::new(
        nlp,
        store,
        node_accessor,
        behaviors,
    ));

    let processor = match EmbeddingProcessor::new(embedding_service.clone(), scheduler, db_id) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to init EmbeddingProcessor — semantic search disabled");
            return;
        }
    };

    // Wire up automatic wake-on-change now that the processor exists.
    node_service.set_embedding_waker(processor.waker());
    // Process any nodes that became stale during the load window.
    processor.wake();

    // Publish to both state handles atomically enough for practical purposes.
    *svc_state.write().await = Some(embedding_service.clone());
    *state.write().await = Some(EmbeddingReady {
        embedding_service,
        processor,
    });
    tracing::info!("Embedding processor wired for database — semantic search now available");
}
