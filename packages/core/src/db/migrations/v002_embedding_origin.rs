//! Adds `embedding.origin` (#182/#183): 'local' = generated on this device,
//! 'remote' = pulled from another device via cloud sync. The cloud-push sweep
//! reads only 'local' rows, so a pulled vector never gets re-pushed (no
//! cross-device re-push loop / write amplification). Rebuilds `idx_emb_modified`
//! with `origin` leading so the filtered push sweep stays an index range scan.

use anyhow::{Context, Result};

pub async fn apply(tx: &libsql::Transaction) -> Result<()> {
    tx.execute(
        "ALTER TABLE embedding ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'",
        (),
    )
    .await
    .context("add embedding.origin column")?;

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
