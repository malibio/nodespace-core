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
use nodespace_agent::local_agent::otlp_tracer;
use nodespace_agent::prompt_assembler::PromptAssembler;
use nodespace_agent::pty::PtySessionManager;
use nodespace_agent::skill_pipeline::{seed_skill_nodes, seed_tool_nodes};
use nodespace_core::markdown::prepare_nodes_from_template;
use nodespace_core::services::{EmbeddingProcessor, NodeAccessor, NodeEmbeddingService};
use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_daemon::services::embeddings_service::EmbeddingReady;
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{
    build_base_router, resolve_db_path, tray, AgentSessionHandler, BaseServices,
    EmbeddingsServiceImpl, ImportServiceImpl, LocalAgentServiceImpl, NodeServiceImpl,
    SettingsServiceImpl,
};
use nodespace_nlp_engine::EmbeddingService;
use tokio::sync::{watch, RwLock};
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

/// Binds a Unix domain socket with no window where it is exposed at the
/// ambient umask (ADR-052 §3). `UnixListener::bind` creates the socket file
/// honoring the process umask, so a plain `bind` then `set_permissions` leaves
/// the socket briefly group/other-reachable if the umask is permissive. We
/// narrow the umask to `0o177` (owner rw only) for the duration of the bind
/// so the socket is created at `0o600` from the instant it appears, then
/// restore the prior umask — `umask` is process-global, so the narrowed
/// window must be as short as possible and restored even on error.
///
/// Precondition: callers must not invoke this concurrently with another bind
/// on the same process (`umask` is process-global, not per-thread). Both call
/// sites in this file are safe because `main` runs exactly one of
/// `serve_headless` / `serve_grpc` per process, before any other task binds
/// a socket of its own.
#[cfg(unix)]
fn bind_uds_owner_only(sock: &std::path::Path) -> Result<tokio::net::UnixListener> {
    use std::os::unix::fs::PermissionsExt;

    // SAFETY: umask(2) is a plain libc call; no pointers involved.
    let prev_umask = unsafe { libc::umask(0o177) };
    let result = tokio::net::UnixListener::bind(sock);
    unsafe { libc::umask(prev_umask) };

    let listener =
        result.with_context(|| format!("Failed to bind Unix socket: {}", sock.display()))?;
    // Defense-in-depth: the umask already guarantees 0o600 at creation, but
    // set it explicitly in case the umask was somehow not honored.
    std::fs::set_permissions(sock, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Failed to set socket permissions: {}", sock.display()))?;
    Ok(listener)
}

#[cfg(windows)]
fn pipe_name() -> String {
    if let Ok(p) = std::env::var("NODESPACED_SOCKET") {
        return p;
    }
    r"\\.\pipe\nodespace-daemon".to_string()
}

/// `tao`'s event loop must own the main thread on macOS (NSApplication is
/// main-thread-only). So `main` builds the tokio runtime explicitly, hands
/// it to a worker thread that hosts the gRPC server, and lets `tray::run`
/// take over the main thread.
///
/// Headless mode is supported for systems that don't have a display (Linux
/// CI, headless servers): if `NODESPACED_HEADLESS=1` is set, the tray loop
/// is skipped and we fall back to a pure async `main` that exits on signals.
fn main() -> Result<()> {
    // Early-exit flags — handled before tracing/runtime init so the installer
    // postinstall script can query these without spinning up the full daemon.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--edition") {
        println!("{}", edition());
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Initialise OTLP tracing when NODESPACE_MLFLOW_URL is set (dev only).
    // Keep the provider alive for the duration of main so the background
    // exporter thread is not torn down prematurely.
    let _otlp_provider = otlp_tracer::init_tracer();

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

/// Returns the build edition: "pro" when compiled with `--features pro`, otherwise "community".
fn edition() -> &'static str {
    if cfg!(feature = "pro") {
        "pro"
    } else {
        "community"
    }
}

/// Headless server loop. Used by Linux CI and any environment without a
/// display server. Shutdown is signal-driven (SIGTERM / SIGINT), there is
/// no tray.
#[cfg(unix)]
async fn serve_headless() -> Result<()> {
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = bind_uds_owner_only(&sock)?;

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let base_services = BaseServices {
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(Server::builder(), base_services)
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await
        .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Tray-driven server loop. Shutdown is owned by [`tray::TrayController`];
/// signal handlers still apply so packaged installs can `kill -TERM` the
/// daemon without going through the menu.
#[cfg(unix)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use tokio_stream::wrappers::UnixListenerStream;

    let sock = socket_path();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), sock = %sock.display(), "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared).await?;

    if let Some(parent) = sock.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create socket directory: {}", parent.display()))?;
    }
    let _ = tokio::fs::remove_file(&sock).await;
    let listener = bind_uds_owner_only(&sock)?;

    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(sock = %sock.display(), "gRPC server listening");

    let sock_cleanup = sock.clone();
    let base_services = BaseServices {
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(
        Server::builder().layer(TrayMetricsLayer::new(controller)),
        base_services,
    )
    .serve_with_incoming_shutdown(UnixListenerStream::new(listener), combined_shutdown)
    .await
    .context("gRPC server terminated with error")?;
    let _ = tokio::fs::remove_file(&sock_cleanup).await;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Newtype so we can implement tonic's `Connected` for `NamedPipeServer`.
/// `NamedPipeServer` is a transport; tonic's blanket bound requires `Connected`
/// on stream items so it can extract peer metadata for request extensions.
#[cfg(windows)]
struct NamedPipeConn(tokio::net::windows::named_pipe::NamedPipeServer);

#[cfg(windows)]
impl tonic::transport::server::Connected for NamedPipeConn {
    type ConnectInfo = ();
    fn connect_info(&self) -> Self::ConnectInfo {}
}

#[cfg(windows)]
impl tokio::io::AsyncRead for NamedPipeConn {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

#[cfg(windows)]
impl tokio::io::AsyncWrite for NamedPipeConn {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

/// Headless server loop for Windows — uses a Named Pipe instead of UDS.
#[cfg(windows)]
async fn serve_headless() -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio_util::sync::CancellationToken;

    let name = pipe_name();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), pipe = %name, "Starting nodespaced (headless, Windows)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared).await?;

    tracing::info!(pipe = %name, "gRPC server listening (Named Pipe)");

    // CancellationToken is cloned into the acceptor stream so that
    // `server.connect().await` races against shutdown rather than blocking
    // indefinitely after tonic stops polling the stream.
    let cancel = CancellationToken::new();
    let cancel_stream = cancel.clone();
    let incoming = {
        let name = name.clone();
        async_stream::stream! {
            loop {
                // .first_pipe_instance(false): multiple clients connect serially
                // to the same pipe name — each iteration creates a fresh instance.
                let server = match ServerOptions::new().first_pipe_instance(false).create(&name) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create Named Pipe server instance");
                        yield Err(e);
                        return;
                    }
                };
                tokio::select! {
                    res = server.connect() => {
                        if let Err(e) = res { yield Err(e); return; }
                        yield Ok::<_, std::io::Error>(NamedPipeConn(server));
                    }
                    _ = cancel_stream.cancelled() => return,
                }
            }
        }
    };

    let base_services = BaseServices {
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(Server::builder(), base_services)
        .serve_with_incoming_shutdown(incoming, async move {
            shutdown.await;
            cancel.cancel();
        })
        .await
        .context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Tray-driven server loop for Windows — uses a Named Pipe instead of UDS.
#[cfg(windows)]
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    use tokio_util::sync::CancellationToken;

    let name = pipe_name();
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), pipe = %name, "Starting nodespaced (tray, Windows)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    // _model_task / _embed_task: dropping a JoinHandle does not cancel the task in tokio — it detaches.
    let (shared, _model_task) = build_shared_services().await?;
    let (bundle, _embed_task) = build_database_services(&db_path, &shared).await?;

    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(pipe = %name, "gRPC server listening (Named Pipe)");

    let cancel = CancellationToken::new();
    let cancel_stream = cancel.clone();
    let incoming = {
        let name = name.clone();
        async_stream::stream! {
            loop {
                let server = match ServerOptions::new().first_pipe_instance(false).create(&name) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to create Named Pipe server instance");
                        yield Err(e);
                        return;
                    }
                };
                tokio::select! {
                    res = server.connect() => {
                        if let Err(e) = res { yield Err(e); return; }
                        yield Ok::<_, std::io::Error>(NamedPipeConn(server));
                    }
                    _ = cancel_stream.cancelled() => return,
                }
            }
        }
    };

    let base_services = BaseServices {
        node_service: bundle.node_service_grpc,
        agent_session: bundle.agent_session,
        import: bundle.import,
        settings: shared.settings,
        local_agent: bundle.local_agent,
        embeddings: bundle.embeddings_service_grpc,
    };
    build_base_router(
        Server::builder().layer(TrayMetricsLayer::new(controller)),
        base_services,
    )
    .serve_with_incoming_shutdown(incoming, async move {
        combined_shutdown.await;
        cancel.cancel();
    })
    .await
    .context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Process-global services shared across every database the daemon serves
