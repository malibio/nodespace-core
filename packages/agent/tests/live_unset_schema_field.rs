//! Verification that a schema field which is *defined but unset* can be
//! written and filtered without the user naming the field.
//!
//! The reported failure: `due_date` is defined on the core `task` schema, but
//! `get_node` returned only *populated* properties, so an unset field was
//! indistinguishable from one that does not exist. The model — correctly
//! following "use the node's own existing property keys" — declined and asked
//! the user "what field name is used on this task node that tracks dates?".
//! The same gap on the read side had it asking the user to confirm that
//! `status` was the field and `open` a legal value. Both are defined on the
//! schema all along.
//!
//! Core types are the whole point here: the `RELEVANT ENTITY TYPES` block
//! excludes them by construction, so for `task`/`text` there was no path —
//! prompt block or tool call — by which the field list could reach the model.
//! The fix delivers it on the tool result instead (ADR-064 rule 4), leaving
//! that exclusion untouched.
//!
//! Two layers, deliberately:
//!
//! - The `deterministic_*` tests drive the real `GraphToolExecutor` over a real
//!   `SqliteStore` and assert on the tool-result *contract*. They need no model
//!   and run in the normal suite, so a regression in what the model is shown is
//!   caught by `test:all` rather than only by an ignored live run.
//! - The `#[ignore]`d live test proves the locked model actually *uses* it.
//!   That distinction is load-bearing on this stack: #1931's guidance verified
//!   3/3 in isolation and still failed live, so a contract test alone does not
//!   establish a fix on this surface.
//!
//! Live run:
//! ```text
//! cargo test -p nodespace-agent --test live_unset_schema_field -- --ignored --nocapture --test-threads=1
//! ```

use std::sync::Arc;

use nodespace_agent::agent_types::{
    ChatInferenceEngine, ChatMessage, InferenceRequest, ModelFamily, Role, StreamingChunk,
    ToolDefinition,
};
use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
use nodespace_agent::local_agent::tools::{all_tool_definitions, GraphToolExecutor};
use nodespace_agent::AgentToolExecutor;
use nodespace_core::db::SqliteStore;
use nodespace_core::services::NodeService;
use nodespace_nlp_engine::chat::ChatConfig;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Independent repetitions for the live test. Single-run results on this stack
/// are not decision-grade — identical code has scored differently across
/// repeated runs at temperature 0.1.
const REPS: usize = 3;

fn model_path() -> String {
    let home = std::env::var("HOME").expect("HOME must be set");
    format!("{home}/.nodespace/models/gemma-4-E4B-it-Q4_K_M.gguf")
}

fn load_engine() -> LlamaChatInferenceEngine {
    let config = ChatConfig {
        n_ctx: 32768,
        default_temperature: 0.1,
        ..Default::default()
    };
    LlamaChatInferenceEngine::load(&model_path(), ModelFamily::Gemma4, config)
        .expect("model must load from the standard catalog path")
}

async fn make_executor() -> (GraphToolExecutor, Arc<NodeService>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("live.db");
    let mut store: Arc<SqliteStore> = Arc::new(SqliteStore::new(db_path).await.unwrap());
    let ns = Arc::new(NodeService::new(&mut store).await.unwrap());
    let executor = GraphToolExecutor {
        node_service: Some(ns.clone()),
        embedding_service: Arc::new(RwLock::new(None)),
        inference_engine: None,
    };
    (executor, ns, tmp)
}

/// Create the reproducing fixture: a task with only `status` populated, so
/// `due_date` is defined-but-unset exactly as in the report.
async fn create_polestar_task(executor: &GraphToolExecutor) -> String {
    let created = executor
        .execute(
            "create_node",
            json!({
                "content": "Schedule chip upgrade on the Polestar",
                "node_type": "task",
                "properties": {"status": "in_progress"},
            }),
        )
        .await
        .expect("fixture creation must succeed");
    created.result["id"]
        .as_str()
        .unwrap()
        .trim_start_matches("nodespace://")
        .to_string()
}

