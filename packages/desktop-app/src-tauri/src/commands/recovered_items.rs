//! Pro-tier "Recovered Items" commands (core#1303 viewer/restore half).
//!
//! When a cloud last-writer-wins conflict overwrites a genuine local edit, the
//! Pro daemon snapshots the superseded content to a per-user **local-only** log
//! `~/.nodespace/recovered-items-<user>.jsonl` (see `nodespaced-pro`'s
//! `record_recovered_item`). These commands read and maintain that log so the
//! desktop UI can surface the lost edit and let the user restore it.
//!
//! All commands no-op (empty list / `Ok`) in community mode — there is no
//! `ProClient` in managed state and no daemon writing the log — so the
//! frontend's Recovered-Items UI stays completely inert in the community build.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::services::ProClient;

/// One superseded local edit, mirroring the JSONL the daemon writes
/// (`record_recovered_item`). Field names match the on-disk snake_case keys for
/// both read and rewrite, so a dismiss never changes the format the daemon keeps
/// appending to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveredItem {
    pub node_id: String,
    pub superseded_content: String,
    pub superseded_modified_at: String,
    pub winning_content: String,
    pub winning_modified_at: String,
    pub recovered_at: String,
}

/// Resolve the recovery-log path exactly as the daemon does:
/// `~/.nodespace/recovered-items-<user>.jsonl`, user from `NODESPACED_PRO_USER_ID`
/// (default `"default"` — what the bundled desktop daemon runs with).
fn recovery_log_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let user = std::env::var("NODESPACED_PRO_USER_ID").unwrap_or_else(|_| "default".to_string());
    Some(
        PathBuf::from(home)
            .join(".nodespace")
            .join(format!("recovered-items-{user}.jsonl")),
    )
}

/// Parse the log into items (oldest first, as appended). Missing file → empty.
/// Malformed lines are skipped — a corrupt entry never breaks the viewer.
fn read_items(path: &PathBuf) -> Vec<RecoveredItem> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<RecoveredItem>(l).ok())
        .collect()
}

/// Rewrite the log to exactly `items`, preserving the daemon's one-object-per-line
/// snake_case format. Empty list removes the file.
fn rewrite(path: &PathBuf, items: &[RecoveredItem]) -> Result<(), String> {
    if items.is_empty() {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| format!("clear recovered items: {e}"))?;
        }
        return Ok(());
    }
    let mut body = String::new();
    for it in items {
        let line =
            serde_json::to_string(it).map_err(|e| format!("serialize recovered item: {e}"))?;
        body.push_str(&line);
        body.push('\n');
    }
    std::fs::write(path, body).map_err(|e| format!("write recovered items: {e}"))
}

/// List the current user's recovered items. Empty in community mode.
#[tauri::command]
pub async fn pro_list_recovered_items(app: AppHandle) -> Result<Vec<RecoveredItem>, String> {
    if app.try_state::<ProClient>().is_none() {
        return Ok(Vec::new());
    }
    let Some(path) = recovery_log_path() else {
        return Ok(Vec::new());
    };
    Ok(read_items(&path))
}

/// Drop every recovered entry for `node_id` (after a restore, or a single dismiss).
/// No-op in community mode or when the log is absent.
///
/// NOTE (known race): this read→filter→rewrite is not atomic against the daemon,
/// which also appends to this file. If the daemon records a fresh recovered item
/// between our read and rewrite, that entry is clobbered. Low-probability and the
/// only loss is a recovery breadcrumb (the node content itself lives in the store
/// and cloud); a proper fix is daemon-mediated mutation (or file locking), tracked
/// as a follow-up. Acceptable for the viewer/restore slice (core#1303).
#[tauri::command]
pub async fn pro_dismiss_recovered_item(app: AppHandle, node_id: String) -> Result<(), String> {
    if app.try_state::<ProClient>().is_none() {
        return Ok(());
    }
    let Some(path) = recovery_log_path() else {
        return Ok(());
    };
    let remaining: Vec<RecoveredItem> = read_items(&path)
        .into_iter()
        .filter(|i| i.node_id != node_id)
        .collect();
    rewrite(&path, &remaining)
}

/// Clear the entire recovered-items log. No-op in community mode or when absent.
#[tauri::command]
pub async fn pro_clear_recovered_items(app: AppHandle) -> Result<(), String> {
    if app.try_state::<ProClient>().is_none() {
        return Ok(());
    }
    let Some(path) = recovery_log_path() else {
        return Ok(());
    };
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("clear recovered items: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line exactly as the Pro daemon's `record_recovered_item` writes it
    /// (snake_case keys via `serde_json::json!`).
    fn daemon_line(node_id: &str, superseded: &str) -> String {
        serde_json::json!({
            "node_id": node_id,
            "superseded_content": superseded,
            "superseded_modified_at": "2026-06-16T00:00:00+00:00",
            "winning_content": "winner",
            "winning_modified_at": "2026-06-16T01:00:00+00:00",
            "recovered_at": "2026-06-16T02:00:00+00:00",
        })
        .to_string()
    }

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ns-recovered-test-{name}-{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn reads_daemon_written_lines_and_skips_blanks_and_garbage() {
        let path = tmp_path("read");
        let body = format!(
            "{}\n\n{}\nnot json\n",
            daemon_line("n1", "edit-1"),
            daemon_line("n2", "edit-2")
        );
        std::fs::write(&path, body).unwrap();

        let items = read_items(&path);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].node_id, "n1");
        assert_eq!(items[0].superseded_content, "edit-1");
        assert_eq!(items[1].node_id, "n2");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_reads_empty() {
        assert!(read_items(&tmp_path("missing")).is_empty());
    }

    #[test]
    fn dismiss_drops_only_matching_node_and_keeps_daemon_format() {
        let path = tmp_path("dismiss");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                daemon_line("keep", "k"),
                daemon_line("drop", "d")
            ),
        )
        .unwrap();

        let remaining: Vec<RecoveredItem> = read_items(&path)
            .into_iter()
            .filter(|i| i.node_id != "drop")
            .collect();
        rewrite(&path, &remaining).unwrap();

        // Re-read: only the kept node survives, still parseable (daemon-compatible).
        let after = read_items(&path);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].node_id, "keep");

        // The rewritten line must carry the snake_case keys the daemon appends with.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"node_id\""));
        assert!(raw.contains("\"superseded_content\""));
        assert!(!raw.contains("\"nodeId\""));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rewrite_empty_removes_file() {
        let path = tmp_path("empty");
        std::fs::write(&path, format!("{}\n", daemon_line("n", "x"))).unwrap();
        rewrite(&path, &[]).unwrap();
        assert!(!path.exists());
    }
}
