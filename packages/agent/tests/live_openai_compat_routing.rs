//! Live routing-reliability matrix for the OpenAI-compatible path.
//!
//! [ADR-056](../../../../nodespace-docs/decisions/056-gemma-4-e4b-locked-native-model.md)
//! locks only the *native* in-process GGUF path to one model. The
//! OpenAI-compatible path stays open to any user-configured server by design,
//! which means ADR-038's two-stage routing runs against models nobody has
//! characterised.
//!
//! `live_openai_compat_smoke.rs` guards that the transport works: discovery
//! reaches the endpoint and an *unrouted* turn produces a tool call. That test
//! cannot see the failure this one looks for, because the failure appears only
//! once Stage-2's candidate block is in the prompt — a block the smoke test
//! never injects.
//!
//! The four arms isolate which part of routing, if any, suppresses
//! tool-calling on a served model:
//!
//! | arm             | candidate block | what it isolates                    |
//! |-----------------|-----------------|-------------------------------------|
//! | `baseline`      | none            | can the model tool-call at all      |
//! | `stage1_only`   | none            | does Stage 1's extra turn cost it   |
//! | `routed_full`   | yes, full       | ADR-064 rule 4's predicted mechanism |
//! | `routed_names`  | yes, names only | whether *content* is the variable   |
//!
//! `routed_names` is the arm that carries the finding. If a model fires on
//! `routed_full` but not `routed_names` — or suppresses on both — the operative
//! variable is the block's *presence*, not how procedural its content is, which
//! is what ADR-064 rule 4 predicts and what was measured on the locked model.
//!
//! Measured against a local Ollama serving three models:
//!
//! ```text
//! model        baseline  stage1_only  routed_full  routed_names
//! mistral:7b   fires     fires        SUPPRESSED   SUPPRESSED
//! ornith:9b    fires     fires        fires        fires
//! gemma4:e4b   fires     fires        fires        fires
//! ```
//!
//! Two conclusions that shape what the code around this may assume:
//!
//! 1. On `mistral:7b` the block's *presence* suppresses tool-calling, not its
//!    content — `routed_names` carries a name and nothing else and still
//!    suppresses. ADR-064 rule 4's mechanism, measured on the locked model,
//!    does not explain this.
//! 2. Suppression is **not** a property of being non-native. `ornith:9b` is
//!    neither locked nor native and routes cleanly. So "disable routing for
//!    everything that isn't the locked model" would be wrong — it would cost
//!    routing on models that handle it fine.
//!
//! Ignored by default — it needs a real server. Run explicitly:
//!
//! ```text
//! cargo test -p nodespace-agent --test live_openai_compat_routing -- --ignored --nocapture
//! ```

use nodespace_agent::agent_types::{
    ChatInferenceEngine, InferenceRequest, SkillCandidate, StreamingChunk, ToolDefinition,
};
use nodespace_agent::local_agent::openai_compat_discovery::discover_models;
use nodespace_agent::local_agent::openai_compat_inference::OpenAiCompatInferenceEngine;
use nodespace_agent::local_agent::routing;
use nodespace_nlp_engine::chat::types::{ChatMessage, Role};
use std::sync::{Arc, Mutex};

const BASE_URL: &str = "http://127.0.0.1:11434/v1";

/// The request every arm sends. Phrased so the correct behaviour is
/// unambiguous — a model that does not call `search_nodes` here has not made a
/// defensible judgement call, it has failed to tool-call.
const USER_MESSAGE: &str = "Find my notes about the Q3 budget.";

/// A minimal system prompt standing in for the assembled one.
///
/// Deliberately short. The production prompt is long enough that it would be a
/// second uncontrolled variable, and this test is isolating the candidate
/// block, not the resident prompt.
const SYSTEM_PROMPT: &str =
    "You are NodeSpace's assistant. Use the available tools to fulfil the user's request.";

/// Which arm, and therefore what goes into the system prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// No routing at all — the shape `live_openai_compat_smoke.rs` covers.
    Baseline,
    /// Stage 1 runs, but retrieval surfaced nothing eligible, so no block is
    /// injected. Separates "the extra turn" from "the injected block".
    Stage1Only,
    /// The routed path as it actually ships: candidates rendered by production
    /// code, instructions and all.
    RoutedFull,
    /// Candidates stripped to bare names — no instructions, no purpose, no
    /// metadata. If this arm suppresses too, content is not the variable.
    RoutedNames,
}

impl Arm {
    const ALL: [Arm; 4] = [
        Arm::Baseline,
        Arm::Stage1Only,
        Arm::RoutedFull,
        Arm::RoutedNames,
    ];

