//! `nodespace diagnostics` — print a developer-facing summary of the daemon's
//! database state. Mirrors the now-deleted Tauri `get_database_diagnostics`
//! command but lives in the CLI because the intended audience (developers
//! debugging persistence) uses the shell, not the desktop UI.
//!
//! With multiple local databases (ADR-053) the report enumerates the whole
//! registry and runs node/root/schema counts against the *targeted* database —
//! the one selected by `--database`, or the daemon's default when none is given.

use anyhow::Result;
use clap::Args;
use nodespace_daemon::nodespace::{
    GetAllSchemasRequest, GetRootsRequest, ListDatabasesRequest, QueryNodesSimpleRequest,
};
use nodespace_daemon::DatabaseServiceClient;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tonic::transport::Channel;

use super::database::status_str;
use crate::NodeClient;

/// Upper bound on the per-query fetch when counting nodes. Diagnostics is a
/// developer tool, not a hot path — keeping this generous avoids surprise
/// truncation in any realistic dev database while still bounding memory.
/// If a database ever exceeds this, `collect()` surfaces a warning via the
/// `errors` field so the operator knows counts are undercounts.
const QUERY_LIMIT: u32 = 100_000;

/// How many recent node IDs to report.
const RECENT_LIMIT: usize = 10;

#[derive(Args, Debug)]
pub struct DiagnosticsArgs {}

/// One registered database as reported by the diagnostics enumeration.
#[derive(Debug)]
pub struct DatabaseSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_default: bool,
    pub status: String,
}

#[derive(Debug)]
pub struct DiagnosticsReport {
    /// Every database registered with the daemon.
    pub databases: Vec<DatabaseSummary>,
    /// Id of the database the counts below were computed against.
    pub targeted_database_id: String,
    /// On-disk path of the targeted database.
    pub targeted_database_path: String,
    /// Size on disk of the targeted database, `None` when the file is absent.
    pub database_size_bytes: Option<u64>,
    pub total_node_count: usize,
    pub root_node_count: usize,
    pub schema_count: i32,
    pub recent_node_ids: Vec<String>,
    pub errors: Vec<String>,
}

pub async fn run(
    node_client: &mut NodeClient,
    db_client: &mut DatabaseServiceClient<Channel>,
    target_id: Option<&str>,
    _args: DiagnosticsArgs,
    json_output: bool,
) -> Result<()> {
    let report = collect(node_client, db_client, target_id).await;
    if json_output {
        print_json(&report)
    } else {
        print_human(&report);
        Ok(())
    }
}