fn available_field<'a>(result: &'a Value, name: &str) -> Option<&'a Value> {
    result["available_properties"]
        .as_array()?
        .iter()
        .find(|f| f["name"] == name)
}

// ---------------------------------------------------------------------------
// Deterministic contract tests — no model required.
// ---------------------------------------------------------------------------

/// The write-path defect, stated as a contract: `get_node` on a core-type node
/// must name a defined-but-unset field and mark it unset. Before the fix
/// `properties` held only `status` and `due_date` appeared nowhere.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_get_node_surfaces_unset_core_schema_fields() {
    let (executor, _ns, _tmp) = make_executor().await;
    let node_id = create_polestar_task(&executor).await;

    let stored = executor
        .execute("get_node", json!({ "id": node_id }))
        .await
        .expect("get_node must succeed");

    let due = available_field(&stored.result, "due_date").unwrap_or_else(|| {
        panic!(
            "due_date is defined on the core task schema but is absent from \
             available_properties — the model cannot name a field it is never shown. \
             result={}",
            stored.result
        )
    });
    assert_eq!(
        due["set"],
        json!(false),
        "due_date has no value on this node, so it must report set=false — that flag is \
         the only thing distinguishing 'defined but unset' from 'does not exist'"
    );

    // The populated field must be distinguishable from the unset one, or the
    // flag carries no information.
    let status = available_field(&stored.result, "status").expect("status must be listed");
    assert_eq!(status["set"], json!(true));

    // Enum values must ride along, or `open` still has to be guessed.
    let allowed = status["allowed_values"]
        .as_array()
        .expect("status is an enum, so its legal values must be exposed");
    assert!(
        allowed.iter().any(|v| v == "done"),
        "status allowed_values should carry the core enum members, got {allowed:?}"
    );
}

/// The name taken from `available_properties` must be directly usable as an
/// `update_node` key. This is the #1937 read/write-shape concern asserted
/// rather than assumed: reads flatten the stored `{"task":{...}}` namespace and
/// writes re-namespace flat keys, so the flat name must round-trip.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_unset_field_name_round_trips_through_update_node() {
    let (executor, _ns, _tmp) = make_executor().await;
    let node_id = create_polestar_task(&executor).await;

    let stored = executor
        .execute("get_node", json!({ "id": node_id }))
        .await
        .expect("get_node must succeed");
    let field_name = available_field(&stored.result, "due_date")
        .expect("due_date must be listed")["name"]
        .as_str()
        .expect("field name must be a string")
        .to_string();

    // Write using exactly the name the tool result advertised — no translation.
    let outcome = executor
        .execute(
            "update_node",
            json!({ "id": node_id, "properties": { field_name.clone(): "2026-08-06" } }),
        )
        .await
        .expect("update_node must accept a schema-defined field name");
    assert!(!outcome.is_error, "update failed: {:?}", outcome);

    // Assert on the STORE, not the tool's own report — the entire #1937 defect
    // was a success result over an unchanged store.
    let after = executor
        .execute("get_node", json!({ "id": node_id }))
        .await
        .expect("get_node must succeed");
    assert_eq!(
        after.result["properties"][&field_name], json!("2026-08-06"),
        "the field name advertised by available_properties did not persist when passed \
         straight back to update_node — read shape and write shape have diverged. \
         stored={}",
        after.result["properties"]
    );
    // And it now reports as set, closing the loop.
    assert_eq!(
        available_field(&after.result, "due_date").unwrap()["set"],
        json!(true)
    );
}

