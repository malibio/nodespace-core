//! Adversarial offline-convergence test for the store-aware `unique` rule (ADR-065).
//!
//! Scope: this file proves the invariant for the schema-declared `unique` rule
//! specifically (the mechanism this issue adds) — NOT for every hard-uniqueness
//! check that exists anywhere in the store. A separate, older, harder mechanism
//! (collection-name uniqueness in `SqliteStore::create_node`) predates this rule
//! and is a genuinely different, unrelated constraint outside this file's scope;
//! it is not exercised or claimed to be covered here.
//!
//! The core invariant under test: NodeSpace is local-first, so a `unique`
//! schema rule can never be *enforced* at creation — two offline devices can
//! each validly create "the same" person, and the conflict only becomes visible
//! once both copies land in one database (sync convergence). Hard rejection
//! anywhere in that path — at either device's creation, or when the peer's copy
//! is applied locally, whether as a brand-new row or as an update to a
//! previously-converged one — would turn an ordinary data-entry duplicate into
//! a sync failure. That must never happen for this rule.
//!
//! These tests are sequential (`await` at every step); they prove correctness
//! under sequential convergence, not under concurrent convergence. A dedicated
//! concurrent test below covers two applies racing into the same store, but a
//! full concurrent-marking stress test is out of scope here.
//!
//! This test does not mock the two-device scenario: it stands up fully
//! independent `SqliteStore` + `NodeService` pairs (separate temp directories,
//! no shared state, no coordination) to play the role of independent offline
//! devices, and only performs "convergence" — applying a peer's fully-formed
//! node into another device's store, via the real `NodeService::create_node` /
//! `update_node` paths `nodespace-sync`'s `apply_node_upsert` also uses — after
//! each device's own write has already succeeded independently.

#[cfg(test)]
mod offline_convergence_tests {
    use anyhow::Result;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::models::{Node, NodeUpdate};
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

