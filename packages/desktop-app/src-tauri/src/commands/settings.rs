//! Settings commands for reading and updating app preferences.
//!
//! Daemon config (database path, gRPC address) is owned by `nodespaced` and
//! fetched/updated via the `SettingsService` gRPC RPC.
//!
//! Display preferences (theme, render_markdown) are UI-only state that remain
//! in Tauri local storage and are never sent to the daemon.

use crate::services::GrpcClient;
use nodespace_daemon::nodespace::{GetDaemonConfigRequest, UpdateDaemonConfigRequest};
use tauri::{AppHandle, Manager};

/// Settings response sent to the frontend.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    /// Currently active database path (from daemon config via gRPC).
    pub active_database_path: String,
    /// Display preferences.
    pub display: DisplaySettingsResponse,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySettingsResponse {
    pub render_markdown: bool,
    pub theme: String,
}

/// Result of a database path update.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUpdateResult {
    pub new_path: String,
    pub success: bool,
    /// Whether the app needs a restart for the change to take effect.
    pub restart_required: bool,
}

/// Get current app settings for the Settings UI.
///
/// Daemon config (database path) is fetched via gRPC; display preferences
/// are read from local Tauri storage.
#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    grpc_client: tauri::State<'_, GrpcClient>,
) -> Result<SettingsResponse, String> {
    let prefs = crate::preferences::load_preferences(&app).await?;

    let mut client = grpc_client.settings_client().await;
    let daemon_config = client
        .get_daemon_config(GetDaemonConfigRequest {})
        .await
        .map_err(|e| format!("Failed to fetch daemon config: {}", e))?
        .into_inner();

    Ok(SettingsResponse {
        active_database_path: daemon_config.active_database_path,
        display: DisplaySettingsResponse {
            render_markdown: prefs.display.render_markdown,
            theme: prefs.display.theme,
        },
    })
}

/// Update display settings (takes effect immediately, no restart required).
///
/// Saves to preferences.json and emits a "settings-changed" Tauri event
/// so all open panes can react to the change.
#[tauri::command]
pub async fn update_display_settings(
    app: AppHandle,
    render_markdown: Option<bool>,
    theme: Option<String>,
) -> Result<(), String> {
    use tauri::Emitter;

    let mut prefs = crate::preferences::load_preferences(&app).await?;

    if let Some(rm) = render_markdown {
        prefs.display.render_markdown = rm;
    }
    if let Some(t) = &theme {
        if !["system", "light", "dark"].contains(&t.as_str()) {
            return Err(format!(
                "Invalid theme value: '{}'. Must be system, light, or dark.",
                t
            ));
        }
        prefs.display.theme = t.clone();
    }

    crate::preferences::save_preferences(&app, &prefs).await?;

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit(
            "settings-changed",
            serde_json::json!({
                "renderMarkdown": prefs.display.render_markdown,
                "theme": prefs.display.theme,
            }),
        );
    }

    Ok(())
}

/// Open native folder picker and save the chosen database path to daemon config.
///
/// The change is persisted to `~/.nodespace/daemon.toml` via gRPC. The daemon
/// must be restarted for the new path to take effect.
#[tauri::command]
pub async fn select_new_database(
    app: tauri::AppHandle,
    grpc_client: tauri::State<'_, GrpcClient>,
) -> Result<DatabaseUpdateResult, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};

    let folder = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .ok_or_else(|| "No folder selected".to_string())?;

    let folder_path = match folder {
        FilePath::Path(path) => path,
        FilePath::Url(url) => std::path::PathBuf::from(url.path()),
    };

    let path_str = folder_path.to_string_lossy().to_string();

    let mut client = grpc_client.settings_client().await;
    client
        .update_daemon_config(UpdateDaemonConfigRequest {
            active_database_path: path_str.clone(),
            grpc_address: String::new(),
        })
        .await
        .map_err(|e| format!("Failed to update daemon config: {}", e))?;

    Ok(DatabaseUpdateResult {
        new_path: path_str,
        success: true,
        restart_required: true,
    })
}

/// Restart the application with graceful GPU/background task shutdown.
///
/// Without explicit cleanup, `app.restart()` calls `std::process::exit()` which
/// triggers C++ destructors via `__cxa_finalize_ranges`. The Metal residency sets
/// for the embedding model are still active, causing a SIGABRT assertion failure
/// in `ggml_metal_rsets_free`.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    tracing::info!("Restart requested, performing graceful shutdown...");
    crate::graceful_shutdown(&app);
    tracing::info!("Graceful shutdown complete, restarting app...");
    app.restart();
}

/// Reset database path to default by updating daemon config.
///
/// The daemon must be restarted for the change to take effect.
#[tauri::command]
pub async fn reset_database_to_default(
    grpc_client: tauri::State<'_, GrpcClient>,
) -> Result<String, String> {
    let default_path = crate::preferences::get_default_database_path()?;
    let path_str = default_path.to_string_lossy().to_string();

    let mut client = grpc_client.settings_client().await;
    client
        .update_daemon_config(UpdateDaemonConfigRequest {
            active_database_path: path_str.clone(),
            grpc_address: String::new(),
        })
        .await
        .map_err(|e| format!("Failed to reset daemon config: {}", e))?;

    Ok(path_str)
}
