//! `nodespaced` — background daemon that owns the SQLite database lock and serves
//! NodeSpace operations over gRPC on a Unix Domain Socket.
//!
//! Lifecycle:
//!   1. Initialize tracing.
//!   2. Install signal handlers (fail-fast — a daemon that can't observe
//!      shutdown signals is broken).
//!   3. Open `SqliteStore` (embedded SQLite/libsql) at the configured path.
//!   4. Build `NodeService` from `nodespace-core`.
//!   5. Bring up the system tray on the main thread and spawn the tonic
//!      `NodeService` handler on a worker tokio runtime.
//!   6. Tear down cleanly on `SIGTERM`, `SIGINT`, or "Quit" from the tray.

use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_agent::acp::context_assembly::GraphContextAssembler;
use nodespace_agent::prompt_assembler::PromptAssembler;
use nodespace_agent::pty::PtySessionManager;
use nodespace_agent::skill_pipeline::seed_skill_nodes;
use nodespace_core::markdown::prepare_nodes_from_template;
use nodespace_core::services::{EmbeddingProcessor, NodeAccessor, NodeEmbeddingService};
use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_daemon::services::embeddings_service::EmbeddingReady;
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{
    resolve_db_path, tray, AgentSessionHandler, AgentSessionServiceServer, EmbeddingsServiceImpl,
    EmbeddingsServiceServer, ImportServiceImpl, ImportServiceServer, LocalAgentServiceImpl,
    LocalAgentServiceServer, NodeServiceImpl, NodeServiceServer, SettingsServiceImpl,
    SettingsServiceServer,
};
use nodespace_nlp_engine::EmbeddingService;
use tokio::sync::RwLock;
use tonic::transport::Server;

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".nodespace")
        .join("daemon.sock")
}

#[cfg(windows)]
compile_error!("Windows Named Pipe transport is not yet implemented. See issue #1176.");

/// `tao`'s event loop must own the main thread on macOS (NSApplication is
/// main-thread-only). So `main` builds the tokio runtime explicitly, hands
/// it to a worker thread that hosts the gRPC server, and lets `tray::run`
/// take over the main thread.
///
/// Headless mode is supported for systems that don't have a display (Linux
/// CI, headless servers): if `NODESPACED_HEADLESS=1` is set, the tray loop
/// is skipped and we fall back to a pure async `main` that exits on signals.
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    if headless() {
        return runtime.block_on(async { serve_headless().await });
    }

    // The tray's seed closure runs synchronously when `tray::run` is called,
    // launching the gRPC server on the tokio runtime so the daemon is
    // serving as soon as the tray appears. The returned `JoinHandle` flows
    // back out of `tray::run` once the user picks Quit.
    let runtime_handle = runtime.handle().clone();
    let grpc_handle = tray::run(move |controller| {
        runtime_handle.spawn(async move { serve_grpc(controller).await })
    })?;

    // `tray::run` returned, so the user picked Quit. Wait for the gRPC
    // server to finish draining before we drop the runtime — otherwise
    // in-flight RPCs would be killed mid-response.
    runtime
        .block_on(grpc_handle)
        .context("gRPC task panicked")?
        .context("gRPC server returned an error")?;

    tracing::info!("nodespaced shutdown complete");
    Ok(())
}

fn headless() -> bool {
    matches!(std::env::var("NODESPACED_HEADLESS").as_deref(), Ok("1"))
}

/// Headless server loop. Used by Linux CI and any environment without a
/// display server. Shutdown is signal-driven (SIGTERM / SIGINT), there is
/// no tray.
#[cfg(unix)]
async fn serve_headless() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _bg_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (bundle, _bg_task) = build_services(&db_path).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = UnixListener::bind(&sock)
        .with_context(|| format!("Failed to bind Unix socket: {}", sock.display()))?;
    std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set socket permissions: {}", sock.display()))?;

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let builder = Server::builder()
        .add_service(NodeServiceServer::new(bundle.node_service_grpc))
        .add_service(AgentSessionServiceServer::new(bundle.agent_session))
        .add_service(ImportServiceServer::new(bundle.import))
        .add_service(SettingsServiceServer::new(bundle.settings))
        .add_service(LocalAgentServiceServer::new(bundle.local_agent));
    let serve = if let Some(emb) = bundle.embeddings_service_grpc {
        builder
            .add_service(EmbeddingsServiceServer::new(emb))
            .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
    } else {
        builder.serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
    };

    serve.await.context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Tray-driven server loop. Shutdown is owned by [`tray::TrayController`];