    /// Delegates to the real read-side accessor under test
    /// (`NodeService::is_possible_duplicate`) rather than re-deriving the
    /// property path locally, so every assertion below that calls `marker()`
    /// doubles as coverage of the accessor itself.
    fn marker(n: &Node) -> bool {
        NodeService::is_possible_duplicate(n)
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
                "sync apply must NEVER reject a write because of a `unique`-rule collision \
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
        // drift between "suggest at creation" and "detect at convergence". Query
        // with B's casing (not A's own, byte-identical value) and exclude A's own
        // id, so this genuinely depends on case-insensitive folding finding B's
        // node — not a vacuous self-match that would pass even with folding
        // completely broken.
        let dup = device_a
            .service
            .find_duplicate_for(
                "person",
                "email",
                "ALICE@example.com",
                Some(alice_a_id.as_str()),
            )
            .await?;
        assert_eq!(
            dup.map(|n| n.id),
            Some(alice_b_id.clone()),
            "post-convergence, excluding A's own node, the case-insensitive lookup \
             for B's exact casing must resolve to B's node specifically"
        );

        // Baselines captured AFTER both nodes are sitting in A's store (post-insert)
        // but BEFORE marking, so the version-preservation assertions below isolate
        // what `mark_possible_duplicates` itself does — decoupled from
        // `create_node`'s own (pre-existing, unrelated-to-this-change) behavior
        // of re-stamping created_at/modified_at at insert time.
        let a_before_marking = device_a
            .service
            .get_node(&alice_a_id)
            .await?
            .expect("still exists");
        let b_before_marking = device_a
            .service
            .get_node(&alice_b_id)
            .await?
            .expect("still exists");

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

        assert!(marker(&a_marked), "Device A's own node must be marked");
        assert!(
            marker(&b_marked),
            "Device B's synced-in node must be marked"
        );

        // --- The marker must be version-preserving on BOTH sides ---
        // A version bump or a domain event here would make the marker look like
        // a content edit to OCC or to the sync engine's dirty-tracking — exactly
        // what must NOT happen for a side-channel bookkeeping flag. Checked on
        // BOTH nodes: A's own node is the more interesting case (it gets written
        // "from the side" while its owner may have an unrelated in-flight edit —
        // exactly the scenario the OCC-bypass design exists for), not just B's.
        assert_eq!(
            a_marked.version, a_before_marking.version,
            "marking must not bump A's own node's OCC version"
        );
        assert_eq!(
            a_marked.modified_at, a_before_marking.modified_at,
            "marking must not touch A's own node's modified_at"
        );
        assert_eq!(
            b_marked.version, b_before_marking.version,
            "marking must not bump B's synced-in node's OCC version"
        );
        assert_eq!(
            b_marked.modified_at, b_before_marking.modified_at,
            "marking must not touch B's synced-in node's modified_at"
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
        assert!(
            !marker(&bob_after),
            "no marker must be set absent a real collision"
        );

        Ok(())
    }

    /// The predicate behind `mark_possible_duplicates` is `LIMIT 1` (by design —
    /// it backs a suggestion, not a merge), so marking a node pairs it with AT
    /// MOST one colliding sibling, not the full colliding set. With three
    /// mutually-colliding devices, exactly two of the three end up marked from a
    /// single call — this test asserts that precisely, rather than a vaguer
    /// "some marking happened", so a change to that semantic is caught here
    /// instead of discovered later against a misleading test name.
    #[tokio::test]
    async fn three_way_convergence_all_survive_and_a_colliding_pair_is_marked() -> Result<()> {
        // THREE independent offline devices each create "the same" person
        // (case-varied email), then all three copies converge onto one store
        // one at a time (as sequential sync pulls would apply them). Every node
        // must survive every step, and no apply may ever reject.
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

        // Mark from the last-applied node's perspective.
        let marked = hub.service.mark_possible_duplicates(&ids[2]).await?;
        assert!(marked);

        let flags = {
            let mut out = Vec::new();
            for id in &ids {
                let n = hub.service.get_node(id).await?.unwrap();
                out.push(marker(&n));
            }
            out
        };
        assert_eq!(
            flags.iter().filter(|&&m| m).count(),
            2,
            "LIMIT-1 pairwise marking must mark exactly one colliding pair (2 of 3 \
             nodes), not the full mutually-colliding set — flags were {flags:?}"
        );

        Ok(())
    }

    /// `apply_node_upsert` in `nodespace-sync` has three branches: create
    /// (node absent locally), update (node already present locally), and an
    /// already-exists fallback to update. The tests above only exercise the
    /// first. This exercises the update branch: a node already present in the
    /// hub (as if pulled by an earlier sync cycle) receives an incoming update
    /// — applied via `NodeService::update_node`, not `create_node` — that
    /// introduces a fresh collision with a different existing node. The update
    /// must succeed unconditionally and the collision must be detectable
    /// afterward, exactly as in the create branch.
    #[tokio::test]
    async fn update_path_convergence_introducing_a_collision_never_rejects() -> Result<()> {
        let hub = device().await?;

        // Alice already exists in the hub.
        let alice_id = hub
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice".to_string(),
                json!({ "person": { "name": "Alice", "email": "alice@example.com" } }),
            ))
            .await?;

