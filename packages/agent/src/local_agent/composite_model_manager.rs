//! Composite model manager: routes between GGUF and Ollama backends.
//!
//! Implements the [`ModelManager`] trait by delegating to either the GGUF model manager
//! (for local GGUF models) or the Ollama model manager (for models served by a local
//! Ollama daemon). Models are distinguished by the "ollama:" prefix on the model ID.
//!
//! Issue #1058

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent_types::{DownloadEvent, ModelError, ModelInfo, ModelManager};
use crate::local_agent::model_manager::GgufModelManager;
use crate::local_agent::ollama_model_manager::OllamaModelManager;

/// Prefix used to identify Ollama models in the composite manager.
pub const OLLAMA_PREFIX: &str = "ollama:";

// ---------------------------------------------------------------------------
// CompositeModelManager
// ---------------------------------------------------------------------------

/// Concrete [`ModelManager`] that routes between GGUF and Ollama backends.
///
/// Models are identified as Ollama models if their ID starts with "ollama:".
/// All other models are treated as GGUF models.
///
/// Thread-safe: delegates to underlying managers which are thread-safe.
pub struct CompositeModelManager {
    /// GGUF model manager for local model files.
    gguf: Arc<GgufModelManager>,
    /// Ollama model manager for models served by Ollama daemon.
    ollama: Arc<OllamaModelManager>,
}

impl CompositeModelManager {
    /// Create a new composite model manager from GGUF and Ollama managers.
    pub fn new(gguf: Arc<GgufModelManager>, ollama: Arc<OllamaModelManager>) -> Self {
        Self { gguf, ollama }
    }

    /// Check if a model ID represents an Ollama model.
    pub fn is_ollama(model_id: &str) -> bool {
        model_id.starts_with(OLLAMA_PREFIX)
    }

    /// Strip the "ollama:" prefix from a model ID.
    ///
    /// Returns the original model ID if it does not have the prefix.
    pub fn strip_ollama_prefix(model_id: &str) -> &str {
        model_id.strip_prefix(OLLAMA_PREFIX).unwrap_or(model_id)
    }

    /// Add the "ollama:" prefix to a model ID.
    pub fn add_ollama_prefix(model_id: &str) -> String {
        format!("{}{}", OLLAMA_PREFIX, model_id)
    }

    /// Get a reference to the GGUF manager.
    pub fn gguf_manager(&self) -> &Arc<GgufModelManager> {
        &self.gguf
    }

    /// Get a reference to the Ollama manager.
    pub fn ollama_manager(&self) -> &Arc<OllamaModelManager> {
        &self.ollama
    }

    /// Check if the Ollama daemon is available and reachable.
    pub async fn ollama_available(&self) -> bool {
        self.ollama.is_available().await
    }

    /// Set the progress callback for GGUF downloads of a specific model.
    pub async fn set_gguf_progress_callback(
        &self,
        model_id: &str,
        cb: Box<dyn Fn(DownloadEvent) + Send + Sync>,
    ) {
        self.gguf.set_progress_callback(model_id, cb).await;
    }

    /// Set the progress callback for Ollama downloads of a specific model.
    ///
    /// `model_id` may carry the `"ollama:"` prefix (as used by the frontend
    /// and `list()`) — it's stripped here to match the unprefixed id that
    /// [`Self::download`] passes to the underlying `OllamaModelManager`,
    /// which is what progress events are actually keyed and looked up by.
    pub async fn set_ollama_progress_callback(
        &self,
        model_id: &str,
        cb: Box<dyn Fn(DownloadEvent) + Send + Sync>,
    ) {
        self.ollama
            .set_progress_callback(Self::strip_ollama_prefix(model_id), cb)
            .await;
    }

    /// Clear the GGUF progress callback for a specific model, dropping any
    /// resources (e.g. channel senders) it holds. Only affects that model's
    /// callback — concurrent downloads of other models are unaffected.
    pub async fn clear_gguf_progress_callback(&self, model_id: &str) {
        self.gguf.clear_progress_callback(model_id).await;
    }

    /// Clear the Ollama progress callback for a specific model, dropping any
    /// resources (e.g. channel senders) it holds. Only affects that model's
    /// callback — concurrent downloads of other models are unaffected.
    ///
    /// `model_id` is de-prefixed for the same reason as
    /// [`Self::set_ollama_progress_callback`].
    pub async fn clear_ollama_progress_callback(&self, model_id: &str) {
        self.ollama
            .clear_progress_callback(Self::strip_ollama_prefix(model_id))
            .await;
    }
}

#[async_trait]
impl ModelManager for CompositeModelManager {
    async fn list(&self) -> Result<Vec<ModelInfo>, ModelError> {
        // Get GGUF models first
        let mut models = self.gguf.list().await?;

        // OllamaModelManager::list() already returns Ok(vec![]) when the daemon is
        // unreachable, so no is_available() pre-check needed — that would double the
        // latency on every call with an extra /api/tags round-trip.
        let ollama_models = self.ollama.list().await?;
        for mut model in ollama_models {
            // Prepend "ollama:" prefix to Ollama model IDs
            model.id = Self::add_ollama_prefix(&model.id);
            models.push(model);
        }

        Ok(models)
    }

