//! Adds `embedding.origin`: 'local' = generated on this device,
//! 'remote' = pulled from another device via cloud sync. The cloud-push sweep
//! reads only 'local' rows, so a pulled vector never gets re-pushed (no
//! cross-device re-push loop / write amplification). Rebuilds `idx_emb_modified`
//! with `origin` leading so the filtered push sweep stays an index range scan.
//!
//! Column-existence check: this migration shipped, as an ad-hoc idempotent
//! check outside `user_version` tracking, before the migration runner existed.
//! A database that already applied it under the old code has `origin` present
//! but `user_version` still at 0 (never set), so this migration must tolerate
//! running against a DB where its own effect already happened.

use anyhow::{Context, Result};

async fn has_origin_column(tx: &libsql::Transaction) -> Result<bool> {
    let mut cols = tx
        .query("PRAGMA table_info(embedding)", ())
        .await
        .context("read embedding table_info")?;
    while let Some(row) = cols.next().await? {
        let name: String = row.get(1)?;
        if name == "origin" {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn apply(tx: &libsql::Transaction) -> Result<()> {
    if !has_origin_column(tx).await? {
        tx.execute(
            "ALTER TABLE embedding ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'",
            (),
        )
        .await
        .context("add embedding.origin column")?;
    }

    tx.execute("DROP INDEX IF EXISTS idx_emb_modified", ())
        .await
        .context("drop legacy idx_emb_modified")?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_emb_modified ON embedding (origin, modified_at, node_id, chunk_index)",
        (),
    )
    .await
    .context("rebuild idx_emb_modified with origin")?;

    Ok(())
}