/// Build a diagnostics report: enumerate the registry via `DatabaseService.List`
/// and count nodes/roots/schemas in the targeted database via `node_client`
/// (which already carries the routing header for that database).
///
/// Split out from `run` so integration tests can drive it against a tempdir
/// daemon. `target_id` is the resolved id of the selected database, or `None`
/// for the daemon's default.
pub async fn collect(
    node_client: &mut NodeClient,
    db_client: &mut DatabaseServiceClient<Channel>,
    target_id: Option<&str>,
) -> DiagnosticsReport {
    let mut errors: Vec<String> = Vec::new();

    // Enumerate the registry.
    let (databases, default_id) = match db_client.list(ListDatabasesRequest {}).await {
        Ok(response) => {
            let inner = response.into_inner();
            let summaries: Vec<DatabaseSummary> = inner
                .databases
                .iter()
                .map(|d| DatabaseSummary {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    path: d.path.clone(),
                    is_default: d.is_default,
                    status: status_str(d.status).to_string(),
                })
                .collect();
            (summaries, inner.default_database_id)
        }
        Err(e) => {
            errors.push(format!("ListDatabases failed: {e}"));
            (Vec::new(), String::new())
        }
    };

    // Identify the targeted database: an explicit selection by id, otherwise the
    // registry's default. Its path drives the size-on-disk figure; the node
    // client is already routed to the same database by its header.
    let targeted = match target_id {
        Some(id) => databases.iter().find(|d| d.id == id),
        None => databases
            .iter()
            .find(|d| d.is_default)
            .or_else(|| databases.iter().find(|d| d.id == default_id)),
    };
    let (targeted_database_id, targeted_database_path) = match targeted {
        Some(d) => (d.id.clone(), d.path.clone()),
        None => {
            match target_id {
                Some(id) => errors.push(format!("targeted database '{id}' is not registered")),
                None => errors.push("no default database is set".to_string()),
            }
            (String::new(), String::new())
        }
    };

    let database_size_bytes = if !targeted_database_path.is_empty() {
        let path = Path::new(&targeted_database_path);
        if path.exists() {
            Some(database_size(path, &mut errors))
        } else {
            None
        }
    } else {
        None
    };

    // Pull all nodes once (bounded by QUERY_LIMIT) to compute total count
    // and surface the most-recently-created IDs from a single snapshot.
    let mut all_nodes = match node_client
        .query_nodes_simple(QueryNodesSimpleRequest {
            id: None,
            mentioned_by: None,
            content_contains: None,
            title_contains: None,
            node_type: None,
            limit: QUERY_LIMIT,
            offset: 0,
        })
        .await
    {
        Ok(response) => response.into_inner().nodes,
        Err(e) => {
            errors.push(format!("QueryNodesSimple failed: {e}"));
            Vec::new()
        }
    };

    // QueryNodesSimple has no LIMIT-overflow signal in its response shape;
    // a full batch is the only hint of truncation. Surface it so operators
    // know counts and recency lists may be undercounts rather than ground
    // truth.
    if all_nodes.len() == QUERY_LIMIT as usize {
        errors.push(format!(
            "Result truncated at QUERY_LIMIT={QUERY_LIMIT}; counts may be undercounts and recent IDs may miss nodes."
        ));
    }

    let total_node_count = all_nodes.len();

    let root_node_count = match node_client
        .get_roots(GetRootsRequest {
            limit: 0,
            offset: 0,
        })
        .await
    {
        Ok(response) => response.into_inner().count as usize,
        Err(e) => {
            errors.push(format!("GetRoots failed: {e}"));
            0
        }
    };

    // QueryNodesSimple doesn't expose ORDER BY, so we sort the in-memory
    // batch by created_at descending before slicing. O(n log n) on n ≤
    // QUERY_LIMIT is fine for a developer tool; doing it here keeps the
    // user-visible "recent" label honest.
    //
    // Invariant: `created_at` is an RFC3339 string emitted by chrono's
    // `DateTime<Utc>::to_rfc3339()` in `node_to_proto` (daemon). All values
    // share the `+00:00` suffix and consistent variable-precision format,
    // so lexicographic comparison is equivalent to chronological order. If
    // a second serialization path appears (e.g. `Z` suffix, local TZ),
    // parse to `DateTime` here instead.
    all_nodes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let recent_node_ids: Vec<String> = all_nodes
        .iter()
        .take(RECENT_LIMIT)
        .map(|n| n.id.clone())
        .collect();

    let schema_count = match node_client.get_all_schemas(GetAllSchemasRequest {}).await {
        Ok(response) => response.into_inner().count,
        Err(e) => {
            errors.push(format!("GetAllSchemas failed: {e}"));
            0
        }
    };

    DiagnosticsReport {
        databases,
        targeted_database_id,
        targeted_database_path,
        database_size_bytes,
        total_node_count,
        root_node_count,
        schema_count,
        recent_node_ids,
        errors,
    }
}

/// On-disk size of the database at `path`.
///
/// libsql/SQLite stores the database as a single file (plus optional `-wal`
/// and `-shm` sidecars), so the common case is a file: size it and its
/// sidecars. A directory is also handled (recursively summed) so the helper
/// stays correct if the path ever points at a containing directory.
///
/// IO errors are accumulated into `errors` so a permissions issue surfaces
/// in the report rather than silently producing a zero byte count — that
/// failure mode is precisely what an operator running `nodespace
/// diagnostics` is usually trying to debug.
fn database_size(path: &Path, errors: &mut Vec<String>) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => directory_size(path, errors),
        Ok(meta) if meta.is_file() => {
            let mut total = meta.len();
            // Include the WAL/SHM sidecars libsql may write alongside the file.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                for suffix in ["-wal", "-shm"] {
                    let sidecar = path.with_file_name(format!("{name}{suffix}"));
                    if let Ok(sidecar_meta) = fs::symlink_metadata(&sidecar) {
                        if sidecar_meta.is_file() {
                            total += sidecar_meta.len();
                        }
                    }
                }
            }
            total
        }
        Ok(_) => 0, // symlink or other special type at the top level: skip
        Err(e) => {
            errors.push(format!("stat {} failed: {e}", path.display()));
            0
        }
    }
}

