//! Centralized application services container.
//!
//! `AppServices` wraps all runtime services (database, node service) behind
//! `Arc<RwLock<>>` for shared ownership across Tauri commands.
//!
//! Registered as a single Tauri managed state via `app.manage(AppServices::new())`.
//! All commands access services through `State<'_, AppServices>`.
//!
//! The GPU lifecycle for embeddings is managed by the in-process gRPC server
//! (`GrpcClient` / `EmbeddingsServiceImpl`) and released when the tokio runtime
//! shuts down (Issue #1135). The `NodeEmbeddingService` Arc is still held here
//! so the local agent's `GraphToolExecutor` can do in-process semantic skill
//! injection without an extra gRPC round-trip.

use crate::commands::nodes::CommandError;
use crate::config::AppConfig;
use nodespace_core::services::NodeEmbeddingService;
use nodespace_core::{NodeService, SurrealStore};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Active services that are initialized after database connection.
struct ActiveServices {
    store: Arc<SurrealStore>,
    node_service: Arc<NodeService>,
    /// Held for in-process use by the local agent (GraphToolExecutor). The GPU
    /// lifecycle is managed by GrpcClient / EmbeddingsServiceImpl (Issue #1135).
    embedding_service: Option<Arc<NodeEmbeddingService>>,
    config: AppConfig,
}

/// Centralized services container.
///
/// Registered as Tauri managed state. Commands access services via accessor methods
/// that return `Result<Arc<T>, CommandError>` — returning a clear error if services
/// aren't initialized yet.
#[derive(Clone)]
pub struct AppServices {
    inner: Arc<RwLock<Option<ActiveServices>>>,
    session_token: Arc<RwLock<Option<CancellationToken>>>,
}

impl Default for AppServices {
    fn default() -> Self {
        Self::new()
    }
}

impl AppServices {
    /// Create an empty container. Services are populated later via `initialize()`.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            session_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the NodeService, or error if not yet initialized.
    pub async fn node_service(&self) -> Result<Arc<NodeService>, CommandError> {
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .map(|s| s.node_service.clone())
            .ok_or_else(|| CommandError {
                message: "Database not initialized. Please wait for startup to complete."
                    .to_string(),
                code: "NOT_INITIALIZED".to_string(),
                details: None,
            })
    }

    /// Get the SurrealStore, or error if not yet initialized.
    pub async fn store(&self) -> Result<Arc<SurrealStore>, CommandError> {
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .map(|s| s.store.clone())
            .ok_or_else(|| CommandError {
                message: "Database not initialized. Please wait for startup to complete."
                    .to_string(),
                code: "NOT_INITIALIZED".to_string(),
                details: None,
            })
    }

    /// Get the NodeEmbeddingService for in-process use (e.g. GraphToolExecutor).
    ///
    /// Returns `None` when embeddings are unavailable (model not loaded).
    pub async fn embedding_service(&self) -> Option<Arc<NodeEmbeddingService>> {
        let guard = self.inner.read().await;
        guard.as_ref()?.embedding_service.clone()
    }

    /// Get the AppConfig, or error if not yet initialized.
    pub async fn config(&self) -> Result<AppConfig, CommandError> {
        let guard = self.inner.read().await;
        guard
            .as_ref()
            .map(|s| s.config.clone())
            .ok_or_else(|| CommandError {
                message: "Database not initialized. Please wait for startup to complete."
                    .to_string(),
                code: "NOT_INITIALIZED".to_string(),
                details: None,
            })
    }

    /// Check whether services have been initialized.
    pub async fn is_initialized(&self) -> bool {
        self.inner.read().await.is_some()
    }

    /// Populate the container with initialized services.
    ///
    /// Called from `db.rs::init_services()` after database and services are ready.
    pub async fn initialize(
        &self,
        store: Arc<SurrealStore>,
        node_service: Arc<NodeService>,
        embedding_service: Option<Arc<NodeEmbeddingService>>,
        config: AppConfig,
        session_cancel_token: CancellationToken,
    ) {
        {
            let mut guard = self.inner.write().await;
            *guard = Some(ActiveServices {
                store,
                node_service,
                embedding_service,
                config,
            });
        }
        {
            let mut token_guard = self.session_token.write().await;
            *token_guard = Some(session_cancel_token);
        }
    }

    /// Get the current session cancellation token (for background task coordination).
    pub async fn session_token(&self) -> Option<CancellationToken> {
        self.session_token.read().await.clone()
    }
}
