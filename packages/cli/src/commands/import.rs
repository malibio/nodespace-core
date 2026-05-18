//! `nodespace import` — import markdown files into NodeSpace via the daemon.

use anyhow::Result;
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{
    FileImportResult, ImportMarkdownFilesRequest, ImportMarkdownRequest, ImportOptions,
};
use nodespace_daemon::ImportServiceClient;
use tokio_stream::StreamExt;
use tonic::{transport::Channel, Request};

#[derive(Subcommand, Debug)]
pub enum ImportAction {
    /// Import a single markdown file.
    File(ImportFileArgs),
    /// Import all markdown files from a directory (recursive).
    Dir(ImportDirArgs),
}

#[derive(Args, Debug)]
pub struct ImportFileArgs {
    /// Path to the markdown file.
    pub file: String,

    /// Collection path to assign the document to (e.g. "docs:rust").
    #[arg(long)]
    pub collection: Option<String>,

    /// Use the filename stem as the document title.
    #[arg(long)]
    pub use_filename_as_title: bool,

    /// Route files to collections based on directory structure.
    #[arg(long)]
    pub auto_collection_routing: bool,
}

#[derive(Args, Debug)]
pub struct ImportDirArgs {
    /// Path to the directory containing markdown files.
    pub directory: String,

    /// Collection path to assign all documents to.
    #[arg(long)]
    pub collection: Option<String>,

    /// Use filename stems as document titles.
    #[arg(long)]
    pub use_filename_as_title: bool,

    /// Route files to collections based on directory structure.
    #[arg(long)]
    pub auto_collection_routing: bool,

    /// Directory names to exclude (repeatable, e.g. --exclude node_modules).
    #[arg(long = "exclude")]
    pub exclude_patterns: Vec<String>,
}

pub async fn run(
    client: &mut ImportServiceClient<Channel>,
    action: ImportAction,
    json: bool,
) -> Result<()> {
    match action {
        ImportAction::File(args) => run_file(client, args, json).await,
        ImportAction::Dir(args) => run_dir(client, args, json).await,
    }
}

fn results_to_json(results: &[FileImportResult]) -> serde_json::Value {
    serde_json::Value::Array(
        results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "file_path": r.file_path,
                    "root_id": r.root_id,
                    "nodes_created": r.nodes_created,
                    "success": r.success,
                    "error": r.error,
                    "collection": r.collection,
                    "archived": r.archived,
                })
            })
            .collect(),
    )
}

async fn run_file(
    client: &mut ImportServiceClient<Channel>,
    args: ImportFileArgs,
    json: bool,
) -> Result<()> {
    let opts = ImportOptions {
        collection: args.collection.unwrap_or_default(),
        use_filename_as_title: args.use_filename_as_title,
        auto_collection_routing: args.auto_collection_routing,
        exclude_patterns: vec![],
        base_directory: String::new(),
    };

    let mut stream = client
        .import_markdown(Request::new(ImportMarkdownRequest {
            file_path: args.file,
            options: Some(opts),
        }))
        .await?
        .into_inner();

    while let Some(event) = stream.next().await {
        let event = event?;
        if json {
            if event.step == 9 {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&results_to_json(&event.results))?
                );
            }
        } else {
            eprintln!("[{}/9] {}: {}", event.step, event.step_name, event.message);
            if event.step == 9 {
                for r in &event.results {
                    if r.success {
                        println!(
                            "✓ {} ({} nodes){}",
                            r.file_path,
                            r.nodes_created,
                            if r.collection.is_empty() {
                                String::new()
                            } else {
                                format!(" → {}", r.collection)
                            }
                        );
                    } else {
                        println!("✗ {}: {}", r.file_path, r.error);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_dir(
    client: &mut ImportServiceClient<Channel>,
    args: ImportDirArgs,
    json: bool,
) -> Result<()> {
    let file_paths = collect_markdown_files(&args.directory, &args.exclude_patterns)?;

    if file_paths.is_empty() {
        if !json {
            eprintln!("No markdown files found in {}", args.directory);
        }
        return Ok(());
    }

    let opts = ImportOptions {
        collection: args.collection.unwrap_or_default(),
        use_filename_as_title: args.use_filename_as_title,
        auto_collection_routing: args.auto_collection_routing,
        exclude_patterns: args.exclude_patterns,
        base_directory: args.directory,
    };

    let mut stream = client
        .import_markdown_files(Request::new(ImportMarkdownFilesRequest {
            file_paths,
            options: Some(opts),
        }))
        .await?
        .into_inner();

    while let Some(event) = stream.next().await {
        let event = event?;
        if json {
            if event.step == 9 {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&results_to_json(&event.results))?
                );
            }
        } else {
            eprintln!("[{}/9] {}: {}", event.step, event.step_name, event.message);
            if event.step == 9 {
                let successful = event.results.iter().filter(|r| r.success).count();
                let failed = event.results.len().saturating_sub(successful);
                println!("Imported {}/{} files", successful, event.results.len());
                if failed > 0 {
                    for r in event.results.iter().filter(|r| !r.success) {
                        println!("  ✗ {}: {}", r.file_path, r.error);
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_markdown_files(dir: &str, exclude_patterns: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect_recursive(std::path::Path::new(dir), &mut files, exclude_patterns)?;
    files.sort();
    Ok(files)
}

fn collect_recursive(
    dir: &std::path::Path,
    files: &mut Vec<String>,
    exclude_patterns: &[String],
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let path_str = path.to_string_lossy();

        let excluded = exclude_patterns.iter().any(|p| {
            path.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .map(|s| s.eq_ignore_ascii_case(p))
                    .unwrap_or(false)
            }) || path_str.contains(p.as_str())
        });

        if excluded {
            continue;
        }

        if path.is_dir() {
            collect_recursive(&path, files, exclude_patterns)?;
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            if let Some(s) = path.to_str() {
                files.push(s.to_string());
            }
        }
    }
    Ok(())
}