/// signal handlers still apply so packaged installs can `kill -TERM` the
/// daemon without going through the menu.
#[cfg(unix)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _bg_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (bundle, _bg_task) = build_services(&db_path).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = UnixListener::bind(&sock)
        .with_context(|| format!("Failed to bind Unix socket: {}", sock.display()))?;
    std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set socket permissions: {}", sock.display()))?;

    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let builder = Server::builder()
        .layer(TrayMetricsLayer::new(controller))
        .add_service(NodeServiceServer::new(bundle.node_service_grpc))
        .add_service(AgentSessionServiceServer::new(bundle.agent_session))
        .add_service(ImportServiceServer::new(bundle.import))
        .add_service(SettingsServiceServer::new(bundle.settings))
        .add_service(LocalAgentServiceServer::new(bundle.local_agent));
    let serve = if let Some(emb) = bundle.embeddings_service_grpc {
        builder
            .add_service(EmbeddingsServiceServer::new(emb))
            .serve_with_incoming_shutdown(UnixListenerStream::new(listener), combined_shutdown)
    } else {
        builder.serve_with_incoming_shutdown(UnixListenerStream::new(listener), combined_shutdown)
    };

    serve.await.context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// All initialized service handles for a daemon startup.
struct ServiceBundle {
    node_service_grpc: NodeServiceImpl,
    agent_session: AgentSessionHandler,
    import: ImportServiceImpl,
    settings: SettingsServiceImpl,
    local_agent: LocalAgentServiceImpl,
    /// Always registered — returns `UNAVAILABLE` while the model loads, then
    /// serves normally. `None` only when no NLP model file exists at all.
    embeddings_service_grpc: Option<EmbeddingsServiceImpl>,
    /// Held so we can drain GPU resources after the server shuts down.
    /// Populated by the background embedding-load task.
    embedding_state: Arc<RwLock<Option<EmbeddingReady>>>,
}

/// Open the database and assemble the gRPC service implementations.
///
/// Phase 1 (this function): initialize NodeService, seed schemas, build all
/// gRPC handlers — fast, ~100ms. The embedding model is NOT loaded here.
///
/// Phase 2 (background task returned alongside the bundle): load the NLP
/// model and populate `embedding_state` once ready.
async fn build_services(
    db_path: &std::path::Path,
) -> Result<(ServiceBundle, Option<tokio::task::JoinHandle<()>>)> {
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

    // Determine the model path now (cheap) — if absent, skip background task.
    let model_path = resolve_model_path();

    let embedding_state: Arc<RwLock<Option<EmbeddingReady>>> = Arc::new(RwLock::new(None));
    // Separate handle for consumers that only need Arc<NodeEmbeddingService> (assembler, etc.)
    let embedding_svc_state: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>> =
        Arc::new(RwLock::new(None));

    let node_service = Arc::new(node_service);

    let node_service_grpc = NodeServiceImpl::new(node_service.clone(), embedding_state.clone());

    // EmbeddingsService is only registered when a model file exists at startup.
    // If the model appears later (e.g. user downloads it), the endpoint is absent
    // until daemon restart. This is intentional — not a regression from prior behavior.
    let embeddings_service_grpc = model_path
        .as_ref()
        .map(|_| EmbeddingsServiceImpl::new(node_service.clone(), embedding_state.clone()));

    let manager = Arc::new(PtySessionManager::new());
    let assembler = Arc::new(GraphContextAssembler::new(
        node_service.clone(),
        embedding_svc_state.clone(),
    ));
    let settings = SettingsServiceImpl::with_default_path()
        .map_err(|e| anyhow::anyhow!("Failed to initialize SettingsService: {}", e))?;
    let capture_config_path = {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::PathBuf::from(home)
            .join(".nodespace")
            .join("daemon.toml")
    };
    let agent_session = AgentSessionHandler::new(
        manager,
        assembler,
        node_service.clone(),
        capture_config_path,
    );

    let import = ImportServiceImpl::new(node_service.clone());
    let local_agent = LocalAgentServiceImpl::new(node_service.clone(), embedding_svc_state.clone());
    local_agent.start_event_watcher();

    // Spawn background model-load task if a model file was found.
    let bg_task = model_path.map(|path| {
        let state = embedding_state.clone();
        let svc_state = embedding_svc_state.clone();
        let store_clone = store.clone();
        let ns_clone = node_service.clone();
        tokio::spawn(async move {
            load_embedding_model_bg(path, store_clone, ns_clone, state, svc_state).await;
        })
    });

    Ok((
        ServiceBundle {
            node_service_grpc,
            agent_session,
            import,
            settings,
            local_agent,
            embeddings_service_grpc,
            embedding_state,
        },
        bg_task,
    ))
}

