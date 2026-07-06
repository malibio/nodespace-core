//! File import commands — thin gRPC proxy to ImportService in nodespaced.
//!
//! All import logic (smart collection routing, two-phase async pipeline) runs
//! inside the daemon so imports continue if the Tauri window is closed
//! mid-import. These handlers subscribe to the progress stream and forward
//! each event to the frontend via Tauri events.

use nodespace_proto::nodespace::{
    ImportMarkdownFilesRequest, ImportMarkdownRequest, ImportOptions as ProtoImportOptions,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio_stream::StreamExt;
use tonic::Request;

use crate::services::GrpcClient;

/// Options for file import (mirrors proto ImportOptions)
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportOptions {
    pub collection: Option<String>,
    #[serde(default)]
    pub use_filename_as_title: bool,
    #[serde(default)]
    pub auto_collection_routing: bool,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    pub base_directory: Option<String>,
}

impl ImportOptions {
    fn into_proto(self) -> ProtoImportOptions {
        ProtoImportOptions {
            collection: self.collection.unwrap_or_default(),
            use_filename_as_title: self.use_filename_as_title,
            auto_collection_routing: self.auto_collection_routing,
            exclude_patterns: self.exclude_patterns,
            base_directory: self.base_directory.unwrap_or_default(),
        }
    }
}

/// Result of importing a single file
#[derive(Debug, Clone, Serialize)]
pub struct FileImportResult {
    pub file_path: String,
    pub root_id: Option<String>,
    pub nodes_created: usize,
    pub success: bool,
    pub error: Option<String>,
    pub collection: Option<String>,
    pub archived: bool,
}

/// Result of batch file import
#[derive(Debug, Serialize)]
pub struct BatchImportResult {
    pub total_files: usize,
    pub successful: usize,
    pub failed: usize,
    pub results: Vec<FileImportResult>,
}

/// Progress event forwarded to the frontend during import
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgressEvent {
    pub step: u8,
    pub step_name: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
}

/// Import a single markdown file via gRPC ImportService
#[tauri::command]
pub async fn import_markdown_file(
    app: AppHandle,
    grpc: State<'_, GrpcClient>,
    file_path: String,
    options: Option<ImportOptions>,
) -> Result<FileImportResult, String> {
    let mut client = grpc.import_client().await;
    let req = ImportMarkdownRequest {
        file_path: file_path.clone(),
        options: Some(options.unwrap_or_default().into_proto()),
    };

    let mut stream = client
        .import_markdown(Request::new(req))
        .await
        .map_err(|e| e.to_string())?
        .into_inner();

    let mut last_result: Option<FileImportResult> = None;

    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| e.to_string())?;

        let _ = app.emit(
            "import-progress",
            ImportProgressEvent {
                step: event.step as u8,
                step_name: event.step_name.clone(),
                message: event.message.clone(),
                current: event.current as usize,
                total: event.total as usize,
            },
        );

        if event.step == 9 {
            if let Some(r) = event.results.into_iter().next() {
                last_result = Some(FileImportResult {
                    file_path: r.file_path,
                    root_id: if r.root_id.is_empty() {
                        None
                    } else {
                        Some(r.root_id)
                    },
                    nodes_created: r.nodes_created as usize,
                    success: r.success,
                    error: if r.error.is_empty() {
                        None
                    } else {
                        Some(r.error)
                    },
                    collection: if r.collection.is_empty() {
                        None
                    } else {
                        Some(r.collection)
                    },
                    archived: r.archived,
                });
            }
        }
    }

    last_result.ok_or_else(|| "Import stream ended without a result".to_string())
}

/// Import multiple markdown files via gRPC ImportService
#[tauri::command]
pub async fn import_markdown_files(
    app: AppHandle,
    grpc: State<'_, GrpcClient>,
    file_paths: Vec<String>,
    options: Option<ImportOptions>,
) -> Result<BatchImportResult, String> {
    let total_files = file_paths.len();
    let mut client = grpc.import_client().await;
    let req = ImportMarkdownFilesRequest {
        file_paths,
        options: Some(options.unwrap_or_default().into_proto()),
    };

    let mut stream = client
        .import_markdown_files(Request::new(req))
        .await
        .map_err(|e| e.to_string())?
        .into_inner();

    let mut final_results: Vec<FileImportResult> = Vec::new();

    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| e.to_string())?;

        let _ = app.emit(
            "import-progress",
            ImportProgressEvent {
                step: event.step as u8,
                step_name: event.step_name.clone(),
                message: event.message.clone(),
                current: event.current as usize,
                total: event.total as usize,
            },
        );

        if event.step == 9 {
            final_results = event
                .results
                .into_iter()
                .map(|r| FileImportResult {
                    file_path: r.file_path,
                    root_id: if r.root_id.is_empty() {
                        None
                    } else {
                        Some(r.root_id)
                    },
                    nodes_created: r.nodes_created as usize,
                    success: r.success,
                    error: if r.error.is_empty() {
                        None
                    } else {
                        Some(r.error)
                    },
                    collection: if r.collection.is_empty() {
                        None
                    } else {
                        Some(r.collection)
                    },
                    archived: r.archived,
                })
                .collect();
        }
    }

    let successful = final_results.iter().filter(|r| r.success).count();
    let failed = final_results.len().saturating_sub(successful);

    Ok(BatchImportResult {
        total_files,
        successful,
        failed,
        results: final_results,
    })
}

