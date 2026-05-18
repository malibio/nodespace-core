//! tonic `ImportService` implementation backed by `nodespace-core`.
//!
//! Preserves the two-phase pipeline from `commands/import.rs`:
//!   Phase 1 — file reads, markdown parsing, link resolution (sync, fast)
//!   Phase 2 — DB writes, collection assignment, mention creation (async background)
//!
//! Progress events are streamed back to the caller via a tokio channel that
//! bridges the background task to the tonic server-streaming response.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nodespace_core::mcp::handlers::markdown::{
    prepare_nodes_from_markdown, transform_links_in_nodes_with_mentions, PreparedNode,
};
use nodespace_core::services::{CollectionService, NodeService as CoreNodeService};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    import_service_server::ImportService as GrpcImportService, FileImportResult,
    ImportMarkdownFilesRequest, ImportMarkdownRequest, ImportOptions, ImportProgressEvent,
};

const CHANNEL_BUFFER: usize = 64;

pub struct ImportServiceImpl {
    node_service: Arc<CoreNodeService>,
}

impl ImportServiceImpl {
    pub fn new(node_service: Arc<CoreNodeService>) -> Self {
        Self { node_service }
    }
}

// ---------------------------------------------------------------------------
// tonic service trait
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl GrpcImportService for ImportServiceImpl {
    type ImportMarkdownStream = ReceiverStream<Result<ImportProgressEvent, Status>>;
    type ImportMarkdownFilesStream = ReceiverStream<Result<ImportProgressEvent, Status>>;

    async fn import_markdown(
        &self,
        request: Request<ImportMarkdownRequest>,
    ) -> Result<Response<Self::ImportMarkdownStream>, Status> {
        let req = request.into_inner();
        let opts = req.options.unwrap_or_default();
        let node_service = Arc::clone(&self.node_service);

        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);

        tokio::spawn(async move {
            run_single_file_import(node_service, req.file_path, opts, tx).await;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn import_markdown_files(
        &self,
        request: Request<ImportMarkdownFilesRequest>,
    ) -> Result<Response<Self::ImportMarkdownFilesStream>, Status> {
        let req = request.into_inner();
        let opts = req.options.unwrap_or_default();
        let node_service = Arc::clone(&self.node_service);

        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);

        tokio::spawn(async move {
            run_batch_import(node_service, req.file_paths, opts, tx).await;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// ---------------------------------------------------------------------------
// Single-file import
// ---------------------------------------------------------------------------

async fn run_single_file_import(
    node_service: Arc<CoreNodeService>,
    file_path: String,
    opts: ImportOptions,
    tx: mpsc::Sender<Result<ImportProgressEvent, Status>>,
) {
    let path = PathBuf::from(&file_path);

    let result = import_single_file(&node_service, &path, &opts).await;

    let _ = tx
        .send(Ok(ImportProgressEvent {
            step: 9,
            step_name: "complete".to_string(),
            message: if result.success {
                format!(
                    "Imported {} ({} nodes)",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&file_path),
                    result.nodes_created
                )
            } else {
                format!(
                    "Failed: {}",
                    result.error.as_deref().unwrap_or("unknown error")
                )
            },
            current: 1,
            total: 1,
            results: vec![proto_file_result(result)],
        }))
        .await;
}

async fn import_single_file(
    node_service: &CoreNodeService,
    path: &Path,
    opts: &ImportOptions,
) -> LocalFileImportResult {
    if !path.exists() {
        return LocalFileImportResult::error(
            path.to_string_lossy().to_string(),
            "File does not exist",
        );
    }
    if !path.is_file() {
        return LocalFileImportResult::error(
            path.to_string_lossy().to_string(),
            "Path is not a file",
        );
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return LocalFileImportResult::error(
                path.to_string_lossy().to_string(),
                &format!("Failed to read file: {}", e),
            );
        }
    };

    let (collection, is_archived) = if opts.auto_collection_routing {
        let base_dir = if opts.base_directory.is_empty() {
            path.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            PathBuf::from(&opts.base_directory)
        };
        let meta = derive_collection_metadata(path, &base_dir);
        (Some(meta.collection), meta.is_archived)
    } else if !opts.collection.is_empty() {
        (Some(opts.collection.clone()), false)
    } else {
        (None, false)
    };

    let title = if opts.use_filename_as_title {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    } else {
        content
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    };

    match import_markdown_content(node_service, &title, &content, is_archived).await {
        Ok((root_id, nodes_created)) => {
            if let Some(ref coll) = collection {
                let collection_service = CollectionService::new(node_service.store(), node_service);
                if let Err(e) = collection_service
                    .add_to_collection_by_path(&root_id, coll)
                    .await
                {
                    return LocalFileImportResult {
                        file_path: path.to_string_lossy().to_string(),
                        root_id: Some(root_id),
                        nodes_created,
                        success: true,
                        error: Some(format!("Imported but failed to add to collection: {}", e)),
                        collection,
                        archived: is_archived,
                    };
                }
            }
            LocalFileImportResult {
                file_path: path.to_string_lossy().to_string(),
                root_id: Some(root_id),
                nodes_created,
                success: true,
                error: None,
                collection,
                archived: is_archived,
            }
        }
        Err(e) => LocalFileImportResult::error(path.to_string_lossy().to_string(), &e),
    }
}

// ---------------------------------------------------------------------------
// Batch import (two-phase pipeline)
// ---------------------------------------------------------------------------

async fn run_batch_import(
    node_service: Arc<CoreNodeService>,
    file_paths: Vec<String>,
    opts: ImportOptions,
    tx: mpsc::Sender<Result<ImportProgressEvent, Status>>,
) {
    let total_files = file_paths.len();

    let base_dir = if !opts.base_directory.is_empty() {
        PathBuf::from(&opts.base_directory)
    } else {
        file_paths
            .first()
            .map(|p| {
                PathBuf::from(p)
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_path_buf()
            })
            .unwrap_or_else(|| PathBuf::from("."))
    };

    // ========================================================================
    // PHASE 1: Parse all files (sync, in-memory)
    // ========================================================================

    send_progress(
        &tx,
        1,
        "scanning",
        "Scanning folder...",
        0,
        total_files,
        vec![],
    )
    .await;

    let mut file_contents: Vec<FileReadResult> = Vec::new();
    let mut failed_results: Vec<LocalFileImportResult> = Vec::new();

    for (index, file_path) in file_paths.iter().enumerate() {
        let path = PathBuf::from(file_path);
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file_path)
            .to_string();

        send_progress(
            &tx,
            2,
            "reading",
            &format!("Reading: {}", filename),
            index + 1,
            total_files,
            vec![],
        )
        .await;

        if !path.exists() || !path.is_file() {
            failed_results.push(LocalFileImportResult::error(
                file_path.clone(),
                "File does not exist or is not a file",
            ));
            continue;
        }

        let (collection_path, is_archived) = if opts.auto_collection_routing {
            let meta = derive_collection_metadata(&path, &base_dir);
            (Some(meta.collection), meta.is_archived)
        } else if !opts.collection.is_empty() {
            (Some(opts.collection.clone()), false)
        } else {
            (None, false)
        };

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let relative_path = path
                    .strip_prefix(&base_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                file_contents.push(FileReadResult {
                    path,
                    content,
                    relative_path,
                    collection_path,
                    is_archived,
                });
            }
            Err(e) => {
                failed_results.push(LocalFileImportResult::error(
                    file_path.clone(),
                    &format!("Failed to read file: {}", e),
                ));
            }
        }
    }

    // Parse phase
    let mut prepared_files: Vec<PreparedFileImport> = Vec::new();

    for (index, file_read) in file_contents.iter().enumerate() {
        let filename = file_read
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&file_read.relative_path)
            .to_string();

        send_progress(
            &tx,
            3,
            "parsing",
            &format!("Parsing: {}", filename),
            index + 1,
            file_contents.len(),
            vec![],
        )
        .await;

        let title = if opts.use_filename_as_title {
            file_read
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            file_read
                .content
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        };

        let root_id = uuid::Uuid::new_v4().to_string();

        let root_content = if title.starts_with('#') {
            title.clone()
        } else {
            format!("# {}", title)
        };

        let content_for_children = {
            let first_line = file_read.content.lines().find(|l| !l.trim().is_empty());
            if first_line == Some(&title) {
                let lines: Vec<&str> = file_read.content.lines().collect();
                let first_idx = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
                lines[first_idx + 1..].join("\n")
            } else {
                file_read.content.clone()
            }
        };

        match prepare_nodes_from_markdown(&content_for_children, Some(root_id.clone())) {
            Ok(children) => {
                prepared_files.push(PreparedFileImport {
                    file_path: file_read.path.clone(),
                    root_id,
                    root_content,
                    is_archived: file_read.is_archived,
                    collection_path: file_read.collection_path.clone(),
                    children,
                });
            }
            Err(e) => {
                failed_results.push(LocalFileImportResult::error(
                    file_read.path.to_string_lossy().to_string(),
                    &format!("Failed to parse markdown: {:?}", e),
                ));
            }
        }
    }

    // Build file→UUID map for link transformation
    let file_to_uuid_map: HashMap<PathBuf, String> = prepared_files
        .iter()
        .map(|f| (f.file_path.clone(), f.root_id.clone()))
        .collect();

    send_progress(
        &tx,
        4,
        "resolving",
        "Resolving internal links...",
        0,
        prepared_files.len(),
        vec![],
    )
    .await;

    let mut all_mentions: Vec<(String, String)> = Vec::new();
    for prepared in &mut prepared_files {
        let result = transform_links_in_nodes_with_mentions(
            &mut prepared.children,
            &file_to_uuid_map,
            Some(&prepared.file_path),
            &prepared.root_id,
        );
        all_mentions.extend(result.mentions);
    }

    // Collect unique collection paths
    let unique_collections: Vec<String> = prepared_files
        .iter()
        .filter_map(|f| f.collection_path.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build Phase 1 results (success entries; failed_results holds failures)
    let mut phase1_results: Vec<LocalFileImportResult> = prepared_files
        .iter()
        .map(|p| LocalFileImportResult {
            file_path: p.file_path.to_string_lossy().to_string(),
            root_id: Some(p.root_id.clone()),
            nodes_created: 1 + p.children.len(),
            success: true,
            error: None,
            collection: p.collection_path.clone(),
            archived: p.is_archived,
        })
        .collect();

    // ========================================================================
    // PHASE 2: DB operations (background task, results streamed back via tx)
    // ========================================================================

    let unique_collections_count = unique_collections.len();
    let successful_files_count = prepared_files.len();
    let total_nodes_count: usize = prepared_files.iter().map(|f| 1 + f.children.len()).sum();
    let all_mentions_count = all_mentions.len();
    let store = Arc::clone(node_service.store());
    let node_service_clone = (*node_service).clone();
    let tx_bg = tx.clone();

    tokio::spawn(async move {
        send_progress(
            &tx_bg,
            5,
            "collections",
            &format!("Creating {} collections...", unique_collections_count),
            0,
            unique_collections_count,
            vec![],
        )
        .await;

        let collection_service = CollectionService::new(&store, &node_service_clone);
        let collection_map = match collection_service
            .bulk_resolve_collections(&unique_collections)
            .await
        {
            Ok(map) => map,
            Err(e) => {
                tracing::error!("Failed to bulk resolve collections: {:?}", e);
                HashMap::new()
            }
        };

        let mut all_nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )> = Vec::new();
        let mut collection_assignments: Vec<(String, String)> = Vec::new();

        for prepared in &prepared_files {
            let mut root_props = serde_json::json!({});
            if prepared.is_archived {
                root_props["lifecycle_status"] = serde_json::json!("archived");
            }
            all_nodes.push((
                prepared.root_id.clone(),
                "header".to_string(),
                prepared.root_content.clone(),
                None,
                1.0,
                root_props,
            ));

            for child in &prepared.children {
                let parent = child
                    .parent_id
                    .clone()
                    .or_else(|| Some(prepared.root_id.clone()));
                all_nodes.push((
                    child.id.clone(),
                    child.node_type.clone(),
                    child.content.clone(),
                    parent,
                    child.order,
                    child.properties.clone(),
                ));
            }

            if let Some(ref coll_path) = prepared.collection_path {
                if let Some(coll_id) = collection_map.get(coll_path) {
                    collection_assignments.push((prepared.root_id.clone(), coll_id.clone()));
                }
            }
        }

        send_progress(
            &tx_bg,
            6,
            "importing",
            &format!("Importing {} nodes...", all_nodes.len()),
            0,
            all_nodes.len(),
            vec![],
        )
        .await;

        let bulk_insert_failed = match node_service_clone
            .bulk_create_hierarchy_trusted(all_nodes)
            .await
        {
            Ok(ids) => {
                tracing::info!("Bulk created {} nodes", ids.len());
                false
            }
            Err(e) => {
                tracing::error!("Failed to bulk create nodes: {:?}", e);
                true
            }
        };

        // When the bulk insert fails, surface it to the caller rather than
        // silently reporting success — mark all phase1 results as failed.
        if bulk_insert_failed {
            for r in &mut phase1_results {
                r.success = false;
                r.error = Some("Bulk node insertion failed; see daemon logs".to_string());
            }
        }

        send_progress(
            &tx_bg,
            7,
            "assigning",
            "Assigning to collections...",
            0,
            collection_assignments.len(),
            vec![],
        )
        .await;

        if !bulk_insert_failed && !collection_assignments.is_empty() {
            match store.bulk_add_to_collections(&collection_assignments).await {
                Ok(count) => tracing::info!("Bulk assigned {} collection memberships", count),
                Err(e) => tracing::error!("Failed to bulk add to collections: {:?}", e),
            }
        }

        send_progress(
            &tx_bg,
            8,
            "references",
            &format!("Creating {} references...", all_mentions_count),
            0,
            all_mentions_count,
            vec![],
        )
        .await;

        if !bulk_insert_failed && !all_mentions.is_empty() {
            match store.bulk_create_mentions(&all_mentions).await {
                Ok(count) => tracing::info!("Bulk created {} mentions", count),
                Err(e) => tracing::error!("Failed to bulk create mentions: {:?}", e),
            }
        }

        if !bulk_insert_failed {
            for prepared in &prepared_files {
                if prepared.is_archived {
                    if let Err(e) = store
                        .update_lifecycle_status(&prepared.root_id, "archived")
                        .await
                    {
                        tracing::warn!(
                            "Failed to set lifecycle_status for {}: {}",
                            prepared.root_id,
                            e
                        );
                    }
                }
            }
        }

        let mut all_results: Vec<LocalFileImportResult> = failed_results;
        all_results.append(&mut phase1_results);
        let proto_results: Vec<FileImportResult> =
            all_results.into_iter().map(proto_file_result).collect();

        send_progress(
            &tx_bg,
            9,
            "complete",
            &format!(
                "Imported {} files ({} nodes)",
                successful_files_count, total_nodes_count
            ),
            total_files,
            total_files,
            proto_results,
        )
        .await;
    });
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn send_progress(
    tx: &mpsc::Sender<Result<ImportProgressEvent, Status>>,
    step: u32,
    step_name: &str,
    message: &str,
    current: usize,
    total: usize,
    results: Vec<FileImportResult>,
) {
    let _ = tx
        .send(Ok(ImportProgressEvent {
            step,
            step_name: step_name.to_string(),
            message: message.to_string(),
            current: current as u32,
            total: total as u32,
            results,
        }))
        .await;
}

fn proto_file_result(r: LocalFileImportResult) -> FileImportResult {
    FileImportResult {
        file_path: r.file_path,
        root_id: r.root_id.unwrap_or_default(),
        nodes_created: r.nodes_created as u32,
        success: r.success,
        error: r.error.unwrap_or_default(),
        collection: r.collection.unwrap_or_default(),
        archived: r.archived,
    }
}

// ---------------------------------------------------------------------------
// Smart collection routing (ported from commands/import.rs)
// ---------------------------------------------------------------------------

struct CollectionMetadata {
    collection: String,
    is_archived: bool,
}

fn derive_collection_metadata(file_path: &Path, base_dir: &Path) -> CollectionMetadata {
    let relative = file_path.strip_prefix(base_dir).unwrap_or(file_path);
    let path_str = relative.to_string_lossy().to_lowercase();
    let segments: Vec<&str> = relative
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    if path_str.contains("/archived/")
        || segments.iter().any(|s| s.eq_ignore_ascii_case("archived"))
    {
        return CollectionMetadata {
            collection: "Archived".to_string(),
            is_archived: true,
        };
    }

    if path_str.contains("/decisions/") || path_str.contains("/adr/") {
        return CollectionMetadata {
            collection: "ADR".to_string(),
            is_archived: false,
        };
    }

    if path_str.contains("/lessons/") || segments.iter().any(|s| s.eq_ignore_ascii_case("lessons"))
    {
        return CollectionMetadata {
            collection: "Lessons".to_string(),
            is_archived: false,
        };
    }

    if segments
        .first()
        .map(|s| s.eq_ignore_ascii_case("troubleshooting"))
        .unwrap_or(false)
    {
        return CollectionMetadata {
            collection: "Troubleshooting".to_string(),
            is_archived: false,
        };
    }

    if segments
        .first()
        .map(|s| s.eq_ignore_ascii_case("architecture"))
        .unwrap_or(false)
    {
        let sub_segments: Vec<&str> = segments.iter().skip(1).copied().collect();

        if sub_segments
            .first()
            .map(|s| s.eq_ignore_ascii_case("components"))
            .unwrap_or(false)
        {
            return CollectionMetadata {
                collection: "Components".to_string(),
                is_archived: false,
            };
        }

        if sub_segments
            .first()
            .map(|s| s.eq_ignore_ascii_case("business-logic"))
            .unwrap_or(false)
        {
            return CollectionMetadata {
                collection: "Business Logic".to_string(),
                is_archived: false,
            };
        }

        if sub_segments
            .first()
            .map(|s| s.eq_ignore_ascii_case("development"))
            .unwrap_or(false)
        {
            let dev_sub: Vec<&str> = sub_segments.iter().skip(1).copied().collect();
            if !dev_sub.is_empty() {
                let nested = dev_sub
                    .iter()
                    .map(|s| to_title_case(s))
                    .collect::<Vec<_>>()
                    .join(":");
                return CollectionMetadata {
                    collection: format!("Development:{}", nested),
                    is_archived: false,
                };
            }
            return CollectionMetadata {
                collection: "Development".to_string(),
                is_archived: false,
            };
        }

        if sub_segments
            .first()
            .map(|s| s.eq_ignore_ascii_case("core"))
            .unwrap_or(false)
        {
            return CollectionMetadata {
                collection: "Architecture:Core".to_string(),
                is_archived: false,
            };
        }

        if !sub_segments.is_empty() {
            let arch_sub = sub_segments
                .iter()
                .map(|s| to_title_case(s))
                .collect::<Vec<_>>()
                .join(":");
            return CollectionMetadata {
                collection: format!("Architecture:{}", arch_sub),
                is_archived: false,
            };
        }

        return CollectionMetadata {
            collection: "Architecture".to_string(),
            is_archived: false,
        };
    }

    if segments
        .first()
        .map(|s| s.eq_ignore_ascii_case("performance"))
        .unwrap_or(false)
    {
        return CollectionMetadata {
            collection: "Performance".to_string(),
            is_archived: false,
        };
    }

    if segments
        .first()
        .map(|s| s.eq_ignore_ascii_case("testing"))
        .unwrap_or(false)
    {
        return CollectionMetadata {
            collection: "Testing".to_string(),
            is_archived: false,
        };
    }

    if segments.is_empty() {
        return CollectionMetadata {
            collection: "Docs".to_string(),
            is_archived: false,
        };
    }

    let collection = segments
        .iter()
        .map(|s| to_title_case(s))
        .collect::<Vec<_>>()
        .join(":");

    CollectionMetadata {
        collection,
        is_archived: false,
    }
}

fn to_title_case(s: &str) -> String {
    s.split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Internal data types (not exposed over wire)
// ---------------------------------------------------------------------------

struct PreparedFileImport {
    file_path: PathBuf,
    root_id: String,
    root_content: String,
    is_archived: bool,
    collection_path: Option<String>,
    children: Vec<PreparedNode>,
}

struct FileReadResult {
    path: PathBuf,
    content: String,
    relative_path: String,
    collection_path: Option<String>,
    is_archived: bool,
}

struct LocalFileImportResult {
    file_path: String,
    root_id: Option<String>,
    nodes_created: usize,
    success: bool,
    error: Option<String>,
    collection: Option<String>,
    archived: bool,
}

impl LocalFileImportResult {
    fn error(file_path: String, msg: &str) -> Self {
        Self {
            file_path,
            root_id: None,
            nodes_created: 0,
            success: false,
            error: Some(msg.to_string()),
            collection: None,
            archived: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Markdown content importer (from commands/import.rs)
// ---------------------------------------------------------------------------

async fn import_markdown_content(
    node_service: &CoreNodeService,
    title: &str,
    content: &str,
    is_archived: bool,
) -> Result<(String, usize), String> {
    use nodespace_core::services::CreateNodeParams;

    let (_, clean_title) = if title.starts_with('#') {
        ("header", title.to_string())
    } else {
        ("header", format!("# {}", title))
    };

    let content_for_children = {
        let first_line = content.lines().find(|l| !l.trim().is_empty());
        if first_line == Some(title) {
            let lines: Vec<&str> = content.lines().collect();
            let first_idx = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
            lines[first_idx + 1..].join("\n")
        } else {
            content.to_string()
        }
    };

    let prepared_nodes = prepare_nodes_from_markdown(&content_for_children, None)
        .map_err(|e| format!("Failed to parse markdown: {:?}", e))?;

    let mut properties = serde_json::json!({});
    if is_archived {
        properties["lifecycle_status"] = serde_json::json!("archived");
    }

    let root_id = node_service
        .create_node_with_parent(CreateNodeParams {
            id: None,
            node_type: "header".to_string(),
            content: clean_title,
            parent_id: None,
            insert_after_node_id: None,
            properties,
        })
        .await
        .map_err(|e| format!("Failed to create root node: {}", e))?;

    if is_archived {
        if let Err(e) = node_service
            .store()
            .update_lifecycle_status(&root_id, "archived")
            .await
        {
            tracing::warn!(
                "Failed to set lifecycle_status to archived for {}: {}",
                root_id,
                e
            );
        }
    }

    let mut nodes_created = 1;

    if !prepared_nodes.is_empty() {
        let nodes_for_bulk: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )> = prepared_nodes
            .iter()
            .map(|n| {
                let parent = n.parent_id.clone().or_else(|| Some(root_id.clone()));
                (
                    n.id.clone(),
                    n.node_type.clone(),
                    n.content.clone(),
                    parent,
                    n.order,
                    n.properties.clone(),
                )
            })
            .collect();

        let created_ids = node_service
            .bulk_create_hierarchy_root_notify(nodes_for_bulk)
            .await
            .map_err(|e| format!("Failed to bulk create nodes: {}", e))?;

        nodes_created += created_ids.len();
    }

    Ok((root_id, nodes_created))
}
