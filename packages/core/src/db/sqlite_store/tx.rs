//! The unit-of-work transaction seam (ADR-069 §1a).
//!
//! [`Tx`] is a newtype over [`libsql::Transaction`], not the raw type, so the
//! two footguns `connections.rs` documents cannot resurface through this
//! door: a `Tx` cannot open a nested transaction (it exposes no `write()` and
//! no way to reach `SqliteStore::conns`), and there is no way to obtain a
//! second one on the same connection while the first is alive. `_in_tx` store
//! methods take `&Tx` and only ever call `Tx::conn()` — they never touch
//! `self.conns` — so "this write forgot the transaction" is a compile error
//! rather than a runtime race.
//!
//! Committing or rolling back consumes the underlying `libsql::Transaction`
//! by value, which is why [`Tx`] stores it as `Option` internally: the public
//! surface never lets an `_in_tx` body commit or roll back on its own —
//! only [`SqliteStore::with_transaction`] does, once, after its closure
//! returns.

use anyhow::{Context, Result};
use std::future::Future;
use std::pin::Pin;

use super::connections::WriteGuard;
use super::SqliteStore;

/// A boxed, transaction-scoped future — the shape `with_transaction`'s
/// closure must return, since a bare `async fn` argument cannot name the
/// higher-ranked lifetime `'t` ties to.
pub type TxFuture<'t, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 't>>;

/// A unit of work in progress. See the module doc for what this type
/// deliberately does not expose.
pub struct Tx<'t> {
    /// `Some` until `with_transaction` commits or rolls back. Never taken by
    /// anything reachable from outside this module.
    inner: Option<libsql::Transaction>,
    /// Keeps the write guard alive for the transaction's whole span. Never
    /// read — its only job is to not be dropped early, exactly like every
    /// other multi-statement store method today.
    _guard: WriteGuard<'t>,
}

impl<'t> Tx<'t> {
    /// The live connection to run statements against. `_in_tx` methods use
    /// this and nothing else — never `SqliteStore::write()`, which would
    /// deadlock (the guard above is already held) or, if it somehow didn't,
    /// would open a second `BEGIN` on the same connection and fail outright.
    pub(crate) fn conn(&self) -> &libsql::Transaction {
        self.inner
            .as_ref()
            .expect("Tx::conn called after commit/rollback — unreachable outside this module")
    }
}

impl SqliteStore {
    /// Run `f` inside one transaction: takes the write guard once, opens one
    /// `db.transaction()`, runs the closure, and commits on `Ok` / rolls back
    /// on `Err`. See ADR-069 §1a.
    ///
    /// Store write methods composed inside `f` must be their `_in_tx`
    /// variants, taking `&Tx`. Calling a non-`_in_tx` method (which opens its
    /// own transaction via `self.write()`) from inside `f` deadlocks, since
    /// the write guard is already held for the duration of this call.
    pub async fn with_transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: for<'t> FnOnce(&'t Tx<'t>) -> TxFuture<'t, T>,
    {
        let guard = self.write().await;
        let raw_tx = guard
            .transaction()
            .await
            .context("Failed to begin transaction")?;
        let tx = Tx {
            inner: Some(raw_tx),
            _guard: guard,
        };

        let result = f(&tx).await;

        // `inner` is `Some` until here in every reachable path — `Tx::conn`
        // is the only accessor and never takes it — so this unwrap cannot
        // observe `None`.
        let raw_tx = tx
            .inner
            .expect("Tx's inner transaction is only taken by commit/rollback below");

        match result {
            Ok(value) => {
                raw_tx
                    .commit()
                    .await
                    .context("Failed to commit transaction")?;
                Ok(value)
            }
            Err(e) => {
                // Best-effort: `Drop = ROLLBACK` already covers this if the
                // explicit rollback itself fails, so a rollback error is
                // logged, not propagated — propagating it would shadow the
                // real failure in `e`, which is what the caller needs to see.
                if let Err(rollback_err) = raw_tx.rollback().await {
                    tracing::warn!(
                        error = %rollback_err,
                        "explicit rollback failed after unit-of-work error (Drop = ROLLBACK still applies)"
                    );
                }
                Err(e)
            }
        }
    }
}
