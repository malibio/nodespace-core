//! The store's SQLite connections — deliberately sealed inside this module.
//!
//! Rust privacy is per-module and *descends*: a private field of `SqliteStore`
//! is still reachable from `sqlite_store::nodes`, `::relationships`,
//! `::embeddings` and `::search`, because those are child modules of the module
//! that declares it. So holding the raw `libsql::Database` as a private field
//! of `SqliteStore` would NOT stop a sibling module from writing
//! `self.database.connect()?.execute(…)` and bypassing the read/write
//! discipline entirely — silently reintroducing exactly the bug this design
//! exists to prevent, with no compiler error.
//!
//! Putting the raw handles here fixes that. `sqlite_store` and its children are
//! not descendants of `sqlite_store::connections`, so the fields below are
//! genuinely unreachable from them: the only ways to obtain a connection are
//! [`Connections::write`] and [`Connections::read`].
//!
//! What is enforced, precisely:
//! - a connection cannot be *minted* outside this module;
//! - a mutating statement cannot be reached through the read path, because
//!   [`ReadConn`] exposes `query` and nothing else.
//!
//! What is not, and cannot be without wrapping libsql's whole surface: a
//! `WriteGuard` derefs to a real `libsql::Connection`, so code that has already
//! been *handed* one can clone it and keep it past the guard. That is visible
//! and deliberate at its one call site (a test that needs a raw held
//! `BEGIN IMMEDIATE`), not something a caller does by accident.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

/// Idle reader connections beyond this count are closed instead of pooled.
/// The pool grows to the peak number of *simultaneously in-flight* reads and
/// then settles back to this many idle handles.
const MAX_IDLE_READERS: usize = 8;

/// How long a connection waits on a lock held by another connection (or
/// process) before giving up with `SQLITE_BUSY`. Applies to both roles: the
/// writer contends with other processes' writers, a reader with WAL recovery.
const BUSY_TIMEOUT_MS: u32 = 5_000;

/// Exclusive access to the store's single writer connection.
///
/// Held for the whole span of a mutating operation — including any reads that
/// operation's writes are computed from — so the operation is atomic with
/// respect to every other writer. Deref gives the underlying connection, so a
/// guard is used exactly like a connection.
pub(crate) type WriteGuard<'a> = tokio::sync::MutexGuard<'a, libsql::Connection>;

/// The store's connections: one writer behind a mutex, and a pool of read-only
/// connections.
pub(crate) struct Connections {
    /// Mints reader connections. Private to this module — see the module doc
    /// for why that placement is the enforcement mechanism, not a formality.
    database: libsql::Database,
    /// Idle reader connections. Each is `PRAGMA query_only`, and no transaction
    /// is ever opened on one, so a read sees a consistent COMMITTED snapshot —
    /// never another task's half-written transaction, and never swept into one.
    /// Under WAL, readers neither block nor are blocked by the writer.
    ///
    /// A checkout is exclusive for the life of its cursor, which is what keeps
    /// reads *fresh* as well as clean (see [`ReadConn::query`]).
    ///
    /// Starts empty and fills on demand, so no reader connection exists until
    /// after migrations have run.
    readers: Arc<Mutex<Vec<libsql::Connection>>>,
    /// The store's only writable connection, reachable exclusively through
    /// [`Connections::write`]. Every mutating statement in the store — whether
    /// it opens a transaction or is a lone `execute` — goes through this mutex.
    ///
    /// This is required for correctness, not merely for tidiness. libsql's
    /// local `transaction()` is a bare `BEGIN DEFERRED` on the connection with
    /// `Drop = ROLLBACK`; it carries no per-task context. So on a shared
    /// connection, an unsynchronized `execute` from another task lands *inside*
    /// whatever transaction happens to be open — to be committed or rolled back
    /// with it — and a second `transaction()` fails outright with "cannot start
    /// a transaction within a transaction" instead of waiting. Serializing here
    /// makes both impossible.
    ///
    /// Serializing writers costs no real concurrency: SQLite permits exactly
    /// one writer at a time regardless, so the alternative is not parallel
    /// writes but `SQLITE_BUSY` after `busy_timeout`. This mutex converts that
    /// failure into a queue. Reads are unaffected — they run on their own
    /// connections.
    writer: tokio::sync::Mutex<libsql::Connection>,
}

