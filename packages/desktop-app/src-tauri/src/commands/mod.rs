//! Tauri commands for desktop app integration
//!
//! This module exposes Rust functionality to the frontend via Tauri commands.

pub mod agent_session;
pub mod chat_models;
pub mod collections;
pub mod embeddings;
pub mod import;
pub mod local_agent;
pub mod nodes;
pub mod onboarding;
pub mod schemas;
pub mod settings;

use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

use crate::constants::EMBEDDING_MODEL_FILENAME;

/// Resolve the path to the bundled NLP model (GGUF format for llama.cpp).
///
/// Checks multiple locations in order:
/// 1. Bundled resources (production builds)
/// 2. User's ~/.nodespace/models/ directory (dev fallback)
pub fn resolve_bundled_model_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(resource_path) = app.path().resolve(
        format!("resources/models/{}", EMBEDDING_MODEL_FILENAME),
        BaseDirectory::Resource,
    ) {
        if resource_path.exists() {
            tracing::info!("Found bundled model at: {:?}", resource_path);
            return Some(resource_path);
        }
    }

    if let Some(home_dir) = dirs::home_dir() {
        let user_model_path = home_dir
            .join(".nodespace")
            .join("models")
            .join(EMBEDDING_MODEL_FILENAME);
        if user_model_path.exists() {
            tracing::info!("Found user model at: {:?}", user_model_path);
            return Some(user_model_path);
        }
    }

    tracing::warn!(
        "Model file not found — semantic search disabled. Download {} to ~/.nodespace/models/",
        EMBEDDING_MODEL_FILENAME
    );
    None
}
