//! Hand-authored golden prompt for agent-matrix scenario 6, built independent
//! of ANY NodeSpace prompt-assembly source (no `STAGE1_SYSTEM_PROMPT`, no
//! `skill_pipeline.rs`, no `agent_guidance.rs` constants). Every string below
//! is authored fresh in this file.
//!
//! Methodology per the standing instruction on #1917: assemble a sequence of
//! literal prompt strings, validate directly against llama.cpp with zero
//! NodeSpace plumbing involved, until a version is found that reliably gets
//! the correct tool call. THAT becomes the target the real assembly pipeline
//! is engineered toward — not the other way around.
//!
//! Prior finding this experiment is built on (#1922, subagent investigation):
//! the model resolves "the 2400 one" -> resolve_query correctly with NO prior
//! history, 3/3. It refuses and asks for a literal node id the moment ANY
//! prior turn is present, 3/3, across 5 independent 3-rep trials on 4
//! different prompt channels (guidance reorder, injected write-record facts,
//! tool-description carve-outs, history composition). The scenario's real
//! conversation history is ~8,322 tokens of mostly-boilerplate wrapped around
//! a 10-word request -- consistent with this project's own resident-prompt
//! dilution finding (10,493 chars scored 50% vs 445 chars at 73% on cases
//! targeting the SAME prompt's own rules).
//!
//! This experiment's question: is the defect "any history at all" (unfixable
//! by prompt engineering, a real capability boundary) or "history above some
//! size/shape" (fixable by compacting what's carried forward)? Tests a
//! COMPACT hand-written summary of the same facts scenario 6's real 3-turn
//! history carries, instead of the full turn-by-turn reconstruction.
//!
//! Ignored by default — loads the 5GB locked native GGUF. Run explicitly:
//! ```text
//! cargo test -p nodespace-agent --test golden_scenario6_handauthored -- --ignored --nocapture --test-threads=1
//! ```

use std::sync::Arc;

use nodespace_agent::agent_types::{
    ChatInferenceEngine, ChatMessage, InferenceRequest, ModelFamily, Role, StreamingChunk,
    ToolDefinition,
};
use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
use nodespace_nlp_engine::chat::ChatConfig;

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

/// Hand-authored `resolve_query` tool -- NOT `def_resolve_query()` from
/// `tools.rs`. Deliberately minimal: enough for the model to understand the
/// tool's purpose, none of the accumulated production prose.
fn hand_authored_resolve_query_tool() -> ToolDefinition {
    ToolDefinition {
        name: "resolve_query".into(),
        description: "Resolve an indirect reference (a bare value, a relative date, a \
            description) to the single node it refers to. Returns the node directly -- do not \
            call search_nodes afterward."
            .into(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "request": {"type": "string", "description": "The user's request, verbatim."},
                "node_type": {"type": "string", "description": "The target node type id."}
            },
            "required": ["request", "node_type"]
        }),
    }
}

fn hand_authored_update_node_tool() -> ToolDefinition {
    ToolDefinition {
        name: "update_node".into(),
        description: "Update an existing node's properties, given its id.".into(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "properties": {"type": "object"}
            },
            "required": ["id"]
        }),
    }
}

/// Run a hand-authored turn and print/return the tool call the model made.
async fn run_turn(
    engine: &LlamaChatInferenceEngine,
    system_prompt: &str,
    history: Vec<ChatMessage>,
    user_message: &str,
) -> Option<(String, String)> {
    let mut messages = vec![ChatMessage::text(Role::System, system_prompt.to_string())];
    messages.extend(history);
    messages.push(ChatMessage::text(Role::User, user_message.to_string()));

    let request = InferenceRequest {
        messages,
        tools: Some(vec![
            hand_authored_resolve_query_tool(),
            hand_authored_update_node_tool(),
        ]),
        temperature: Some(0.1),
        max_tokens: Some(512),
    };

    let chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
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
    })?;
    let args: String = collected
        .iter()
        .filter_map(|c| match c {
            StreamingChunk::ToolCallArgs { args_json, .. } => Some(args_json.as_str()),
            _ => None,
        })
        .collect();
    Some((name, args))
}

/// Baseline control: zero history, minimal hand-authored prompt. Expected
/// (per #1922's finding) to call resolve_query correctly -- confirms this
/// harness's minimal tools/prompt reproduce the known-good case before
/// testing the compact-history variant below.
#[tokio::test]
#[ignore = "requires the locked native GGUF on disk"]
async fn golden_zero_history_control() {
    let engine = load_engine();
    let system = "You are a graph-editing assistant. When a request refers to something \
        indirectly (a bare value, a description) rather than by name, call resolve_query to \
        find it. Never ask the user to supply a node id yourself.";

    let result = run_turn(
        &engine,
        system,
        vec![],
        "The 2400 one came back — set it to returned",
    )
    .await;

    match &result {
        Some((name, args)) => println!("GOLDEN[zero-history] {name}({args})"),
        None => println!("GOLDEN[zero-history] no tool call parsed"),
    }
    assert_eq!(
        result.as_ref().map(|(n, _)| n.as_str()),
        Some("resolve_query"),
        "zero-history control must call resolve_query — if this fails, the harness itself \
         (not scenario 6) has a problem, since #1922 already confirmed this case works"
    );
}

/// The actual experiment: a COMPACT hand-written summary of scenario 6's real
/// history (schema created with a replacement_cost field; one equipment item
/// logged with cost 2400) instead of the full multi-turn reconstruction with
/// tool-call records, write-receipts, and routing candidate blocks. ~120
/// words vs the real turn's ~8,322 tokens.
///
/// If this passes reliably, the defect is prompt SIZE/SHAPE (fixable by
/// compaction) not "any history" (a hard capability boundary). If it fails
/// the same way as the full history, size/shape is refuted and the boundary
/// really is presence-of-any-history.
#[tokio::test]
#[ignore = "requires the locked native GGUF on disk"]
async fn golden_compact_history_variant() {
    let engine = load_engine();
    let system = "You are a graph-editing assistant. When a request refers to something \
        indirectly (a bare value, a description) rather than by name, call resolve_query to \
        find it. Never ask the user to supply a node id yourself.";

    let compact_history = vec![ChatMessage::text(
        Role::System,
        "Context: the user has an 'equipment' type with a replacement_cost field. They logged \
         one equipment item with replacement_cost 2400."
            .to_string(),
    )];

    let result = run_turn(
        &engine,
        system,
        compact_history,
        "The 2400 one came back — set it to returned",
    )
    .await;

    match &result {
        Some((name, args)) => println!("GOLDEN[compact-history] {name}({args})"),
        None => println!("GOLDEN[compact-history] no tool call parsed"),
    }
    // Not asserted pass/fail — this is the discriminating experiment itself.
    // The printed result IS the finding.
}