impl Connections {
    /// Open the database and its writer connection.
    ///
    /// The caller is responsible for having registered any SQLite extensions
    /// (`sqlite-vec`) beforehand — every connection this type mints, now or
    /// later, inherits the auto-extension state at open time.
    pub(crate) async fn open(db_path: &Path) -> Result<Self> {
        let database = libsql::Builder::new_local(db_path)
            .build()
            .await
            .context("Failed to build libsql database")?;
        let writer = database
            .connect()
            .context("Failed to connect to libsql database")?;
        apply_writer_pragmas(&writer).await?;

        Ok(Self {
            database,
            readers: Arc::new(Mutex::new(Vec::new())),
            writer: tokio::sync::Mutex::new(writer),
        })
    }

    /// Acquire exclusive use of the writer connection.
    ///
    /// Every mutating statement in the store must go through a guard from this
    /// method, and the guard must stay alive for the whole span the operation
    /// needs to be atomic — a multi-statement transaction, and any read whose
    /// result the subsequent write depends on (sibling-order reads, `SELECT
    /// changes()` OCC probes, …).
    ///
    /// The guard is **not** re-entrant. A method holding one must not call
    /// another store method that also takes one; pass the guard down to a
    /// helper (`&libsql::Connection`) or drop it first. Read-path methods take
    /// no guard, so calling one while holding a write guard is safe — but it
    /// reads through a pooled reader connection, so it will NOT see the guard
    /// holder's uncommitted writes.
    pub(crate) async fn write(&self) -> WriteGuard<'_> {
        self.writer.lock().await
    }

    /// Check out a reader connection. Takes no write lock, so reads never queue
    /// behind a long write (a bulk import, a subtree delete) and never block
    /// one — WAL gives each read a committed snapshot of its own.
    ///
    /// Reuses a pooled connection when one is idle and opens a new one
    /// otherwise, so two reads that overlap in time never share a connection.
    pub(crate) async fn read(&self) -> Result<ReadConn> {
        let pooled = lock_pool(&self.readers).pop();
        let conn = match pooled {
            Some(conn) => conn,
            None => open_reader(&self.database).await?,
        };
        Ok(ReadConn {
            conn: Some(conn),
            pool: self.readers.clone(),
            healthy: true,
        })
    }
}

/// Lock the reader pool, recovering from poisoning rather than propagating it.
///
/// The pool is a plain `Vec` of connections and the critical sections are a
/// single `pop`/`push`, so a panic elsewhere in the process cannot leave it
/// logically inconsistent — only flagged. Treating that flag as fatal would
/// turn any unrelated panic into a permanent, silent read outage for the rest
/// of the daemon's life, which is a far worse outcome than reusing a `Vec` that
/// is definitionally fine.
fn lock_pool(
    pool: &Mutex<Vec<libsql::Connection>>,
) -> std::sync::MutexGuard<'_, Vec<libsql::Connection>> {
    pool.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Per-connection session settings for the writer. Unlike the tables/indexes in
/// `db::migrations`, these are NOT persisted schema state — `journal_mode` is
/// durable in the DB file but re-asserting it is harmless, while
/// `foreign_keys`, `synchronous`, and `busy_timeout` reset to SQLite defaults
/// on every new connection and must be set every time. Must run outside any
/// transaction: SQLite forbids changing `synchronous` inside one.
async fn apply_writer_pragmas(conn: &libsql::Connection) -> Result<()> {
    conn.query("PRAGMA journal_mode = WAL", ())
        .await
        .context("Failed to set journal_mode")?;
    conn.query("PRAGMA foreign_keys = ON", ())
        .await
        .context("Failed to set foreign_keys")?;
    conn.query("PRAGMA synchronous = NORMAL", ())
        .await
        .context("Failed to set synchronous")?;
    conn.query(&format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}"), ())
        .await
        .context("Failed to set busy_timeout")?;
    Ok(())
}

