use crate::db::fractional_ordering::FractionalOrderCalculator;
use crate::models::{DeleteResult, Node, NodeQuery, NodeUpdate};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Normalise a stored date string to YYYY-MM-DD on read.
/// Accepts YYYY-MM-DD (pass-through) or RFC 3339 (extract date portion).
/// Returns the original string for unrecognised values so callers can surface them.
fn normalize_date_field(s: &str) -> String {
    if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
        return s.to_string();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.format("%Y-%m-%d").to_string();
    }
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return dt.format("%Y-%m-%d").to_string();
    }
    s.to_string()
}

const DOMAIN_EVENT_CHANNEL_CAPACITY: usize = 128;
/// KNN over-fetch factor: vec0 returns top-k *chunks*, but search results are grouped
/// by node and many chunks map to one node, so we fetch `limit * this` to cover enough
/// distinct nodes.
const EMBEDDING_KNN_OVERFETCH: i64 = 10;
const BM25_MAX_TOKENS: usize = 4;
const BM25_STOP_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can",
    "need", "dare", "ought", "i", "me", "my", "we", "our", "you", "your", "he", "she", "it",
    "they", "them", "their", "what", "which", "who", "whom", "this", "that", "these", "those",
    "to", "of", "in", "on", "at", "by", "for", "with", "about", "as", "how", "when", "where",
    "why",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationshipRecord {
    pub id: String,
    #[serde(rename = "in")]
    pub in_node: String,
    pub out_node: String,
    pub relationship_type: String,
    #[serde(default)]
    pub properties: Value,
}

impl RelationshipRecord {
    pub fn order(&self) -> f64 {
        self.properties
            .get("order")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOperation {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct StoreChange {
    pub operation: StoreOperation,
    pub node: Node,
    pub source: Option<String>,
    pub previous_node: Option<Node>,
    pub playbook_context: Option<crate::db::events::PlaybookExecutionContext>,
}

pub type StoreNotifier = Arc<dyn Fn(StoreChange) + Send + Sync>;

/// Register the statically-linked `sqlite-vec` extension as a SQLite auto-extension so
/// every connection opened afterwards has the `vec0` virtual-table module available.
/// Runs exactly once per process and must complete before any real store connection is
/// opened — `SqliteStore::new` awaits it first.
///
/// Ordering is critical: libsql lazily calls `sqlite3_config(SQLITE_CONFIG_SERIALIZED)`
/// on its first `connect()` (via a process-global `Once`), and `sqlite3_config` fails
/// once SQLite has been initialized. Registering an auto-extension auto-initializes
/// SQLite, so we open (and drop) a throwaway in-memory libsql connection FIRST to run
/// libsql's config, THEN register. Doing this in a single async `OnceCell` keeps the two
/// steps atomic so concurrent `new()` calls can't interleave the warm-up and the
/// registration.
async fn ensure_sqlite_vec_registered() {
    /// The libsql FFI signature for a SQLite extension entry point.
    type EntryPoint = unsafe extern "C" fn(
        *mut libsql::ffi::sqlite3,
        *mut *const std::os::raw::c_char,
        *const libsql::ffi::sqlite3_api_routines,
    ) -> std::os::raw::c_int;

    static VEC_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    VEC_INIT
        .get_or_init(|| async {
            // Warm up libsql so its one-time sqlite3_config(SERIALIZED) runs before we
            // auto-initialize SQLite via sqlite3_auto_extension.
            if let Ok(db) = libsql::Builder::new_local(":memory:").build().await {
                let _ = db.connect();
            }
            // SAFETY: the `sqlite-vec` crate declares `sqlite3_vec_init` as a zero-arg
            // `extern "C"` fn, but its real C entry point has the 3-arg SQLite signature
            // `EntryPoint`. We transmute via `*const ()` to that true signature — the same
            // pattern the crate's own rusqlite example uses — and hand it to
            // `sqlite3_auto_extension`, which expects exactly that type.
            unsafe {
                let entry: EntryPoint =
                    std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
                libsql::ffi::sqlite3_auto_extension(Some(entry));
            }
        })
        .await;
}

pub struct SqliteStore {
    db: Arc<libsql::Connection>,
    event_tx: broadcast::Sender<crate::db::events::EventEnvelope>,
    valid_node_types: HashSet<String>,
    notifier: Option<StoreNotifier>,
}

impl SqliteStore {
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        // Register sqlite-vec (once per process) BEFORE opening our connection, so the
        // connection picks up the `vec0` module. See `ensure_sqlite_vec_registered`.
        ensure_sqlite_vec_registered().await;

        let database = libsql::Builder::new_local(&db_path)
            .build()
            .await
            .context("Failed to build libsql database")?;
        let conn = database
            .connect()
            .context("Failed to connect to libsql database")?;

        Self::initialize_schema(&conn).await?;

        let valid_node_types = Self::build_schema_caches(&conn).await?;
        let (event_tx, _) = broadcast::channel(DOMAIN_EVENT_CHANNEL_CAPACITY);

        Ok(Self {
            db: Arc::new(conn),
            event_tx,
            valid_node_types,
            notifier: None,
        })
    }

    async fn initialize_schema(conn: &libsql::Connection) -> Result<()> {
        let schema_sql = include_str!("schema.sql");
        for stmt in schema_sql.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            // PRAGMAs that return rows (e.g. journal_mode) must be queried, not executed
            if stmt.to_uppercase().starts_with("PRAGMA") && stmt.contains('=') {
                conn.query(stmt, ()).await.with_context(|| {
                    format!("Failed to execute PRAGMA: {}", &stmt[..stmt.len().min(80)])
                })?;
            } else {
                conn.execute(stmt, ()).await.with_context(|| {
                    format!("Failed to execute DDL: {}", &stmt[..stmt.len().min(80)])
                })?;
            }
        }

        // FTS5 virtual table for BM25 full-text search
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS node_fts USING fts5(id UNINDEXED, content, content='node', content_rowid='rowid')",
            ()
        ).await.context("Failed to create FTS5 table")?;

        conn.execute(
            r#"CREATE TRIGGER IF NOT EXISTS node_fts_insert AFTER INSERT ON node BEGIN
                INSERT INTO node_fts(rowid, id, content) VALUES (new.rowid, new.id, new.content);
            END"#,
            (),
        )
        .await
        .context("Failed to create FTS5 insert trigger")?;

        conn.execute(
            r#"CREATE TRIGGER IF NOT EXISTS node_fts_update AFTER UPDATE ON node BEGIN
                INSERT INTO node_fts(node_fts, rowid, id, content) VALUES('delete', old.rowid, old.id, old.content);
                INSERT INTO node_fts(rowid, id, content) VALUES (new.rowid, new.id, new.content);
            END"#,
            ()
        ).await.context("Failed to create FTS5 update trigger")?;

        conn.execute(
            r#"CREATE TRIGGER IF NOT EXISTS node_fts_delete AFTER DELETE ON node BEGIN
                INSERT INTO node_fts(node_fts, rowid, id, content) VALUES('delete', old.rowid, old.id, old.content);
            END"#,
            ()
        ).await.context("Failed to create FTS5 delete trigger")?;

        Self::backfill_fts_if_stale(conn).await?;

        // sqlite-vec virtual table for embedding KNN search. Keyed by `embedding.id`
        // (the per-chunk UUID); holds ONLY real, non-stale vectors (see upsert/delete/
        // mark-stale paths). vec0 is a fast brute-force SIMD scan, not an ANN index.
        conn.execute(
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

        Self::migrate_embedding_origin(conn).await?;

        Ok(())
    }

    /// Idempotent DDL migration for the embedding `origin` column (#182/#183).
    ///
    /// `schema.sql` is applied with `CREATE TABLE/INDEX IF NOT EXISTS`, which
    /// CANNOT alter a pre-existing `embedding` table — and `packages/core` ships
    /// in the community desktop app, whose user DBs are never reset (lazy
    /// node-migration design). On such a DB the new column + reshaped index would
    /// be silently absent, so every embedding write and the push sweep's
    /// `WHERE origin = 'local'` would fail with `no such column: origin`. Add the
    /// column and rebuild the index here when missing. No-op on fresh DBs (the
    /// column already exists) and on re-runs.
    async fn migrate_embedding_origin(conn: &libsql::Connection) -> Result<()> {
        let mut cols = conn
            .query("PRAGMA table_info(embedding)", ())
            .await
            .context("read embedding table_info")?;
        let mut has_origin = false;
        while let Some(row) = cols.next().await? {
            // table_info columns: (cid, name, type, notnull, dflt_value, pk)
            let name: String = row.get(1)?;
            if name == "origin" {
                has_origin = true;
                break;
            }
        }
        if has_origin {
            return Ok(());
        }

        conn.execute(
            "ALTER TABLE embedding ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'",
            (),
        )
        .await
        .context("add embedding.origin column")?;
        // `idx_emb_modified` already exists under its old (originless) definition,
        // so CREATE INDEX IF NOT EXISTS in schema.sql was skipped — drop it and
        // rebuild with `origin` leading so the filtered push sweep stays covered.
        conn.execute("DROP INDEX IF EXISTS idx_emb_modified", ())
            .await
            .context("drop legacy idx_emb_modified")?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_emb_modified ON embedding (origin, modified_at, node_id, chunk_index)",
            (),
        )
        .await
        .context("rebuild idx_emb_modified with origin")?;
        Ok(())
    }

    /// One-time FTS5 backfill (#1428). The external-content `node_fts` triggers
    /// only index FUTURE writes, so any node predating the FTS table (user DBs are
    /// never reset — same reason `migrate_embedding_origin` exists) is absent from
    /// the index and never returned by `bm25_search_roots`. Rebuild the index from
    /// `node`, but ONLY when it is out of sync, so a healthy DB does not re-index
    /// its whole corpus on every startup.
    ///
    /// The staleness signal is the count of ACTUALLY-INDEXED documents, read from
    /// FTS5's `node_fts_docsize` shadow table — NOT `count(*) FROM node_fts`, which
    /// for an external-content table reads rowids from the content table (`node`)
    /// and so always equals the node count regardless of index population. When the
    /// indexed-doc count differs from `node`, rebuild. No-op on a fresh DB (both 0)
    /// and after the first rebuild.
    async fn backfill_fts_if_stale(conn: &libsql::Connection) -> Result<()> {
        let count = |sql: &'static str| async move {
            let mut r = conn.query(sql, ()).await?;
            let n: i64 = r
                .next()
                .await?
                .map(|row| row.get(0))
                .transpose()?
                .unwrap_or(0);
            Ok::<i64, anyhow::Error>(n)
        };
        let indexed = count("SELECT count(*) FROM node_fts_docsize")
            .await
            .context("Failed to count indexed FTS docs")?;
        let node_count = count("SELECT count(*) FROM node")
            .await
            .context("Failed to count node rows")?;
        if indexed != node_count {
            conn.execute("INSERT INTO node_fts(node_fts) VALUES('rebuild')", ())
                .await
                .context("Failed to backfill FTS5 index")?;
        }
        Ok(())
    }

    async fn build_schema_caches(conn: &libsql::Connection) -> Result<HashSet<String>> {
        let mut rows = conn
            .query("SELECT id FROM node WHERE node_type = 'schema'", ())
            .await
            .context("Failed to query schema nodes for cache")?;

        let mut types = HashSet::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            types.insert(id);
        }
        Ok(types)
    }

    pub fn set_notifier(&mut self, notifier: StoreNotifier) {
        self.notifier = Some(notifier);
    }

    fn notify(&self, change: StoreChange) {
        if let Some(notifier) = &self.notifier {
            notifier(change);
        }
    }

    pub fn subscribe_to_events(&self) -> broadcast::Receiver<crate::db::events::EventEnvelope> {
        self.event_tx.subscribe()
    }

    fn validate_node_type(&self, node_type: &str) -> Result<()> {
        if node_type.is_empty() {
            return Err(anyhow::anyhow!("Node type cannot be empty"));
        }
        if self.valid_node_types.contains(node_type) {
            return Ok(());
        }
        // Allow schema node type always
        if node_type == "schema" {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "Invalid node type '{}'. Valid types: {:?}",
            node_type,
            self.valid_node_types
        ))
    }

    pub(crate) fn add_to_schema_cache(&mut self, type_name: String) {
        self.valid_node_types.insert(type_name);
    }

    pub fn close(&self) -> Result<()> {
        Ok(())
    }

    fn row_to_node(row: &libsql::Row) -> Result<Node> {
        let id: String = row.get(0)?;
        let node_type: String = row.get(1)?;
        let content: String = row.get(2)?;
        let properties_str: String = row.get(3)?;
        let title: Option<String> = row.get(4)?;
        let lifecycle_status: String = row.get(5)?;
        let version: i64 = row.get(6)?;
        // col 7 = sync_seq (ignored)
        let created_at_str: String = row.get(8)?;
        let modified_at_str: String = row.get(9)?;

        let properties: Value =
            serde_json::from_str(&properties_str).unwrap_or(serde_json::json!({}));
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .with_context(|| format!("Invalid created_at timestamp: {}", created_at_str))?
            .with_timezone(&Utc);
        let modified_at = DateTime::parse_from_rfc3339(&modified_at_str)
            .with_context(|| format!("Invalid modified_at timestamp: {}", modified_at_str))?
            .with_timezone(&Utc);

        Ok(Node {
            id,
            node_type,
            content,
            version,
            created_at,
            modified_at,
            properties,
            mentions: Vec::new(),
            mentioned_in: Vec::new(),
            title,
            lifecycle_status,
        })
    }

    fn row_to_relationship(row: &libsql::Row) -> Result<RelationshipRecord> {
        let id: String = row.get(0)?;
        let in_node: String = row.get(1)?;
        let out_node: String = row.get(2)?;
        let relationship_type: String = row.get(3)?;
        let props_str: String = row.get(4)?;
        let properties: Value = serde_json::from_str(&props_str).unwrap_or(serde_json::json!({}));
        Ok(RelationshipRecord {
            id,
            in_node,
            out_node,
            relationship_type,
            properties,
        })
    }

    async fn query_nodes_from_sql(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
    ) -> Result<Vec<Node>> {
        let mut rows = self
            .db
            .query(sql, params)
            .await
            .context("Failed to query nodes")?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }
}

