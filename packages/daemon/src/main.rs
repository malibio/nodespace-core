//! `nodespaced` — background daemon that owns the RocksDB lock and serves
//! NodeSpace operations over gRPC on `localhost:50051`.
//!
//! Lifecycle:
//!   1. Initialize tracing.
//!   2. Install signal handlers (fail-fast — a daemon that can't observe
//!      shutdown signals is broken).
//!   3. Open `SurrealStore` (embedded RocksDB) at the configured path.
//!   4. Build `NodeService` from `nodespace-core`.
//!   5. Bring up the system tray on the main thread and spawn the tonic
//!      `NodeService` handler on a worker tokio runtime.
//!   6. Tear down cleanly on `SIGTERM`, `SIGINT`, or "Quit" from the tray.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use nodespace_agent::acp::context_assembly::GraphContextAssembler;
use nodespace_agent::pty::PtySessionManager;
use nodespace_core::services::{EmbeddingProcessor, NodeAccessor, NodeEmbeddingService};
use nodespace_core::{NodeService as CoreNodeService, SurrealStore};
use nodespace_daemon::tray::layer::TrayMetricsLayer;
use nodespace_daemon::{
    resolve_db_path, tray, AgentSessionHandler, AgentSessionServiceServer, EmbeddingsServiceImpl,
    EmbeddingsServiceServer, ImportServiceImpl, ImportServiceServer, NodeServiceImpl,
    NodeServiceServer,
};
use nodespace_nlp_engine::EmbeddingService;
use tonic::transport::Server;

/// Default address the daemon binds to. ADR-031 standardizes on
/// `localhost:50051` for the loopback-only gRPC endpoint.
const DEFAULT_ADDR: &str = "[::1]:50051";

/// Resolve the daemon's bind address. Honors `NODESPACED_ADDR`.
fn bind_addr() -> Result<SocketAddr> {
    let raw = std::env::var("NODESPACED_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    raw.parse()
        .with_context(|| format!("Invalid NODESPACED_ADDR: {raw}"))
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
async fn serve_headless() -> Result<()> {
    let addr = bind_addr()?;
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), %addr, "Starting nodespaced (headless)");

    let shutdown = install_shutdown_handler().context("Failed to install signal handlers")?;
    let bundle = build_services(&db_path).await?;

    tracing::info!(%addr, "gRPC server listening");
    let builder = Server::builder()
        .add_service(NodeServiceServer::new(bundle.node_service_grpc))
        .add_service(AgentSessionServiceServer::new(bundle.agent_session))
        .add_service(ImportServiceServer::new(bundle.import));
    let serve = if let Some(emb) = bundle.embeddings_service_grpc {
        builder
            .add_service(EmbeddingsServiceServer::new(emb))
            .serve_with_shutdown(addr, shutdown)
    } else {
        builder.serve_with_shutdown(addr, shutdown)
    };

    serve.await.context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// Tray-driven server loop. Shutdown is owned by [`tray::TrayController`];
/// signal handlers still apply so packaged installs can `kill -TERM` the
/// daemon without going through the menu.
async fn serve_grpc(controller: tray::TrayController) -> Result<()> {
    let addr = bind_addr()?;
    let db_path = resolve_db_path()?;

    tracing::info!(db_path = %db_path.display(), %addr, "Starting nodespaced (tray)");

    let signal_shutdown =
        install_shutdown_handler().context("Failed to install signal handlers")?;
    let bundle = build_services(&db_path).await?;

    // `TrayController` is `Clone`; one copy goes to the metrics layer, the
    // other drives the shutdown future.
    let shutdown_controller = controller.clone();
    let combined_shutdown = async move {
        tokio::select! {
            _ = signal_shutdown => tracing::info!("OS signal triggered shutdown"),
            _ = shutdown_controller.shutdown() => tracing::info!("Tray Quit triggered shutdown"),
        }
    };

    tracing::info!(%addr, "gRPC server listening");
    let builder = Server::builder()
        .layer(TrayMetricsLayer::new(controller))
        .add_service(NodeServiceServer::new(bundle.node_service_grpc))
        .add_service(AgentSessionServiceServer::new(bundle.agent_session))
        .add_service(ImportServiceServer::new(bundle.import));
    let serve = if let Some(emb) = bundle.embeddings_service_grpc {
        builder
            .add_service(EmbeddingsServiceServer::new(emb))
            .serve_with_shutdown(addr, combined_shutdown)
    } else {
        builder.serve_with_shutdown(addr, combined_shutdown)
    };

    serve.await.context("gRPC server terminated with error")?;
    drain_gpu(bundle.embedding_state).await;
    Ok(())
}

/// All initialized service handles for a daemon startup.
struct ServiceBundle {
    node_service_grpc: NodeServiceImpl,
    agent_session: AgentSessionHandler,
    import: ImportServiceImpl,
    /// `None` when the NLP model is absent — the daemon starts without semantic
    /// search rather than refusing to run. The `EmbeddingsService` gRPC endpoint
    /// is simply not registered in that case.
    embeddings_service_grpc: Option<EmbeddingsServiceImpl>,
    /// Held so we can drain GPU resources after the server shuts down.
    embedding_state: Option<(Arc<NodeEmbeddingService>, Arc<EmbeddingProcessor>)>,
}

/// Open the database and assemble the gRPC service implementations.
async fn build_services(db_path: &std::path::Path) -> Result<ServiceBundle> {
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!("Failed to create database parent dir: {}", parent.display())
        })?;
    }

    let mut store = Arc::new(
        SurrealStore::new(db_path.to_path_buf())
            .await
            .context("Failed to initialize SurrealStore")?,
    );

    let mut node_service = CoreNodeService::new(&mut store)
        .await
        .context("Failed to initialize NodeService")?;

    let embedding_state = build_embedding_state(&store, &mut node_service);
    let node_service = Arc::new(node_service);

    let embedding_service_opt = embedding_state.as_ref().map(|(svc, _)| svc.clone());
    let node_service_grpc = NodeServiceImpl::new(node_service.clone(), embedding_service_opt.clone());

    let embeddings_service_grpc = embedding_state.as_ref().map(|(svc, proc)| {
        EmbeddingsServiceImpl::new(node_service.clone(), svc.clone(), proc.clone())
    });

    let manager = Arc::new(PtySessionManager::new());
    let mut assembler = GraphContextAssembler::new(node_service.clone(), embedding_service_opt);
    if let Ok(shim_dir) = std::env::var("NODESPACED_SHIM_DIR") {
        assembler = assembler.with_shim_dir(std::path::PathBuf::from(shim_dir));
    }
    let assembler = Arc::new(assembler);
    let agent_session = AgentSessionHandler::new(manager, assembler);

    let import = ImportServiceImpl::new(node_service);

    Ok(ServiceBundle {
        node_service_grpc,
        agent_session,
        import,
        embeddings_service_grpc,
        embedding_state,
    })
}

