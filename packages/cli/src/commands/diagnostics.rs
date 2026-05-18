//! `nodespace diagnostics` — print a developer-facing summary of the
//! daemon's database state. Mirrors the now-deleted Tauri
//! `get_database_diagnostics` command but lives in the CLI because the
//! intended audience (developers debugging persistence) uses the shell, not
//! the desktop UI.

use anyhow::{Context, Result};
use clap::Args;
use nodespace_daemon::nodespace::{GetAllSchemasRequest, QueryNodesSimpleRequest};
use nodespace_daemon::{resolve_db_path, NodeServiceClient};
use serde_json::json;
use std::fs;
use std::path::Path;
use tonic::transport::Channel;

/// Upper bound on the per-query fetch when counting nodes. Diagnostics is a
/// developer tool, not a hot path — keeping this generous avoids surprise
/// truncation in any realistic dev database while still bounding memory.
const QUERY_LIMIT: u32 = 100_000;

/// How many recent node IDs to report.
const RECENT_LIMIT: usize = 10;

#[derive(Args, Debug)]
pub struct DiagnosticsArgs {}

#[derive(Debug)]
struct DiagnosticsReport {
    database_path: String,
    database_exists: bool,
    database_size_bytes: Option<u64>,
    total_node_count: usize,
    root_node_count: usize,
    schema_count: i32,
    recent_node_ids: Vec<String>,
    errors: Vec<String>,
}

pub async fn run(
    client: &mut NodeServiceClient<Channel>,
    _args: DiagnosticsArgs,
    json_output: bool,
) -> Result<()> {
    let report = collect(client).await?;
    if json_output {
        print_json(&report)
    } else {
        print_human(&report);
        Ok(())
    }
}

async fn collect(client: &mut NodeServiceClient<Channel>) -> Result<DiagnosticsReport> {
    let mut errors: Vec<String> = Vec::new();

    let db_path = resolve_db_path().context("resolve daemon database path")?;
    let database_path = db_path.to_string_lossy().to_string();
    let database_exists = db_path.exists();
    let database_size_bytes = if database_exists {
        Some(directory_size(&db_path))
    } else {
        None
    };

    // Pull all nodes once (bounded by QUERY_LIMIT). We compute total count
    // and root count from the same batch so the two figures are consistent
    // with each other; if a node is created mid-query, both numbers reflect
    // the same snapshot.
    let all_nodes = match client
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

    let total_node_count = all_nodes.len();
    let root_node_count = all_nodes
        .iter()
        .filter(|n| n.parent_id.as_deref().unwrap_or("").is_empty())
        .count();

    // QueryNodesSimple doesn't expose a sort order; without an `ORDER BY
    // created_at DESC` we report the first `RECENT_LIMIT` IDs from the
    // result set rather than fabricating recency. Matches the original
    // Tauri behavior ("For now just get some node IDs").
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

    Ok(DiagnosticsReport {
        database_path,
        database_exists,
        database_size_bytes,
        total_node_count,
        root_node_count,
        schema_count,
        recent_node_ids,
        errors,
    })
}

fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            match entry.file_type() {
                Ok(ft) if ft.is_file() => {
                    if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                }
                Ok(ft) if ft.is_dir() => {
                    total += directory_size(&entry_path);
                }
                _ => {}
            }
        }
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
        assert_eq!(directory_size(p), 0);
    }
}