    fn label(self) -> &'static str {
        match self {
            Arm::Baseline => "baseline",
            Arm::Stage1Only => "stage1_only",
            Arm::RoutedFull => "routed_full",
            Arm::RoutedNames => "routed_names",
        }
    }

    /// Whether this arm puts a candidate block in the prompt.
    fn injects_block(self) -> bool {
        matches!(self, Arm::RoutedFull | Arm::RoutedNames)
    }
}

/// The skill a correct route would surface for [`USER_MESSAGE`].
///
/// Scored well clear of `READ_SKILL_SCORE_BAR` so the mechanical gate is not
/// the thing under test — this test is about the model's response to the
/// block, not about whether the gate admits it.
fn research_candidate() -> SkillCandidate {
    SkillCandidate {
        id: "skill-research".to_string(),
        name: "Research".to_string(),
        description: "Find existing nodes and answer questions about their content.".to_string(),
        score: 0.9,
        tools: vec!["search_nodes".to_string()],
        instructions: "Call search_nodes with a query describing what the user is looking for. \
             Prefer specific terms from the user's request over generic ones. Report what you \
             found; do not invent nodes that were not returned."
            .to_string(),
        schema_metadata: serde_json::json!([]),
    }
}

/// The same candidate reduced to its name.
///
/// Every field the full rendering would surface as prose is emptied, so the
/// block that reaches the model carries a name and nothing else.
fn names_only_candidate() -> SkillCandidate {
    SkillCandidate {
        description: String::new(),
        instructions: String::new(),
        schema_metadata: serde_json::json!([]),
        ..research_candidate()
    }
}

/// The one tool every arm offers.
fn search_tool() -> ToolDefinition {
    ToolDefinition {
        name: "search_nodes".to_string(),
        description: "Search the knowledge graph for nodes matching a query.".to_string(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What to search for."}
            },
            "required": ["query"]
        }),
    }
}

/// Build an arm's system prompt, using **production** rendering for the routed
/// arms so this measures what ships rather than a lookalike written here.
fn system_prompt_for(arm: Arm) -> String {
    let block = match arm {
        Arm::Baseline | Arm::Stage1Only => None,
        Arm::RoutedFull => routing::render_candidates_for_prompt(&[research_candidate()]),
        Arm::RoutedNames => routing::render_candidates_for_prompt(&[names_only_candidate()]),
    };

    // Mirrors the concatenation at the real injection point in `agent_loop`.
    match block {
        Some(b) => format!("{SYSTEM_PROMPT}\n\n{b}"),
        None => SYSTEM_PROMPT.to_string(),
    }
}

/// Run one arm against one model. Returns the tool calls it produced.
async fn run_arm(model: &str, arm: Arm) -> Vec<String> {
    let engine =
        OpenAiCompatInferenceEngine::new(BASE_URL.to_string(), String::new(), model.to_string());

    // Stage 1 is a separate generation ahead of the real turn. Arms that model
    // it pay that cost so the extra turn is not a hidden difference between
    // arms; its output is deliberately discarded, exactly as the shipped loop
    // discards it when retrieval yields nothing eligible.
    if arm == Arm::Stage1Only || arm.injects_block() {
        let stage1 = InferenceRequest {
            messages: vec![
                ChatMessage::text(Role::System, "Route the user's request."),
                ChatMessage::text(Role::User, USER_MESSAGE),
            ],
            tools: Some(routing::stage1_tool_definitions()),
            temperature: Some(0.0),
            max_tokens: None,
        };
        // A Stage-1 failure is not this test's subject; the shipped loop warns
        // and continues unrouted, so mirror that rather than failing the arm.
        let _ = engine.generate(stage1, Box::new(|_| {})).await;
    }

    let request = InferenceRequest {
        messages: vec![
            ChatMessage::text(Role::System, system_prompt_for(arm)),
            ChatMessage::text(Role::User, USER_MESSAGE),
        ],
        tools: Some(vec![search_tool()]),
        temperature: Some(0.0),
        max_tokens: None,
    };

    let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&calls);

    let generated = engine
        .generate(
            request,
            Box::new(move |chunk| {
                if let StreamingChunk::ToolCallStart { name, .. } = chunk {
                    sink.lock().expect("sink not poisoned").push(name);
                }
            }),
        )
        .await;

    if let Err(e) = generated {
        // Report rather than panic: one model erroring should not hide the
        // matrix for every other model the box serves.
        println!("    [{}] generation error: {e}", arm.label());
        return Vec::new();
    }

    let observed = calls.lock().expect("sink not poisoned").clone();
    observed
}