/// Seed prompt and skill nodes on first launch. Idempotent — existing nodes are skipped.
async fn seed_agent_nodes(node_service: &mut CoreNodeService) {
    let prompt_templates = PromptAssembler::seed_prompt_nodes();
    let skill_templates = seed_skill_nodes();

    let mut all_template_nodes = Vec::new();
    for tmpl in prompt_templates.iter().chain(skill_templates.iter()) {
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
        let home = std::env::var("HOME").ok()?;
        std::path::PathBuf::from(home)
            .join(".nodespace")
            .join("models")
            .join("nomic-embed-text-v1.5.Q8_0.gguf")
    };
    if !p.exists() {
        tracing::warn!(path = %p.display(), "NLP model not found — semantic search disabled");
        return None;
    }
    Some(p)
}

/// Background task: load the NLP model and populate `state` when done.
///
/// Non-fatal: logs errors and leaves `state` as `None` if anything fails.
async fn load_embedding_model_bg(
    model_path: std::path::PathBuf,
    store: Arc<SqliteStore>,
    node_service: Arc<CoreNodeService>,
    state: Arc<RwLock<Option<EmbeddingReady>>>,
    svc_state: Arc<RwLock<Option<Arc<NodeEmbeddingService>>>>,
) {
    tracing::info!(path = %model_path.display(), "Loading embedding model in background");

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

    let node_accessor: Arc<dyn NodeAccessor> = Arc::new((*node_service).clone());
    let behaviors = node_service.behaviors().clone();
    let embedding_service = Arc::new(NodeEmbeddingService::new(
        nlp,
        store,
        node_accessor,
        behaviors,
    ));

    let processor = match EmbeddingProcessor::new(embedding_service.clone()) {
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
    tracing::info!("Embedding model loaded — semantic search now available");
}

/// GPU drain protocol: drop processor first (shuts down background task),
/// then release the GPU context from the NLP engine.
async fn drain_gpu(state: Arc<RwLock<Option<EmbeddingReady>>>) {
    let ready = state.write().await.take();
    if let Some(ready) = ready {
        drop(ready.processor);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tracing::info!("Releasing GPU context...");
        ready.embedding_service.nlp_engine().release_gpu_context();
        tracing::info!("GPU context released");
    }
}

/// Install the shutdown signal future at boot time so a failure to register
/// the handlers becomes a startup error rather than a silent runtime fault.
///
/// On Unix we listen for SIGTERM and SIGINT. On other platforms we fall back
/// to `tokio::signal::ctrl_c`, which fails synchronously here only if the
/// platform doesn't support it.
#[cfg(unix)]
fn install_shutdown_handler() -> Result<impl std::future::Future<Output = ()>> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?;
    let mut sigint = signal(SignalKind::interrupt()).context("install SIGINT handler")?;

    Ok(async move {
        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received — initiating graceful shutdown"),
            _ = sigint.recv()  => tracing::info!("SIGINT received — initiating graceful shutdown"),
        }
    })
}

#[cfg(not(unix))]
fn install_shutdown_handler() -> Result<impl std::future::Future<Output = ()>> {
    Ok(async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("Ctrl-C received — initiating graceful shutdown"),
            Err(e) => tracing::error!(error = %e, "ctrl_c handler failed; shutting down"),
        }
    })
}
