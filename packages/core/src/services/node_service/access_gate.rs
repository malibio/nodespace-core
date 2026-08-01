//! Pre-delete subtree access gate (ADR-041 "CASCADE requires read access across the whole subtree").
//!
//! `delete_subtree_atomic` hard-deletes a `has_child` subtree unconditionally — that's correct
//! for OCC (a concurrent edit to a descendant must not abort the delete) but says nothing about
//! whether the actor may *read* every descendant being destroyed. Community installs have no
//! access-control concept (single local user, no tenant), so the gate is a no-op there. A synced
//! Pro daemon (`nodespaced-pro`, built in the sibling `nodespace-sync` repo) supplies a real
//! implementation backed by the tenant's access predicate.
//!
//! The gate's authority is the Pro tenant schema (Postgres RLS), not this trait — this is
//! advisory/UX plumbing that surfaces the refusal before committing a local delete a synced
//! peer would otherwise reject. See ADR-041 and `architecture/tenant-isolation-rls.md` (P7).

use async_trait::async_trait;

/// Outcome of a pre-delete subtree readability check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtreeAccessDecision {
    /// The actor can read every node in the checked set.
    Allowed,
    /// At least one node in the checked set is unreadable by the actor.
    ///
    /// `inaccessible_count` is the minimum disclosure needed to explain the refusal — no
    /// identifying information (ids, names, types) about the inaccessible nodes is carried here.
    Denied { inaccessible_count: u64 },
}

/// Checks whether the current actor may read every node in a `has_child` subtree, before a
/// cascade delete commits.
///
/// Implementors receive the exact id set `delete_subtree_atomic` is about to delete (target +
/// all descendants), computed once by the caller so the walk isn't duplicated.
#[async_trait]
pub trait SubtreeAccessGate: Send + Sync {
    /// `node_ids` is the target node followed by every descendant that would be deleted.
    async fn check_subtree_access(&self, node_ids: &[String]) -> SubtreeAccessDecision;
}

/// Default gate for community (non-Pro) installs: always allows.
///
/// A local-only database has no tenant, no restricted collections, and no concept of "actor" —
/// there is nothing to check access against, so this preserves today's unconditional-cascade
/// behavior exactly.
pub struct AlwaysAllowGate;

#[async_trait]
impl SubtreeAccessGate for AlwaysAllowGate {
    async fn check_subtree_access(&self, _node_ids: &[String]) -> SubtreeAccessDecision {
        SubtreeAccessDecision::Allowed
    }
}

impl super::NodeService {
    /// Inject the real subtree access gate. Called by a Pro daemon once its tenant
    /// connection is established. Silently ignored if called more than once (mirrors
    /// `set_embedding_waker`). Works on `Arc<NodeService>`/any clone since the `OnceLock`
    /// is shared via `Arc`.
    pub fn set_subtree_access_gate(&self, gate: std::sync::Arc<dyn SubtreeAccessGate>) {
        let _ = self.subtree_access_gate.set(gate);
    }

    /// The active gate: the injected Pro gate if one has been set, otherwise
    /// [`AlwaysAllowGate`] (community default).
    pub(crate) fn subtree_access_gate(&self) -> &dyn SubtreeAccessGate {
        // Safe to hand out a `&'static` reference to a stack-local `static` only because
        // AlwaysAllowGate is a zero-sized unit struct with no fields to ever go stale — if it
        // grows state, this would need to move to an `Arc`/`OnceLock` like the injected case.
        static DEFAULT: AlwaysAllowGate = AlwaysAllowGate;
        self.subtree_access_gate
            .get()
            .map(|g| g.as_ref())
            .unwrap_or(&DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn always_allow_gate_allows_any_set() {
        let gate = AlwaysAllowGate;
        let decision = gate
            .check_subtree_access(&["a".to_string(), "b".to_string()])
            .await;
        assert_eq!(decision, SubtreeAccessDecision::Allowed);
    }

    #[tokio::test]
    async fn always_allow_gate_allows_empty_set() {
        let gate = AlwaysAllowGate;
        let decision = gate.check_subtree_access(&[]).await;
        assert_eq!(decision, SubtreeAccessDecision::Allowed);
    }
}
