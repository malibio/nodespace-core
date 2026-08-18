//! Adversarial offline-convergence test for collection-name collisions.
//!
//! Scope: this file covers the OLDER, harder, pre-ADR-065 hard-uniqueness
//! check that used to live in `SqliteStore::create_node` for the `collection`
//! node type specifically — a hardcoded, type-specific check, unrelated to
//! the schema-declared `unique` field mechanism ADR-065 introduced (see
//! `person_duplicate_convergence_test.rs`, which explicitly scopes itself
//! OUT of this check). Before the fix under test, `SqliteStore::create_node`
//! called `bail!` on a collection-name collision; that error propagated
//! straight out of `nodespace-sync`'s `apply_node_upsert`, and per that
//! function's own contract the sync cursor never advances past a failed row
//! — a benign duplicate collection name could wedge a sync cursor
//! permanently.
//!
//! The core invariant under test: NodeSpace is local-first, so collection-name
//! uniqueness can never be *enforced* at creation — two offline devices can
//! each validly create a collection named "Work", and the conflict only
//! becomes visible once both copies land in one database (sync convergence).
//! Hard rejection anywhere in that path would turn an ordinary benign
//! duplicate into a stuck sync. That must never happen.
//!
//! Like its person-duplicate counterpart, this test does not mock the
//! two-device scenario: it stands up fully independent `SqliteStore` +
//! `NodeService` pairs (separate temp directories, no shared state, no
//! coordination) to play the role of independent offline devices, and only
//! performs "convergence" by applying a peer's fully-formed node into another
//! device's store via the real `NodeService::create_node` path
//! `nodespace-sync`'s `apply_node_upsert` also uses.
//!
//! Collections are created here via `Node::new` (a fresh random UUID id) —
//! deliberately NOT via `CollectionService::create_collection` /
//! `collection_ops::create_collection`, which derive a deterministic id from
//! the normalized name specifically so two devices creating the same name
//! converge onto ONE node id instead of ever hitting this collision at all.
//! The vulnerable path this bug affects is any caller that creates a
//! `collection` node directly (an import, an agent tool, or — critically — a
//! sync-pulled peer node, which always keeps the peer's own id) rather than
//! going through that higher-level, id-deterministic helper.

#[cfg(test)]
mod collection_name_convergence_tests {
    use anyhow::Result;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::models::Node;
    use nodespace_core::services::NodeService;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// One simulated device: its own on-disk SQLite file, its own `NodeService`,
    /// no relationship to any other `Device` in the test.
    struct Device {
        service: NodeService,
        _temp_dir: TempDir, // kept alive for the duration of the test
    }

