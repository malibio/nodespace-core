//! Partial expression indexes for the hot `task`/`project` properties the agent
//! filters and sorts on. `node.properties` is a JSON blob with no index on
//! individual values, so `QueryService`'s `json_extract(properties, '$.<type>.
//! <field>')` filters seek the `node_type` partition and then evaluate
//! `json_extract` per row (plus a filesort for `ORDER BY`). These are `WHERE
//! node_type='<type>'` partial indexes — cheap to maintain (only rows of that
//! type are indexed) and directly matching the query shape.
//!
//! `idx_task_status_due_date` is a composite index serving "open tasks ordered
//! by due date" (`status = ?` equality + `due_date` range/sort) without a
//! filesort; `idx_task_due_date` remains for filters that sort/range on
//! `due_date` without also filtering on `status`.
//!
//! `idx_task_status` is redundant with `idx_task_status_due_date` for reads —
//! SQLite's leftmost-prefix rule means the composite index already serves a
//! `status`-only filter. It's kept anyway: a single-column index is cheaper to
//! maintain (smaller B-tree, no `due_date` payload) for the common
//! status-only-filter case, and dropping it would require confirming no query
//! path relies on it being chosen over the composite for cost reasons.

use anyhow::{Context, Result};

const INDEX_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_task_status ON node (json_extract(properties, '$.task.status')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_due_date ON node (json_extract(properties, '$.task.due_date')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_priority ON node (json_extract(properties, '$.task.priority')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_status_due_date ON node (json_extract(properties, '$.task.status'), json_extract(properties, '$.task.due_date')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_project_status ON node (json_extract(properties, '$.project.status')) WHERE node_type = 'project';
"#;

pub async fn apply(tx: &libsql::Transaction) -> Result<()> {
    for stmt in INDEX_SQL.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        tx.execute(stmt, ())
            .await
            .with_context(|| format!("Failed to create index: {}", &stmt[..stmt.len().min(80)]))?;
    }

    // Refreshes planner stats so the query planner picks these new expression
    // indexes immediately instead of waiting for organic table churn to
    // trigger SQLite's automatic ANALYZE.
    tx.execute("ANALYZE", ())
        .await
        .context("Failed to ANALYZE after creating property indexes")?;

    Ok(())
}
