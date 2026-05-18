//! `nodespace diagnostics` — print a developer-facing summary of the
//! daemon's database state. Mirrors the now-deleted Tauri
//! `get_database_diagnostics` command but lives in the CLI because the
//! intended audience (developers debugging persistence) uses the shell, not
//! the desktop UI.

use anyhow::{Context, Result};
use clap::Args;
use nodespace_daemon::nodespace::{GetAllSchemasRequest, GetRootsRequest, QueryNodesSimpleRequest};
use nodespace_daemon::{resolve_db_path, NodeServiceClient};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tonic::transport::Channel;

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

#[derive(Debug)]
pub struct DiagnosticsReport {
    pub database_path: String,
    pub database_exists: bool,
    pub database_size_bytes: Option<u64>,
    pub total_node_count: usize,
    pub root_node_count: usize,
    pub schema_count: i32,
    pub recent_node_ids: Vec<String>,
    pub errors: Vec<String>,
}

pub async fn run(
    client: &mut NodeServiceClient<Channel>,
    _args: DiagnosticsArgs,
    json_output: bool,
) -> Result<()> {
    let db_path = resolve_db_path().context("resolve daemon database path")?;
    let report = collect(client, &db_path).await;
    if json_output {
        print_json(&report)
    } else {
        print_human(&report);
        Ok(())
    }
}

/// Build a diagnostics report against the given `db_path`. Split out from
/// `run` so integration tests can point at a tempdir-backed daemon without
/// the env-var dance `resolve_db_path()` requires.
pub async fn collect(client: &mut NodeServiceClient<Channel>, db_path: &Path) -> DiagnosticsReport {
    let mut errors: Vec<String> = Vec::new();

    let database_path = db_path.to_string_lossy().to_string();
    let database_exists = db_path.exists();
    let database_size_bytes = if database_exists {
        Some(directory_size(db_path, &mut errors))
    } else {
        None
    };

    // Pull all nodes once (bounded by QUERY_LIMIT) to compute total count
    // and surface the most-recently-created IDs from a single snapshot.
    let mut all_nodes = match client
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

    let root_node_count = match client
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

    let schema_count = match client.get_all_schemas(GetAllSchemasRequest {}).await {
        Ok(response) => response.into_inner().count,
        Err(e) => {
            errors.push(format!("GetAllSchemas failed: {e}"));
            0
        }
    };

    DiagnosticsReport {
        database_path,
        database_exists,
        database_size_bytes,
        total_node_count,
        root_node_count,
        schema_count,
        recent_node_ids,
        errors,
    }
}

/// Recursive directory size. Symlinks are intentionally not followed —
/// `DirEntry::file_type` uses `lstat`, so a symlinked directory inside the
/// RocksDB tree cannot trigger an infinite descent. This also means
/// symlinked entries don't contribute to the total, which is fine because
/// RocksDB stores no symlinks; do not "fix" this back to `fs::metadata()`
/// (which follows symlinks) without reintroducing loop protection.
///
/// IO errors are accumulated into `errors` so a permissions issue surfaces
/// in the report rather than silently producing a zero byte count — that
/// failure mode is precisely what an operator running `nodespace
/// diagnostics` is usually trying to debug.
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
    println!("Database path:   {}", r.database_path);
    println!(
        "Database exists: {}",
        if r.database_exists { "yes" } else { "no" }
    );
    match r.database_size_bytes {
        Some(bytes) => println!("Database size:   {}", format_size(bytes)),
        None => println!("Database size:   n/a"),
    }
    println!("Total nodes:     {}", r.total_node_count);
    println!("Root nodes:      {}", r.root_node_count);
    println!("Schemas:         {}", r.schema_count);
    if r.recent_node_ids.is_empty() {
        println!("Recent node IDs: (none)");
    } else {
        println!("Recent node IDs: {}", r.recent_node_ids.join(", "));
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
        "database_path": r.database_path,
        "database_exists": r.database_exists,
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