/// Open and configure one read-only connection.
///
/// Deliberately narrower than [`apply_writer_pragmas`]: `journal_mode` is a
/// durable property of the database file (already WAL, set by the writer) and
/// re-asserting it costs a lock on every pool miss, while `synchronous` and
/// `foreign_keys` only govern writes this connection cannot perform.
/// `busy_timeout` still matters — a reader can contend on WAL recovery.
async fn open_reader(database: &libsql::Database) -> Result<libsql::Connection> {
    let conn = database
        .connect()
        .context("Failed to open read connection to libsql database")?;
    conn.query(&format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}"), ())
        .await
        .context("Failed to set busy_timeout on read connection")?;
    conn.query("PRAGMA query_only = ON", ())
        .await
        .context("Failed to mark read connection query-only")?;
    Ok(conn)
}

/// A reader connection checked out of the store's pool, returned by
/// [`Connections::read`]. Goes back to the pool when dropped.
///
/// It deliberately exposes `query` and nothing else, so a mutating statement
/// can only be reached through [`Connections::write`] — "this write forgot the
/// lock" is a compile error rather than a race nobody notices until data goes
/// missing.
pub(crate) struct ReadConn {
    /// `Some` until `Drop` hands the connection back.
    conn: Option<libsql::Connection>,
    pool: Arc<Mutex<Vec<libsql::Connection>>>,
    /// Cleared when a statement on this connection fails. An errored connection
    /// is closed rather than pooled: most read errors are benign (bad SQL), but
    /// the ones that are not — I/O failure, corruption, an interrupted
    /// statement — would otherwise be handed to the next unsuspecting caller,
    /// and replacing a connection costs one `open` on the next miss.
    healthy: bool,
}

impl ReadConn {
    /// Run a query, consuming the checkout: the resulting [`ReadRows`] owns this
    /// connection and only releases it once the cursor is dropped.
    ///
    /// That ownership is not ceremony. A SQLite connection with a partially
    /// consumed statement is inside an implicit read transaction, and every
    /// other statement on that same connection is pinned to its snapshot until
    /// it finishes. Since these cursors are drained across `await` points,
    /// sharing one reader connection would let one task's in-flight cursor make
    /// another task's read arbitrarily stale — including a read of a row that
    /// task had itself just committed.
    pub(crate) async fn query(
        mut self,
        sql: &str,
        params: impl libsql::params::IntoParams,
    ) -> libsql::Result<ReadRows> {
        let result = self
            .conn
            .as_ref()
            .expect("connection is taken only in Drop")
            .query(sql, params)
            .await;
        match result {
            Ok(rows) => Ok(ReadRows { rows, conn: self }),
            Err(e) => {
                self.healthy = false;
                Err(e)
            }
        }
    }
}

impl Drop for ReadConn {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            if !self.healthy {
                return;
            }
            let mut pool = lock_pool(&self.pool);
            if pool.len() < MAX_IDLE_READERS {
                pool.push(conn);
            }
        }
    }
}

/// A result cursor that holds its reader connection for as long as it lives.
///
/// Field order matters: `rows` is declared first so it drops first, finalizing
/// the statement and ending the implicit read transaction *before* the
/// connection is returned to the pool. A connection therefore never re-enters
/// the pool pinned to a stale snapshot. Do not reorder these fields.
pub(crate) struct ReadRows {
    rows: libsql::Rows,
    conn: ReadConn,
}

impl ReadRows {
    pub(crate) async fn next(&mut self) -> libsql::Result<Option<libsql::Row>> {
        match self.rows.next().await {
            Ok(row) => Ok(row),
            Err(e) => {
                // Retire rather than pool a connection whose cursor failed
                // mid-iteration; see `ReadConn::healthy`.
                self.conn.healthy = false;
                Err(e)
            }
        }
    }
}
