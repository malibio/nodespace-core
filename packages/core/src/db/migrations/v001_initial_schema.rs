//! Baseline schema: `node`, `relationship`, `embedding` tables, their indexes, the
//! `node_fts` full-text index (and its sync triggers), and the `vec_embeddings`
//! vector index. This is the schema every NodeSpace database started life with,
//! canonicalized as migration 1.
//!
//! Connection-level PRAGMAs (`journal_mode`, `foreign_keys`, `synchronous`,
//! `busy_timeout`) are NOT part of this migration — SQLite forbids changing
//! `synchronous` inside a transaction, and `foreign_keys`/`synchronous`/
//! `busy_timeout` are per-connection session settings, not persisted schema
//! state, so they're set unconditionally on every connection open in
//! `SqliteStore::new`, independent of migration state.
//!
//! Frozen as of this migration's creation — do not edit to match later schema
//! changes (those are later-numbered migrations, e.g. `v002_embedding_origin`
//! adds `embedding.origin`, which this baseline intentionally omits). See
//! `db/schema.sql` for a human-readable reference of the CURRENT schema (after
//! all migrations), not this baseline.

use anyhow::{Context, Result};

const BASELINE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS node (
    id               TEXT    PRIMARY KEY,
    node_type        TEXT    NOT NULL,
    content          TEXT    NOT NULL DEFAULT '',
    properties       TEXT    NOT NULL DEFAULT '{}',
    title            TEXT,
    lifecycle_status TEXT    NOT NULL DEFAULT 'active',
    version          INTEGER NOT NULL DEFAULT 1,
    sync_seq         INTEGER,
    created_at       TEXT    NOT NULL,
    modified_at      TEXT    NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_node_type      ON node (node_type);
CREATE INDEX IF NOT EXISTS idx_node_modified  ON node (modified_at);
CREATE INDEX IF NOT EXISTS idx_node_lifecycle ON node (lifecycle_status);

CREATE TABLE IF NOT EXISTS relationship (
    id                TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    in_node           TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    out_node          TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    relationship_type TEXT    NOT NULL,
    properties        TEXT    NOT NULL DEFAULT '{}',
    version           INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT    NOT NULL,
    modified_at       TEXT    NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_rel_type  ON relationship (relationship_type);
CREATE INDEX IF NOT EXISTS idx_rel_in    ON relationship (in_node, relationship_type);
CREATE INDEX IF NOT EXISTS idx_rel_out   ON relationship (out_node, relationship_type);
CREATE UNIQUE INDEX IF NOT EXISTS idx_rel_unique ON relationship (in_node, out_node, relationship_type);

CREATE TABLE IF NOT EXISTS embedding (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    node_id      TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    vector       BLOB    NOT NULL,
    dimension    INTEGER NOT NULL DEFAULT 768,
    model_name   TEXT    NOT NULL DEFAULT 'nomic-embed-text-v1.5',
    chunk_index  INTEGER NOT NULL DEFAULT 0,
    chunk_start  INTEGER NOT NULL DEFAULT 0,
    chunk_end    INTEGER,
    total_chunks INTEGER NOT NULL DEFAULT 1,
    content_hash TEXT,
    token_count  INTEGER,
    stale        INTEGER NOT NULL DEFAULT 1,
    error_count  INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT,
    created_at   TEXT    NOT NULL,
    modified_at  TEXT    NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_emb_node      ON embedding (node_id);
CREATE INDEX IF NOT EXISTS idx_emb_stale_mod ON embedding (stale, modified_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_emb_unique ON embedding (node_id, model_name, chunk_index);
CREATE INDEX IF NOT EXISTS idx_emb_modified ON embedding (modified_at, node_id, chunk_index);
"#;

pub async fn apply(tx: &libsql::Transaction) -> Result<()> {
    // Naive `;`-splitting is safe here ONLY because BASELINE_SQL is plain
    // CREATE TABLE/INDEX statements with no semicolons inside string literals or
    // multi-statement trigger bodies (those are added separately below via
    // individual `tx.execute` calls). Do not extend BASELINE_SQL with triggers or
    // other multi-statement DDL without switching to a real statement splitter.
    for stmt in BASELINE_SQL.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        tx.execute(stmt, ())
            .await
            .with_context(|| format!("Failed to execute DDL: {}", &stmt[..stmt.len().min(80)]))?;
    }

    // FTS5 virtual table for BM25 full-text search
    tx.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS node_fts USING fts5(id UNINDEXED, content, content='node', content_rowid='rowid')",
        ()
    ).await.context("Failed to create FTS5 table")?;

    tx.execute(
        r#"CREATE TRIGGER IF NOT EXISTS node_fts_insert AFTER INSERT ON node BEGIN
            INSERT INTO node_fts(rowid, id, content) VALUES (new.rowid, new.id, new.content);
        END"#,
        (),
    )
    .await
    .context("Failed to create FTS5 insert trigger")?;

    tx.execute(
        r#"CREATE TRIGGER IF NOT EXISTS node_fts_update AFTER UPDATE ON node BEGIN
            INSERT INTO node_fts(node_fts, rowid, id, content) VALUES('delete', old.rowid, old.id, old.content);
            INSERT INTO node_fts(rowid, id, content) VALUES (new.rowid, new.id, new.content);
        END"#,
        ()
    ).await.context("Failed to create FTS5 update trigger")?;

    tx.execute(
        r#"CREATE TRIGGER IF NOT EXISTS node_fts_delete AFTER DELETE ON node BEGIN
            INSERT INTO node_fts(node_fts, rowid, id, content) VALUES('delete', old.rowid, old.id, old.content);
        END"#,
        ()
    ).await.context("Failed to create FTS5 delete trigger")?;

    // sqlite-vec virtual table for embedding KNN search. Keyed by `embedding.id`
    // (the per-chunk UUID); holds ONLY real, non-stale vectors (see upsert/delete/
    // mark-stale paths). vec0 is a fast brute-force SIMD scan, not an ANN index.
    tx.execute(
        &format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(\
                embedding_id TEXT PRIMARY KEY, \
                vector FLOAT[{}] distance_metric=cosine\
            )",
            crate::models::embedding::DEFAULT_EMBEDDING_DIMENSION
        ),
        (),
    )
    .await
    .context("Failed to create vec0 embeddings table")?;

    Ok(())
}
