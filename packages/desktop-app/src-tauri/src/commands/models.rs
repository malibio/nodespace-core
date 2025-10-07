//! Model management - extract bundled models to ~/.nodespace/models/
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tokio::fs;

/// Extract bundled models to ~/.nodespace/models/ on first run
///
/// Models are bundled with the application in the resources directory.
/// On first run, they are extracted to ~/.nodespace/models/ for use.
///
/// # Arguments
/// * `app` - Tauri application handle
///
/// # Returns
/// * `Ok(String)` - Path where models were extracted
/// * `Err(String)` - Error if extraction fails
#[tauri::command]
pub async fn ensure_models_installed(app: AppHandle) -> Result<String, String> {
    let home_dir = dirs::home_dir().ok_or_else(|| "Failed to get home directory".to_string())?;

    let models_dir = home_dir.join(".nodespace").join("models");
    let model_dir = models_dir.join("BAAI-bge-small-en-v1.5");

    // Check if model already exists
    if model_dir.exists() && model_dir.join("model.onnx").exists() {
        tracing::info!("Models already installed at {:?}", model_dir);
        return Ok(model_dir.to_string_lossy().to_string());
    }

    tracing::info!("Extracting bundled models to {:?}", model_dir);

    // Create models directory
    fs::create_dir_all(&models_dir)
        .await
        .map_err(|e| format!("Failed to create models directory: {}", e))?;

    // Get bundled resources path
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource directory: {}", e))?;

    let bundled_model = resource_path
        .join("resources")
        .join("models")
        .join("BAAI-bge-small-en-v1.5");

    if !bundled_model.exists() {
        return Err(format!(
            "Bundled model not found at {:?}. Please ensure models are downloaded during build.",
            bundled_model
        ));
    }

    // Copy bundled model to user's home directory
    copy_dir_all(&bundled_model, &model_dir)
        .map_err(|e| format!("Failed to copy model files: {}", e))?;

    tracing::info!("Successfully extracted models to {:?}", model_dir);

    Ok(model_dir.to_string_lossy().to_string())
}

/// Recursively copy a directory
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