#[tokio::test]
#[ignore = "requires a running OpenAI-compatible server"]
async fn routing_does_not_suppress_tool_calling_on_served_models() {
    let models = discover_models(BASE_URL, "")
        .await
        .expect("discovery should reach the endpoint");
    assert!(!models.is_empty(), "expected at least one served model");

    println!("\nrouting reliability matrix — {BASE_URL}");
    println!("served: {models:?}\n");

    // Models whose baseline fires but which lose tool-calling under an
    // injected block. This is the finding the issue is about; it is reported,
    // not asserted, because it is a property of the served model rather than a
    // NodeSpace regression.
    let mut suppressed: Vec<(String, Vec<&'static str>)> = Vec::new();
    // Models that cannot tool-call even unrouted. A different failure — not
    // routing's fault, and not something this test should report as one.
    let mut cannot_tool_call: Vec<String> = Vec::new();

    for model in &models {
        println!("  {model}");
        let mut fired: Vec<(Arm, bool)> = Vec::new();

        for arm in Arm::ALL {
            let calls = run_arm(model, arm).await;
            let ok = calls.iter().any(|c| c == "search_nodes");
            println!(
                "    {:<13} {:<10} {calls:?}",
                arm.label(),
                if ok { "fires" } else { "SUPPRESSED" }
            );
            fired.push((arm, ok));
        }

        let baseline_fires = fired.iter().any(|(a, ok)| *a == Arm::Baseline && *ok);

        if !baseline_fires {
            cannot_tool_call.push(model.clone());
            println!("    -> cannot tool-call unrouted; routing arms are not interpretable\n");
            continue;
        }

        let lost: Vec<&'static str> = fired
            .iter()
            .filter(|(a, ok)| a.injects_block() && !ok)
            .map(|(a, _)| a.label())
            .collect();

        if lost.is_empty() {
            println!("    -> routing safe on this model\n");
        } else {
            println!("    -> SUPPRESSED under: {}\n", lost.join(", "));
            suppressed.push((model.clone(), lost));
        }
    }

    if !cannot_tool_call.is_empty() {
        println!("models that cannot tool-call at all: {cannot_tool_call:?}");
    }

    if suppressed.is_empty() {
        println!("no served model lost tool-calling under routing.");
    } else {
        println!("ROUTING-SUPPRESSED MODELS:");
        for (model, arms) in &suppressed {
            let content_independent = arms.contains(&"routed_names");
            println!(
                "  {model}: suppressed under {} — {}",
                arms.join(", "),
                if content_independent {
                    "content-INDEPENDENT (block presence alone; contradicts ADR-064 rule 4's mechanism)"
                } else {
                    "content-dependent (consistent with ADR-064 rule 4)"
                }
            );
        }
    }

    // The only hard assertion: at least one served model must be able to
    // tool-call unrouted, otherwise the run measured nothing and a green
    // result would be misleading.
    assert!(
        cannot_tool_call.len() < models.len(),
        "no served model could tool-call even unrouted — the matrix is uninterpretable"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The routed arms must actually differ in what they send, or the matrix
    /// compares two identical inputs and reports a difference that cannot
    /// exist. Runs without a server.
    #[test]
    fn the_four_arms_produce_distinct_prompts() {
        let baseline = system_prompt_for(Arm::Baseline);
        let stage1 = system_prompt_for(Arm::Stage1Only);
        let full = system_prompt_for(Arm::RoutedFull);
        let names = system_prompt_for(Arm::RoutedNames);

        // The unrouted arms differ from each other only in the extra Stage-1
        // generation, which is not part of the prompt.
        assert_eq!(baseline, stage1);

        assert_ne!(baseline, full, "routed_full must inject a block");
        assert_ne!(baseline, names, "routed_names must inject a block");
        assert_ne!(
            full, names,
            "the two routed arms must differ, or the content variable is not being tested"
        );
    }

    /// `routed_names` is only meaningful if it really is names-only. If the
    /// production renderer ever starts emitting instructions for an empty
    /// instruction field, this arm would silently become a duplicate of
    /// `routed_full` and the content-independence claim would be unfounded.
    #[test]
    fn the_names_only_arm_carries_no_instructions() {
        let names = system_prompt_for(Arm::RoutedNames);
        assert!(
            names.contains("Research"),
            "the candidate's name must survive: {names}"
        );
        assert!(
            !names.contains("Call search_nodes with a query"),
            "names-only arm must not carry the instruction subtree: {names}"
        );
        assert!(
            !names.contains("Purpose:"),
            "names-only arm must not carry a purpose line: {names}"
        );

        // And the full arm must carry exactly what the names arm drops.
        let full = system_prompt_for(Arm::RoutedFull);
        assert!(full.contains("Call search_nodes with a query"));
        assert!(full.contains("Purpose:"));
    }
}
