//! Request-scoped database routing middleware (ADR-053: "One Daemon, Multiple
//! Local Databases").
//!
//! [`DbManagerLayer`] injects the process-global [`DatabaseManager`] into every
//! request's extensions so the per-database gRPC handlers can resolve the
//! `x-ns-database-id` routing header and dispatch to the right database. The
//! layer is installed only by the community/core serve loops; the Pro daemon
//! (`nodespaced-pro`) does not install it, so its handlers find no manager in
//! the extensions and fall back to the default database — behavior identical to
//! before this stage.
//!
//! The layer is a thin synchronous injector: it only clones an `Arc` into the
//! request extensions. The (async) resolve-and-open work happens inside the
//! handlers, where errors turn naturally into `tonic::Status`.

use std::sync::Arc;
use std::task::{Context, Poll};

use tonic::codegen::http;
use tower::{Layer, Service};

use crate::services::database_manager::DatabaseManager;

/// The metadata/header key a client sets to target a specific registered
/// database. Absent → the default database (single-database clients are
/// unchanged); present but unregistered → the handler rejects the request
/// rather than silently serving the default (no cross-database leak).
pub const DATABASE_ID_HEADER: &str = "x-ns-database-id";

/// `tower::Layer` that inserts the shared [`Arc<DatabaseManager>`] into each
/// request's extensions. See the module docs for why this is core-only and
/// behavior-preserving for the Pro daemon.
#[derive(Clone)]
pub struct DbManagerLayer {
    manager: Arc<DatabaseManager>,
}

impl DbManagerLayer {
    pub fn new(manager: Arc<DatabaseManager>) -> Self {
        Self { manager }
    }
}

impl<S> Layer<S> for DbManagerLayer {
    type Service = DbManagerInjector<S>;

    fn layer(&self, inner: S) -> Self::Service {
        DbManagerInjector {
            inner,
            manager: self.manager.clone(),
        }
    }
}

/// The [`Service`] produced by [`DbManagerLayer`]. Clones the manager into the
/// request extensions, then defers to the inner service unchanged.
#[derive(Clone)]
pub struct DbManagerInjector<S> {
    inner: S,
    manager: Arc<DatabaseManager>,
}

impl<S, B> Service<http::Request<B>> for DbManagerInjector<S>
where
    S: Service<http::Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
        // tonic carries http-request extensions through to the typed
        // `tonic::Request`, so handlers read this via `req.extensions()`.
        req.extensions_mut().insert(self.manager.clone());
        self.inner.call(req)
    }
}