    async fn download(&self, model_id: &str) -> Result<(), ModelError> {
        if Self::is_ollama(model_id) {
            self.ollama
                .download(Self::strip_ollama_prefix(model_id))
                .await
        } else {
            self.gguf.download(model_id).await
        }
    }

    async fn cancel_download(&self, model_id: &str) -> Result<(), ModelError> {
        if Self::is_ollama(model_id) {
            self.ollama
                .cancel_download(Self::strip_ollama_prefix(model_id))
                .await
        } else {
            self.gguf.cancel_download(model_id).await
        }
    }

    async fn delete(&self, model_id: &str) -> Result<(), ModelError> {
        if Self::is_ollama(model_id) {
            self.ollama
                .delete(Self::strip_ollama_prefix(model_id))
                .await
        } else {
            self.gguf.delete(model_id).await
        }
    }

    async fn load(&self, model_id: &str) -> Result<(), ModelError> {
        if Self::is_ollama(model_id) {
            self.ollama.load(Self::strip_ollama_prefix(model_id)).await
        } else {
            self.gguf.load(model_id).await
        }
    }

    async fn unload(&self) -> Result<(), ModelError> {
        // Unload from GGUF first
        let gguf_result = self.gguf.unload().await;

        // OllamaModelManager::unload() is safe to call unconditionally — it no-ops
        // when nothing is loaded, so no is_available() guard needed here.
        let ollama_result = self.ollama.unload().await;

        // Return the first error if either fails, otherwise Ok
        gguf_result.and(ollama_result)
    }

    async fn loaded_model(&self) -> Result<Option<String>, ModelError> {
        // Check GGUF first
        if let Some(id) = self.gguf.loaded_model().await? {
            return Ok(Some(id));
        }

        // Then check Ollama (only if available)
        if self.ollama.is_available().await {
            if let Some(id) = self.ollama.loaded_model().await? {
                return Ok(Some(Self::add_ollama_prefix(&id)));
            }
        }

        Ok(None)
    }

    async fn recommended_model(&self) -> Result<String, ModelError> {
        self.gguf.recommended_model().await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ollama_prefix() {
        // Test various model IDs
        assert!(CompositeModelManager::is_ollama("ollama:llama3.2:3b"));
        assert!(!CompositeModelManager::is_ollama("ministral-3b-q4km"));
        assert!(CompositeModelManager::is_ollama("ollama:"));
        assert!(!CompositeModelManager::is_ollama(""));
    }

    #[test]
    fn test_strip_ollama_prefix() {
        // Strip the prefix correctly
        assert_eq!(
            CompositeModelManager::strip_ollama_prefix("ollama:llama3.2:3b"),
            "llama3.2:3b"
        );
        // Leave unchanged if no prefix
        assert_eq!(
            CompositeModelManager::strip_ollama_prefix("ministral"),
            "ministral"
        );
    }

    #[test]
    fn test_add_ollama_prefix() {
        // Add prefix correctly
        assert_eq!(
            CompositeModelManager::add_ollama_prefix("llama3.2:3b"),
            "ollama:llama3.2:3b"
        );
    }

    // -- Ollama progress callback de-prefixing (issue #1471 re-review) ------
    //
    // `download()` strips the "ollama:" prefix before delegating to the
    // underlying OllamaModelManager, which keys its progress callbacks and
    // fires progress events by that same unprefixed id. If
    // set/clear_ollama_progress_callback didn't also strip the prefix, a
    // callback registered via the composite manager (with the prefixed id,
    // as the daemon does) would never be found when progress actually fires,
    // silently dropping every progress tick for Ollama downloads.

    fn test_composite_manager() -> (CompositeModelManager, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let gguf = Arc::new(GgufModelManager::with_dir(tmp.path().to_path_buf()).unwrap());
        let ollama = Arc::new(OllamaModelManager::with_base_url(
            "http://127.0.0.1:19999".to_string(),
        ));
        (CompositeModelManager::new(gguf, ollama), tmp)
    }

    #[tokio::test]
    async fn ollama_progress_callback_set_with_prefixed_id_is_stored_under_stripped_key() {
        let (mgr, _tmp) = test_composite_manager();
        let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);

        // Register via the composite manager using the prefixed id, exactly
        // as the daemon does in `local_agent_service.rs`.
        mgr.set_ollama_progress_callback(
            "ollama:llama3.2:3b",
            Box::new(move |_evt| {
                let _ = tx.try_send(());
            }),
        )
        .await;

        // Clearing via the *unprefixed* id (mirroring how OllamaModelManager
        // itself would look it up during `fire_progress`, keyed by the
        // unprefixed id `download()` uses) must find and drop it.
        mgr.ollama_manager()
            .clear_progress_callback("llama3.2:3b")
            .await;
        assert!(rx.is_closed());
    }

    #[tokio::test]
    async fn clear_ollama_progress_callback_accepts_prefixed_id() {
        let (mgr, _tmp) = test_composite_manager();
        let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);

        mgr.set_ollama_progress_callback(
            "ollama:llama3.2:3b",
            Box::new(move |_evt| {
                let _ = tx.try_send(());
            }),
        )
        .await;
        assert!(!rx.is_closed());

        // Clearing through the composite manager with the *same prefixed*
        // id it was registered with must also successfully remove it.
        mgr.clear_ollama_progress_callback("ollama:llama3.2:3b")
            .await;
        assert!(rx.is_closed());
    }
}