/// Try to initialize NLP engine and embedding services. Non-fatal: returns
/// `None` when the model is absent or fails to load.
fn build_embedding_state(
    store: &Arc<SurrealStore>,
    node_service: &mut CoreNodeService,
) -> Option<(Arc<NodeEmbeddingService>, Arc<EmbeddingProcessor>)> {
    let model_path = {
        // Allow override via env var so CI and alternate deployments can redirect.
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
        p
    };

    let config = nodespace_nlp_engine::EmbeddingConfig {
        model_path: Some(model_path),
        ..Default::default()
    };

    let mut nlp = match EmbeddingService::new(config) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create NLP engine — semantic search disabled");
            return None;
        }
    };
    if let Err(e) = nlp.initialize() {
        tracing::warn!(error = %e, "Failed to load NLP model — semantic search disabled");
        return None;
    }
    let nlp = Arc::new(nlp);

    let node_accessor: Arc<dyn NodeAccessor> = Arc::new(node_service.clone());
    let behaviors = node_service.behaviors().clone();
    let embedding_service = Arc::new(NodeEmbeddingService::new(
        nlp,
        store.clone(),
        node_accessor,
        behaviors,
    ));

    let processor = match EmbeddingProcessor::new(embedding_service.clone()) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to init EmbeddingProcessor — semantic search disabled");
            return None;
        }
    };
    node_service.set_embedding_waker(processor.waker());
    processor.wake();

    Some((embedding_service, processor))
}

/// GPU drain protocol: drop processor first (shuts down background task),
/// then release the GPU context from the NLP engine.
async fn drain_gpu(state: Option<(Arc<NodeEmbeddingService>, Arc<EmbeddingProcessor>)>) {
    if let Some((svc, proc)) = state {
        drop(proc);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tracing::info!("Releasing GPU context...");
        svc.nlp_engine().release_gpu_context();
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