impl SqliteStore {
    pub async fn create_node(
        &self,
        node: Node,
        source: Option<String>,
        playbook_context: Option<crate::db::events::PlaybookExecutionContext>,
    ) -> Result<Node> {
        if node.node_type == "collection" {
            if let Some(existing) = self.get_collection_by_name(&node.content).await? {
                anyhow::bail!(
                    "Collection with name '{}' already exists (id: {})",
                    node.content,
                    existing.id
                );
            }
        }

        let properties = if node.properties.is_null() {
            serde_json::json!({})
        } else {
            node.properties.clone()
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db
            .execute(
                "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                libsql::params![
                    node.id.clone(),
                    node.node_type.clone(),
                    node.content.clone(),
                    props_json,
                    node.title.clone(),
                    node.lifecycle_status.clone(),
                    node.version,
                    now.clone(),
                    now,
                ],
            )
            .await
            .context("Failed to create node")?;

        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context,
        });

        Ok(node)
    }

    pub async fn create_child_node_atomic(
        &self,
        parent_id: &str,
        node_type: &str,
        content: &str,
        properties: Value,
        source: Option<String>,
    ) -> Result<Node> {
        self.validate_node_type(node_type)?;

        let node_id = uuid::Uuid::new_v4().to_string();

        let parent_exists = self.get_node(parent_id).await?;
        if parent_exists.is_none() {
            return Err(anyhow::anyhow!("Parent node not found: {}", parent_id));
        }

        self.validate_no_cycle(parent_id, &node_id).await?;

        // Get last child order
        let mut rows = self.db.query(
            "SELECT json_extract(r.properties, '$.order') as ord FROM relationship r WHERE r.in_node = ?1 AND r.relationship_type = 'has_child' ORDER BY json_extract(r.properties, '$.order') DESC LIMIT 1",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get last child order")?;

        let last_order = if let Some(row) = rows.next().await? {
            row.get::<Option<f64>>(0)?.unwrap_or(0.0)
        } else {
            0.0
        };

        let new_order = if last_order > 0.0 {
            FractionalOrderCalculator::calculate_order(Some(last_order), None)
        } else {
            FractionalOrderCalculator::calculate_order(None, None)
        };

        let properties = if properties.is_null() {
            serde_json::json!({})
        } else {
            properties
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin transaction")?;

        tx.execute(
            "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, NULL, 'active', 1, ?5, ?6)",
            libsql::params![node_id.clone(), node_type.to_string(), content.to_string(), props_json, now.clone(), now.clone()],
        ).await.context("Failed to insert child node")?;

        let rel_id = uuid::Uuid::new_v4().to_string();
        let rel_props = serde_json::json!({"order": new_order}).to_string();
        tx.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
            libsql::params![rel_id, parent_id.to_string(), node_id.clone(), rel_props, now.clone(), now],
        ).await.context("Failed to insert parent-child relationship")?;

        tx.commit().await.context("Failed to commit transaction")?;

        let node = self
            .get_node(&node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after creation: {}", node_id))?;

        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(node)
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to query node")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(Self::row_to_node(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn node_exists(&self, id: &str) -> Result<bool> {
        let mut rows = self
            .db
            .query(
                "SELECT 1 FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to check node existence")?;
        Ok(rows.next().await?.is_some())
    }

    pub async fn get_nodes_by_ids(&self, ids: &[String]) -> Result<HashMap<String, Node>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT * FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );

        let params: Vec<libsql::Value> = ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to batch query nodes")?;

        let mut result = HashMap::new();
        while let Some(row) = rows.next().await? {
            let node = Self::row_to_node(&row)?;
            result.insert(node.id.clone(), node);
        }
        Ok(result)
    }

    pub async fn update_node(
        &self,
        id: &str,
        update: NodeUpdate,
        source: Option<String>,
    ) -> Result<Node> {
        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", id))?;

        let updated_content = update.content.unwrap_or(current.content);
        let updated_node_type = update.node_type.unwrap_or(current.node_type.clone());

        let properties_update = if let Some(ref updated_props) = update.properties {
            let mut merged = current.properties.as_object().cloned().unwrap_or_default();
            if let Some(new_props) = updated_props.as_object() {
                for (key, value) in new_props {
                    merged.insert(key.clone(), value.clone());
                }
            }
            Some(serde_json::Value::Object(merged))
        } else {
            None
        };

        let now = Utc::now().to_rfc3339();

        if let Some(ref props) = properties_update {
            let props_json =
                serde_json::to_string(props).context("Failed to serialize properties")?;
            self.db.execute(
                "UPDATE node SET content = ?1, node_type = ?2, properties = ?3, version = version + 1, modified_at = ?4 WHERE id = ?5",
                libsql::params![updated_content.clone(), updated_node_type.clone(), props_json, now.clone(), id.to_string()],
            ).await.context("Failed to update node")?;
        } else {
            self.db.execute(
                "UPDATE node SET content = ?1, node_type = ?2, version = version + 1, modified_at = ?3 WHERE id = ?4",
                libsql::params![updated_content.clone(), updated_node_type.clone(), now.clone(), id.to_string()],
            ).await.context("Failed to update node")?;
        }

        if let Some(title) = update.title {
            self.db
                .execute(
                    "UPDATE node SET title = ?1 WHERE id = ?2",
                    libsql::params![title, id.to_string()],
                )
                .await
                .context("Failed to update title")?;
        }

        if let Some(status) = update.lifecycle_status {
            self.db
                .execute(
                    "UPDATE node SET lifecycle_status = ?1 WHERE id = ?2",
                    libsql::params![status, id.to_string()],
                )
                .await
                .context("Failed to update lifecycle_status")?;
        }

        let updated_node = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after update"))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: updated_node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(updated_node)
    }

    pub async fn create_schema_node_atomic(
        &self,
        node: Node,
        _ddl_statements: Vec<String>,
        source: Option<String>,
    ) -> Result<Node> {
        if node.node_type != "schema" {
            return Err(anyhow::anyhow!(
                "create_schema_node_atomic only accepts schema nodes, got '{}'",
                node.node_type
            ));
        }
        // Legacy graph-DB DDL statements (DEFINE TABLE etc.) are not applicable to SQLite — ignore.
        self.create_node(node, source, None).await
    }

    pub async fn update_schema_node_atomic(
        &self,
        id: &str,
        update: NodeUpdate,
        _ddl_statements: Vec<String>,
        source: Option<String>,
    ) -> Result<Node> {
        // Legacy graph-DB DDL statements are not applicable to SQLite — ignore.
        self.update_node(id, update, source).await
    }

    pub async fn switch_node_type_atomic(
        &self,
        node_id: &str,
        new_type: &str,
        new_properties: Value,
        source: Option<String>,
    ) -> Result<Node> {
        self.validate_node_type(new_type)?;

        let new_properties = if new_properties.is_null() {
            serde_json::json!({})
        } else {
            new_properties
        };
        let props_json =
            serde_json::to_string(&new_properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db
            .execute(
                "UPDATE node SET node_type = ?1, properties = ?2, version = version + 1, modified_at = ?3 WHERE id = ?4",
                libsql::params![new_type.to_string(), props_json, now, node_id.to_string()],
            )
            .await
            .context("Failed to switch node type")?;

        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after type switch: {}", node_id))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: node.clone(),
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(node)
    }

    pub async fn update_node_with_version_check(
        &self,
        id: &str,
        expected_version: i64,
        update: NodeUpdate,
        source: Option<String>,
        playbook_context: Option<crate::db::events::PlaybookExecutionContext>,
    ) -> Result<Option<Node>> {
        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", id))?;

        let previous_node = current.clone();
        let new_version = expected_version + 1;

        let updated_content = update.content.unwrap_or(current.content);
        let updated_node_type = update.node_type.unwrap_or(current.node_type.clone());
        let updated_props = match update.properties {
            Some(p) => serde_json::to_string(&p).context("Failed to serialize properties")?,
            None => serde_json::to_string(&current.properties)
                .context("Failed to serialize current properties")?,
        };
        let updated_title = match update.title {
            Some(t) => t,          // Some(Some(x)) sets, Some(None) clears
            None => current.title, // None means no change
        };
        let updated_status = update
            .lifecycle_status
            .unwrap_or(current.lifecycle_status.clone());
        let now = Utc::now().to_rfc3339();

        let rows_affected = self.db.execute(
            "UPDATE node SET content = ?1, node_type = ?2, properties = ?3, title = ?4, lifecycle_status = ?5, version = ?6, modified_at = ?7 WHERE id = ?8 AND version = ?9",
            libsql::params![updated_content, updated_node_type, updated_props, updated_title, updated_status, new_version, now, id.to_string(), expected_version],
        ).await.context("Failed to update node with version check")?;

        if rows_affected == 0 {
            return Ok(None);
        }

        let node = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found after update"))?;

        self.notify(StoreChange {
            operation: StoreOperation::Updated,
            node: node.clone(),
            source,
            previous_node: Some(previous_node),
            playbook_context,
        });

        Ok(Some(node))
    }

    pub async fn update_lifecycle_status(&self, id: &str, status: &str) -> Result<()> {
        self.db
            .execute(
                "UPDATE node SET lifecycle_status = ?1 WHERE id = ?2",
                libsql::params![status.to_string(), id.to_string()],
            )
            .await
            .context("Failed to update lifecycle_status")?;
        Ok(())
    }

    pub async fn delete_node(&self, id: &str, source: Option<String>) -> Result<DeleteResult> {
        let node = match self.get_node(id).await? {
            Some(n) => n,
            None => {
                return Ok(DeleteResult {
                    existed: false,
                    deleted_count: 0,
                })
            }
        };

        // FK CASCADE handles relationship and embedding deletion
        self.db
            .execute(
                "DELETE FROM node WHERE id = ?1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to delete node")?;

        self.notify(StoreChange {
            operation: StoreOperation::Deleted,
            node,
            source,
            previous_node: None,
            playbook_context: None,
        });

        Ok(DeleteResult {
            existed: true,
            deleted_count: 1,
        })
    }

    /// Atomically delete a node and its entire `has_child` subtree in a single transaction.
    ///
    /// **OCC contract:** Version-checks only the target node at `expected_version`. Descendants
    /// are removed unconditionally inside the same transaction. A target version mismatch rolls
    /// back the entire transaction — no partial deletion is possible.
    ///
    /// **Event emission:** Emits one `NodeDeleted` notification per deleted node (target +
    /// every descendant) after the commit, so the frontend store reconciles each removal.
    ///
    /// Returns `(existed, deleted_nodes)` where `deleted_nodes` contains the target and all
    /// descendants that were deleted (empty vec when target didn't exist).
    pub async fn delete_subtree_atomic(
        &self,
        node_id: &str,
        expected_version: i64,
        source: Option<String>,
    ) -> Result<(bool, Vec<Node>)> {
        // Collect the target + all descendants before mutating.
        let target = match self.get_node(node_id).await? {
            Some(n) => n,
            None => return Ok((false, vec![])),
        };

        // OCC check on target before entering the transaction.
        if target.version != expected_version {
            return Err(anyhow::anyhow!(
                "version_conflict:{}:{}:{}",
                node_id,
                expected_version,
                target.version
            ));
        }

        // Walk has_child recursively to collect the full descendant set.
        let mut id_rows = self.db.query(
            r#"WITH RECURSIVE subtree(node_id) AS (
                SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node FROM relationship r
                JOIN subtree s ON r.in_node = s.node_id
                WHERE r.relationship_type = 'has_child'
            )
            SELECT DISTINCT node_id FROM subtree"#,
            libsql::params![node_id.to_string()],
        ).await.context("Failed to collect subtree descendants")?;

        let mut all_ids: Vec<String> = vec![node_id.to_string()];
        while let Some(row) = id_rows.next().await? {
            all_ids.push(row.get(0)?);
        }

        // Fetch all node records (needed for post-commit notifications).
        let placeholders: Vec<String> = (1..=all_ids.len()).map(|i| format!("?{}", i)).collect();
        let fetch_sql = format!(
            "SELECT * FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let fetch_params: Vec<libsql::Value> = all_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut node_rows = self
            .db
            .query(&fetch_sql, fetch_params)
            .await
            .context("Failed to fetch subtree nodes for deletion")?;
        let mut nodes_to_delete: Vec<Node> = Vec::new();
        while let Some(row) = node_rows.next().await? {
            nodes_to_delete.push(Self::row_to_node(&row)?);
        }

        // Single-transaction delete — FK CASCADE cleans relationship and embedding rows.
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin subtree delete transaction")?;

        // Re-check target version inside the transaction to close the TOCTOU window.
        let check_sql = "SELECT version FROM node WHERE id = ?1";
        let mut check_rows = tx
            .query(check_sql, libsql::params![node_id.to_string()])
            .await
            .context("Failed to re-check target version")?;
        let actual_version: i64 = match check_rows.next().await? {
            Some(row) => row.get(0)?,
            None => {
                // Node disappeared between pre-check and transaction — idempotent.
                return Ok((false, vec![]));
            }
        };
        if actual_version != expected_version {
            return Err(anyhow::anyhow!(
                "version_conflict:{}:{}:{}",
                node_id,
                expected_version,
                actual_version
            ));
        }

        let del_placeholders: Vec<String> =
            (1..=all_ids.len()).map(|i| format!("?{}", i)).collect();
        let del_sql = format!(
            "DELETE FROM node WHERE id IN ({})",
            del_placeholders.join(", ")
        );
        let del_params: Vec<libsql::Value> = all_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        tx.execute(&del_sql, del_params)
            .await
            .context("Failed to delete subtree nodes")?;

        tx.commit()
            .await
            .context("Failed to commit subtree delete transaction")?;

        // Emit one NodeDeleted notification per deleted node after commit.
        for node in &nodes_to_delete {
            self.notify(StoreChange {
                operation: StoreOperation::Deleted,
                node: node.clone(),
                source: source.clone(),
                previous_node: None,
                playbook_context: None,
            });
        }

        Ok((true, nodes_to_delete))
    }

    /// Delete all descendants of `parent_id` (the full child subtree) without touching the parent.
    ///
    /// Uses a recursive CTE to collect all descendant IDs, then deletes them in one statement.
    /// FK CASCADE cleans relationship and embedding rows automatically.
    /// No OCC check — intended for internal system operations where version checks are unnecessary.
    pub async fn delete_children_subtree_unchecked(&self, parent_id: &str) -> Result<()> {
        self.db
            .execute(
                r#"WITH RECURSIVE subtree(node_id) AS (
                    SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                    UNION ALL
                    SELECT r.out_node FROM relationship r
                    JOIN subtree s ON r.in_node = s.node_id
                    WHERE r.relationship_type = 'has_child'
                )
                DELETE FROM node WHERE id IN (SELECT DISTINCT node_id FROM subtree)"#,
                libsql::params![parent_id.to_string()],
            )
            .await
            .context("Failed to delete children subtree")?;
        Ok(())
    }

    /// Remove a single top-level key from a node's properties JSON in-place using `json_remove`.
    ///
    /// Used by migrations to clear legacy fields (e.g. `description`) after they have been
    /// moved to the child-subtree representation, making the migration idempotent.
    pub async fn remove_property_key(&self, node_id: &str, key: &str) -> Result<()> {
        let json_path = format!("$.{}", key);
        self.db
            .execute(
                "UPDATE node SET properties = json_remove(properties, ?1), modified_at = ?2 WHERE id = ?3",
                libsql::params![json_path, chrono::Utc::now().to_rfc3339(), node_id.to_string()],
            )
            .await
            .context("Failed to remove property key")?;
        Ok(())
    }

    pub async fn delete_with_version_check(
        &self,
        id: &str,
        expected_version: i64,
        source: Option<String>,
    ) -> Result<usize> {
        let node = match self.get_node(id).await? {
            Some(n) => n,
            None => return Ok(0),
        };

        if node.version != expected_version {
            return Ok(0);
        }

        let result = self.delete_node(id, source).await?;
        Ok(if result.existed { 1 } else { 0 })
    }

    pub async fn query_nodes(&self, query: NodeQuery) -> Result<Vec<Node>> {
        if let Some(ref mentioned_node_id) = query.mentioned_by {
            let mut rows = self.db.query(
                "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'mentions'",
                libsql::params![mentioned_node_id.clone()],
            ).await.context("Failed to query mentioned_by nodes")?;
            let mut nodes = Vec::new();
            while let Some(row) = rows.next().await? {
                nodes.push(Self::row_to_node(&row)?);
            }
            return Ok(nodes);
        }

        if let Some(ref search_q) = query.content_contains {
            let search_lower = format!("%{}%", search_q.to_lowercase());
            let sql = match (query.limit, query.offset) {
                (None, None) => "SELECT * FROM node WHERE LOWER(content) LIKE ?1".to_string(),
                (Some(l), None) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT {}",
                    l
                ),
                (None, Some(o)) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT -1 OFFSET {}",
                    o
                ),
                (Some(l), Some(o)) => format!(
                    "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT {} OFFSET {}",
                    l, o
                ),
            };
            return self
                .query_nodes_from_sql(&sql, libsql::params![search_lower])
                .await;
        }

        let mut conditions = Vec::new();
        let mut bind_values: Vec<libsql::Value> = Vec::new();
        let mut param_idx = 1usize;

        if let Some(ref search_q) = query.title_contains {
            let search_lower = format!("%{}%", search_q.to_lowercase());
            conditions.push(format!(
                "title IS NOT NULL AND LOWER(title) LIKE ?{}",
                param_idx
            ));
            bind_values.push(libsql::Value::Text(search_lower));
            param_idx += 1;
        }

        if let Some(ref nt) = query.node_type {
            conditions.push(format!("node_type = ?{}", param_idx));
            bind_values.push(libsql::Value::Text(nt.clone()));
        }

        let where_clause = if !conditions.is_empty() {
            format!("WHERE {}", conditions.join(" AND "))
        } else {
            String::new()
        };

        let limit_offset = match (query.limit, query.offset) {
            (None, None) => String::new(),
            (Some(l), None) => format!(" LIMIT {}", l),
            (None, Some(o)) => format!(" LIMIT -1 OFFSET {}", o),
            (Some(l), Some(o)) => format!(" LIMIT {} OFFSET {}", l, o),
        };

        let sql = format!("SELECT * FROM node {} {}", where_clause, limit_offset);
        let mut rows = self
            .db
            .query(&sql, bind_values)
            .await
            .context("Failed to query nodes")?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_children(&self, parent_id: &str) -> Result<Vec<Node>> {
        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.out_node = n.id WHERE r.in_node = ?1 AND r.relationship_type = 'has_child' ORDER BY json_extract(r.properties, '$.order') ASC",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get children")?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_roots(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Node>> {
        let limit_offset = match (limit, offset) {
            (None, None) => String::new(),
            (Some(l), None) => format!(" LIMIT {}", l),
            (None, Some(o)) => format!(" LIMIT -1 OFFSET {}", o),
            (Some(l), Some(o)) => format!(" LIMIT {} OFFSET {}", l, o),
        };

        let sql = format!(
            "SELECT * FROM node WHERE id NOT IN (SELECT out_node FROM relationship WHERE relationship_type = 'has_child') ORDER BY id ASC{}",
            limit_offset
        );

        self.query_nodes_from_sql(&sql, ()).await
    }

    pub async fn get_parent(&self, child_id: &str) -> Result<Option<Node>> {
        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'has_child' LIMIT 1",
            libsql::params![child_id.to_string()],
        ).await.context("Failed to get parent")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(Self::row_to_node(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_parent_id(&self, child_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child' LIMIT 1",
            libsql::params![child_id.to_string()],
        ).await.context("Failed to get parent id")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_node_type(&self, node_id: &str) -> Result<Option<String>> {
        let mut rows = self
            .db
            .query(
                "SELECT node_type FROM node WHERE id = ?1 LIMIT 1",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to get node type")?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_node_tree(&self, root_id: &str) -> Result<Option<serde_json::Value>> {
        let root_node = match self.get_node(root_id).await? {
            Some(node) => node,
            None => return Ok(None),
        };

        let tree = self.build_node_tree_recursive(&root_node).await?;
        Ok(Some(tree))
    }

    fn build_node_tree_recursive<'a>(
        &'a self,
        node: &'a Node,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        const MAX_DEPTH: usize = 100;
        Box::pin(async move {
            self.build_node_tree_with_guards(node, 0, MAX_DEPTH, &mut HashSet::new())
                .await
        })
    }

    fn build_node_tree_with_guards<'a>(
        &'a self,
        node: &'a Node,
        depth: usize,
        max_depth: usize,
        visited: &'a mut HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        Box::pin(async move {
            if depth >= max_depth {
                return Err(anyhow::anyhow!(
                    "Maximum tree depth ({}) exceeded at node '{}'",
                    max_depth,
                    node.id
                ));
            }
            if visited.contains(&node.id) {
                return Err(anyhow::anyhow!(
                    "Cycle detected: node '{}' appears multiple times",
                    node.id
                ));
            }
            visited.insert(node.id.clone());

            let children_nodes = self.get_children(&node.id).await?;
            let mut children_json = Vec::new();
            for child in &children_nodes {
                let child_tree = self
                    .build_node_tree_with_guards(child, depth + 1, max_depth, visited)
                    .await?;
                children_json.push(child_tree);
            }
            visited.remove(&node.id);

            Ok(serde_json::json!({
                "id": node.id,
                "type": node.node_type,
                "content": node.content,
                "version": node.version,
                "created_at": node.created_at,
                "modified_at": node.modified_at,
                "mentions": node.mentions,
                "mentionedIn": node.mentioned_in,
                "data": node.properties,
                "variants": serde_json::Value::Null,
                "_schema_version": 1,
                "children": children_json
            }))
        })
    }

    pub async fn get_nodes_in_subtree(&self, root_id: &str) -> Result<Vec<Node>> {
        let (all_nodes, _) = self.get_subtree_with_relationships(root_id).await?;
        Ok(all_nodes.into_iter().filter(|n| n.id != root_id).collect())
    }

    pub async fn get_subtree_with_relationships(
        &self,
        root_id: &str,
    ) -> Result<(Vec<Node>, Vec<RelationshipRecord>)> {
        let start = std::time::Instant::now();

        // WITH RECURSIVE to collect all descendants
        let mut id_rows = self.db.query(
            r#"WITH RECURSIVE subtree(node_id, depth) AS (
                SELECT out_node, 1 FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node, s.depth + 1 FROM relationship r
                JOIN subtree s ON r.in_node = s.node_id
                WHERE r.relationship_type = 'has_child' AND s.depth < 100
            )
            SELECT DISTINCT node_id FROM subtree"#,
            libsql::params![root_id.to_string()],
        ).await.context("Failed to query descendants")?;

        let mut descendant_ids = vec![root_id.to_string()];
        while let Some(row) = id_rows.next().await? {
            descendant_ids.push(row.get(0)?);
        }

        if descendant_ids.len() == 1 {
            // Just the root — verify it exists
            let root = match self.get_node(root_id).await? {
                Some(n) => n,
                None => return Ok((vec![], vec![])),
            };
            return Ok((vec![root], vec![]));
        }

        // Fetch all nodes
        let placeholders: Vec<String> = (1..=descendant_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let sql = format!(
            "SELECT * FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<libsql::Value> = descendant_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut node_rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to fetch subtree nodes")?;
        let mut all_nodes = Vec::new();
        while let Some(row) = node_rows.next().await? {
            all_nodes.push(Self::row_to_node(&row)?);
        }

        if all_nodes.is_empty() {
            return Ok((vec![], vec![]));
        }

        // Fetch relationships within subtree
        let rel_placeholders: Vec<String> = (1..=descendant_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let rel_sql = format!(
            "SELECT id, in_node, out_node, relationship_type, properties FROM relationship WHERE in_node IN ({}) AND relationship_type = 'has_child'",
            rel_placeholders.join(", ")
        );
        let rel_params: Vec<libsql::Value> = descendant_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut rel_rows = self
            .db
            .query(&rel_sql, rel_params)
            .await
            .context("Failed to fetch subtree relationships")?;
        let mut relationships = Vec::new();
        while let Some(row) = rel_rows.next().await? {
            relationships.push(Self::row_to_relationship(&row)?);
        }

        tracing::debug!(
            "get_subtree_with_relationships: {:?} for root_id={} ({} nodes)",
            start.elapsed(),
            root_id,
            all_nodes.len()
        );

        Ok((all_nodes, relationships))
    }

    pub async fn get_relationships_in_subtree(
        &self,
        root_id: &str,
    ) -> Result<Vec<RelationshipRecord>> {
        let (_, relationships) = self.get_subtree_with_relationships(root_id).await?;
        Ok(relationships)
    }

    pub async fn search_nodes_by_content(
        &self,
        search_query: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Node>> {
        let search_lower = format!("%{}%", search_query.to_lowercase());
        let sql = if let Some(l) = limit {
            format!(
                "SELECT * FROM node WHERE LOWER(content) LIKE ?1 LIMIT {}",
                l
            )
        } else {
            "SELECT * FROM node WHERE LOWER(content) LIKE ?1".to_string()
        };
        self.query_nodes_from_sql(&sql, libsql::params![search_lower])
            .await
    }

    pub async fn mention_autocomplete(
        &self,
        search_query: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Node>> {
        let effective_limit = limit.unwrap_or(10);
        let search_lower = format!("%{}%", search_query.to_lowercase());
        let sql = format!(
            "SELECT * FROM node WHERE title IS NOT NULL AND node_type != 'collection' AND LOWER(title) LIKE ?1 LIMIT {}",
            effective_limit
        );
        self.query_nodes_from_sql(&sql, libsql::params![search_lower])
            .await
    }

    async fn validate_no_cycle(&self, parent_id: &str, child_id: &str) -> Result<()> {
        // Check if parent_id is a descendant of child_id (would create cycle)
        let mut rows = self.db.query(
            r#"WITH RECURSIVE desc(node_id, depth) AS (
                SELECT out_node, 1 FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child'
                UNION ALL
                SELECT r.out_node, d.depth + 1 FROM relationship r
                JOIN desc d ON r.in_node = d.node_id
                WHERE r.relationship_type = 'has_child' AND d.depth < 100
            )
            SELECT node_id FROM desc WHERE node_id = ?2 LIMIT 1"#,
            libsql::params![child_id.to_string(), parent_id.to_string()],
        ).await.context("Failed to check for cycles")?;

        if rows.next().await?.is_some() {
            return Err(anyhow::anyhow!(
                "Cannot create parent-child relationship: would create cycle. \
                Node '{}' is a descendant of node '{}'.",
                parent_id,
                child_id
            ));
        }
        Ok(())
    }

    /// Cycle guard for collection-hierarchy `member_of` edges (#1427). The
    /// `has_child` tree has `validate_no_cycle`, but collection hierarchy is built
    /// from `member_of` (a sub-collection is a member_of its parent) and had no
    /// equivalent — so `a member_of b` + `b member_of a` produced a cycle in the
    /// supposed DAG, which (post-#1426) makes the recursive members walk loop.
    ///
    /// `member_of` stores in_node = child/member, out_node = parent/collection.
    /// Adding `source member_of target` makes `target` an ancestor of `source`;
    /// that is a cycle iff `target` is already a DESCENDANT of `source`. Walk the
    /// member_of subtree downward from `source` (out_node = current → in_node =
    /// child) and fail if it reaches `target`. A self-edge is trivially a cycle.
    pub async fn validate_no_member_of_cycle(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> Result<()> {
        if source_id == target_id {
            return Err(anyhow::anyhow!(
                "collection_cycle: '{}' cannot be a member of itself",
                source_id
            ));
        }
        let mut rows = self
            .db
            .query(
                r#"WITH RECURSIVE descendants(node_id, depth) AS (
                SELECT in_node, 1 FROM relationship
                  WHERE out_node = ?1 AND relationship_type = 'member_of'
                UNION ALL
                SELECT r.in_node, d.depth + 1 FROM relationship r
                JOIN descendants d ON r.out_node = d.node_id
                WHERE r.relationship_type = 'member_of' AND d.depth < 100
            )
            SELECT node_id FROM descendants WHERE node_id = ?2 LIMIT 1"#,
                libsql::params![source_id.to_string(), target_id.to_string()],
            )
            .await
            .context("Failed to check for member_of cycle")?;
        if rows.next().await?.is_some() {
            return Err(anyhow::anyhow!(
                "collection_cycle: '{}' is already a descendant of '{}', so making it the parent would create a cycle",
                target_id,
                source_id
            ));
        }
        Ok(())
    }

    async fn rebalance_children_for_parent(&self, parent_id: &str) -> Result<()> {
        let mut rows = self.db.query(
            "SELECT id, out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' ORDER BY json_extract(properties, '$.order') ASC",
            libsql::params![parent_id.to_string()],
        ).await.context("Failed to get children for rebalancing")?;

        let mut rels: Vec<(String, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            rels.push((row.get(0)?, row.get(1)?));
        }

        if rels.is_empty() {
            return Ok(());
        }

        let new_orders = FractionalOrderCalculator::rebalance(rels.len());
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin rebalance transaction")?;

        for (i, (rel_id, _)) in rels.iter().enumerate() {
            let props = serde_json::json!({"order": new_orders[i]}).to_string();
            tx.execute(
                "UPDATE relationship SET properties = ?1 WHERE id = ?2",
                libsql::params![props, rel_id.clone()],
            )
            .await
            .context("Failed to rebalance relationship")?;
        }

        tx.commit().await.context("Failed to commit rebalance")?;
        Ok(())
    }

    pub async fn move_node(
        &self,
        node_id: &str,
        new_parent_id: Option<&str>,
        insert_after_sibling_id: Option<&str>,
    ) -> Result<f64> {
        let node_id = node_id.to_string();
        let new_parent_id = new_parent_id.map(|s| s.to_string());
        let insert_after_sibling_id = insert_after_sibling_id.map(|s| s.to_string());

        if !self.node_exists(&node_id).await? {
            return Err(anyhow::anyhow!("Node not found: {}", node_id));
        }

        let current_parent_id = self.get_parent_id(&node_id).await?;
        let is_same_parent_reorder = match (&new_parent_id, &current_parent_id) {
            (Some(new_pid), Some(cur_pid)) => new_pid == cur_pid,
            (None, None) => true,
            _ => false,
        };

        if let Some(ref parent_id) = new_parent_id {
            if !self.node_exists(parent_id).await? {
                return Err(anyhow::anyhow!("Parent node not found: {}", parent_id));
            }
            self.validate_no_cycle(parent_id, &node_id).await?;
        }

        let new_order = if let Some(ref parent_id) = new_parent_id {
            // Get ordered siblings excluding the moving node
            let mut rows = self.db.query(
                "SELECT out_node, json_extract(properties, '$.order') as ord FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' AND out_node != ?2 ORDER BY json_extract(properties, '$.order') ASC",
                libsql::params![parent_id.clone(), node_id.clone()],
            ).await.context("Failed to get sibling relationships")?;

            let mut siblings: Vec<(String, f64)> = Vec::new();
            while let Some(row) = rows.next().await? {
                let child_id: String = row.get(0)?;
                let ord: Option<f64> = row.get(1)?;
                siblings.push((child_id, ord.unwrap_or(0.0)));
            }

            if let Some(after_id) = insert_after_sibling_id {
                if let Some(after_index) = siblings.iter().position(|(id, _)| id == &after_id) {
                    let prev_order = siblings[after_index].1;
                    let next_order = siblings.get(after_index + 1).map(|(_, o)| *o);

                    if let Some(next) = next_order {
                        if (next - prev_order) < 0.0001 {
                            self.rebalance_children_for_parent(parent_id).await?;
                            // Re-query after rebalancing
                            let mut rows2 = self.db.query(
                                "SELECT out_node, json_extract(properties, '$.order') as ord FROM relationship WHERE in_node = ?1 AND relationship_type = 'has_child' AND out_node != ?2 ORDER BY json_extract(properties, '$.order') ASC",
                                libsql::params![parent_id.clone(), node_id.clone()],
                            ).await.context("Failed to get siblings after rebalancing")?;
                            let mut siblings2: Vec<(String, f64)> = Vec::new();
                            while let Some(row) = rows2.next().await? {
                                let cid: String = row.get(0)?;
                                let ord: Option<f64> = row.get(1)?;
                                siblings2.push((cid, ord.unwrap_or(0.0)));
                            }
                            if let Some(after_index2) =
                                siblings2.iter().position(|(id, _)| id == &after_id)
                            {
                                let prev2 = siblings2[after_index2].1;
                                let next2 = siblings2.get(after_index2 + 1).map(|(_, o)| *o);
                                FractionalOrderCalculator::calculate_order(Some(prev2), next2)
                            } else {
                                let last = siblings2.last().map(|(_, o)| *o);
                                FractionalOrderCalculator::calculate_order(last, None)
                            }
                        } else {
                            FractionalOrderCalculator::calculate_order(Some(prev_order), next_order)
                        }
                    } else {
                        FractionalOrderCalculator::calculate_order(Some(prev_order), None)
                    }
                } else {
                    let last = siblings.last().map(|(_, o)| *o);
                    FractionalOrderCalculator::calculate_order(last, None)
                }
            } else {
                let first = siblings.first().map(|(_, o)| *o);
                FractionalOrderCalculator::calculate_order(None, first)
            }
        } else {
            0.0
        };

        let now = Utc::now().to_rfc3339();

        if let Some(ref parent_id) = new_parent_id {
            if is_same_parent_reorder {
                let props = serde_json::json!({"order": new_order}).to_string();
                self.db.execute(
                    "UPDATE relationship SET properties = ?1, version = version + 1, modified_at = ?2 WHERE in_node = ?3 AND out_node = ?4 AND relationship_type = 'has_child'",
                    libsql::params![props, now, parent_id.clone(), node_id.clone()],
                ).await.context("Failed to update relationship order")?;
            } else {
                // Cross-parent move: delete the old has_child edge, create the new
                // one. These MUST be one transaction — if the INSERT fails
                // (constraint / IO / crash / cancel) after the DELETE committed, the
                // node is left with NO has_child edge: a silently-orphaned root.
                // Wrapping in a tx makes it all-or-nothing, matching the atomicity
                // of `move_children_to_parent` / `delete_subtree_atomic`.
                let rel_id = uuid::Uuid::new_v4().to_string();
                let props = serde_json::json!({"order": new_order}).to_string();
                let tx = self
                    .db
                    .transaction()
                    .await
                    .context("Failed to begin move_node reparent transaction")?;
                tx.execute(
                    "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'",
                    libsql::params![node_id.clone()],
                ).await.context("Failed to delete old parent relationship")?;
                tx.execute(
                    "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                    libsql::params![rel_id, parent_id.clone(), node_id.clone(), props, now.clone(), now],
                ).await.context("Failed to create new parent relationship")?;
                tx.commit()
                    .await
                    .context("Failed to commit move_node reparent transaction")?;
            }
        } else {
            // Make root: delete parent relationship
            self.db.execute(
                "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'",
                libsql::params![node_id.clone()],
            ).await.context("Failed to delete parent relationship")?;
        }

        Ok(new_order)
    }

    /// Re-parent an ordered set of existing children to `new_parent_id` in a
    /// single transaction. Validates each child's version inside the transaction
    /// using `SELECT changes()` after a version-gated DELETE — any mismatch causes
    /// the full transaction to roll back (all-or-nothing OCC).
    ///
    /// Returns the assigned fractional order for each child, preserving input
    /// array order as sibling order under the new parent.
    pub async fn move_children_to_parent(
        &self,
        new_parent_id: &str,
        children: &[(&str, i64)],
    ) -> Result<Vec<f64>> {
        if children.is_empty() {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();

        // Compute sequential fractional orders iteratively so each step uses the
        // previously assigned order as its `prev`, producing a properly spaced
        // sequence (e.g. [1.0, 2.0, 3.0]) rather than arbitrary constants.
        let mut orders: Vec<f64> = Vec::with_capacity(children.len());
        for _ in 0..children.len() {
            let prev = orders.last().copied();
            orders.push(FractionalOrderCalculator::calculate_order(prev, None));
        }

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin move_children_to_parent transaction")?;

        for ((child_id, expected_version), &order) in children.iter().zip(orders.iter()) {
            let child_id = child_id.to_string();

            // Delete the old has_child edge only if the node's version matches.
            // If no rows are affected the version has been bumped by a concurrent
            // writer — detect that with SELECT changes() and abort the transaction.
            tx.execute(
                "DELETE FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child' AND EXISTS (SELECT 1 FROM node WHERE id = ?1 AND version = ?2)",
                libsql::params![child_id.clone(), *expected_version],
            )
            .await
            .context("Failed to delete old has_child edge")?;

            // Verify the DELETE actually removed a row (i.e. the version matched).
            let mut changes_rows = tx
                .query("SELECT changes()", libsql::params![])
                .await
                .context("Failed to query changes()")?;
            if let Some(row) = changes_rows.next().await? {
                let affected: i64 = row.get(0)?;
                if affected == 0 {
                    return Err(anyhow::anyhow!(
                        "VERSION_CONFLICT: node '{}' version mismatch (expected {})",
                        child_id,
                        expected_version
                    ));
                }
            }

            // Insert new has_child edge under new_parent_id.
            let rel_id = uuid::Uuid::new_v4().to_string();
            let rel_props = serde_json::json!({"order": order}).to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                libsql::params![
                    rel_id,
                    new_parent_id.to_string(),
                    child_id,
                    rel_props,
                    now.clone(),
                    now.clone()
                ],
            )
            .await
            .context("Failed to insert new has_child edge")?;
        }

        tx.commit()
            .await
            .context("Failed to commit move_children_to_parent")?;

        Ok(orders)
    }

    pub async fn create_mention(&self, source_id: &str, target_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to check for existing mention")?;

        if rows.next().await?.is_some() {
            return Ok(None);
        }

        let rel_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.db.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'mentions', '{}', 1, ?4, ?5)",
            libsql::params![rel_id.clone(), source_id.to_string(), target_id.to_string(), now.clone(), now],
        ).await.context("Failed to create mention")?;

        Ok(Some(rel_id))
    }

    pub async fn delete_mention(&self, source_id: &str, target_id: &str) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to get mention id")?;

        let existing_id: Option<String> = if let Some(row) = rows.next().await? {
            Some(row.get(0)?)
        } else {
            None
        };

        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions'",
            libsql::params![source_id.to_string(), target_id.to_string()],
        ).await.context("Failed to delete mention")?;

        Ok(existing_id.map(|id| format!("relationship:{}", id)))
    }

    pub async fn get_outgoing_mentions(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = match self.db.query(
            "SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to query outgoing mentions for {}: {}", node_id, e);
                return Ok(Vec::new());
            }
        };

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn get_incoming_mentions(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get incoming mentions")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn get_incoming_mention_containers(
        &self,
        node_id: &str,
    ) -> Result<Vec<crate::models::NodeReference>> {
        let start = std::time::Instant::now();

        // Get all nodes that mention this node
        let mut rows = self.db.query(
            "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'mentions'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get mentioning sources")?;

        let mut source_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            source_ids.push(row.get(0)?);
        }

        if source_ids.is_empty() {
            return Ok(Vec::new());
        }

        // For each source, determine its container (task=itself, else walk up to root)
        let mut container_ids: HashSet<String> = HashSet::new();

        for source_id in &source_ids {
            let node_type = self.get_node_type(source_id).await?;
            if node_type.as_deref() == Some("task") {
                container_ids.insert(source_id.clone());
            } else {
                // Walk up to root using recursive CTE
                let mut root_rows = self.db.query(
                    r#"WITH RECURSIVE ancestors(node_id, depth) AS (
                        SELECT in_node, 1 FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'
                        UNION ALL
                        SELECT r.in_node, a.depth + 1 FROM relationship r
                        JOIN ancestors a ON r.out_node = a.node_id
                        WHERE r.relationship_type = 'has_child' AND a.depth < 100
                    )
                    SELECT node_id FROM ancestors ORDER BY depth DESC LIMIT 1"#,
                    libsql::params![source_id.clone()],
                ).await.context("Failed to get ancestor chain")?;

                if let Some(row) = root_rows.next().await? {
                    container_ids.insert(row.get(0)?);
                } else {
                    // Source is already a root
                    container_ids.insert(source_id.clone());
                }
            }
        }

        if container_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Batch fetch containers
        let placeholders: Vec<String> = (1..=container_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        let sql = format!(
            "SELECT id, title, node_type FROM node WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<libsql::Value> = container_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        let mut container_rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to fetch containers")?;

        let mut result = Vec::new();
        while let Some(row) = container_rows.next().await? {
            result.push(crate::models::NodeReference {
                id: row.get(0)?,
                title: row.get(1)?,
                node_type: row.get(2)?,
            });
        }

        tracing::debug!(
            "get_incoming_mention_containers: {} containers in {:?} for node_id={}",
            result.len(),
            start.elapsed(),
            node_id
        );

        Ok(result)
    }

    pub async fn get_schema(&self, node_type: &str) -> Result<Option<Value>> {
        let node = self.get_node(node_type).await?;
        Ok(node.map(|n| n.properties))
    }

    pub async fn update_schema(&self, node_type: &str, schema: &Value) -> Result<()> {
        let schema_id = node_type.to_string();

        if self.get_node(&schema_id).await?.is_some() {
            let update = NodeUpdate {
                properties: Some(schema.clone()),
                ..Default::default()
            };
            self.update_node(&schema_id, update, None).await?;
        } else {
            let node = Node::new_with_id(
                schema_id,
                "schema".to_string(),
                node_type.to_string(),
                schema.clone(),
            );
            self.create_node(node, None, None).await?;
        }

        Ok(())
    }

    pub async fn rename_schema_field(&self, type_id: &str, from: &str, to: &str) -> Result<u64> {
        if from.is_empty() || to.is_empty() {
            return Err(anyhow::anyhow!("Field names must not be empty"));
        }
        if from == to {
            return Err(anyhow::anyhow!(
                "Source and destination field names are the same: '{}'",
                from
            ));
        }

        let mut rows = self
            .db
            .query(
                "SELECT id, properties FROM node WHERE node_type = ?1",
                libsql::params![type_id.to_string()],
            )
            .await
            .context("Failed to fetch nodes for field rename")?;

        let mut nodes: Vec<(String, Value)> = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let props_str: String = row.get(1)?;
            let props: Value = serde_json::from_str(&props_str).unwrap_or(serde_json::json!({}));
            nodes.push((id, props));
        }

        let mut affected = 0u64;
        let now = Utc::now().to_rfc3339();

        for (node_id, mut properties) in nodes {
            let had_field = if let Some(ns_obj) = properties
                .as_object_mut()
                .and_then(|p| p.get_mut(type_id))
                .and_then(|ns| ns.as_object_mut())
            {
                if let Some(value) = ns_obj.remove(from) {
                    ns_obj.insert(to.to_string(), value);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if had_field {
                let props_json =
                    serde_json::to_string(&properties).context("Failed to serialize properties")?;
                self.db
                    .execute(
                        "UPDATE node SET properties = ?1, modified_at = ?2 WHERE id = ?3",
                        libsql::params![props_json, now.clone(), node_id],
                    )
                    .await
                    .context("Failed to update node during field rename")?;
                affected += 1;
            }
        }

        tracing::info!(
            type_id = %type_id,
            from = %from,
            to = %to,
            affected = affected,
            "rename_schema_field: migrated {} node(s)",
            affected
        );

        Ok(affected)
    }

    /// Atomically update a batch of nodes in a single transaction.
    ///
    /// **OCC contract:** This is an intentional last-write-wins fast-path for trusted internal
    /// callers (e.g. AI skill pipelines, markdown import). It does NOT perform version-checked
    /// OCC. Callers that need conflict detection should use `update_node` (the single-node path)
    /// in a loop or use the daemon `update_nodes_batch` RPC which threads per-node versions.
    ///
    /// **TOCTOU note:** All fields are written without a pre-read. Unspecified fields
    /// (`None`) are preserved via SQL `COALESCE` — the entire read-modify-write is one
    /// atomic SQL statement per node inside the transaction with no pool reads escaping
    /// the tx.
    ///
    /// **`properties` semantics:** `Some(value)` **replaces** the existing JSON object
    /// entirely (unlike `update_node`, which merges key-by-key). `None` keeps the existing
    /// value unchanged.
    ///
    /// **`title` semantics:** `Some(Some("text"))` sets a new title; `Some(None)` clears
    /// it to NULL; `None` leaves the existing title unchanged.
    ///
    /// Returns an error (and rolls back) if any node id is not found.
    pub async fn bulk_update(&self, updates: Vec<(String, NodeUpdate)>) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        const MAX_BATCH_SIZE: usize = 1000;
        if updates.len() > MAX_BATCH_SIZE {
            return Err(anyhow::anyhow!(
                "Bulk update batch size ({}) exceeds maximum ({})",
                updates.len(),
                MAX_BATCH_SIZE
            ));
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk update transaction")?;

        for (id, update) in &updates {
            let props_json = update
                .properties
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .context("Failed to serialize properties in bulk update")?;

            // All five NodeUpdate fields are covered. COALESCE preserves the existing
            // column value when the caller passes None, closing the TOCTOU window with
            // no pre-read escaping the transaction.
            let affected = tx
                .execute(
                    "UPDATE node SET \
                    content          = COALESCE(?1, content), \
                    node_type        = COALESCE(?2, node_type), \
                    properties       = COALESCE(?3, properties), \
                    lifecycle_status = COALESCE(?4, lifecycle_status), \
                    version          = version + 1, \
                    modified_at      = ?5 \
                WHERE id = ?6",
                    libsql::params![
                        update.content.clone(),
                        update.node_type.clone(),
                        props_json,
                        update.lifecycle_status.clone(),
                        now.clone(),
                        id.clone()
                    ],
                )
                .await
                .context("Failed to update node in bulk update")?;

            if affected == 0 {
                return Err(anyhow::anyhow!("Node not found: {}", id));
            }

            // `title` uses Option<Option<String>>: Some(Some(t)) sets, Some(None) clears
            // to NULL, None skips. COALESCE can't express "write NULL intentionally", so
            // we handle title with a separate statement only when the caller touches it.
            if let Some(title) = &update.title {
                tx.execute(
                    "UPDATE node SET title = ?1 WHERE id = ?2",
                    libsql::params![title.clone(), id.clone()],
                )
                .await
                .context("Failed to update title in bulk update")?;
            }
        }

        tx.commit().await.context("Failed to commit bulk update")?;
        Ok(())
    }

    pub async fn batch_create_nodes(&self, nodes: Vec<Node>) -> Result<Vec<Node>> {
        let mut created = Vec::new();
        for node in nodes {
            created.push(self.create_node(node, None, None).await?);
        }
        Ok(created)
    }

    /// Insert a batch of nodes (and optional parent→child relationships) in a single transaction.
    ///
    /// **Partial-failure contract:** All inserts occur inside one transaction. Any error
    /// (duplicate id, constraint violation, serialization failure) triggers an early `?`-return,
    /// which drops `tx` without calling `commit()` — the entire batch is rolled back atomically.
    /// Callers never observe a partial write.
    ///
    /// **Validation note:** `validate_node_type` is a pure in-memory check against
    /// `self.valid_node_types`; it does not touch the database and therefore cannot read
    /// uncommitted state from `tx`.
    pub async fn bulk_create_hierarchy(
        &self,
        nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )>,
    ) -> Result<Vec<String>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk hierarchy transaction")?;

        for (id, node_type, content, parent_id, order, properties) in &nodes {
            self.validate_node_type(node_type)?;

            let properties = if properties.is_null() {
                serde_json::json!({})
            } else {
                properties.clone()
            };
            let props_json =
                serde_json::to_string(&properties).context("Failed to serialize properties")?;

            let title =
                Self::compute_title_for_bulk_insert(node_type, parent_id.as_deref(), content);

            tx.execute(
                "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, 'active', 1, ?6, ?7)",
                libsql::params![id.clone(), node_type.clone(), content.clone(), props_json, title, now.clone(), now.clone()],
            ).await.context("Failed to insert node in bulk hierarchy")?;

            if let Some(parent) = parent_id {
                let rel_id = uuid::Uuid::new_v4().to_string();
                let rel_props = serde_json::json!({"order": order}).to_string();
                tx.execute(
                    "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                    libsql::params![rel_id, parent.clone(), id.clone(), rel_props, now.clone(), now.clone()],
                ).await.context("Failed to insert relationship in bulk hierarchy")?;
            }
        }

        tx.commit()
            .await
            .context("Failed to commit bulk hierarchy")?;

        let ids: Vec<String> = nodes.into_iter().map(|(id, ..)| id).collect();

        // Notify for each created node
        for id in &ids {
            if let Ok(Some(node)) = self.get_node(id).await {
                self.notify(StoreChange {
                    operation: StoreOperation::Created,
                    node,
                    source: None,
                    previous_node: None,
                    playbook_context: None,
                });
            }
        }

        Ok(ids)
    }

    pub async fn bulk_create_hierarchy_root_notify(
        &self,
        nodes: Vec<(
            String,
            String,
            String,
            Option<String>,
            f64,
            serde_json::Value,
        )>,
        root_ids: Vec<String>,
    ) -> Result<Vec<String>> {
        let created = self.bulk_create_hierarchy(nodes).await?;

        // Create stale embedding markers for roots
        self.create_stale_embedding_markers_bulk(&root_ids).await?;

        Ok(created)
    }

    pub async fn create_node_streaming(
        &self,
        id: String,
        node_type: String,
        content: String,
        parent_id: Option<String>,
        order: f64,
        properties: serde_json::Value,
    ) -> Result<String> {
        self.validate_node_type(&node_type)?;

        let properties = if properties.is_null() {
            serde_json::json!({})
        } else {
            properties
        };
        let props_json =
            serde_json::to_string(&properties).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();

        self.db.execute(
            "INSERT INTO node (id, node_type, content, properties, title, lifecycle_status, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, NULL, 'active', 1, ?5, ?6)",
            libsql::params![id.clone(), node_type.clone(), content.clone(), props_json, now.clone(), now.clone()],
        ).await.context("Failed to create node (streaming)")?;

        if let Some(ref parent) = parent_id {
            let rel_id = uuid::Uuid::new_v4().to_string();
            let rel_props = serde_json::json!({"order": order}).to_string();
            self.db.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'has_child', ?4, 1, ?5, ?6)",
                libsql::params![rel_id, parent.clone(), id.clone(), rel_props, now.clone(), now],
            ).await.context("Failed to create relationship (streaming)")?;
        }

        let node = Node {
            id: id.clone(),
            node_type: node_type.clone(),
            content,
            version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            properties,
            mentions: vec![],
            mentioned_in: vec![],
            title: None,
            lifecycle_status: "active".to_string(),
        };
        self.notify(StoreChange {
            operation: StoreOperation::Created,
            node,
            source: Some("streaming_import".to_string()),
            previous_node: None,
            playbook_context: None,
        });

        Ok(id)
    }

    fn compute_title_for_bulk_insert(
        node_type: &str,
        parent_id: Option<&str>,
        content: &str,
    ) -> Option<String> {
        if matches!(node_type, "date" | "schema" | "checkbox") {
            None
        } else if parent_id.is_none() || matches!(node_type, "task" | "collection") {
            let stripped = crate::utils::strip_markdown(content);
            Some(stripped)
        } else {
            None
        }
    }

    pub async fn get_task_node(&self, id: &str) -> Result<Option<crate::models::TaskNode>> {
        let node = self.get_node(id).await?;
        Ok(node.and_then(|n| {
            if n.node_type != "task" {
                return None;
            }
            let props = &n.properties;
            let task_props = props.get("task").cloned().unwrap_or(serde_json::json!({}));
            Some(crate::models::TaskNode {
                id: n.id,
                node_type: n.node_type,
                content: n.content,
                version: n.version,
                created_at: n.created_at,
                modified_at: n.modified_at,
                properties: n.properties,
                status: task_props
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                priority: task_props
                    .get("priority")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok()),
                due_date: task_props
                    .get("due_date")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
                assignee: task_props
                    .get("assignee")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                started_at: task_props
                    .get("started_at")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
                completed_at: task_props
                    .get("completed_at")
                    .and_then(|v| v.as_str())
                    .map(normalize_date_field),
            })
        }))
    }

    pub async fn update_task_node(
        &self,
        id: &str,
        expected_version: i64,
        update: crate::models::TaskNodeUpdate,
    ) -> Result<crate::models::TaskNode> {
        let current = self
            .get_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task node not found: {}", id))?;

        if current.version != expected_version {
            return Err(anyhow::anyhow!(
                "VersionMismatch: expected {}, got {}",
                expected_version,
                current.version
            ));
        }

        let mut props = current.properties.clone();
        let task_obj = props
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid properties"))?
            .entry("task")
            .or_insert(serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid task properties"))?
            .clone();

        let mut task_obj_owned = task_obj;

        if let Some(ref status) = update.status {
            task_obj_owned.insert("status".to_string(), serde_json::json!(status.as_str()));
        }
        if let Some(ref priority_opt) = update.priority {
            match priority_opt {
                Some(p) => {
                    task_obj_owned.insert("priority".to_string(), serde_json::json!(p.as_str()));
                }
                None => {
                    task_obj_owned.remove("priority");
                }
            }
        }
        if let Some(ref due_date_opt) = update.due_date {
            match due_date_opt {
                Some(s) => {
                    task_obj_owned.insert("due_date".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("due_date");
                }
            }
        }
        if let Some(ref assignee_opt) = update.assignee {
            match assignee_opt {
                Some(a) => {
                    task_obj_owned.insert("assignee".to_string(), serde_json::json!(a));
                }
                None => {
                    task_obj_owned.remove("assignee");
                }
            }
        }
        if let Some(ref started_at_opt) = update.started_at {
            match started_at_opt {
                Some(s) => {
                    task_obj_owned.insert("started_at".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("started_at");
                }
            }
        }
        if let Some(ref completed_at_opt) = update.completed_at {
            match completed_at_opt {
                Some(s) => {
                    task_obj_owned.insert("completed_at".to_string(), serde_json::json!(s));
                }
                None => {
                    task_obj_owned.remove("completed_at");
                }
            }
        }

        // Re-insert updated task object back into properties
        if let Some(props_obj) = props.as_object_mut() {
            props_obj.insert(
                "task".to_string(),
                serde_json::Value::Object(task_obj_owned),
            );
        }

        let props_json = serde_json::to_string(&props).context("Failed to serialize properties")?;
        let now = Utc::now().to_rfc3339();
        let new_version = expected_version + 1;

        let mut sql = "UPDATE node SET properties = ?1, version = ?2, modified_at = ?3".to_string();
        let mut sql_params: Vec<libsql::Value> = vec![
            libsql::Value::Text(props_json),
            libsql::Value::Integer(new_version),
            libsql::Value::Text(now),
        ];

        if let Some(ref content) = update.content {
            sql.push_str(", content = ?4");
            sql_params.push(libsql::Value::Text(content.clone()));
            sql.push_str(&format!(" WHERE id = ?{}", sql_params.len() + 1));
        } else {
            sql.push_str(&format!(" WHERE id = ?{}", sql_params.len() + 1));
        }
        sql_params.push(libsql::Value::Text(id.to_string()));

        self.db
            .execute(&sql, sql_params)
            .await
            .context("Failed to update task node")?;

        self.get_task_node(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task node '{}' not found after update", id))
    }

    pub async fn get_schema_node(&self, id: &str) -> Result<Option<crate::models::SchemaNode>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE id = ?1 AND node_type = 'schema' LIMIT 1",
                libsql::params![id.to_string()],
            )
            .await
            .context("Failed to query schema node")?;

        if let Some(row) = rows.next().await? {
            let node = Self::row_to_node(&row)?;
            match crate::models::SchemaNode::from_node(node) {
                Ok(schema) => Ok(Some(schema)),
                Err(e) => {
                    tracing::warn!("Failed to parse schema node '{}': {}", id, e);
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_all_schemas(&self) -> Result<Vec<crate::models::SchemaNode>> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM node WHERE node_type = 'schema' ORDER BY id",
                (),
            )
            .await
            .context("Failed to query all schema nodes")?;

        let mut schemas = Vec::new();
        while let Some(row) = rows.next().await? {
            let node = Self::row_to_node(&row)?;
            match crate::models::SchemaNode::from_node(node) {
                Ok(schema) => schemas.push(schema),
                Err(e) => tracing::warn!("Skipping invalid schema node: {}", e),
            }
        }
        Ok(schemas)
    }

    /// Replace a node's embeddings with locally-generated vectors (`origin =
    /// 'local'`). This is what the embedding generation path uses; the cloud-push
    /// sweep reads only `'local'` rows.
    pub async fn upsert_embeddings(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
    ) -> Result<()> {
        self.upsert_embeddings_with_origin(node_id, embeddings, "local")
            .await
    }

    /// Replace a node's embeddings with vectors PULLED from another device
    /// (`origin = 'remote'`, #182/#183). Identical to `upsert_embeddings` except
    /// for the provenance tag, which keeps the push sweep from re-pushing a vector
    /// this device merely received (no cross-device re-push loop).
    pub async fn apply_remote_embeddings(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
    ) -> Result<()> {
        self.upsert_embeddings_with_origin(node_id, embeddings, "remote")
            .await
    }

    async fn upsert_embeddings_with_origin(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
        origin: &str,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        // Replace the node's embeddings atomically across both `embedding` and the
        // `vec_embeddings` vec0 mirror. vec0 is keyed by embedding_id, so the leading
        // DELETE must clear the node's existing vec rows via its current embedding ids
        // BEFORE the rows disappear from `embedding`.
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin upsert_embeddings transaction")?;

        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for node")?;

        tx.execute(
            "DELETE FROM embedding WHERE node_id = ?1",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to delete existing embeddings")?;

        let now = Utc::now().to_rfc3339();
        for emb in embeddings {
            let id = uuid::Uuid::new_v4().to_string();
            let dimension = emb.vector.len() as i64;
            let vector_blob: Vec<u8> = emb.vector.iter().flat_map(|f| f.to_le_bytes()).collect();
            let model_name = emb
                .model_name
                .unwrap_or_else(|| "nomic-embed-text-v1.5".to_string());

            tx.execute(
                "INSERT INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, origin, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, NULL, ?12, ?13, ?14)",
                libsql::params![
                    id.clone(),
                    emb.node_id.clone(),
                    vector_blob.clone(),
                    dimension,
                    model_name,
                    emb.chunk_index as i64,
                    emb.chunk_start as i64,
                    emb.chunk_end as i64,
                    emb.total_chunks as i64,
                    emb.content_hash,
                    emb.token_count as i64,
                    origin.to_string(),
                    now.clone(),
                    now.clone(),
                ],
            ).await.context("Failed to insert embedding")?;

            // Mirror the (non-stale) vector into vec0 for KNN search, using the same id.
            tx.execute(
                "INSERT INTO vec_embeddings (embedding_id, vector) VALUES (?1, ?2)",
                libsql::params![id, vector_blob],
            )
            .await
            .context("Failed to insert into vec_embeddings")?;
        }

        tx.commit()
            .await
            .context("Failed to commit upsert_embeddings transaction")?;

        Ok(())
    }

    /// Decode a stored embedding row into the `Embedding` model. Vectors are
    /// persisted by `upsert_embeddings` as a little-endian f32 blob; decode it
    /// back to `Vec<f32>`. Column order must match the SELECTs below.
    fn row_to_embedding(row: &libsql::Row) -> Result<crate::models::Embedding> {
        let id: String = row.get(0)?;
        let node: String = row.get(1)?;
        let vector_blob: Vec<u8> = row.get(2)?;
        let dimension: i64 = row.get(3)?;
        let model_name: String = row.get(4)?;
        let chunk_index: i64 = row.get(5)?;
        let chunk_start: i64 = row.get(6)?;
        let chunk_end: Option<i64> = row.get(7)?;
        let total_chunks: i64 = row.get(8)?;
        let content_hash: Option<String> = row.get(9)?;
        let token_count: Option<i64> = row.get(10)?;
        let stale: i64 = row.get(11)?;
        let error_count: i64 = row.get(12)?;
        let last_error: Option<String> = row.get(13)?;
        let created_at_str: String = row.get(14)?;
        let modified_at_str: String = row.get(15)?;

        let vector: Vec<f32> = vector_blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .with_context(|| format!("Invalid embedding created_at: {}", created_at_str))?
            .with_timezone(&Utc);
        let modified_at = DateTime::parse_from_rfc3339(&modified_at_str)
            .with_context(|| format!("Invalid embedding modified_at: {}", modified_at_str))?
            .with_timezone(&Utc);

        Ok(crate::models::Embedding {
            id,
            node,
            vector,
            dimension: dimension as i32,
            model_name,
            chunk_index: chunk_index as i32,
            chunk_start: chunk_start as i32,
            chunk_end: chunk_end.map(|v| v as i32),
            total_chunks: total_chunks as i32,
            content_hash,
            token_count: token_count.map(|v| v as i32),
            stale: stale != 0,
            error_count: error_count as i32,
            last_error,
            created_at,
            modified_at,
        })
    }

    /// Read all locally-stored embedding records for a node (one per chunk),
    /// ordered by chunk index. Used by the Pro daemon's cloud push (#97) to
    /// mirror a node's vectors into Supabase pgvector.
    pub async fn get_embeddings(&self, node_id: &str) -> Result<Vec<crate::models::Embedding>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, node_id, vector, dimension, model_name, chunk_index, chunk_start, \
                 chunk_end, total_chunks, content_hash, token_count, stale, error_count, \
                 last_error, created_at, modified_at \
                 FROM embedding WHERE node_id = ?1 ORDER BY chunk_index",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to query embeddings for node")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(Self::row_to_embedding(&row)?);
        }
        Ok(out)
    }

    /// Read **locally-generated** (`origin = 'local'`) embedding records modified
    /// at or after `since`, across all nodes, ordered by `modified_at`. Drives the
    /// Pro daemon's cloud-push sweep (#97): the daemon keeps a cursor over
    /// `modified_at` and pushes newly (re)computed vectors. Stale rows are
    /// included — the caller decides whether to skip them.
    ///
    /// The `origin = 'local'` filter (#182/#183) excludes vectors PULLED from
    /// other devices, so a received vector is never re-pushed — without it, a
    /// pull's `modified_at = now` would re-arm this sweep and bounce the vector
    /// back to cloud, amplifying writes and (on heterogeneous devices) looping.
    ///
    /// INVARIANT: assumes every writer stores `modified_at` as a UTC rfc3339
    /// string (`Utc::now().to_rfc3339()`, as `upsert_embeddings` does). The cursor
    /// compares lexicographically, which equals chronological order ONLY for that
    /// fixed `+00:00`-offset form; a `Z`-suffixed or non-UTC timestamp would break
    /// ordering and make the sweep skip rows. Served by `idx_emb_modified`.
    pub async fn embeddings_modified_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<crate::models::Embedding>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, node_id, vector, dimension, model_name, chunk_index, chunk_start, \
                 chunk_end, total_chunks, content_hash, token_count, stale, error_count, \
                 last_error, created_at, modified_at \
                 FROM embedding WHERE origin = 'local' AND modified_at >= ?1 \
                 ORDER BY modified_at, node_id, chunk_index",
                libsql::params![since.to_rfc3339()],
            )
            .await
            .context("Failed to query embeddings modified since")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(Self::row_to_embedding(&row)?);
        }
        Ok(out)
    }

    pub async fn mark_root_embedding_stale(&self, node_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin mark-stale transaction")?;

        // Stale vectors must not be searchable: drop them from the vec0 mirror.
        // `upsert_embeddings` repopulates vec0 when the node is re-embedded.
        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for stale node")?;

        tx.execute(
            "UPDATE embedding SET stale = 1, modified_at = ?1 WHERE node_id = ?2",
            libsql::params![now, node_id.to_string()],
        )
        .await
        .context("Failed to mark embedding stale")?;

        tx.commit()
            .await
            .context("Failed to commit mark-stale transaction")?;
        Ok(())
    }

    pub async fn get_stale_embedding_root_ids(
        &self,
        limit: Option<i64>,
        debounce_secs: u64,
        max_retries: u8,
    ) -> Result<Vec<String>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(debounce_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let max_retries_i = max_retries as i64;

        let sql = if let Some(l) = limit {
            format!(
                "SELECT DISTINCT node_id FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at < ?2 LIMIT {}",
                l
            )
        } else {
            "SELECT DISTINCT node_id FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at < ?2".to_string()
        };

        let mut rows = self
            .db
            .query(&sql, libsql::params![max_retries_i, cutoff_str])
            .await
            .context("Failed to get stale embedding root IDs")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn has_pending_stale_embeddings(
        &self,
        debounce_secs: u64,
        max_retries: u8,
    ) -> Result<bool> {
        let cutoff = Utc::now() - chrono::Duration::seconds(debounce_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let max_retries_i = max_retries as i64;

        let mut rows = self.db.query(
            "SELECT COUNT(*) FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at >= ?2",
            libsql::params![max_retries_i, cutoff_str],
        ).await.context("Failed to check for pending stale embeddings")?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    pub async fn has_embeddings(&self, node_id: &str) -> Result<bool> {
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM embedding WHERE node_id = ?1",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to check for embeddings")?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    pub async fn delete_embeddings(&self, node_id: &str) -> Result<()> {
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin delete_embeddings transaction")?;

        // Clear the vec0 mirror first (keyed by embedding_id, so resolve via embedding).
        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for node")?;

        tx.execute(
            "DELETE FROM embedding WHERE node_id = ?1",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to delete embeddings")?;

        tx.commit()
            .await
            .context("Failed to commit delete_embeddings transaction")?;
        Ok(())
    }

    pub async fn record_embedding_error(
        &self,
        node_id: &str,
        error: &str,
        max_retries: u8,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let max_retries_i = max_retries as i64;

        // Increment error_count, set last_error, clear stale if error_count reaches max_retries
        self.db.execute(
            "UPDATE embedding SET error_count = error_count + 1, last_error = ?1, modified_at = ?2, stale = CASE WHEN error_count + 1 >= ?3 THEN 0 ELSE stale END WHERE node_id = ?4",
            libsql::params![error.to_string(), now, max_retries_i, node_id.to_string()],
        ).await.context("Failed to record embedding error")?;
        Ok(())
    }

    pub async fn search_embeddings(
        &self,
        query_vector: &[f32],
        limit: i64,
        threshold: Option<f64>,
    ) -> Result<Vec<crate::models::EmbeddingSearchResult>> {
        let min_score = threshold.unwrap_or(0.5);

        // vec0 KNN over the compact vector store, then JOIN back to recover node_id /
        // total_chunks. vec0 holds only non-stale vectors (see upsert/delete/mark-stale),
        // so `e.stale = 0` is a cheap defensive guard. We over-fetch chunks because many
        // chunks map to one node and results are grouped per node.
        //
        // Note: with KNN, `matching_chunks` counts a node's chunks that landed in the
        // top-k near the query — not all of its chunks (as the old full scan did). So
        // `density` now genuinely measures "fraction of the node's chunks near the query"
        // rather than always being ~1.0; it still feeds the same composite formula.
        let query_blob: Vec<u8> = query_vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        let k = (limit * EMBEDDING_KNN_OVERFETCH).max(limit);

        let mut rows = self
            .db
            .query(
                "SELECT e.node_id, e.total_chunks, v.distance \
                 FROM vec_embeddings v JOIN embedding e ON e.id = v.embedding_id \
                 WHERE v.vector MATCH ?1 AND k = ?2 AND e.stale = 0",
                libsql::params![query_blob, k],
            )
            .await
            .context("Failed to run vec0 KNN search")?;

        // Group by node_id: track max similarity, chunk counts
        let mut node_scores: HashMap<String, (f64, i64, i64)> = HashMap::new(); // node_id -> (max_sim, matching_chunks, total_chunks)

        while let Some(row) = rows.next().await? {
            let node_id: String = row.get(0)?;
            let total_chunks: i64 = row.get(1)?;
            let distance: f64 = row.get(2)?;
            // vec0 cosine distance_metric returns distance = 1 - cosine similarity
            let similarity = 1.0 - distance;

            let entry = node_scores.entry(node_id).or_insert((0.0, 0, total_chunks));
            if similarity > entry.0 {
                entry.0 = similarity;
            }
            entry.1 += 1;
        }

        // Compute composite scores and filter
        let mut results: Vec<crate::models::EmbeddingSearchResult> = Vec::new();
        for (node_id, (max_similarity, matching_chunks, total_chunks)) in node_scores {
            let density = if total_chunks > 0 {
                matching_chunks as f64 / total_chunks as f64
            } else {
                1.0
            };
            let composite_score = max_similarity * (1.0 + 0.3 * density);

            if composite_score > min_score {
                let node = self.get_node(&node_id).await?;
                results.push(crate::models::EmbeddingSearchResult {
                    node_id: node_id.clone(),
                    score: composite_score,
                    max_similarity,
                    matching_chunks,
                    node,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit as usize);
        Ok(results)
    }

    pub async fn search_embeddings_by_node_type(
        &self,
        query_vector: &[f32],
        node_type: &str,
        limit: i64,
        threshold: Option<f64>,
    ) -> Result<Vec<crate::models::EmbeddingSearchResult>> {
        let min_score = threshold.unwrap_or(0.5);

        // Same vec0 KNN as `search_embeddings`, with the node-type filter folded into the
        // JOIN. The type filter is applied AFTER KNN, so use a larger over-fetch to keep
        // enough surviving candidates of the requested type.
        let query_blob: Vec<u8> = query_vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        let k = (limit * EMBEDDING_KNN_OVERFETCH * 5).max(limit);

        let mut rows = self
            .db
            .query(
                "SELECT e.node_id, e.total_chunks, v.distance \
             FROM vec_embeddings v \
             JOIN embedding e ON e.id = v.embedding_id \
             JOIN node n ON n.id = e.node_id \
             WHERE v.vector MATCH ?1 AND k = ?2 AND e.stale = 0 AND n.node_type = ?3",
                libsql::params![query_blob, k, node_type.to_string()],
            )
            .await
            .context("Failed to run typed vec0 KNN search")?;

        let mut node_scores: HashMap<String, (f64, i64, i64)> = HashMap::new();

        while let Some(row) = rows.next().await? {
            let node_id: String = row.get(0)?;
            let total_chunks: i64 = row.get(1)?;
            let distance: f64 = row.get(2)?;
            let similarity = 1.0 - distance;

            let entry = node_scores.entry(node_id).or_insert((0.0, 0, total_chunks));
            if similarity > entry.0 {
                entry.0 = similarity;
            }
            entry.1 += 1;
        }

        let mut results = Vec::new();
        for (node_id, (max_similarity, matching_chunks, total_chunks)) in node_scores {
            let density = if total_chunks > 0 {
                matching_chunks as f64 / total_chunks as f64
            } else {
                1.0
            };
            let composite_score = max_similarity * (1.0 + 0.3 * density);

            if composite_score > min_score {
                let node = self.get_node(&node_id).await?;
                results.push(crate::models::EmbeddingSearchResult {
                    node_id: node_id.clone(),
                    score: composite_score,
                    max_similarity,
                    matching_chunks,
                    node,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // The node_type filter runs after the global top-k, so a type that is rare
        // relative to the corpus can be crowded out of the KNN window — surfacing as
        // fewer than `limit` results. Surface that as a debug signal rather than failing
        // silently; raising EMBEDDING_KNN_OVERFETCH is the lever if recall suffers.
        if (results.len() as i64) < limit {
            tracing::debug!(
                node_type,
                returned = results.len(),
                limit,
                k,
                "typed embedding search returned fewer than `limit` results; node_type may be under-represented in the KNN window"
            );
        }

        results.truncate(limit as usize);
        Ok(results)
    }

    pub async fn bm25_search_roots(
        &self,
        query: &str,
        candidate_limit: i64,
    ) -> Result<HashSet<String>> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| {
                t.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty() && !BM25_STOP_WORDS.contains(&t.as_str()))
            .take(BM25_MAX_TOKENS)
            .collect();

        if tokens.is_empty() {
            return Ok(HashSet::new());
        }

        // Build FTS5 query: "token1" OR "token2" OR ...
        let fts_query = tokens
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");

        let sql = format!(
            "SELECT n.id FROM node n JOIN node_fts f ON f.id = n.id WHERE node_fts MATCH ?1 AND n.lifecycle_status != 'deleted' ORDER BY rank LIMIT {}",
            candidate_limit
        );

        let mut rows = self
            .db
            .query(&sql, libsql::params![fts_query])
            .await
            .context("Failed to execute BM25 search")?;

        let mut matching_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            matching_ids.push(row.get(0)?);
        }

        if matching_ids.is_empty() {
            return Ok(HashSet::new());
        }

        // Resolve each match to its root using recursive CTE
        let mut root_ids = HashSet::new();

        for node_id in matching_ids {
            let mut rows = self.db.query(
                r#"WITH RECURSIVE ancestors(node_id, depth) AS (
                    SELECT in_node, 1 FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'
                    UNION ALL
                    SELECT r.in_node, a.depth + 1 FROM relationship r
                    JOIN ancestors a ON r.out_node = a.node_id
                    WHERE r.relationship_type = 'has_child' AND a.depth < 100
                )
                SELECT node_id FROM ancestors ORDER BY depth DESC LIMIT 1"#,
                libsql::params![node_id.clone()],
            ).await.context("Failed to get root for BM25 match")?;

            if let Some(row) = rows.next().await? {
                let root_id: String = row.get(0)?;
                // Verify root is not deleted
                if let Ok(Some(n)) = self.get_node(&root_id).await {
                    if n.lifecycle_status != "deleted" {
                        root_ids.insert(root_id);
                    }
                }
            } else {
                // node_id is itself the root
                if let Ok(Some(n)) = self.get_node(&node_id).await {
                    if n.lifecycle_status != "deleted" {
                        root_ids.insert(node_id);
                    }
                }
            }
        }

        Ok(root_ids)
    }

    async fn get_next_order_for_relationship(
        &self,
        node_id: &str,
        relationship_type: &str,
        use_out_as_anchor: bool,
    ) -> Result<f64> {
        let anchor_field = if use_out_as_anchor {
            "out_node"
        } else {
            "in_node"
        };
        let sql = format!(
            "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE {} = ?1 AND relationship_type = ?2 ORDER BY json_extract(properties, '$.order') DESC LIMIT 1",
            anchor_field
        );

        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), relationship_type.to_string()],
            )
            .await
            .context("Failed to get last order for relationship")?;

        let last_order = if let Some(row) = rows.next().await? {
            row.get::<Option<f64>>(0)?.unwrap_or(0.0)
        } else {
            0.0
        };

        Ok(if last_order > 0.0 {
            FractionalOrderCalculator::calculate_order(Some(last_order), None)
        } else {
            FractionalOrderCalculator::calculate_order(None, None)
        })
    }

    pub async fn get_next_member_order(&self, collection_id: &str) -> Result<f64> {
        self.get_next_order_for_relationship(collection_id, "member_of", true)
            .await
    }

    pub async fn get_next_child_order(&self, parent_id: &str) -> Result<f64> {
        self.get_next_order_for_relationship(parent_id, "has_child", false)
            .await
    }

    pub async fn add_to_collection(
        &self,
        member_id: &str,
        collection_id: &str,
    ) -> Result<Option<String>> {
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin add_to_collection transaction")?;

        let mut rows = tx
            .query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of' LIMIT 1",
                libsql::params![member_id.to_string(), collection_id.to_string()],
            )
            .await
            .context("Failed to check existing membership")?;

        if rows.next().await?.is_some() {
            return Ok(None);
        }

        let mut order_rows = tx
            .query(
                "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE out_node = ?1 AND relationship_type = 'member_of' ORDER BY json_extract(properties, '$.order') DESC LIMIT 1",
                libsql::params![collection_id.to_string()],
            )
            .await
            .context("Failed to get last member order")?;

        let last_order = if let Some(row) = order_rows.next().await? {
            row.get::<Option<f64>>(0)?.unwrap_or(0.0)
        } else {
            0.0
        };
        let new_order = FractionalOrderCalculator::calculate_order(
            if last_order > 0.0 {
                Some(last_order)
            } else {
                None
            },
            None,
        );

        let rel_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let props = serde_json::json!({"order": new_order}).to_string();

        tx.execute(
            "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'member_of', ?4, 1, ?5, ?6)",
            libsql::params![rel_id.clone(), member_id.to_string(), collection_id.to_string(), props, now.clone(), now],
        )
        .await
        .context("Failed to add to collection")?;

        tx.commit()
            .await
            .context("Failed to commit add_to_collection")?;

        Ok(Some(rel_id))
    }

    pub async fn remove_from_collection(
        &self,
        member_id: &str,
        collection_id: &str,
    ) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of'",
            libsql::params![member_id.to_string(), collection_id.to_string()],
        ).await.context("Failed to get membership ID")?;

        let existing_id: Option<String> = if let Some(row) = rows.next().await? {
            Some(row.get(0)?)
        } else {
            None
        };

        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of'",
            libsql::params![member_id.to_string(), collection_id.to_string()],
        ).await.context("Failed to remove from collection")?;

        Ok(existing_id.map(|id| format!("relationship:{}", id)))
    }

    pub async fn get_node_memberships(&self, node_id: &str) -> Result<Vec<String>> {
        let mut rows = self.db.query(
            "SELECT out_node FROM relationship WHERE in_node = ?1 AND relationship_type = 'member_of'",
            libsql::params![node_id.to_string()],
        ).await.context("Failed to get node memberships")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn get_collection_members(&self, collection_id: &str) -> Result<Vec<Node>> {
        let start = std::time::Instant::now();

        let mut rows = self.db.query(
            "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = 'member_of' ORDER BY json_extract(r.properties, '$.order') ASC",
            libsql::params![collection_id.to_string()],
        ).await.context("Failed to get collection members")?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }

        tracing::debug!(
            "get_collection_members: {:?} for {} nodes",
            start.elapsed(),
            nodes.len()
        );

        Ok(nodes)
    }

    pub async fn get_collection_by_name(&self, name: &str) -> Result<Option<Node>> {
        let normalized = name.to_lowercase();

        let mut rows = self
            .db
            .query(
                "SELECT id FROM node WHERE node_type = 'collection' AND LOWER(title) = ?1 LIMIT 1",
                libsql::params![normalized],
            )
            .await
            .context("Failed to search for collection by name")?;

        if let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            self.get_node(&id).await
        } else {
            Ok(None)
        }
    }

    pub async fn get_collections_by_names(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Node>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }

        let normalized: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let placeholders: Vec<String> = (1..=normalized.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT id, title FROM node WHERE node_type = 'collection' AND LOWER(title) IN ({})",
            placeholders.join(", ")
        );

        let params: Vec<libsql::Value> = normalized
            .iter()
            .map(|n| libsql::Value::Text(n.clone()))
            .collect();
        let mut rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to batch search collections by names")?;

        let mut collections = HashMap::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            if let Ok(Some(node)) = self.get_node(&id).await {
                let key = title.unwrap_or_default().to_lowercase();
                collections.insert(key, node);
            }
        }

        Ok(collections)
    }

    pub async fn get_collection_members_recursive(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>> {
        // Get all collections in the subtree using WITH RECURSIVE.
        //
        // #1426: collection hierarchy is built from `member_of` edges (a
        // sub-collection is a member_of its parent), NOT `has_child` — the old
        // recursive arm followed `has_child` and so matched nothing, leaving
        // `coll_subtree` as just the seed and silently dropping every
        // sub-collection's members. member_of stores in_node = member/child,
        // out_node = collection/parent, so we descend parent→child by joining on
        // `r.out_node = cs.node_id` and taking `r.in_node`, restricted to
        // collection children (a content member isn't a sub-collection). A depth
        // cap bounds traversal in case a cycle slips in (see #1427).
        let mut rows = self
            .db
            .query(
                r#"WITH RECURSIVE coll_subtree(node_id, depth) AS (
                SELECT ?1, 0
                UNION ALL
                SELECT r.in_node, cs.depth + 1 FROM relationship r
                JOIN coll_subtree cs ON r.out_node = cs.node_id
                JOIN node n ON n.id = r.in_node AND n.node_type = 'collection'
                WHERE r.relationship_type = 'member_of' AND cs.depth < 100
            )
            SELECT DISTINCT r.in_node FROM relationship r
            JOIN coll_subtree cs ON r.out_node = cs.node_id
            WHERE r.relationship_type = 'member_of'"#,
                libsql::params![collection_id.to_string()],
            )
            .await
            .context("Failed to get recursive collection members")?;

        let mut member_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            member_ids.push(row.get(0)?);
        }

        member_ids.sort();
        member_ids.dedup();
        Ok(member_ids)
    }

    pub async fn get_all_collection_names(&self) -> Result<Vec<String>> {
        let mut rows = self
            .db
            .query(
                "SELECT content FROM node WHERE node_type = 'collection' ORDER BY content ASC",
                (),
            )
            .await
            .context("Failed to get all collection names")?;

        let mut names = Vec::new();
        while let Some(row) = rows.next().await? {
            names.push(row.get(0)?);
        }
        Ok(names)
    }

    pub async fn get_all_collections_with_member_counts(
        &self,
    ) -> Result<Vec<(Node, usize, Vec<String>)>> {
        let collections = self.get_all_collections().await?;
        if collections.is_empty() {
            return Ok(vec![]);
        }

        // Count members per collection
        let mut rows = self.db.query(
            "SELECT out_node, COUNT(*) FROM relationship WHERE relationship_type = 'member_of' GROUP BY out_node",
            (),
        ).await.context("Failed to get member counts")?;

        let mut count_map: HashMap<String, usize> = HashMap::new();
        while let Some(row) = rows.next().await? {
            let coll_id: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            count_map.insert(coll_id, count as usize);
        }

        // Get collection-to-collection hierarchy edges
        let mut rows2 = self.db.query(
            "SELECT r.in_node, r.out_node FROM relationship r JOIN node n1 ON n1.id = r.in_node JOIN node n2 ON n2.id = r.out_node WHERE r.relationship_type = 'member_of' AND n1.node_type = 'collection' AND n2.node_type = 'collection'",
            (),
        ).await.context("Failed to get collection hierarchy")?;

        let mut parent_map: HashMap<String, Vec<String>> = HashMap::new();
        while let Some(row) = rows2.next().await? {
            let child: String = row.get(0)?;
            let parent: String = row.get(1)?;
            parent_map.entry(child).or_default().push(parent);
        }

        Ok(collections
            .into_iter()
            .map(|node| {
                let count = count_map.get(&node.id).copied().unwrap_or(0);
                let parents = parent_map.get(&node.id).cloned().unwrap_or_default();
                (node, count, parents)
            })
            .collect())
    }

    async fn get_all_collections(&self) -> Result<Vec<Node>> {
        self.query_nodes_from_sql(
            "SELECT * FROM node WHERE node_type = 'collection' ORDER BY content ASC",
            (),
        )
        .await
    }

    pub async fn bulk_add_to_collections(&self, memberships: &[(String, String)]) -> Result<usize> {
        if memberships.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();

        // Group by collection to calculate orders correctly
        let mut by_collection: HashMap<&str, Vec<&str>> = HashMap::new();
        for (node_id, collection_id) in memberships {
            by_collection
                .entry(collection_id.as_str())
                .or_default()
                .push(node_id.as_str());
        }

        let mut ordered: Vec<(String, String, f64)> = Vec::with_capacity(memberships.len());
        for (collection_id, node_ids) in &by_collection {
            let base_order = self.get_next_member_order(collection_id).await?;
            for (i, node_id) in node_ids.iter().enumerate() {
                ordered.push((
                    node_id.to_string(),
                    collection_id.to_string(),
                    base_order + i as f64,
                ));
            }
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk add transaction")?;

        let mut created = 0;
        for (node_id, collection_id, order) in &ordered {
            let mut rows = tx.query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'member_of' LIMIT 1",
                libsql::params![node_id.clone(), collection_id.clone()],
            ).await.context("Failed to check existing membership")?;

            if rows.next().await?.is_some() {
                continue;
            }

            let rel_id = uuid::Uuid::new_v4().to_string();
            let props = serde_json::json!({"order": order}).to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'member_of', ?4, 1, ?5, ?6)",
                libsql::params![rel_id, node_id.clone(), collection_id.clone(), props, now.clone(), now.clone()],
            ).await.context("Failed to insert membership")?;
            created += 1;
        }

        tx.commit().await.context("Failed to commit bulk add")?;

        tracing::debug!(
            "bulk_add_to_collections: {} memberships in {:?}",
            created,
            start.elapsed()
        );

        Ok(created)
    }

    pub async fn bulk_create_mentions(&self, mentions: &[(String, String)]) -> Result<usize> {
        if mentions.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let valid_mentions: Vec<_> = mentions.iter().filter(|(s, t)| s != t).collect();
        if valid_mentions.is_empty() {
            return Ok(0);
        }

        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin bulk mentions transaction")?;

        let mut created = 0;
        for (source_id, target_id) in &valid_mentions {
            let mut rows = tx.query(
                "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = 'mentions' LIMIT 1",
                libsql::params![source_id.clone().to_string(), target_id.clone().to_string()],
            ).await.context("Failed to check existing mention")?;

            if rows.next().await?.is_some() {
                continue;
            }

            let rel_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, 'mentions', '{}', 1, ?4, ?5)",
                libsql::params![rel_id, source_id.clone().to_string(), target_id.clone().to_string(), now.clone(), now.clone()],
            ).await.context("Failed to insert mention")?;
            created += 1;
        }

        tx.commit()
            .await
            .context("Failed to commit bulk mentions")?;

        tracing::debug!(
            "bulk_create_mentions: {} mentions in {:?}",
            created,
            start.elapsed()
        );

        Ok(created)
    }

    pub async fn create_stale_embedding_marker(&self, node_id: &str) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        // Deliberately NOT mirrored into vec_embeddings: this is a stale (stale=1)
        // placeholder with a dummy vector and must never surface in KNN results.
        // Unit vector [1, 0, 0, ...] as 768×f32 LE bytes
        let mut vector_bytes = vec![0u8; 768 * 4];
        vector_bytes[0..4].copy_from_slice(&1.0f32.to_le_bytes());

        self.db.execute(
            "INSERT OR IGNORE INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, created_at, modified_at) VALUES (?1, ?2, ?3, 768, 'nomic-embed-text-v1.5', 0, 0, NULL, 1, NULL, NULL, 1, 0, NULL, ?4, ?5)",
            libsql::params![id, node_id.to_string(), vector_bytes, now.clone(), now],
        ).await.context("Failed to create stale embedding marker")?;
        Ok(())
    }

    pub async fn create_stale_embedding_markers_bulk(&self, node_ids: &[String]) -> Result<usize> {
        if node_ids.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let now = Utc::now().to_rfc3339();
        // Stale placeholders are deliberately NOT mirrored into vec_embeddings (see
        // create_stale_embedding_marker) — they must never appear in KNN results.
        let mut vector_bytes = vec![0u8; 768 * 4];
        vector_bytes[0..4].copy_from_slice(&1.0f32.to_le_bytes());

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin markers transaction")?;
        for node_id in node_ids {
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT OR IGNORE INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, created_at, modified_at) VALUES (?1, ?2, ?3, 768, 'nomic-embed-text-v1.5', 0, 0, NULL, 1, NULL, NULL, 1, 0, NULL, ?4, ?5)",
                libsql::params![id, node_id.clone(), vector_bytes.clone(), now.clone(), now.clone()],
            ).await.context("Failed to insert stale embedding marker")?;
        }
        tx.commit()
            .await
            .context("Failed to commit markers transaction")?;

        tracing::debug!(
            "create_stale_embedding_markers_bulk: {} markers in {:?}",
            node_ids.len(),
            start.elapsed()
        );

        Ok(node_ids.len())
    }

    pub async fn check_relationship_exists(&self, source_id: &str, rel_type: &str) -> Result<i64> {
        let mut rows = self.db.query(
            "SELECT COUNT(*) as cnt FROM relationship WHERE in_node = ?1 AND relationship_type = ?2",
            libsql::params![source_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to check relationship existence")?;
        let row = rows
            .next()
            .await
            .context("No row returned")?
            .ok_or_else(|| anyhow::anyhow!("Empty result for relationship count"))?;
        Ok(row.get::<i64>(0).unwrap_or(0))
    }

    pub async fn relationship_exists(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<bool> {
        let mut rows = self.db.query(
            "SELECT 1 FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3 LIMIT 1",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to check relationship existence")?;
        Ok(rows.next().await?.is_some())
    }

    pub async fn create_generic_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: &serde_json::Value,
    ) -> Result<String> {
        let now = chrono::Utc::now().to_rfc3339();
        let rel_id = uuid::Uuid::new_v4().to_string();
        let props_json = serde_json::to_string(properties).unwrap_or_else(|_| "{}".to_string());
        self.db.execute(
            "INSERT OR IGNORE INTO relationship (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
            libsql::params![rel_id.clone(), source_id.to_string(), target_id.to_string(), rel_type.to_string(), props_json, now.clone(), now],
        ).await.context("Failed to create generic relationship")?;
        Ok(rel_id)
    }

    pub async fn get_relationship_id(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<Option<String>> {
        let mut rows = self.db.query(
            "SELECT id FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3 LIMIT 1",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to get relationship ID")?;
        if let Some(row) = rows.next().await? {
            Ok(Some(row.get::<String>(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_generic_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<()> {
        self.db.execute(
            "DELETE FROM relationship WHERE in_node = ?1 AND out_node = ?2 AND relationship_type = ?3",
            libsql::params![source_id.to_string(), target_id.to_string(), rel_type.to_string()],
        ).await.context("Failed to delete relationship")?;
        Ok(())
    }

    pub async fn get_nodes_by_relationship(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<Vec<Node>> {
        let sql = match direction {
            "out" => "SELECT n.* FROM node n JOIN relationship r ON r.out_node = n.id WHERE r.in_node = ?1 AND r.relationship_type = ?2",
            "in" => "SELECT n.* FROM node n JOIN relationship r ON r.in_node = n.id WHERE r.out_node = ?1 AND r.relationship_type = ?2",
            _ => return Err(anyhow::anyhow!("Invalid direction: {}", direction)),
        };
        let mut rows = self
            .db
            .query(
                sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to get related nodes")?;
        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(Self::row_to_node(&row)?);
        }
        Ok(nodes)
    }

    pub async fn count_nodes_by_type(&self, node_type: &str) -> Result<i64> {
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM node WHERE node_type = ?1",
                libsql::params![node_type.to_string()],
            )
            .await
            .context("Failed to count nodes by type")?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No result for count"))?;
        Ok(row.get::<i64>(0).unwrap_or(0))
    }

    pub async fn get_relationship_orders(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<Vec<Option<f64>>> {
        let filter_col = if direction == "in_node" {
            "in_node"
        } else {
            "out_node"
        };
        let sql = format!(
            "SELECT json_extract(properties, '$.order') as ord FROM relationship WHERE {} = ?1 AND relationship_type = ?2 ORDER BY json_extract(properties, '$.order') ASC",
            filter_col
        );
        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to get relationship orders")?;
        let mut orders = Vec::new();
        while let Some(row) = rows.next().await? {
            let order: Option<f64> = row.get(0).ok();
            orders.push(order);
        }
        Ok(orders)
    }

    pub async fn get_relationship_count(
        &self,
        node_id: &str,
        rel_type: &str,
        direction: &str,
    ) -> Result<usize> {
        let filter_col = if direction == "in_node" {
            "in_node"
        } else {
            "out_node"
        };
        let sql = format!(
            "SELECT COUNT(*) FROM relationship WHERE {} = ?1 AND relationship_type = ?2",
            filter_col
        );
        let mut rows = self
            .db
            .query(
                &sql,
                libsql::params![node_id.to_string(), rel_type.to_string()],
            )
            .await
            .context("Failed to count relationships")?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No count result"))?;
        Ok(row.get::<i64>(0).unwrap_or(0) as usize)
    }

    pub async fn query_node_ids_raw(&self, sql: &str) -> Result<Vec<String>> {
        let mut rows = self
            .db
            .query(sql, ())
            .await
            .context("Failed to execute node query")?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Ok(id) = row.get::<String>(0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    async fn create_test_store() -> Result<(Arc<SqliteStore>, TempDir)> {
        use crate::services::NodeService;

        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut store_arc = Arc::new(SqliteStore::new(db_path).await?);

        let _ = NodeService::new(&mut store_arc)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize NodeService: {}", e))?;

        Ok((store_arc, temp_dir))
    }

    #[tokio::test]
    async fn test_collection_members_recursive_includes_subcollection_members() -> Result<()> {
        // #1426: members of a SUB-collection must be returned for the parent.
        // member_of stores in_node = member/child, out_node = collection/parent;
        // add_to_collection(member, collection) creates that edge.
        let (store, _t) = create_test_store().await?;

        let parent = Node::new("collection".to_string(), "Parent".to_string(), json!({}));
        let parent_id = parent.id.clone();
        store.create_node(parent, None, None).await?;

        let sub = Node::new("collection".to_string(), "Sub".to_string(), json!({}));
        let sub_id = sub.id.clone();
        store.create_node(sub, None, None).await?;
        store.add_to_collection(&sub_id, &parent_id).await?; // Sub member_of Parent

        let direct = Node::new("text".to_string(), "direct member".to_string(), json!({}));
        let direct_id = direct.id.clone();
        store.create_node(direct, None, None).await?;
        store.add_to_collection(&direct_id, &parent_id).await?;

        let nested = Node::new("text".to_string(), "nested member".to_string(), json!({}));
        let nested_id = nested.id.clone();
        store.create_node(nested, None, None).await?;
        store.add_to_collection(&nested_id, &sub_id).await?; // member of the SUB only

        let members = store.get_collection_members_recursive(&parent_id).await?;
        assert!(
            members.contains(&nested_id),
            "a member of a sub-collection must be returned for the parent (was silently dropped); got {members:?}"
        );
        assert!(members.contains(&direct_id), "direct member missing");
        assert!(
            members.contains(&sub_id),
            "sub-collection itself is a member of the parent"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_member_of_cycle_is_rejected() -> Result<()> {
        // #1427: collection hierarchy is a DAG. With `B member_of A`, adding
        // `A member_of B` would close a cycle and must be rejected.
        let (store, _t) = create_test_store().await?;

        let a = Node::new("collection".to_string(), "A".to_string(), json!({}));
        let a_id = a.id.clone();
        store.create_node(a, None, None).await?;
        let b = Node::new("collection".to_string(), "B".to_string(), json!({}));
        let b_id = b.id.clone();
        store.create_node(b, None, None).await?;

        store.add_to_collection(&b_id, &a_id).await?; // B member_of A

        // Adding A member_of B closes the cycle → error.
        let err = store
            .validate_no_member_of_cycle(&a_id, &b_id)
            .await
            .expect_err("A member_of B must be rejected as a cycle");
        assert!(err.to_string().contains("collection_cycle"));

        // A self-edge is a cycle.
        assert!(store
            .validate_no_member_of_cycle(&a_id, &a_id)
            .await
            .is_err());

        // A non-cyclic hierarchy edge is allowed (B member_of A already holds; a
        // fresh unrelated parent is fine).
        let c = Node::new("collection".to_string(), "C".to_string(), json!({}));
        let c_id = c.id.clone();
        store.create_node(c, None, None).await?;
        assert!(store
            .validate_no_member_of_cycle(&a_id, &c_id)
            .await
            .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_backfill_fts_reindexes_unindexed_nodes() -> Result<()> {
        // #1428: a node present in `node` but missing from `node_fts` (the pre-FTS
        // corpus) must be re-indexed by the one-time backfill.
        let (store, _t) = create_test_store().await?;

        let node = Node::new(
            "text".to_string(),
            "alpha uniquetoken9173".to_string(),
            json!({}),
        );
        let nid = node.id.clone();
        store.create_node(node, None, None).await?;

        // Simulate a pre-FTS node: drop it from the external-content index.
        let rowid: i64 = {
            let mut r = store
                .db
                .query(
                    "SELECT rowid FROM node WHERE id = ?1",
                    libsql::params![nid.clone()],
                )
                .await?;
            r.next().await?.unwrap().get(0)?
        };
        store
            .db
            .execute(
                "INSERT INTO node_fts(node_fts, rowid, id, content) VALUES('delete', ?1, ?2, ?3)",
                libsql::params![rowid, nid.clone(), "alpha uniquetoken9173"],
            )
            .await?;

        let matches = |store: Arc<SqliteStore>| async move {
            let mut r = store
                .db
                .query(
                    "SELECT count(*) FROM node_fts WHERE node_fts MATCH 'uniquetoken9173'",
                    (),
                )
                .await?;
            Ok::<i64, anyhow::Error>(r.next().await?.unwrap().get(0)?)
        };
        assert_eq!(
            matches(store.clone()).await?,
            0,
            "node should be missing from the index"
        );

        SqliteStore::backfill_fts_if_stale(&store.db).await?;

        assert_eq!(
            matches(store.clone()).await?,
            1,
            "backfill should re-index the node"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_create_and_get_node() -> Result<()> {
        let (store, _temp_dir) = create_test_store().await?;

        let node = Node::new("text".to_string(), "Test content".to_string(), json!({}));
        let created = store.create_node(node.clone(), None, None).await?;
        assert_eq!(created.id, node.id);
        assert_eq!(created.content, "Test content");

        let fetched = store.get_node(&node.id).await?;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, node.id);

        Ok(())
    }

    #[tokio::test]
    async fn test_sqlite_file_format() -> Result<()> {
        // Phase 1 acceptance criterion: verify the file is a valid SQLite file
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test_node.db");

        let store = SqliteStore::new(db_path.clone()).await?;

        let node = Node::new("text".to_string(), "Hello SQLite".to_string(), json!({}));
        store.create_node(node, None, None).await?;

        // Verify file exists and starts with SQLite magic bytes
        let file_bytes = std::fs::read(&db_path)?;
        assert!(file_bytes.len() > 16, "DB file too small");
        assert_eq!(
            &file_bytes[0..16],
            b"SQLite format 3\0",
            "Not a valid SQLite file"
        );

        Ok(())
    }

    // ---- sqlite-vec (#1221) ----

    /// One 768-dim embedding whose only nonzero component is `axis` (a unit vector).
    /// Two such vectors are identical iff they share an axis (cosine sim 1.0) and
    /// orthogonal otherwise (cosine sim 0.0) — handy for deterministic KNN assertions.
    fn unit_embedding(node_id: &str, axis: usize) -> crate::models::NewEmbedding {
        let mut vector = vec![0.0f32; 768];
        vector[axis] = 1.0;
        crate::models::NewEmbedding {
            node_id: node_id.to_string(),
            vector,
            model_name: Some("test-model".to_string()),
            chunk_index: 0,
            chunk_start: 0,
            chunk_end: 100,
            total_chunks: 1,
            content_hash: format!("hash-{axis}"),
            token_count: 10,
        }
    }

    fn unit_query(axis: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 768];
        v[axis] = 1.0;
        v
    }

    async fn vec_row_count(store: &SqliteStore) -> Result<i64> {
        let mut rows = store
            .db
            .query("SELECT COUNT(*) FROM vec_embeddings", ())
            .await?;
        Ok(rows.next().await?.unwrap().get(0)?)
    }

    #[tokio::test]
    async fn test_migrate_embedding_origin_upgrades_legacy_db() -> Result<()> {
        // A pre-#182 DB: `embedding` table WITHOUT `origin`, old index shape, with
        // a row already present (community desktop DBs aren't reset). The migration
        // must add the column (default 'local' for existing rows) and rebuild the
        // index, idempotently — otherwise embedding writes / the push sweep would
        // hit `no such column: origin`.
        let tmp = TempDir::new().unwrap();
        let db = libsql::Builder::new_local(tmp.path().join("legacy.db"))
            .build()
            .await?;
        let conn = db.connect()?;
        conn.execute(
            "CREATE TABLE embedding (id TEXT PRIMARY KEY, node_id TEXT NOT NULL, vector BLOB NOT NULL, \
             dimension INTEGER NOT NULL DEFAULT 768, model_name TEXT NOT NULL DEFAULT 'm', \
             chunk_index INTEGER NOT NULL DEFAULT 0, chunk_start INTEGER NOT NULL DEFAULT 0, \
             chunk_end INTEGER, total_chunks INTEGER NOT NULL DEFAULT 1, content_hash TEXT, \
             token_count INTEGER, stale INTEGER NOT NULL DEFAULT 1, error_count INTEGER NOT NULL DEFAULT 0, \
             last_error TEXT, created_at TEXT NOT NULL, modified_at TEXT NOT NULL)",
            (),
        )
        .await?;
        conn.execute(
            "CREATE INDEX idx_emb_modified ON embedding (modified_at, node_id, chunk_index)",
            (),
        )
        .await?;
        conn.execute(
            "INSERT INTO embedding (id, node_id, vector, created_at, modified_at) \
             VALUES ('e1', 'n1', x'00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await?;

        SqliteStore::migrate_embedding_origin(&conn).await?;

        // Column added; the existing row defaulted to 'local'.
        let mut rows = conn
            .query("SELECT origin FROM embedding WHERE id = 'e1'", ())
            .await?;
        let origin: String = rows.next().await?.unwrap().get(0)?;
        assert_eq!(origin, "local");

        // Index rebuilt to lead with `origin`.
        let mut idx = conn
            .query(
                "SELECT sql FROM sqlite_master WHERE type='index' AND name='idx_emb_modified'",
                (),
            )
            .await?;
        let sql: String = idx.next().await?.unwrap().get(0)?;
        assert!(
            sql.contains("origin"),
            "index must include origin; was: {sql}"
        );

        // Idempotent: a second run is a no-op.
        SqliteStore::migrate_embedding_origin(&conn).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_embeddings_roundtrip_and_modified_since() -> Result<()> {
        // #97 read-API: vectors must round-trip out of the le-f32 blob, both
        // chunks come back in order, and the modified-since cursor filters.
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "vec node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        let mut e0 = unit_embedding(&node.id, 0);
        e0.total_chunks = 2;
        let mut e1 = unit_embedding(&node.id, 5);
        e1.chunk_index = 1;
        e1.total_chunks = 2;
        store.upsert_embeddings(&node.id, vec![e0, e1]).await?;

        let got = store.get_embeddings(&node.id).await?;
        assert_eq!(got.len(), 2, "both chunks returned");
        assert_eq!(got[0].node, node.id);
        assert_eq!(got[0].chunk_index, 0);
        assert_eq!(got[0].vector.len(), 768);
        assert_eq!(got[0].vector[0], 1.0, "axis-0 unit vector round-trips");
        assert!(!got[0].stale, "freshly upserted vectors are not stale");
        assert_eq!(got[1].chunk_index, 1);
        assert_eq!(got[1].vector[5], 1.0, "axis-5 unit vector round-trips");

        // Cursor: epoch returns everything (local-origin), a far-future cursor none.
        let epoch = DateTime::<Utc>::from_timestamp(0, 0).unwrap();
        assert_eq!(store.embeddings_modified_since(epoch).await?.len(), 2);
        let future = Utc::now() + chrono::Duration::days(1);
        assert!(store.embeddings_modified_since(future).await?.is_empty());

        // Provenance (#182/#183): a node's REMOTE (pulled) embedding must NOT show
        // up in the push sweep, so a received vector is never re-pushed.
        let other = store
            .create_node(
                Node::new("text".to_string(), "remote node".to_string(), json!({})),
                None,
                None,
            )
            .await?;
        store
            .apply_remote_embeddings(&other.id, vec![unit_embedding(&other.id, 7)])
            .await?;
        // get_embeddings still returns it locally (it IS stored)...
        assert_eq!(store.get_embeddings(&other.id).await?.len(), 1);
        // ...but the push sweep only sees the 2 local-origin chunks, not the remote one.
        assert_eq!(
            store.embeddings_modified_since(epoch).await?.len(),
            2,
            "remote-origin embeddings are excluded from the push sweep"
        );

        // The recurring sweep must ride idx_emb_modified, not full-scan + filesort
        // (the #1416 review concern): assert the query plan uses the index and the
        // ORDER BY is index-covered.
        let mut plan = store
            .db
            .query(
                "EXPLAIN QUERY PLAN SELECT id FROM embedding WHERE origin = 'local' AND modified_at >= ?1 \
                 ORDER BY modified_at, node_id, chunk_index",
                libsql::params!["1970-01-01T00:00:00+00:00".to_string()],
            )
            .await?;
        let mut detail = String::new();
        while let Some(row) = plan.next().await? {
            let d: String = row.get(3)?; // EXPLAIN QUERY PLAN: (id, parent, notused, detail)
            detail.push_str(&d);
            detail.push(' ');
        }
        assert!(
            detail.contains("idx_emb_modified"),
            "sweep must use idx_emb_modified; plan was: {detail}"
        );
        assert!(
            !detail.to_uppercase().contains("TEMP B-TREE"),
            "ORDER BY must be index-covered (no filesort); plan was: {detail}"
        );

        // A node with no embeddings reads back empty (not an error).
        let other = store
            .create_node(
                Node::new("text".to_string(), "no emb".to_string(), json!({})),
                None,
                None,
            )
            .await?;
        assert!(store.get_embeddings(&other.id).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_search_embeddings_self_query() -> Result<()> {
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "vec node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        store
            .upsert_embeddings(&node.id, vec![unit_embedding(&node.id, 0)])
            .await?;

        let results = store
            .search_embeddings(&unit_query(0), 10, Some(0.5))
            .await?;

        assert!(!results.is_empty(), "self-query should match");
        assert_eq!(results[0].node_id, node.id);
        assert!(
            (results[0].max_similarity - 1.0).abs() < 1e-3,
            "self-similarity should be ~1.0, got {}",
            results[0].max_similarity
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_delete_embeddings_clears_vec0() -> Result<()> {
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "vec node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        store
            .upsert_embeddings(&node.id, vec![unit_embedding(&node.id, 0)])
            .await?;
        assert_eq!(vec_row_count(&store).await?, 1);
        assert!(!store
            .search_embeddings(&unit_query(0), 10, Some(0.5))
            .await?
            .is_empty());

        store.delete_embeddings(&node.id).await?;

        assert_eq!(vec_row_count(&store).await?, 0, "vec0 should be cleared");
        assert!(
            store
                .search_embeddings(&unit_query(0), 10, Some(0.5))
                .await?
                .is_empty(),
            "search should return nothing after delete"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_reupsert_replaces_vec0() -> Result<()> {
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "vec node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        // First embed on axis 0, then re-embed on axis 1.
        store
            .upsert_embeddings(&node.id, vec![unit_embedding(&node.id, 0)])
            .await?;
        store
            .upsert_embeddings(&node.id, vec![unit_embedding(&node.id, 1)])
            .await?;

        assert_eq!(vec_row_count(&store).await?, 1, "no stale duplicate rows");

        // Query by the NEW axis ranks the node highly...
        let by_new = store
            .search_embeddings(&unit_query(1), 10, Some(0.5))
            .await?;
        assert_eq!(by_new[0].node_id, node.id);
        assert!((by_new[0].max_similarity - 1.0).abs() < 1e-3);

        // ...and the OLD axis no longer matches (orthogonal → sim 0).
        let by_old = store
            .search_embeddings(&unit_query(0), 10, Some(0.5))
            .await?;
        assert!(
            by_old.iter().all(|r| r.node_id != node.id),
            "old vector should no longer match"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_mark_stale_excludes_from_search() -> Result<()> {
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "vec node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        store
            .upsert_embeddings(&node.id, vec![unit_embedding(&node.id, 0)])
            .await?;
        store.mark_root_embedding_stale(&node.id).await?;

        assert_eq!(
            vec_row_count(&store).await?,
            0,
            "stale node removed from vec0"
        );
        assert!(
            store
                .search_embeddings(&unit_query(0), 10, Some(0.5))
                .await?
                .is_empty(),
            "stale embeddings must not be searchable"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_stale_marker_never_in_knn() -> Result<()> {
        let (store, _tmp) = create_test_store().await?;
        let node = store
            .create_node(
                Node::new("text".to_string(), "marker node".to_string(), json!({})),
                None,
                None,
            )
            .await?;

        // A placeholder marker carries a dummy unit [1,0,0,...] vector but stale=1.
        store.create_stale_embedding_marker(&node.id).await?;

        assert_eq!(
            vec_row_count(&store).await?,
            0,
            "markers not mirrored to vec0"
        );
        // Search by the placeholder's own vector must not surface the marker node.
        let results = store
            .search_embeddings(&unit_query(0), 10, Some(0.5))
            .await?;
        assert!(
            results.iter().all(|r| r.node_id != node.id),
            "stale marker must never appear in KNN results"
        );
        Ok(())
    }

    // C3c: store-layer atomicity — exercises the in-transaction version check
    // that the service-level pre-validation cannot catch (concurrent version bump).
    #[tokio::test]
    async fn test_move_children_to_parent_store_rejects_stale_version() -> Result<()> {
        let (store, _temp) = create_test_store().await?;

        let parent = Node::new("text".to_string(), "Parent".to_string(), json!({}));
        let parent_id = parent.id.clone();
        store.create_node(parent, None, None).await?;

        let new_parent = Node::new("text".to_string(), "New Parent".to_string(), json!({}));
        let new_parent_id = new_parent.id.clone();
        store.create_node(new_parent, None, None).await?;

        // Create children and wire parent edges via move_node (store layer).
        let child1 = Node::new("text".to_string(), "Child 1".to_string(), json!({}));
        let child1_id = child1.id.clone();
        let child1_version = child1.version;
        store.create_node(child1, None, None).await?;
        store.move_node(&child1_id, Some(&parent_id), None).await?;

        let child2 = Node::new("text".to_string(), "Child 2".to_string(), json!({}));
        let child2_id = child2.id.clone();
        // Use a stale version (0) — this bypasses service-level pre-validation,
        // so only the in-transaction SELECT changes() check catches the mismatch.
        let stale_version: i64 = 0;
        store.create_node(child2, None, None).await?;
        store.move_node(&child2_id, Some(&parent_id), None).await?;

        let result = store
            .move_children_to_parent(
                &new_parent_id,
                &[
                    (child1_id.as_str(), child1_version),
                    (child2_id.as_str(), stale_version),
                ],
            )
            .await;

        assert!(
            result.is_err(),
            "stale version should cause store-level failure"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("VERSION_CONFLICT"),
            "error should contain VERSION_CONFLICT, got: {}",
            err_msg
        );

        // ALL-OR-NOTHING: child1 must NOT have moved (transaction was rolled back).
        let parent_of_child1 = store.get_parent_id(&child1_id).await?;
        assert_eq!(
            parent_of_child1.as_deref(),
            Some(parent_id.as_str()),
            "child1 should still be under original parent after rollback"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_move_node_cross_parent_leaves_exactly_one_edge() -> Result<()> {
        // #1429: the cross-parent move deletes the old has_child edge and inserts
        // the new one in ONE transaction. After a successful move the node must
        // have EXACTLY ONE has_child edge, pointing at the new parent — never zero
        // (orphaned root, the bug when a non-transactional INSERT failed after the
        // DELETE committed) and never two (old edge left behind).
        let (store, _temp) = create_test_store().await?;

        let parent_a = Node::new("text".to_string(), "Parent A".to_string(), json!({}));
        let parent_a_id = parent_a.id.clone();
        store.create_node(parent_a, None, None).await?;

        let parent_b = Node::new("text".to_string(), "Parent B".to_string(), json!({}));
        let parent_b_id = parent_b.id.clone();
        store.create_node(parent_b, None, None).await?;

        let child = Node::new("text".to_string(), "Child".to_string(), json!({}));
        let child_id = child.id.clone();
        store.create_node(child, None, None).await?;

        // Wire under A, then move to B — this exercises the cross-parent branch.
        store.move_node(&child_id, Some(&parent_a_id), None).await?;
        store.move_node(&child_id, Some(&parent_b_id), None).await?;

        let mut rows = store
            .db
            .query(
                "SELECT in_node FROM relationship WHERE out_node = ?1 AND relationship_type = 'has_child'",
                libsql::params![child_id.clone()],
            )
            .await?;
        let mut parents: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            parents.push(row.get(0)?);
        }
        assert_eq!(
            parents,
            vec![parent_b_id.clone()],
            "after a cross-parent move the child must have exactly one has_child edge, under the new parent"
        );
        assert_eq!(
            store.get_parent_id(&child_id).await?.as_deref(),
            Some(parent_b_id.as_str()),
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_move_children_to_parent_store_success_and_order() -> Result<()> {
        let (store, _temp) = create_test_store().await?;

        let parent = Node::new("text".to_string(), "Parent".to_string(), json!({}));
        let parent_id = parent.id.clone();
        store.create_node(parent, None, None).await?;

        let new_parent = Node::new("text".to_string(), "New Parent".to_string(), json!({}));
        let new_parent_id = new_parent.id.clone();
        store.create_node(new_parent, None, None).await?;

        let mut child_ids = Vec::new();
        let mut child_versions = Vec::new();
        for i in 0..3 {
            let child = Node::new("text".to_string(), format!("Child {}", i), json!({}));
            let cid = child.id.clone();
            let ver = child.version;
            store.create_node(child, None, None).await?;
            store.move_node(&cid, Some(&parent_id), None).await?;
            child_ids.push(cid);
            child_versions.push(ver);
        }

        let pairs: Vec<(&str, i64)> = child_ids
            .iter()
            .zip(child_versions.iter())
            .map(|(id, &ver)| (id.as_str(), ver))
            .collect();

        let orders = store
            .move_children_to_parent(&new_parent_id, &pairs)
            .await?;

        assert_eq!(orders.len(), 3);
        // Orders must be strictly increasing (sibling order preserved).
        assert!(orders[0] < orders[1] && orders[1] < orders[2]);

        // All children now live under new_parent.
        let new_children = store.get_children(&new_parent_id).await?;
        let new_ids: Vec<&str> = new_children.iter().map(|n| n.id.as_str()).collect();
        for id in &child_ids {
            assert!(
                new_ids.contains(&id.as_str()),
                "child {} should be under new_parent",
                id
            );
        }

        Ok(())
    }
}
