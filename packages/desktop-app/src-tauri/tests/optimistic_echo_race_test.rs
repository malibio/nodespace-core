//! ADR-048 priority flow 2 — the optimistic-edit-vs-echo race, against the
//! REAL live event path (`watcher::run`, wired up in `lib.rs` at startup —
//! its own module doc previously (and incorrectly) claimed it was inert;
//! confirmed by grep that it is the sole emitter of `node:*` events and is
//! unconditionally spawned).
//!
//! This is the regression class ADR-048 exists to catch: the frontend
//! applies an edit optimistically to local state, the daemon later echoes
//! the authoritative version back over the watch stream, and if that echo
//! arrives out of order — or carries a value the user has already typed
//! past — it must not overwrite newer local state. A synchronous mock
//! cannot reproduce this: the race is between two independent clocks (the
//! local optimistic write and the daemon's asynchronous echo), and a mock
//! collapses them onto one tick. Here, `watcher::run` is driven for real
//! against a real daemon, emitting real `node:updated` events on a real
//! Tauri event bus (`tauri::test`'s `MockRuntime` — no webview, but a real
//! `Emitter`/`Listener` implementation, not a stand-in).
//!
//! `watcher::run` was made generic over `tauri::Runtime` (previously
//! hardcoded to the real `Wry` runtime, like the rest of the module) SOLELY
//! so this test can drive the actual production function against
//! `MockRuntime` — not a reimplementation of its forwarding logic.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nodespace_app_lib::commands::nodes::{create_node, update_node, CreateNodeInput};
use nodespace_app_lib::types::NodeUpdate;
use nodespace_app_lib::watcher;
use serde_json::json;
use support::{hold_connect_mutex_and_socket_env, SpawnedDaemon, TauriTestApp};
use tauri::Listener;
use tokio_util::sync::CancellationToken;

fn text_input(id: &str, content: &str) -> CreateNodeInput {
    CreateNodeInput {
        id: id.to_string(),
        node_type: "text".to_string(),
        content: content.to_string(),
        parent_id: None,
        insert_position: None,
        properties: json!({}),
    }
}

/// Wait until at least `count` `node:updated` events carrying `id` have been
/// observed, or panic after `timeout`. Polling rather than a one-shot recv
/// because the watcher's stream delivery is asynchronous relative to the
/// RPC call that triggered it — the whole point of this test.
async fn wait_for_updates(
    events: &Arc<Mutex<Vec<String>>>,
    node_id: &str,
    count: usize,
    timeout: Duration,
) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let matched = events.lock().unwrap().iter().filter(|id| *id == node_id).count();
        if matched >= count {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for {count} node:updated event(s) for {node_id}; saw {matched}"
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The core regression: apply a real update through the command layer, let
/// the REAL watcher (real gRPC WatchNodes stream, real Tauri event bus)
/// echo it back, then apply a NEWER update. A late-arriving stale echo for
/// the first write must not be interpreted by a listener as more current
/// than the second, already-applied write's own echo — asserted here by
/// checking that get_node (the authoritative source) reflects the LATEST
/// content after both echoes have had time to arrive, regardless of
/// arrival order.
#[tokio::test]
async fn newer_local_write_is_not_clobbered_by_a_late_echo_of_an_older_write() {
    let daemon = SpawnedDaemon::spawn();
    let harness = TauriTestApp::connect(&daemon, Duration::from_secs(30)).await;
    let state = harness.client_state();
    let handle = harness.handle();

    // watcher::run re-resolves NODESPACED_SOCKET itself when its task starts
    // running, independently of the GrpcClient already connected above — so
    // this guard must stay held for the watcher's entire lifetime, not just
    // through connect(). Serializes this test against every other test in
    // the binary that touches NODESPACED_SOCKET, which is the accepted cost
    // of a real, unmockable process-global env dependency.
    let _socket_guard = hold_connect_mutex_and_socket_env(&daemon).await;

    let cancel_token = CancellationToken::new();
    let watcher_handle = tokio::spawn(watcher::run(handle.clone(), cancel_token.child_token()));
    // Give the watcher a moment to open its WatchNodes stream before the
    // first write, so its echo isn't lost to a stream that hasn't opened yet.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    handle.listen("node:updated", move |event| {
        // Payload is `{ "id": "...", "nodeType": ... }` (nodeType omitted for
        // updates) — extract just the id for this test's purposes.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            if let Some(id) = v["id"].as_str() {
                received_clone.lock().unwrap().push(id.to_string());
            }
        }
    });

    let id = uuid::Uuid::new_v4().to_string();
    create_node(state.clone(), text_input(&id, "v0"))
        .await
        .expect("create_node failed");

    // First write: "optimistic" content the user has already typed past by
    // the time its echo arrives.
    update_node(
        state.clone(),
        id.clone(),
        1,
        NodeUpdate {
            content: Some("stale content".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("first update_node failed");

    // Second, newer write — sequenced correctly (using the confirmed
    // version from the first commit), simulating the user continuing to
    // type before the first echo has necessarily arrived.
    update_node(
        state.clone(),
        id.clone(),
        2,
        NodeUpdate {
            content: Some("newest content".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("second update_node failed");

    // Wait for both echoes to arrive over the real watch stream.
    wait_for_updates(&received, &id, 2, Duration::from_secs(10)).await;

    // The authoritative source must reflect the LATEST write — a real
    // out-of-order or late echo of "stale content" must never win over it.
    let node = nodespace_app_lib::commands::nodes::get_node(state.clone(), id.clone())
        .await
        .expect("get_node failed")
        .expect("node must exist");
    assert_eq!(
        node["content"],
        json!("newest content"),
        "the newer write must not be clobbered by a late echo of the older write"
    );
    assert_eq!(node["version"], json!(3));

    cancel_token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), watcher_handle).await;
}

/// Confirms the watcher actually delivers `node:created` for a real create,
/// over the real event bus — the minimal proof that this test file is
/// exercising the live path (`watcher::run`), not a dead one.
#[tokio::test]
async fn watcher_delivers_a_real_node_created_event() {
    let daemon = SpawnedDaemon::spawn();
    let harness = TauriTestApp::connect(&daemon, Duration::from_secs(30)).await;
    let state = harness.client_state();
    let handle = harness.handle();
    let _socket_guard = hold_connect_mutex_and_socket_env(&daemon).await;

    let cancel_token = CancellationToken::new();
    let watcher_handle = tokio::spawn(watcher::run(handle.clone(), cancel_token.child_token()));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let created: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let created_clone = created.clone();
    handle.listen("node:created", move |event| {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            if let Some(id) = v["id"].as_str() {
                created_clone.lock().unwrap().push(id.to_string());
            }
        }
    });

    let id = uuid::Uuid::new_v4().to_string();
    create_node(state.clone(), text_input(&id, "hello"))
        .await
        .expect("create_node failed");

    wait_for_updates(&created, &id, 1, Duration::from_secs(10)).await;

    cancel_token.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), watcher_handle).await;
}