/// Import markdown from a directory — collects .md files and delegates to batch import
#[tauri::command]
pub async fn import_markdown_directory(
    app: AppHandle,
    grpc: State<'_, GrpcClient>,
    directory_path: String,
    options: Option<ImportOptions>,
) -> Result<BatchImportResult, String> {
    use std::path::PathBuf;

    let dir = PathBuf::from(&directory_path);
    if !dir.exists() {
        return Err("Directory does not exist".to_string());
    }
    if !dir.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let dir = dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve directory: {}", e))?;
    check_within_allowed_base(&dir)?;

    let mut opts = options.unwrap_or_default();
    let exclude_patterns = opts.exclude_patterns.clone();

    // Set base_directory if not provided
    if opts.base_directory.is_none() {
        opts.base_directory = Some(directory_path.clone());
    }

    let mut md_files: Vec<String> = Vec::new();
    collect_markdown_files_with_exclusions(&dir, &mut md_files, &exclude_patterns)?;
    md_files.sort();

    tracing::info!(
        "Found {} markdown files in {} (excluded patterns: {:?})",
        md_files.len(),
        directory_path,
        exclude_patterns
    );

    import_markdown_files(app, grpc, md_files, Some(opts)).await
}

/// Reject import directories outside the user's home directory.
///
/// Defense-in-depth: under the current single-user-local trust model the
/// Tauri command boundary is already restricted to the daemon owner, but this
/// keeps a directory-import call from walking arbitrary system paths (e.g.
/// `/etc`, `/System`) if that boundary is ever weakened. `dir` must already be
/// canonicalized so symlink/`..` tricks can't escape the check.
fn check_within_allowed_base(dir: &std::path::Path) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let home = home
        .canonicalize()
        .map_err(|e| format!("Failed to resolve home directory: {}", e))?;

    if !dir.starts_with(&home) {
        return Err(format!(
            "Import directory must be within the home directory ({})",
            home.display()
        ));
    }

    Ok(())
}

fn collect_markdown_files_with_exclusions(
    dir: &std::path::Path,
    files: &mut Vec<String>,
    exclude_patterns: &[String],
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        let path_str = path.to_string_lossy();
        let should_exclude = exclude_patterns.iter().any(|pattern| {
            path.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .map(|s| s.eq_ignore_ascii_case(pattern))
                    .unwrap_or(false)
            }) || path_str.contains(pattern)
        });

        if should_exclude {
            tracing::debug!("Excluding path: {}", path_str);
            continue;
        }

        // Use symlink_metadata (lstat semantics) rather than path.is_dir()/is_file(),
        // which follow symlinks: a symlink inside the confined root could otherwise
        // point outside the allowed base and bypass check_within_allowed_base.
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to read metadata for {}: {}", path_str, e))?;

        if metadata.is_symlink() {
            tracing::debug!("Skipping symlink: {}", path_str);
            continue;
        }

        if metadata.is_dir() {
            collect_markdown_files_with_exclusions(&path, files, exclude_patterns)?;
        } else if metadata.is_file() && path.extension().map(|e| e == "md").unwrap_or(false) {
            if let Some(s) = path.to_str() {
                files.push(s.to_string());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_home_directory_itself() {
        let home = dirs::home_dir().unwrap().canonicalize().unwrap();
        assert!(check_within_allowed_base(&home).is_ok());
    }

    #[test]
    fn accepts_subdirectory_of_home() {
        let home = dirs::home_dir().unwrap().canonicalize().unwrap();
        assert!(check_within_allowed_base(&home.join("Documents")).is_ok());
    }

    #[test]
    fn rejects_path_outside_home() {
        let outside = std::path::Path::new("/etc");
        let result = check_within_allowed_base(outside);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("home directory"));
    }

    #[test]
    fn rejects_root() {
        let result = check_within_allowed_base(std::path::Path::new("/"));
        assert!(result.is_err());
    }

    #[test]
    fn does_not_follow_symlinked_directories_during_collection() {
        let tmp =
            std::env::temp_dir().join(format!("ns-import-symlink-test-{}", std::process::id()));
        let import_root = tmp.join("import_root");
        let outside = tmp.join("outside");
        let outside_file = outside.join("secret.md");
        let link = import_root.join("escape");

        std::fs::create_dir_all(&import_root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(&outside_file, "# secret").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, &link).unwrap();

        let mut files = Vec::new();
        collect_markdown_files_with_exclusions(&import_root, &mut files, &[]).unwrap();

        assert!(
            files.is_empty(),
            "symlinked directory outside the import root should not be traversed, found: {:?}",
            files
        );

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
