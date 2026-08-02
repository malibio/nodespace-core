//! Live check for #1909: does Stage 1 (ADR-038) have a correct move for a
//! compound, unambiguous request — one message carrying two distinct intents,
//! neither of which is unclear on its own?
//!
//! `routing.rs` offers exactly two Stage-1 tools: `route_query` (one
//! capability, one search string) and `route_clarify` (the request is too
//! ambiguous to describe). Neither has an obviously correct answer for "log
//! this expense and remind me to follow up Friday" — nothing is ambiguous
//! about either half, there are just two of them. This is a structural gap
//! identified by reading the code, not yet a measured defect; this test is
//! the live measurement #1909 asks for before proposing a fix.
//!
//! Fires production's exact Stage-1 request — `STAGE1_SYSTEM_PROMPT`,
//! `stage1_tool_definitions()`, temperature 0.1, `STAGE1_MAX_TOKENS` — against
//! the **locked native model, loaded in-process** (ADR-056: Gemma 4 E4B via
//! llama.cpp), for a handful of compound-but-unambiguous prompts. Records the
//! raw tool call plus the parsed `RouteDecision`. No retrieval, no Stage 2:
//! this isolates Stage 1's own choice, which is what the issue is about.
//!
//! Deliberately native, not OpenAI-compatible/HTTP — `routing_latency.rs`
//! documents why a served stand-in (even a served copy of the same weights)
//! measures a different stack and does not describe what ships. This file
//! reuses that module's `resolve_backend` pattern so the two probes cannot
//! drift on how the locked model is loaded.
//!
//! Ignored by default — loads a ~9.6GB GGUF and runs 10 live generations, too
//! slow for the default `test:all` run. Skips gracefully (rather than
//! failing) when the locked model is not downloaded. Run explicitly:
//!
//! ```text
//! cargo test -p nodespace-agent --test live_stage1_compound_intent -- --ignored --nocapture
//! ```

use nodespace_agent::agent_types::{
    ChatInferenceEngine, InferenceRequest, ModelFamily, StreamingChunk,
};
use nodespace_agent::local_agent::agent_loop::{STAGE1_MAX_TOKENS, STAGE1_SYSTEM_PROMPT};
use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::routing::{
    self, RouteDecision, ROUTE_CLARIFY_TOOL, ROUTE_QUERY_TOOL,
};
use nodespace_nlp_engine::chat::types::{ChatMessage, Role};
use nodespace_nlp_engine::ChatConfig;
use std::sync::{Arc, Mutex};

/// The locked native agent model (ADR-056). This probe measures only this
/// model — a routing result on anything else does not describe what ships.
const LOCKED_NATIVE_MODEL: &str = "gemma-4-e4b-q4km";

/// Load the locked native model in-process, the way NodeSpace runs it.
///
/// Mirrors `routing_latency.rs`'s `resolve_backend` exactly: there is
/// deliberately no OpenAI-compatible/HTTP path here, for the same reason that
/// file documents — measuring through a served stand-in measures a different
/// stack, on whatever model that server happens to have loaded, and has
/// already produced a suppression result on this project that turned out not
/// to apply to the shipped model.
async fn resolve_locked_model() -> Option<Arc<dyn ChatInferenceEngine>> {
    let gguf = GgufModelManager::new().ok()?;
    let path = gguf.model_path(LOCKED_NATIVE_MODEL).ok()?;
    if !path.exists() {
        return None;
    }
    let path_str = path.to_string_lossy().to_string();
    let engine = tokio::task::spawn_blocking(move || {
        LlamaChatInferenceEngine::load(&path_str, ModelFamily::Gemma4, ChatConfig::default())
    })
    .await
    .ok()?
    .ok()?;
    Some(Arc::new(engine) as Arc<dyn ChatInferenceEngine>)
}

