//! Versioned DDL migration runner.
//!
//! Schema changes are tracked via SQLite's `PRAGMA user_version` (an integer stored
//! in the database file header). On startup, `run` compares the database's current
//! version against the known migrations and applies any with a higher version, in
//! order, each in its own transaction that also bumps `user_version` — so a crash
//! mid-migration leaves the database at a consistent, re-resumable version.
//!
//! Adding a schema change: add a new `vNNN_description` module below, add its
//! version to `LATEST_VERSION`, and add a matching arm to `apply_migration`. Never
//! edit a past migration's `apply` fn once it has shipped — existing databases will
//! already have applied it under its old definition, and rerunning a changed
//! version won't happen (each migration runs at most once per database).

mod v001_initial_schema;
mod v002_embedding_origin;
mod v003_property_indexes;

use anyhow::{Context, Result};

/// Highest migration version known to this build. Bump when adding a migration.
pub const LATEST_VERSION: i64 = 3;

async fn apply_migration(tx: &libsql::Transaction, version: i64) -> Result<()> {
    match version {
        1 => v001_initial_schema::apply(tx).await,
        2 => v002_embedding_origin::apply(tx).await,
        3 => v003_property_indexes::apply(tx).await,
        _ => unreachable!("no migration defined for version {version}"),
    }
}

fn migration_name(version: i64) -> &'static str {
    match version {
        1 => "initial_schema",
        2 => "embedding_origin",
        3 => "property_indexes",
        _ => "unknown",
    }
}

/// Read `PRAGMA user_version` (0 on a brand-new database).
async fn current_version(conn: &libsql::Connection) -> Result<i64> {
    let mut rows = conn
        .query("PRAGMA user_version", ())
        .await
        .context("Failed to read PRAGMA user_version")?;
    let version: i64 = rows
        .next()
        .await?
        .map(|row| row.get(0))
        .transpose()?
        .unwrap_or(0);
    Ok(version)
}

/// Apply all migrations with `current_version < version <= target_version`, in
/// ascending order. Each migration runs in its own transaction that also bumps
/// `user_version` to that migration's version, so a database is never left
/// between two versions.
pub async fn run_up_to(conn: &libsql::Connection, target_version: i64) -> Result<()> {
    let start_version = current_version(conn).await?;

    for version in (start_version + 1)..=target_version {
        let tx = conn
            .transaction()
            .await
            .with_context(|| format!("Failed to begin transaction for migration {version}"))?;

        apply_migration(&tx, version).await.with_context(|| {
            format!(
                "Migration {:03}_{} failed",
                version,
                migration_name(version)
            )
        })?;

        tx.execute(&format!("PRAGMA user_version = {version}"), ())
            .await
            .with_context(|| format!("Failed to bump user_version to {version}"))?;

        tx.commit()
            .await
            .with_context(|| format!("Failed to commit migration {version}"))?;
    }

    Ok(())
}

/// Apply all pending migrations, bringing the database to [`LATEST_VERSION`].
pub async fn run(conn: &libsql::Connection) -> Result<()> {
    run_up_to(conn, LATEST_VERSION).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against the panic in `apply_migration`'s `unreachable!` arm: if
    /// `LATEST_VERSION` is bumped without adding a matching `apply_migration` arm,
    /// `run` would panic against a real user database instead of failing a build.
    /// This test converts that mistake into a compile-time-adjacent, always-run
    /// test failure instead.
    #[tokio::test]
    async fn every_version_up_to_latest_has_a_defined_migration() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("version_coverage.db");
        crate::db::ensure_sqlite_vec_registered().await;
        let database = libsql::Builder::new_local(&db_path).build().await.unwrap();
        let conn = database.connect().unwrap();

        // Panics via `unreachable!` if any version in range lacks an arm.
        run_up_to(&conn, LATEST_VERSION).await.unwrap();
    }
}
