//! ADR-048 priority flow 4 — ai-chat send → idle with a real (not mocked)
//! model, through the real Tauri command layer against a real headless
//! daemon.
//!
//! There is no dedicated "send chat message" Tauri command or gRPC RPC.
//! Per `scripts/aichat.ts`'s doc comment (the existing CLI harness for this
//! same mechanism): the daemon's event watcher runs an inference turn when
//! an ai-chat node's `properties['ai-chat']` has `status: "processing"` AND
//! a trailing `role: "user"` message. So a turn is: create/update the node
//! with that shape, then poll `get_node` until `status` returns to `"idle"`
//! with a new assistant message appended.
//!
//! No lightweight stub/test-double model backend exists anywhere in this
//! codebase (confirmed: `ChatInferenceEngine`'s only implementors are the
//! real `LlamaChatInferenceEngine` and `OllamaInferenceEngine`; the daemon's
//! only placeholder, `NoOpInferenceEngine`, always errors rather than
//! completing a turn, so it cannot drive this flow at all). Adding one would
//! mean carving a test-only code path into the production model-selection
//! logic in `packages/daemon/src/services/local_agent_service.rs` — a
//! change to a real inference subsystem's behavior, not a test concern.
//! Instead, this uses `ministral-3b-q4km` — the smallest model in the
//! catalog, already downloaded under `~/.nodespace/models/` on this
//! machine — as the fastest available REAL model. It is a real inference
//! call, not instant, so this test's timeout is generous (well beyond CRUD
//! test scale) and it is the slowest test in this suite by design.

use std::time::Duration;

use nodespace_app_lib::commands::nodes::{create_node, get_node, update_node, CreateNodeInput};
use nodespace_app_lib::types::NodeUpdate;
use nodespace_app_test_support::{SpawnedDaemon, TauriTestApp};
use nodespace_proto::nodespace::EnsureModelReadyRequest;
use serde_json::json;
use tokio_stream::StreamExt;

const MODEL_ID: &str = "ministral-3b-q4km";

fn ai_chat_input(
    id: &str,
    provider_model: &str,
    status: &str,
    messages: serde_json::Value,
) -> CreateNodeInput {
    CreateNodeInput {
        id: id.to_string(),
        node_type: "ai-chat".to_string(),
        content: "Test chat".to_string(),
        parent_id: None,
        insert_position: None,
        properties: json!({
            "ai-chat": {
                "provider": "native",
                "model": provider_model,
                "status": status,
                "messages": messages
            }
        }),
    }
}

async fn poll_until_idle_with_new_assistant_reply(
    state: &tauri::State<'_, nodespace_app_lib::services::GrpcClient>,
    id: &str,
    prior_assistant_count: usize,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let node = get_node(state.clone(), id.to_string())
            .await
            .expect("get_node failed")
            .expect("node must exist");

        let status = node["status"].as_str().unwrap_or_default();
        let messages = node["messages"].as_array().cloned().unwrap_or_default();
        let assistant_count = messages
            .iter()
            .filter(|m| m["role"] == json!("assistant"))
            .count();

        if status == "idle" && assistant_count > prior_assistant_count {
            return node;
        }

        if tokio::time::Instant::now() >= deadline {
            panic!(
                "ai-chat turn did not reach idle-with-new-reply within {timeout:?}; \
                 last status={status:?}, assistant_count={assistant_count}"
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test]
async fn ai_chat_send_reaches_idle_with_no_stuck_processing_state() {
    let daemon = SpawnedDaemon::spawn();
    let harness = TauriTestApp::connect(&daemon, Duration::from_secs(30)).await;
    let state = harness.client_state();

    // chat_model_load's gRPC call only updates the model manager's
    // bookkeeping — it does NOT swap in a real inference engine (that swap,
    // `LocalAgentServiceImpl::replace_engine`, only happens inside
    // `load_model_and_collect_events`, reached solely via the
    // `EnsureModelReady` streaming RPC). Drive that RPC directly so the
    // daemon's ai-chat turn actually has a working engine to run against.
    let mut local_agent_client = state.local_agent_client().await;
    let mut ready_stream = local_agent_client
        .ensure_model_ready(EnsureModelReadyRequest {
            model_id: MODEL_ID.to_string(),
        })
        .await
        .expect("EnsureModelReady RPC failed")
        .into_inner();
    while let Some(event) = ready_stream.next().await {
        let event = event.expect("EnsureModelReady stream yielded a transport error");
        if event.event_type == "error" {
            panic!(
                "EnsureModelReady reached an error state: {:?}",
                event.error_message
            );
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    create_node(
        state.clone(),
        ai_chat_input(&id, MODEL_ID, "idle", json!([])),
    )
    .await
    .expect("create ai-chat node failed");

    // "Send a message": append a user message and flip status to processing
    // — the exact mechanism scripts/aichat.ts's cmdSend documents.
    let after_send = update_node(
        state.clone(),
        id.clone(),
        1,
        NodeUpdate {
            properties: Some(json!({
                "ai-chat": {
                    "provider": "native",
                    "model": MODEL_ID,
                    "status": "processing",
                    "messages": [
                        { "role": "user", "content": "Reply with exactly one word: OK" }
                    ]
                }
            })),
            ..Default::default()
        },
    )
    .await
    .expect("update_node (send message) failed");
    assert_eq!(
        after_send["status"],
        json!("processing"),
        "update_node's own response must reflect the processing status just written: {after_send:?}"
    );

    let final_node =
        poll_until_idle_with_new_assistant_reply(&state, &id, 0, Duration::from_secs(180)).await;

    assert_eq!(final_node["status"], json!("idle"));
    let messages = final_node["messages"]
        .as_array()
        .expect("messages must be an array");
    let assistant_reply = messages
        .iter()
        .rev()
        .find(|m| m["role"] == json!("assistant"))
        .expect("an assistant reply must be present");
    assert!(
        !assistant_reply["content"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "assistant reply content must not be empty"
    );
}