/// Compound, unambiguous prompts: two distinct operations, no ambiguity about
/// either half. Each carries a short label naming the two intents so the
/// printed trace is legible without re-reading the prompt text.
const COMPOUND_PROMPTS: &[(&str, &str)] = &[
    (
        "expense + reminder",
        "Log a $42 lunch expense and remind me to follow up with Sarah on Friday.",
    ),
    (
        "search + create",
        "Find my notes about the Q3 budget and create a task to review them tomorrow.",
    ),
    (
        "two tasks",
        "Add a task to call the plumber and add another task to pay the electric bill.",
    ),
    (
        "note + task",
        "Write down that the client meeting moved to 3pm and add a task to update the calendar invite.",
    ),
    (
        "delete + search",
        "Delete the draft invoice for Acme Corp and find all other invoices from last month.",
    ),
];

/// A single-intent control for each compound prompt above, same domain and
/// phrasing style, so a comparison can tell "this model routes badly in
/// general" apart from "this model specifically mishandles compounding".
const SINGLE_INTENT_CONTROLS: &[(&str, &str)] = &[
    ("expense only", "Log a $42 lunch expense."),
    ("search only", "Find my notes about the Q3 budget."),
    ("task only", "Add a task to call the plumber."),
    (
        "note only",
        "Write down that the client meeting moved to 3pm.",
    ),
    ("delete only", "Delete the draft invoice for Acme Corp."),
];

/// What Stage 1 actually did on one prompt.
#[derive(Debug, Clone)]
enum Stage1Outcome {
    /// A routing tool was called and parsed cleanly.
    Decided(RouteDecision),
    /// A routing tool was called but `parse_route_decision` rejected it
    /// (blank query, blank question, or malformed JSON).
    CalledButUnparseable { tool: String, args: String },
    /// The model called neither Stage-1 tool.
    NoToolCall,
    /// More than one tool call in a single Stage-1 turn — itself notable,
    /// since `STAGE1_SYSTEM_PROMPT` says "call exactly one tool".
    MultipleToolCalls(Vec<(String, String)>),
    /// The generation itself failed. Not a finding about routing.
    Errored(String),
}

async fn run_stage1(engine: &Arc<dyn ChatInferenceEngine>, user_message: &str) -> Stage1Outcome {
    let request = InferenceRequest {
        messages: vec![
            ChatMessage::text(Role::System, STAGE1_SYSTEM_PROMPT.to_string()),
            ChatMessage::text(Role::User, user_message.to_string()),
        ],
        tools: Some(routing::stage1_tool_definitions()),
        temperature: Some(0.1),
        max_tokens: Some(STAGE1_MAX_TOKENS),
    };

    // Collect (id, name, args_json) per tool call, preserving call order.
    let calls: Arc<Mutex<Vec<(String, String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&calls);

    let generated = engine
        .generate(
            request,
            Box::new(move |chunk| {
                let mut g = sink.lock().expect("sink not poisoned");
                match chunk {
                    StreamingChunk::ToolCallStart { id, name } => {
                        g.push((id, name, String::new()));
                    }
                    StreamingChunk::ToolCallArgs { id, args_json } => {
                        if let Some(entry) = g.iter_mut().find(|(cid, _, _)| *cid == id) {
                            entry.2.push_str(&args_json);
                        }
                    }
                    _ => {}
                }
            }),
        )
        .await;

    if let Err(e) = generated {
        return Stage1Outcome::Errored(e.to_string());
    }

    let observed = calls.lock().expect("sink not poisoned").clone();
    match observed.len() {
        0 => Stage1Outcome::NoToolCall,
        1 => {
            let (_, name, args) = &observed[0];
            match routing::parse_route_decision(name, args) {
                Some(decision) => Stage1Outcome::Decided(decision),
                None => Stage1Outcome::CalledButUnparseable {
                    tool: name.clone(),
                    args: args.clone(),
                },
            }
        }
        _ => {
            Stage1Outcome::MultipleToolCalls(observed.into_iter().map(|(_, n, a)| (n, a)).collect())
        }
    }
}

fn display(outcome: &Stage1Outcome) -> String {
    match outcome {
        Stage1Outcome::Decided(RouteDecision::Query(q)) => {
            format!("route_query(\"{q}\")")
        }
        Stage1Outcome::Decided(RouteDecision::Clarify { question, options }) => {
            format!("route_clarify(\"{question}\", options={options:?})")
        }
        Stage1Outcome::CalledButUnparseable { tool, args } => {
            format!("UNPARSEABLE {tool}({args})")
        }
        Stage1Outcome::NoToolCall => "NO TOOL CALL".to_string(),
        Stage1Outcome::MultipleToolCalls(calls) => {
            format!("MULTIPLE CALLS {calls:?}")
        }
        Stage1Outcome::Errored(e) => format!("ERROR {e}"),
    }
}