/// Recursive directory size. Symlinks are intentionally not followed —
/// `DirEntry::file_type` uses `lstat`, so a symlinked directory inside the
/// database directory cannot trigger an infinite descent. This also means
/// symlinked entries don't contribute to the total; do not "fix" this back to
/// `fs::metadata()` (which follows symlinks) without reintroducing loop protection.
fn directory_size(path: &Path, errors: &mut Vec<String>) -> u64 {
    let mut total = 0u64;
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(format!("read_dir {} failed: {e}", path.display()));
            return 0;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                errors.push(format!("dir entry under {} failed: {e}", path.display()));
                continue;
            }
        };
        let entry_path: PathBuf = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                errors.push(format!("file_type {} failed: {e}", entry_path.display()));
                continue;
            }
        };

        if file_type.is_file() {
            match entry.metadata() {
                Ok(meta) => total += meta.len(),
                Err(e) => errors.push(format!("metadata {} failed: {e}", entry_path.display())),
            }
        } else if file_type.is_dir() {
            total += directory_size(&entry_path, errors);
        }
        // Symlinks and other special types: intentionally skipped (see above).
    }
    total
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn print_human(r: &DiagnosticsReport) {
    println!("NodeSpace Diagnostics");
    println!("─────────────────────────────────────");
    if r.databases.is_empty() {
        println!("Registered databases: (none)");
    } else {
        println!("Registered databases:");
        for d in &r.databases {
            let marker = if d.is_default { "*" } else { " " };
            println!("  {marker} {} [{}] {} — {}", d.name, d.status, d.id, d.path);
        }
    }
    println!();
    println!("Targeted database: {}", r.targeted_database_id);
    println!("Targeted path:     {}", r.targeted_database_path);
    match r.database_size_bytes {
        Some(bytes) => println!("Database size:     {}", format_size(bytes)),
        None => println!("Database size:     n/a"),
    }
    println!("Total nodes:       {}", r.total_node_count);
    println!("Root nodes:        {}", r.root_node_count);
    println!("Schemas:           {}", r.schema_count);
    if r.recent_node_ids.is_empty() {
        println!("Recent node IDs:   (none)");
    } else {
        println!("Recent node IDs:   {}", r.recent_node_ids.join(", "));
    }
    if !r.errors.is_empty() {
        println!();
        println!("Errors:");
        for err in &r.errors {
            println!("  - {err}");
        }
    }
}

fn print_json(r: &DiagnosticsReport) -> Result<()> {
    let value = json!({
        "databases": r.databases.iter().map(|d| json!({
            "id": d.id,
            "name": d.name,
            "path": d.path,
            "is_default": d.is_default,
            "status": d.status,
        })).collect::<Vec<_>>(),
        "targeted_database_id": r.targeted_database_id,
        "targeted_database_path": r.targeted_database_path,
        "database_size_bytes": r.database_size_bytes,
        "total_node_count": r.total_node_count,
        "root_node_count": r.root_node_count,
        "schema_count": r.schema_count,
        "recent_node_ids": r.recent_node_ids,
        "errors": r.errors,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_picks_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn directory_size_handles_missing_path() {
        let p = Path::new("/nonexistent/path/for/diagnostics/test");
        let mut errors = Vec::new();
        assert_eq!(directory_size(p, &mut errors), 0);
        assert_eq!(
            errors.len(),
            1,
            "missing path should surface a read_dir error"
        );
        assert!(errors[0].contains("read_dir"));
    }

    #[test]
    fn directory_size_sums_nested_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("a.txt"), vec![0u8; 100]).expect("write a");
        let nested = tmp.path().join("nested");
        fs::create_dir(&nested).expect("create nested");
        fs::write(nested.join("b.bin"), vec![0u8; 250]).expect("write b");

        let mut errors = Vec::new();
        let total = directory_size(tmp.path(), &mut errors);
        assert_eq!(total, 350, "should sum files across nested dirs");
        assert!(
            errors.is_empty(),
            "happy path must not produce errors: {errors:?}"
        );
    }
}