/// The read-path defect: a zero-result type-scoped search must say which fields
/// exist, so `count: 0` stops being indistinguishable from "that field does not
/// exist" — the ambiguity that had the model asking the user to confirm
/// `status` and `open`.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_empty_search_names_filterable_fields() {
    let (executor, _ns, _tmp) = make_executor().await;
    create_polestar_task(&executor).await;

    let found = executor
        .execute(
            "search_nodes",
            json!({
                "query": "",
                "node_type": "task",
                "filters": [{"type": "property", "property": "status", "operator": "equals", "value": "open"}],
            }),
        )
        .await
        .expect("search_nodes must succeed");

    assert_eq!(found.result["count"], json!(0), "fixture is in_progress, not open");

    let fields = found.result["filterable_properties"]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "an empty type-scoped search must name the type's filterable fields, or the \
                 model cannot tell an empty result from a bad field name. result={}",
                found.result
            )
        });
    let status = fields
        .iter()
        .find(|f| f["name"] == "status")
        .expect("status must be listed as filterable");
    let allowed = status["allowed_values"]
        .as_array()
        .expect("status enum values must be exposed so `open` need not be guessed");
    assert!(
        allowed.iter().any(|v| v == "in_progress"),
        "expected core status values, got {allowed:?}"
    );
    // `set` is per-node and there is no node here — leaving it in would assert
    // something false about every field.
    assert!(
        status.get("set").is_none(),
        "filterable_properties describes a type, not a node, so `set` is meaningless here"
    );
}

/// ADR-064 rule 4: a tool result owns *facts*, not procedures. The field list
/// is a fact and belongs here; prose telling the model what to do about it is a
/// plan, and belongs in the tool description (rule 2) where tool-selection
/// guidance lives.
///
/// An earlier draft of this fix shipped a `no_matches_hint` prose string
/// alongside the list. It read as helpful and violated the doctrine — and the
/// doctrine is measured, not stylistic: instructions delivered as a tool result
/// dropped continuation from 100% to 44% in this codebase's own testing.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_empty_search_returns_facts_not_instructions() {
    let (executor, _ns, _tmp) = make_executor().await;
    create_polestar_task(&executor).await;

    let found = executor
        .execute(
            "search_nodes",
            json!({
                "query": "",
                "node_type": "task",
                "filters": [{"type": "property", "property": "status", "operator": "equals", "value": "open"}],
            }),
        )
        .await
        .expect("search_nodes must succeed");

    let obj = found.result.as_object().expect("result must be an object");
    let allowed = ["count", "nodes", "filterable_properties"];
    let unexpected: Vec<&String> = obj.keys().filter(|k| !allowed.contains(&k.as_str())).collect();
    assert!(
        unexpected.is_empty(),
        "the result carries key(s) {unexpected:?} beyond the facts {allowed:?} — if that is \
         procedural prose it belongs in the tool description, not the tool result (ADR-064 rule 4)"
    );
}

/// A successful search must NOT carry the field block. Appending schema to
/// every result would put a block in front of the model on turns that never
/// needed one — the dilution the resident-prompt findings warn against.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_successful_search_omits_the_field_block() {
    let (executor, _ns, _tmp) = make_executor().await;
    create_polestar_task(&executor).await;

    let found = executor
        .execute("search_nodes", json!({ "query": "", "node_type": "task" }))
        .await
        .expect("search_nodes must succeed");

    assert!(found.result["count"].as_u64().unwrap_or(0) > 0, "fixture must match");
    assert!(
        found.result.get("filterable_properties").is_none(),
        "a non-empty result needs no field list: {}",
        found.result
    );
}

