//! Coverage for the versioned DDL migration runner (`db::migrations::run`):
//! fresh DB, already-current DB, and a multi-step upgrade path.

use nodespace_core::db::migrations::{self, LATEST_VERSION};

async fn open_raw(db_path: &std::path::Path) -> libsql::Connection {
    nodespace_core::db::ensure_sqlite_vec_registered().await;
    let database = libsql::Builder::new_local(db_path)
        .build()
        .await
        .expect("build libsql database");
    database.connect().expect("connect")
}

async fn user_version(conn: &libsql::Connection) -> i64 {
    let mut rows = conn
        .query("PRAGMA user_version", ())
        .await
        .expect("query user_version");
    rows.next()
        .await
        .expect("read row")
        .map(|row| row.get(0).expect("read version"))
        .unwrap_or(0)
}

async fn has_column(conn: &libsql::Connection, table: &str, column: &str) -> bool {
    let mut rows = conn
        .query(&format!("PRAGMA table_info({table})"), ())
        .await
        .expect("table_info");
    while let Some(row) = rows.next().await.expect("next row") {
        let name: String = row.get(1).expect("column name");
        if name == column {
            return true;
        }
    }
    false
}

#[tokio::test]
async fn fresh_database_ends_up_at_latest_version_with_full_schema() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("fresh.db");
    let conn = open_raw(&db_path).await;

    migrations::run(&conn).await.expect("run migrations");

    assert_eq!(user_version(&conn).await, LATEST_VERSION);
    assert!(has_column(&conn, "node", "id").await);
    assert!(has_column(&conn, "embedding", "origin").await);

    // Fresh DBs go straight to the latest schema — no migration is skipped.
    let mut rows = conn
        .query(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name='idx_emb_modified'",
            (),
        )
        .await
        .unwrap();
    let sql: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert!(
        sql.contains("origin"),
        "idx_emb_modified should be rebuilt with origin leading: {sql}"
    );
}

#[tokio::test]
async fn already_current_database_is_a_no_op() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("current.db");
    let conn = open_raw(&db_path).await;

    migrations::run(&conn).await.expect("first run");
    assert_eq!(user_version(&conn).await, LATEST_VERSION);

    // Insert a row so a re-applied migration (e.g. a non-idempotent ALTER TABLE)
    // would be detectable by a failure, not just a silent duplicate.
    conn.execute(
        "INSERT INTO node (id, node_type, content, properties, lifecycle_status, version, created_at, modified_at) \
         VALUES ('n1', 'text', 'hello', '{}', 'active', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        (),
    )
    .await
    .expect("insert node");

    migrations::run(&conn)
        .await
        .expect("second run must be a no-op");
    assert_eq!(user_version(&conn).await, LATEST_VERSION);

    let mut rows = conn.query("SELECT count(*) FROM node", ()).await.unwrap();
    let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(count, 1, "existing data must survive a no-op migration run");
}

#[tokio::test]
async fn pre_runner_database_with_origin_already_applied_upgrades_without_error() {
    // Regression: databases from before this migration runner existed may have
    // had `embedding.origin` added by the old ad-hoc `migrate_embedding_origin`
    // check, which ran outside `user_version` tracking. Such a DB has the column
    // present but `user_version` still at 0. Migration 2's ALTER TABLE must not
    // blow up with "duplicate column name" in that case.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("pre_runner.db");
    let conn = open_raw(&db_path).await;

    migrations::run_up_to(&conn, 1).await.expect("apply v1");
    conn.execute(
        "ALTER TABLE embedding ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'",
        (),
    )
    .await
    .expect("simulate pre-existing ad-hoc origin column");
    assert_eq!(
        user_version(&conn).await,
        1,
        "precondition: version stayed at 1, only the column was added out-of-band"
    );

    migrations::run(&conn)
        .await
        .expect("must not fail on a DB that already has origin");

    assert_eq!(user_version(&conn).await, LATEST_VERSION);
    assert!(has_column(&conn, "embedding", "origin").await);
}

#[tokio::test]
async fn database_on_migration_one_upgrades_through_all_subsequent_migrations() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("upgrade.db");
    let conn = open_raw(&db_path).await;

    // Simulate a database that only ever had migration 1 applied (pre-`origin`
    // column), the way a real pre-existing user database would look.
    migrations::run_up_to(&conn, 1).await.expect("apply v1");

    assert!(
        !has_column(&conn, "embedding", "origin").await,
        "precondition: v1-only DB must not yet have the origin column"
    );

    migrations::run(&conn).await.expect("upgrade from v1");

    assert_eq!(user_version(&conn).await, LATEST_VERSION);
    assert!(
        has_column(&conn, "embedding", "origin").await,
        "upgrading from v1 must apply migration 2 (embedding.origin)"
    );
}