    async fn device() -> Result<Device> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("device.db");
        let mut store = Arc::new(SqliteStore::new(db_path).await?);
        let service = NodeService::new(&mut store).await?;
        Ok(Device {
            service,
            _temp_dir: temp_dir,
        })
    }

    /// Apply a fully-formed `Node` (as fetched from a peer device) into `into`'s
    /// store, preserving its id/content/properties exactly as sync-pull would,
    /// via the "node absent locally" branch of a real apply (a fresh
    /// `create_node`).
    async fn apply_incoming(into: &NodeService, incoming: Node) -> Result<String> {
        let id = into.create_node(incoming).await?;
        Ok(id)
    }

    fn marker(n: &Node) -> bool {
        n.properties
            .get("collection")
            .and_then(|p| p.get("_possible_duplicate"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn two_offline_devices_create_the_same_collection_name_and_converge_without_rejection(
    ) -> Result<()> {
        // --- Device A, entirely offline, unaware Device B exists ---
        let device_a = device().await?;
        let work_a_id = device_a
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Work".to_string(),
                json!({}),
            ))
            .await?;

        // --- Device B, entirely offline, unaware Device A exists ---
        // A different collection of the SAME name gets created independently —
        // different device, different local id (a fresh random UUID, exactly
        // as any collection-node create not going through the deterministic-id
        // helper would get — e.g. a sync-pulled peer node), same name.
        let device_b = device().await?;
        let work_b_id = device_b
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Work".to_string(),
                json!({}),
            ))
            .await?;

        assert_ne!(
            work_a_id, work_b_id,
            "the two devices must have produced genuinely distinct node ids"
        );

        // Both devices' local writes succeeded independently — neither device
        // ever saw the other's data, so nothing could have been rejected. This
        // is the local-first baseline the rest of the test builds on.
        assert!(device_a.service.get_node(&work_a_id).await?.is_some());
        assert!(device_b.service.get_node(&work_b_id).await?.is_some());

        // --- Convergence: Device A pulls Device B's node in (sync-apply) ---
        // Fetch B's fully-formed node exactly as a sync pull would receive it
        // over the wire, then apply it into A's store, id and all.
        let bs_node = device_b
            .service
            .get_node(&work_b_id)
            .await?
            .expect("device B's node must exist");

        let applied_id = apply_incoming(&device_a.service, bs_node.clone())
            .await
            .expect(
                "sync apply must NEVER reject a write because of a collection-name \
                 collision — a hard error here is exactly the bug that wedges a sync \
                 cursor permanently on a benign duplicate name",
            );
        assert_eq!(applied_id, work_b_id, "the incoming node keeps its own id");

        // --- No data loss: BOTH nodes now exist, side by side, in A's store ---
        let a_after = device_a
            .service
            .get_node(&work_a_id)
            .await?
            .expect("Device A's own collection must still exist");
        let b_after = device_a
            .service
            .get_node(&work_b_id)
            .await?
            .expect("Device B's synced-in collection must now exist in A's store");
        assert_eq!(a_after.content, "Work");
        assert_eq!(b_after.content, "Work");
        assert_eq!(a_after.node_type, "collection");
        assert_eq!(b_after.node_type, "collection");

        // --- Both sides are marked as a possible duplicate, automatically ---
        // Unlike the person-email mechanism (a separate opt-in
        // `mark_possible_duplicates` call), the collection-name marker is set
        // synchronously inside `SqliteStore::create_node` itself, so no
        // additional call is needed here — it must already be true.
        assert!(marker(&a_after), "Device A's own collection must be marked");
        assert!(
            marker(&b_after),
            "Device B's synced-in collection must be marked"
        );

        Ok(())
    }

    #[tokio::test]
    async fn convergence_with_no_name_collision_marks_nothing_and_still_never_rejects() -> Result<()>
    {
        let device_a = device().await?;
        let work_id = device_a
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Work".to_string(),
                json!({}),
            ))
            .await?;

        let device_b = device().await?;
        let personal_id = device_b
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Personal".to_string(),
                json!({}),
            ))
            .await?;

        let personal_node = device_b.service.get_node(&personal_id).await?.unwrap();
        let applied_id = apply_incoming(&device_a.service, personal_node).await?;
        assert_eq!(applied_id, personal_id);

        // Both distinct collections now coexist in A with no collision at all.
        assert!(device_a.service.get_node(&work_id).await?.is_some());
        let personal_after = device_a.service.get_node(&personal_id).await?.unwrap();
        assert!(
            !marker(&personal_after),
            "two genuinely distinct collection names must never be marked"
        );
        let work_after = device_a.service.get_node(&work_id).await?.unwrap();
        assert!(
            !marker(&work_after),
            "no marker must be set on the pre-existing side absent a real collision"
        );

        Ok(())
    }

    /// `get_collection_by_name` (the predicate both the pre-existing hard check
    /// and this fix's collision detection use) matches case-insensitively. A
    /// collision that differs only in case must be detected and marked exactly
    /// like a byte-identical collision, never rejected.
    #[tokio::test]
    async fn case_insensitive_collection_name_collision_converges_without_rejection() -> Result<()>
    {
        let device_a = device().await?;
        let work_a_id = device_a
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Work".to_string(),
                json!({}),
            ))
            .await?;

        let device_b = device().await?;
        let work_b_id = device_b
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "WORK".to_string(),
                json!({}),
            ))
            .await?;

        let bs_node = device_b.service.get_node(&work_b_id).await?.unwrap();
        let applied_id = apply_incoming(&device_a.service, bs_node)
            .await
            .expect("a case-varied collection-name collision must never be rejected");
        assert_eq!(applied_id, work_b_id);

        let a_after = device_a.service.get_node(&work_a_id).await?.unwrap();
        let b_after = device_a.service.get_node(&work_b_id).await?.unwrap();
        assert_eq!(
            a_after.content, "Work",
            "each side's own casing is preserved"
        );
        assert_eq!(
            b_after.content, "WORK",
            "each side's own casing is preserved"
        );
        assert!(marker(&a_after));
        assert!(marker(&b_after));

        Ok(())
    }

    /// A narrow, real concurrency case: two devices' same-named collections
    /// applied CONCURRENTLY into one hub (as two racing sync-pull tasks
    /// might), rather than the sequential applies every other test in this
    /// file uses. Both must land, and neither may error — proving the
    /// no-rejection guarantee isn't an artifact of strict sequencing.
    #[tokio::test]
    async fn concurrent_convergence_of_two_same_named_collections_never_rejects() -> Result<()> {
        let hub = Arc::new(device().await?);

        let device_a = device().await?;
        let a_id = device_a
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Concurrent".to_string(),
                json!({}),
            ))
            .await?;
        let a_node = device_a.service.get_node(&a_id).await?.unwrap();

        let device_b = device().await?;
        let b_id = device_b
            .service
            .create_node(Node::new(
                "collection".to_string(),
                "Concurrent".to_string(),
                json!({}),
            ))
            .await?;
        let b_node = device_b.service.get_node(&b_id).await?.unwrap();

        let hub_a = Arc::clone(&hub);
        let hub_b = Arc::clone(&hub);
        let (result_a, result_b) = tokio::join!(
            tokio::spawn(async move { apply_incoming(&hub_a.service, a_node).await }),
            tokio::spawn(async move { apply_incoming(&hub_b.service, b_node).await }),
        );

        result_a
            .expect("task must not panic")
            .expect("concurrent apply of A must never be rejected on a name collision");
        result_b
            .expect("task must not panic")
            .expect("concurrent apply of B must never be rejected on a name collision");

        assert!(hub.service.get_node(&a_id).await?.is_some());
        assert!(hub.service.get_node(&b_id).await?.is_some());

        Ok(())
    }
}
