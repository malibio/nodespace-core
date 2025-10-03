//! Database initialization and path management commands

use nodespace_core::{DatabaseService, NodeService};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const DB_PATH_PREFERENCE_KEY: &str = "database_path";

/// Save database path preference to app config
fn save_db_path_preference(app: &AppHandle, path: &Path) -> Result<(), String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config directory: {}", e))?;

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let pref_file = config_dir.join("preferences.json");
    let prefs = serde_json::json!({
        DB_PATH_PREFERENCE_KEY: path.to_string_lossy().to_string()
    });

    std::fs::write(&pref_file, serde_json::to_string_pretty(&prefs).unwrap())
        .map_err(|e| format!("Failed to write preferences: {}", e))?;

    Ok(())
}

/// Load database path preference from app config
fn load_db_path_preference(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get config directory: {}", e))?;

    let pref_file = config_dir.join("preferences.json");

    if !pref_file.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&pref_file)
        .map_err(|e| format!("Failed to read preferences: {}", e))?;

    let prefs: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse preferences: {}", e))?;

    if let Some(path_str) = prefs.get(DB_PATH_PREFERENCE_KEY).and_then(|v| v.as_str()) {
        Ok(Some(PathBuf::from(path_str)))
    } else {
        Ok(None)
    }
}

/// Initialize database services with user-selected or default path
async fn init_services(app: &AppHandle, db_path: PathBuf) -> Result<(), String> {
    let db_service = DatabaseService::new(db_path)
        .await
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    let node_service = NodeService::new(db_service.clone())
        .await
        .map_err(|e| format!("Failed to initialize node service: {}", e))?;

    app.manage(db_service);
    app.manage(node_service);

    Ok(())
}

/// Select database location using native folder picker
#[tauri::command]
pub async fn select_db_location(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};

    let folder = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .ok_or_else(|| "No folder selected".to_string())?;

    let folder_path = match folder {
        FilePath::Path(path) => path,
        FilePath::Url(url) => PathBuf::from(url.path()),
    };

    let db_path = folder_path.join("nodespace.db");

    // Save preference
    save_db_path_preference(&app, &db_path)?;

    // Initialize services
    init_services(&app, db_path.clone()).await?;

    Ok(db_path.to_string_lossy().to_string())
}

/// Initialize database with saved preference or default path
#[tauri::command]
pub async fn initialize_database(app: AppHandle) -> Result<String, String> {
    let db_path = if let Some(saved) = load_db_path_preference(&app)? {
        saved
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {}", e))?
            .join("nodespace.db")
    };

    // Initialize services
    init_services(&app, db_path.clone()).await?;

    Ok(db_path.to_string_lossy().to_string())
}
