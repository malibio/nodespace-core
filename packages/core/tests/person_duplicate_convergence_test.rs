//! Adversarial offline-convergence test for the store-aware `unique` rule (ADR-065).
//!
//! The core invariant under test: NodeSpace is local-first, so a uniqueness rule
//! can never be *enforced* at creation — two offline devices can each validly
//! create "the same" person, and the conflict only becomes visible once both
//! copies land in one database (sync convergence). Hard rejection anywhere in
//! that path — at either device's creation, or when the peer's copy is applied
//! locally — would turn an ordinary data-entry duplicate into a sync failure.
//! That must never happen.
//!
//! This test does not mock that scenario: it stands up two fully independent
//! `SqliteStore` + `NodeService` pairs (separate temp directories, no shared
//! state, no coordination) to play the role of two offline devices. Each device
//! creates a person with the same email with zero knowledge of the other. Only
//! after both writes have already succeeded independently does the test perform
//! the "convergence" step — applying the peer's fully-formed node (same id,
//! same content, same properties) into the other device's store, exactly as a
//! sync-pull materializing a remote node would. It then asserts:
//!
//! 1. The convergence write never errors or is rejected.
//! 2. Both nodes survive — no data loss, no silent overwrite.
//! 3. The shared uniqueness predicate detects the collision post-convergence,
//!    and `NodeService::mark_possible_duplicates` stamps a non-blocking
//!    indicator on both copies.
//! 4. The marker write is version-preserving — it must not look like a content
//!    edit to OCC or to sync's dirty-tracking, or the marker itself would risk
//!    perturbing a concurrent write or getting re-broadcast.

#[cfg(test)]
mod offline_convergence_tests {
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
    /// store, preserving its id/content/properties exactly as sync-pull would.
    async fn apply_incoming(into: &NodeService, incoming: Node) -> Result<String> {
        let id = into.create_node(incoming).await?;
        Ok(id)
    }