/// (ADR-053: one daemon, multiple local databases). Built once by
/// [`build_shared_services`]; each per-database service set borrows what it
/// needs from here rather than constructing its own copy.
struct SharedServices {
    /// PTY sessions are process-global — one manager backs all databases.
    pty_manager: Arc<PtySessionManager>,
    /// Daemon-wide settings (`daemon.toml`); registered once on the router.
    settings: SettingsServiceImpl,
    /// The embedding model, loaded once for the whole process and published over
    /// a watch channel so each database's embedding wiring can await it. Holds
    /// `None` until the background load completes; a closed channel means the
    /// load failed or no model file exists.
    model: watch::Receiver<Option<Arc<EmbeddingService>>>,
    /// Whether an NLP model file was found at startup. Gates both the
    /// per-database embedding wiring and the `EmbeddingsService` registration.
    has_model: bool,
}

/// The service set backing a single database. One of these is assembled per
/// open database by [`build_database_services`]; the shared model and PTY
/// manager come from [`SharedServices`].
struct DatabaseServices {
    node_service_grpc: NodeServiceImpl,
    agent_session: AgentSessionHandler,
    import: ImportServiceImpl,
    local_agent: LocalAgentServiceImpl,
    /// Always registered when a model exists — returns `UNAVAILABLE` while the
    /// model loads, then serves normally. `None` only when no NLP model file
    /// exists at all.
    embeddings_service_grpc: Option<EmbeddingsServiceImpl>,
    /// Held so we can drain GPU resources after the server shuts down.
    /// Populated by the background embedding-wiring task.
    embedding_state: Arc<RwLock<Option<EmbeddingReady>>>,
}

