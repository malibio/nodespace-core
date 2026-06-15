PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;

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
-- Cloud-push sweep (#97): SqliteStore::embeddings_modified_since does a
-- stale-agnostic `modified_at >= ?` range scan ORDER BY modified_at, node_id,
-- chunk_index. idx_emb_stale_mod can't serve it (leading column is `stale`), so
-- this composite makes the recurring sweep an index range scan that also covers
-- the ORDER BY (no filesort).
CREATE INDEX IF NOT EXISTS idx_emb_modified ON embedding (modified_at, node_id, chunk_index);