        // Bob also already exists in the hub (e.g. from an earlier, unrelated
        // sync pull) with a distinct email — no collision yet.
        let bob_id = hub
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Bob".to_string(),
                json!({ "person": { "name": "Bob", "email": "bob@example.com" } }),
            ))
            .await?;
        let bob_before = hub.service.get_node(&bob_id).await?.unwrap();

        // An incoming pulled UPDATE to Bob (e.g. he changed his email on another
        // device) now collides with Alice's. Applied via update_node — the
        // "node already present locally" branch a real apply takes when the
        // incoming node's id already exists in this store.
        let updated = hub
            .service
            .update_node(
                &bob_id,
                bob_before.version,
                NodeUpdate::new().with_properties(
                    json!({ "person": { "name": "Bob", "email": "ALICE@example.com" } }),
                ),
            )
            .await
            .expect(
                "an update that introduces a `unique`-rule collision must never be \
                 rejected — this is the update-branch analogue of the create-branch \
                 no-rejection guarantee",
            );
        assert_eq!(updated.id, bob_id);

        // Both nodes survive, Bob's update landed.
        let alice_after = hub.service.get_node(&alice_id).await?.unwrap();
        let bob_after = hub.service.get_node(&bob_id).await?.unwrap();
        assert_eq!(
            alice_after
                .properties
                .get("person")
                .and_then(|p| p.get("email"))
                .and_then(|v| v.as_str()),
            Some("alice@example.com")
        );
        assert_eq!(
            bob_after
                .properties
                .get("person")
                .and_then(|p| p.get("email"))
                .and_then(|v| v.as_str()),
            Some("ALICE@example.com"),
            "Bob's updated email must have actually landed"
        );

        // The collision is detectable and markable exactly as in the create case.
        let marked = hub.service.mark_possible_duplicates(&bob_id).await?;
        assert!(marked);
        assert!(marker(&hub.service.get_node(&alice_id).await?.unwrap()));
        assert!(marker(&hub.service.get_node(&bob_id).await?.unwrap()));

        Ok(())
    }

    /// A narrow, real concurrency case: two devices' colliding nodes applied
    /// CONCURRENTLY into one hub (as two racing sync-pull tasks might), rather
    /// than the sequential applies every other test in this file uses. Both
    /// must land, and neither may error — proving the no-rejection guarantee
    /// isn't an artifact of strict sequencing. This does not exercise
    /// concurrent MARKING (a harder, separate question); it exercises
    /// concurrent WRITE application, which is the part sync's pull pipeline
    /// can genuinely race.
    #[tokio::test]
    async fn concurrent_convergence_of_two_colliding_devices_never_rejects() -> Result<()> {
        let hub = Arc::new(device().await?);

        let device_a = device().await?;
        let a_id = device_a
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice A".to_string(),
                json!({ "person": { "name": "Alice A", "email": "concurrent@example.com" } }),
            ))
            .await?;
        let a_node = device_a.service.get_node(&a_id).await?.unwrap();

        let device_b = device().await?;
        let b_id = device_b
            .service
            .create_node(Node::new(
                "person".to_string(),
                "Alice B".to_string(),
                json!({ "person": { "name": "Alice B", "email": "CONCURRENT@example.com" } }),
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
            .expect("concurrent apply of A must never be rejected on a uniqueness collision");
        result_b
            .expect("task must not panic")
            .expect("concurrent apply of B must never be rejected on a uniqueness collision");

        assert!(hub.service.get_node(&a_id).await?.is_some());
        assert!(hub.service.get_node(&b_id).await?.is_some());

        Ok(())
    }

    /// `NodeService::is_possible_duplicate` is the read-side accessor the
    /// desktop UI badge relies on (core#2116) to decide whether to render —
    /// it must default to `false` for every "nothing to show" shape (a fresh
    /// node with no marker property at all, and a node whose marker was
    /// explicitly written as `false`), and only ever report `true` once
    /// `mark_possible_duplicates` has actually stamped it. This is a pure,
    /// synchronous reader — no store round-trip — so it is exercised directly
    /// against `Node` values rather than through a `Device`.
    #[tokio::test]
    async fn is_possible_duplicate_defaults_false_and_reflects_the_written_marker() -> Result<()> {
        let unmarked = Node::new(
            "person".to_string(),
            "Alice".to_string(),
            json!({ "person": { "name": "Alice", "email": "alice@example.com" } }),
        );
        assert!(
            !NodeService::is_possible_duplicate(&unmarked),
            "a node with no marker property at all must read as not-flagged"
        );

        let explicitly_false = Node::new(
            "person".to_string(),
            "Alice".to_string(),
            json!({ "person": { "name": "Alice", "_possible_duplicate": false } }),
        );
        assert!(
            !NodeService::is_possible_duplicate(&explicitly_false),
            "an explicit `false` marker must read as not-flagged, same as absent"
        );

        // Exercise the real write path (mark_possible_duplicates) end to end,
        // then confirm the accessor sees exactly what it wrote — the accessor
        // is the read-side counterpart, so it must never disagree with the
        // writer about the property's location or shape.
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
                json!({ "person": { "name": "Bob", "email": "alice@example.com" } }),
            ))
            .await?;
        let bobs_node = device_b.service.get_node(&bob_id).await?.unwrap();
        apply_incoming(&device_a.service, bobs_node).await?;

        let before = device_a.service.get_node(&alice_id).await?.unwrap();
        assert!(
            !NodeService::is_possible_duplicate(&before),
            "not flagged until mark_possible_duplicates actually runs"
        );

        assert!(device_a.service.mark_possible_duplicates(&bob_id).await?);

        let after = device_a.service.get_node(&alice_id).await?.unwrap();
        assert!(
            NodeService::is_possible_duplicate(&after),
            "must read true once mark_possible_duplicates has written it"
        );

        Ok(())
    }
}