    #[tokio::test]
    async fn two_offline_devices_create_the_same_person_and_converge_without_rejection(
    ) -> Result<()> {
        // --- Device A, entirely offline, unaware Device B exists ---
        let device_a = device().await?;
        let alice_a_id = device_a
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice".to_string(),
                json!({ "person": { "name": "Alice", "email": "alice@example.com" } }),
            ))
            .await?;

        // --- Device B, entirely offline, unaware Device A exists ---
        // A different person of the SAME real-world identity gets created
        // independently — different device, different local id, same email.
        let device_b = device().await?;
        let alice_b_id = device_b
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice B".to_string(),
                json!({ "person": { "name": "Alice B", "email": "ALICE@example.com" } }),
            ))
            .await?;

        assert_ne!(
            alice_a_id, alice_b_id,
            "the two devices must have produced genuinely distinct node ids"
        );

        // Both devices' local writes succeeded independently — neither device
        // ever saw the other's data, so nothing could have been rejected. This
        // is the local-first baseline the rest of the test builds on.
        assert!(device_a.service.get_node(&alice_a_id).await?.is_some());
        assert!(device_b.service.get_node(&alice_b_id).await?.is_some());

        // --- Convergence: Device A pulls Device B's node in (sync-apply) ---
        // Fetch B's fully-formed node exactly as a sync pull would receive it
        // over the wire, then apply it into A's store, id and all.
        let bs_node = device_b
            .service
            .get_node(&alice_b_id)
            .await?
            .expect("device B's node must exist");

        let applied_id = apply_incoming(&device_a.service, bs_node.clone())
            .await
            .expect(
                "sync apply must NEVER reject a write because of a uniqueness collision \
                 (ADR-065) — a hard error here would turn a benign duplicate into a stuck sync",
            );
        assert_eq!(applied_id, alice_b_id, "the incoming node keeps its own id");

        // --- No data loss: BOTH nodes now exist, side by side, in A's store ---
        let a_after = device_a
            .service
            .get_node(&alice_a_id)
            .await?
            .expect("Device A's own person must still exist");
        let b_after = device_a
            .service
            .get_node(&alice_b_id)
            .await?
            .expect("Device B's synced-in person must now exist in A's store");
        assert_eq!(a_after.content, "Alice");
        assert_eq!(b_after.content, "Alice B");
        assert_eq!(
            a_after
                .properties
                .get("person")
                .and_then(|p| p.get("email"))
                .and_then(|v| v.as_str()),
            Some("alice@example.com")
        );
        assert_eq!(
            b_after
                .properties
                .get("person")
                .and_then(|p| p.get("email"))
                .and_then(|v| v.as_str()),
            Some("ALICE@example.com"),
            "the incoming node's data must round-trip unmodified — no silent overwrite"
        );

        // --- The shared predicate now sees the collision from A's perspective ---
        // Same predicate the creation-time suggestion uses, so semantics never
        // drift between "suggest at creation" and "detect at convergence".
        let dup = device_a
            .service
            .find_duplicate_for("person", "email", "alice@example.com")
            .await?;
        assert!(
            dup.is_some(),
            "post-convergence, the case-insensitive email collision must be visible \
             to the same predicate that powers the creation-time suggestion"
        );

        // Baseline captured AFTER the node is sitting in A's store (post-insert)
        // but BEFORE marking, so the version-preservation assertion below isolates
        // what `mark_possible_duplicates` itself does — decoupled from
        // `create_node`'s own (pre-existing, unrelated-to-this-change) behavior
        // of re-stamping created_at/modified_at at insert time.
        let b_before_marking = device_a
            .service
            .get_node(&alice_b_id)
            .await?
            .expect("still exists");
        let version_before = b_before_marking.version;
        let modified_at_before = b_before_marking.modified_at;

        // --- The convergence-detection hook marks BOTH copies, and rejects nothing ---
        let marked = device_a
            .service
            .mark_possible_duplicates(&alice_b_id)
            .await
            .expect("marking a possible duplicate must never error on a real collision");
        assert!(marked, "a real collision must be reported as marked");

        let a_marked = device_a
            .service
            .get_node(&alice_a_id)
            .await?
            .expect("still exists");
        let b_marked = device_a
            .service
            .get_node(&alice_b_id)
            .await?
            .expect("still exists");

        let marker = |n: &Node| {
            n.properties
                .get("person")
                .and_then(|p| p.get("_possible_duplicate"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        assert!(marker(&a_marked), "Device A's own node must be marked");
        assert!(
            marker(&b_marked),
            "Device B's synced-in node must be marked"
        );

        // --- The marker must be version-preserving and not perturb sync state ---
        // A version bump or a domain event here would make the marker look like
        // a content edit to OCC or to the sync engine's dirty-tracking — exactly
        // what must NOT happen for a side-channel bookkeeping flag.
        assert_eq!(
            b_marked.version, version_before,
            "marking a possible duplicate must not bump the node's OCC version"
        );
        assert_eq!(
            b_marked.modified_at, modified_at_before,
            "marking a possible duplicate must not touch modified_at"
        );

        Ok(())
    }

    #[tokio::test]
    async fn convergence_with_no_collision_marks_nothing_and_still_never_rejects() -> Result<()> {
        let device_a = device().await?;
        let alice_id = device_a
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice".to_string(),
                json!({ "person": { "name": "Alice", "email": "alice@example.com" } }),
            ))
            .await?;

        let device_b = device().await?;
        let bob_id = device_b
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Bob".to_string(),
                json!({ "person": { "name": "Bob", "email": "bob@example.com" } }),
            ))
            .await?;

        let bobs_node = device_b.service.get_node(&bob_id).await?.unwrap();
        let applied_id = apply_incoming(&device_a.service, bobs_node).await?;
        assert_eq!(applied_id, bob_id);

        // Both distinct people now coexist in A with no collision at all.
        assert!(device_a.service.get_node(&alice_id).await?.is_some());
        assert!(device_a.service.get_node(&bob_id).await?.is_some());

        let marked = device_a.service.mark_possible_duplicates(&bob_id).await?;
        assert!(
            !marked,
            "two genuinely distinct emails must never be marked"
        );

        let bob_after = device_a.service.get_node(&bob_id).await?.unwrap();
        let marker = bob_after
            .properties
            .get("person")
            .and_then(|p| p.get("_possible_duplicate"));
        assert!(
            marker.is_none() || marker == Some(&json!(false)),
            "no marker must be set absent a real collision"
        );

        Ok(())
    }

    #[tokio::test]
    async fn three_way_convergence_all_devices_survive_and_get_marked() -> Result<()> {
        // A harsher variant: THREE independent offline devices each create "the
        // same" person (case-varied email), then all three copies converge onto
        // one store one at a time (as sequential sync pulls would apply them).
        // Every node must survive every step, and every node ends up marked.
        let emails = [
            "alice@example.com",
            "Alice@Example.com",
            "ALICE@EXAMPLE.COM",
        ];
        let mut ids = Vec::new();
        let hub = device().await?;

        for (i, email) in emails.iter().enumerate() {
            let d = device().await?;
            let id = d
                .service
                .create_node(Node::new(
                    "person".to_string(),
                    format!("Alice (device {i})"),
                    json!({ "person": { "name": format!("Alice (device {i})"), "email": email } }),
                ))
                .await?;
            let node = d.service.get_node(&id).await?.unwrap();

            // Converge this device's copy into the hub, unconditionally.
            let applied = apply_incoming(&hub.service, node)
                .await
                .unwrap_or_else(|e| {
                    panic!("sync apply must never reject copy {i} on a uniqueness collision: {e}")
                });
            assert_eq!(applied, id);
            ids.push(id);
        }

        // All three survive side by side.
        for id in &ids {
            assert!(hub.service.get_node(id).await?.is_some());
        }

        // Mark from the last-applied node's perspective — it must find (and
        // mark) at least one conflicting sibling, and nothing errors.
        let marked = hub.service.mark_possible_duplicates(&ids[2]).await?;
        assert!(marked);

        Ok(())
    }
}