/// ADR-063 guard on the new channel, asserted rather than assumed. The issue is
/// explicit that core schema fields reaching the model again must be re-verified
/// not to reopen the bare-key hole. `available_properties` can only ever echo
/// names the core schema already defines, so it introduces no path to writing a
/// *new* bare key onto a core type.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_available_properties_advertises_no_undefined_bare_keys() {
    let (executor, ns, _tmp) = make_executor().await;
    let node_id = create_polestar_task(&executor).await;

    let stored = executor
        .execute("get_node", json!({ "id": node_id }))
        .await
        .expect("get_node must succeed");

    let schema = ns
        .get_schema_node("task")
        .await
        .expect("schema lookup must succeed")
        .expect("task is a seeded core schema");
    let defined: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();

    let advertised = stored.result["available_properties"]
        .as_array()
        .expect("available_properties must be present");
    assert!(!advertised.is_empty(), "task defines fields, so the list must be non-empty");

    for field in advertised {
        let name = field["name"].as_str().expect("each entry must name a field");
        assert!(
            defined.contains(&name),
            "available_properties advertised '{name}', which the core task schema does not \
             define — this channel must never invent a key, since a bare key written onto a \
             core type is the ADR-063 violation the routing block's is_core filter exists to \
             prevent. defined={defined:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Live test — does the locked model actually act on it?
// ---------------------------------------------------------------------------

/// Only the tools this scenario needs. These are the REAL production
/// definitions, filtered — not hand-authored schemas.
fn write_path_tools() -> Vec<ToolDefinition> {
    all_tool_definitions()
        .into_iter()
        .filter(|t| t.name == "update_node" || t.name == "get_node")
        .collect()
}

/// Runs one turn and returns the parsed (tool_name, args_json), if any.
async fn run_turn(
    engine: &LlamaChatInferenceEngine,
    system_prompt: &str,
    tools: Vec<ToolDefinition>,
    messages: Vec<ChatMessage>,
) -> (Option<(String, String)>, String) {
    let mut all = vec![ChatMessage::text(Role::System, system_prompt.to_string())];
    all.extend(messages);

    let request = InferenceRequest {
        messages: all,
        tools: Some(tools),
        temperature: Some(0.1),
        max_tokens: Some(512),
    };

    let chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = chunks.clone();
    engine
        .generate(
            request,
            Box::new(move |c| {
                if let Ok(mut g) = sink.lock() {
                    g.push(c);
                }
            }),
        )
        .await
        .expect("generation must complete");

    let collected = chunks.lock().expect("chunk mutex").clone();
    let name = collected.iter().find_map(|c| match c {
        StreamingChunk::ToolCallStart { name, .. } => Some(name.clone()),
        _ => None,
    });
    let args: String = collected
        .iter()
        .filter_map(|c| match c {
            StreamingChunk::ToolCallArgs { args_json, .. } => Some(args_json.as_str()),
            _ => None,
        })
        .collect();
    let raw: String = collected
        .iter()
        .filter_map(|c| match c {
            StreamingChunk::Token { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    (name.map(|n| (n, args)), raw)
}

/// The reported scenario end to end: the model is shown the node exactly as
/// `get_node` renders it and asked to set the due date, without ever being told
/// the field name.
///
/// Asserts on the STORE, and on the one behaviour the report is about: the model
/// must not come back asking the user to name the field.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the locked native GGUF on disk"]
async fn live_model_sets_an_unset_field_without_asking_for_its_name() {
    let engine = load_engine();
    let system = "You are a graph-editing assistant. Act on the user's request immediately \
        using the tools available. Do not ask the user to confirm.";

    let mut persisted = 0usize;
    let mut usable_reps = 0usize;

    for rep in 0..REPS {
        let (executor, _ns, _tmp) = make_executor().await;
        let node_id = create_polestar_task(&executor).await;

        // Turn 1 — the model looks the node up, exactly as it does in production.
        let lookup = executor
            .execute("get_node", json!({ "id": node_id }))
            .await
            .expect("get_node must succeed");

        // Feed that real tool result back as the model would see it, then ask
        // for the change. The field name is never mentioned in the prompt.
        let messages = vec![
            ChatMessage::text(
                Role::User,
                "Could you set this task's due date to 06-August-2026?".to_string(),
            ),
            ChatMessage::text(
                Role::Assistant,
                format!("Tool result for get_node:\n{}", lookup.result),
            ),
            ChatMessage::text(
                Role::User,
                "Go ahead and make that change now.".to_string(),
            ),
        ];

        let (call, raw) = run_turn(&engine, system, write_path_tools(), messages).await;

        let Some((name, args_json)) = call else {
            // This IS the reported failure mode: no call, and a question back
            // to the user about which field to use.
            println!("LIVE[rep {rep}] no tool call. raw: {raw:?}");
            let lowered = raw.to_lowercase();
            assert!(
                !(lowered.contains("field name") || lowered.contains("which property")),
                "rep {rep}: the model asked the user to name the field, which is the exact \
                 reported defect — available_properties should have supplied it: {raw:?}"
            );
            continue;
        };
        println!("LIVE[rep {rep}] {name}({args_json})");

        if name != "update_node" {
            println!("LIVE[rep {rep}] non-write tool, skipping");
            continue;
        }

        let args: Value = match serde_json::from_str(&args_json) {
            Ok(v) => v,
            Err(e) => {
                println!("LIVE[rep {rep}] unparseable args ({e}), skipping");
                continue;
            }
        };
        usable_reps += 1;

        let outcome = executor.execute("update_node", args).await;
        let after = executor
            .execute("get_node", json!({ "id": node_id }))
            .await
            .expect("get_node must succeed");
        let props = &after.result["properties"];

        match outcome {
            Err(e) => println!("LIVE[rep {rep}] rejected: {e}"),
            Ok(result) if result.is_error => {
                println!("LIVE[rep {rep}] tool error: {}", result.result)
            }
            Ok(_) => {
                // The date must have landed on a schema-defined field. Which
                // date field the model picks is its business; that SOME defined
                // field now holds the value is the fix working.
                let wrote_a_date = props.as_object().is_some_and(|o| {
                    o.iter().any(|(k, v)| {
                        k != "status" && v.as_str().is_some_and(|s| s.contains("2026-08-06"))
                    })
                });
                assert!(
                    wrote_a_date,
                    "rep {rep}: update reported success but no schema field holds the date: {props}"
                );
                persisted += 1;
            }
        }
    }

    println!("LIVE SUMMARY: persisted={persisted} of {usable_reps} usable reps ({REPS} total)");
    assert!(
        persisted > 0,
        "no rep persisted the date — the model still cannot write a defined-but-unset field"
    );
}

/// A node of a type that defines no fields (`text`) must come back unchanged
/// rather than erroring or carrying an empty list — the lookup the model asked
/// for is what matters, and the field list is an addition to it, never a
/// precondition.
///
/// The list must be *absent*, not an empty array: an empty array reads as "this
/// type has no fields", which is a stronger claim than "no fields were found",
/// and on a type that does define fields it would be actively wrong.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_fieldless_type_is_unaffected() {
    let (executor, _ns, _tmp) = make_executor().await;
    let created = executor
        .execute(
            "create_node",
            json!({ "content": "just a note", "node_type": "text" }),
        )
        .await
        .expect("text node creation must succeed");
    let node_id = created.result["id"]
        .as_str()
        .unwrap()
        .trim_start_matches("nodespace://")
        .to_string();

    let stored = executor
        .execute("get_node", json!({ "id": node_id }))
        .await
        .expect("get_node must succeed regardless of the field list");
    assert!(!stored.is_error);
    assert_eq!(stored.result["content"], json!("just a note"));

    // Whatever `text` defines, the key is either absent or a non-empty list —
    // never an empty array asserting the type has nothing.
    if let Some(available) = stored.result.get("available_properties") {
        assert!(
            !available.as_array().expect("must be an array").is_empty(),
            "an empty available_properties claims the type defines no fields; omit the key instead"
        );
    }
}

/// Fetching a schema node itself must not recurse into "the schema of a
/// schema". Schema nodes use a non-namespaced property format, so treating
/// their fields as instance properties would describe them wrongly.
#[tokio::test(flavor = "multi_thread")]
async fn deterministic_get_node_on_a_schema_node_is_sane() {
    let (executor, _ns, _tmp) = make_executor().await;

    let stored = executor
        .execute("get_node", json!({ "id": "task" }))
        .await
        .expect("fetching the task schema node must succeed");

    assert!(!stored.is_error, "schema fetch errored: {:?}", stored);
    // Whatever it reports, it must not claim the *schema* node has task's
    // instance fields set on it.
    if let Some(available) = stored.result.get("available_properties") {
        let named: Vec<&str> = available
            .as_array()
            .map(|a| a.iter().filter_map(|f| f["name"].as_str()).collect())
            .unwrap_or_default();
        assert!(
            !named.contains(&"due_date"),
            "the task SCHEMA node was described using task's own instance fields: {named:?}"
        );
    }
}