/// Whether a query decision mentions both intents (crude substring check on
/// hand-picked keywords per prompt) rather than silently dropping one.
///
/// This is a heuristic, not a verdict: a query can legitimately paraphrase.
/// It exists to flag the single-intent-silently-dropped failure mode for
/// human inspection in the printed trace, not to assert pass/fail — that
/// judgement belongs to a human reading the raw traces, per the issue's own
/// acceptance criteria.
fn mentions_both(query: &str, keywords: (&str, &str)) -> bool {
    let lower = query.to_ascii_lowercase();
    lower.contains(&keywords.0.to_ascii_lowercase())
        && lower.contains(&keywords.1.to_ascii_lowercase())
}

/// Per-prompt keyword pairs used only for the `mentions_both` hint printed
/// alongside each result — see [`mentions_both`].
fn keyword_pair_for(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "expense + reminder" => Some(("expense", "remind")),
        "search + create" => Some(("budget", "task")),
        "two tasks" => Some(("plumber", "electric")),
        "note + task" => Some(("meeting", "calendar")),
        "delete + search" => Some(("delete", "invoice")),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a local inference backend; loads the locked native GGUF and runs 10 live generations"]
async fn stage1_on_compound_unambiguous_requests() {
    let Some(engine) = resolve_locked_model().await else {
        eprintln!(
            "locked native model ({LOCKED_NATIVE_MODEL}) not downloaded — \
             skipping compound-intent probe"
        );
        return;
    };

    println!("\nStage-1 compound-intent probe — {LOCKED_NATIVE_MODEL} (native, in-process)\n");

    let mut report = String::new();
    report.push_str(&format!("== {LOCKED_NATIVE_MODEL} (native) ==\n"));

    println!("-- single-intent controls --");
    for (label, prompt) in SINGLE_INTENT_CONTROLS {
        let outcome = run_stage1(&engine, prompt).await;
        let line = format!("  [{label:<14}] {prompt}\n      -> {}", display(&outcome));
        println!("{line}");
        report.push_str(&line);
        report.push('\n');
    }

    println!("-- compound, unambiguous prompts --");
    for (label, prompt) in COMPOUND_PROMPTS {
        let outcome = run_stage1(&engine, prompt).await;
        let mut line = format!("  [{label:<14}] {prompt}\n      -> {}", display(&outcome));
        if let Stage1Outcome::Decided(RouteDecision::Query(q)) = &outcome {
            if let Some(kw) = keyword_pair_for(label) {
                line.push_str(&format!(
                    "\n      (mentions both intents by keyword: {})",
                    mentions_both(q, kw)
                ));
            }
        }
        println!("{line}");
        report.push_str(&line);
        report.push('\n');
    }

    match std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("stage1-compound-intent-latest.txt"),
        &report,
    ) {
        Ok(()) => println!("\nraw trace written to tests/stage1-compound-intent-latest.txt"),
        Err(e) => println!("\ncould not write trace artifact: {e}"),
    }

    // No assertions on model behaviour: per the issue, this test exists to
    // produce raw traces for a human decision, not to encode a pass/fail bar
    // for a question not yet answered. The only hard failure is a broken run.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ROUTE_QUERY_TOOL`/`ROUTE_CLARIFY_TOOL` are re-exported so this file's
    /// dependency on the wire names cannot silently drift from `routing.rs`.
    #[test]
    fn tool_name_constants_match_stage1_tool_definitions() {
        let defs = routing::stage1_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec![ROUTE_QUERY_TOOL, ROUTE_CLARIFY_TOOL]);
    }

    #[test]
    fn every_compound_prompt_has_a_single_intent_control() {
        assert_eq!(COMPOUND_PROMPTS.len(), SINGLE_INTENT_CONTROLS.len());
    }
}
