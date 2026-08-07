-- Human-readable reference of the CURRENT schema (all migrations applied).
-- NOT executed directly — the database is built by running the numbered
-- migrations in db/migrations/ in order. Update this file alongside any new
-- migration so it stays an accurate snapshot; never apply it as-is to a DB.
--
-- The PRAGMAs below are set on every connection in
-- SqliteStore::apply_connection_pragmas (per-connection session settings, not
-- migrated schema state) — shown here for a complete picture of DB behavior.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;

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

-- Partial expression indexes on the hot task/project properties (agent's
-- equality/range/sort queries). Each covers only rows of its `node_type`,
-- matching QueryService's json_extract(properties, '$.<type>.<field>') filters.
CREATE INDEX IF NOT EXISTS idx_task_status ON node (json_extract(properties, '$.task.status')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_due_date ON node (json_extract(properties, '$.task.due_date')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_priority ON node (json_extract(properties, '$.task.priority')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_task_assignee ON node (json_extract(properties, '$.task.assignee')) WHERE node_type = 'task';
-- Composite index serving "open tasks ordered by due date" without a filesort.
CREATE INDEX IF NOT EXISTS idx_task_status_due_date ON node (json_extract(properties, '$.task.status'), json_extract(properties, '$.task.due_date')) WHERE node_type = 'task';
CREATE INDEX IF NOT EXISTS idx_project_status ON node (json_extract(properties, '$.project.status')) WHERE node_type = 'project';

-- Holds BOTH instance-level edges (task→person assigned_to, has_child, …) and
-- schema relationship DECLARATIONS (v004): a declaration row connects two
-- schema nodes (in_node = declaring schema, out_node = target schema, or a
-- self-edge when untyped) under the declared name, with the full
-- SchemaRelationship JSON in `properties`. The two kinds share
-- relationship_type values and are distinguished by endpoint node_type
-- ('schema' vs instance), never by name — which is why declared names may not
-- collide with the built-in structural types.
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
    -- Provenance (#182/#183): 'local' = generated on this device, 'remote' =
    -- pulled from another device via cloud sync. The cloud-push sweep reads only
    -- 'local' rows, so a pulled vector never gets re-pushed (no cross-device
    -- re-push loop / write amplification).
    origin       TEXT    NOT NULL DEFAULT 'local',
    created_at   TEXT    NOT NULL,
    modified_at  TEXT    NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_emb_node      ON embedding (node_id);
CREATE INDEX IF NOT EXISTS idx_emb_stale_mod ON embedding (stale, modified_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_emb_unique ON embedding (node_id, model_name, chunk_index);
-- Cloud-push sweep (#97/#182): SqliteStore::embeddings_modified_since does an
-- `origin = 'local' AND modified_at >= ?` range scan ORDER BY modified_at,
-- node_id, chunk_index. Leading on `origin` (equality) then `modified_at`
-- (range) makes the recurring sweep an index range scan that also covers the
-- ORDER BY (no filesort).
CREATE INDEX IF NOT EXISTS idx_emb_modified ON embedding (origin, modified_at, node_id, chunk_index);