/// Build the process-global services shared across every database (ADR-053):
/// the PTY manager, daemon settings, and the single embedding model. The model
/// is loaded once in the background and published over a watch channel so each
/// database's embedding wiring can await it. Returns the shared set plus the
/// model-load task handle (`None` when no model file exists).
async fn build_shared_services() -> Result<(SharedServices, Option<tokio::task::JoinHandle<()>>)> {
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

    Ok((
        SharedServices {
            pty_manager,
            settings,
            model: model_rx,
            has_model,
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
async fn build_database_services(
    db_path: &std::path::Path,
    shared: &SharedServices,
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

    let node_service_grpc = NodeServiceImpl::new(node_service.clone(), embedding_state.clone());

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
        capture_config_path,
    );

    let import = ImportServiceImpl::new(node_service.clone());
    let local_agent = LocalAgentServiceImpl::new(node_service.clone(), embedding_svc_state.clone());
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
        tokio::spawn(async move {
            wire_database_embeddings_bg(model, store, ns, state, svc_state).await;
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
    tracing::info!("Embedding processor wired for database — semantic search now available");
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

#[cfg(all(test, unix))]
mod uds_permission_tests {
    use super::bind_uds_owner_only;

    /// `umask` is process-global, so both assertions run in one test to avoid
    /// racing against a second test thread mutating it concurrently.
    ///
    /// Covers: even under a permissive ambient umask (which would otherwise
    /// yield a group/other-reachable socket), the bound socket ends up
    /// owner-only; and the ambient umask is restored afterward so later code
    /// in the process does not inherit the narrowed `0o177`.
    #[tokio::test]
    async fn bind_uds_owner_only_is_0o600_and_restores_umask() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("race-test.sock");

        // SAFETY: umask(2) is a plain libc call with no pointers. This test
        // is the sole owner of process umask mutation for its duration —
        // no other test in this module runs concurrently with it.
        let ambient_prev = unsafe { libc::umask(0o022) };
        let listener = bind_uds_owner_only(&sock).expect("bind should succeed");
        let observed = unsafe { libc::umask(ambient_prev) };

        assert_eq!(
            observed, 0o022,
            "umask must be restored after bind, not left narrowed"
        );

        let mode = std::fs::metadata(&sock)
            .expect("socket file should exist")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "socket must be owner-only regardless of ambient umask"
        );

        drop(listener);
    }
}
