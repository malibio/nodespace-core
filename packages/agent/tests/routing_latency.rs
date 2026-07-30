//! Latency cost of ADR-038's two-stage routing, measured on a real model.
//!
//! ADR-038 accepts an extra model turn as the price of bounded retrieval and a
//! judged gate, and requires that price be **measured, not assumed**: KV-cache
//! reuse "reduces but does not eliminate the multi-turn cost: Turn 2 injects
//! retrieved skills not present in Turn 1, so the cached prefix diverges at the
//! injection point."
//!
//! Both arms run the same production `LocalAgentLoop` against the same engine
//! and the same request. They differ in exactly one thing — whether the
//! executor reports `routing_available()` — so the delta is the routing
//! overhead and nothing else.
//!
//! Run with:
//!   cargo test -p nodespace-agent --test routing_latency -- --nocapture
//!
//! Skips gracefully when no inference backend is reachable.

use async_trait::async_trait;
use nodespace_agent::agent_types::{
    AgentToolExecutor, ChatInferenceEngine, ModelFamily, SkillCandidate, SkillRetrieval,
    ToolDefinition, ToolError, ToolResult,
};
use nodespace_agent::local_agent::agent_loop::LocalAgentService;
use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::openai_compat_discovery::discover_models;
use nodespace_agent::local_agent::openai_compat_inference::OpenAiCompatInferenceEngine;
use nodespace_nlp_engine::ChatConfig;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

const LOCAL_OPENAI_COMPAT_BASE_URL: &str = "http://127.0.0.1:11434/v1";

/// Repetitions per arm. Small — this is a wall-clock cost estimate to inform
/// the workstream, not a variance study.
const RUNS: usize = 3;

/// Requests that should route to a skill, phrased as a user would.
const PROMPTS: &[&str] = &[
    "add a task to review the billing docs tomorrow",
    "find my notes about payment processing",
    "start tracking the invoices my clients owe me",
];

async fn resolve_backend() -> Option<(Arc<dyn ChatInferenceEngine>, String)> {
    if let Ok(models) = discover_models(LOCAL_OPENAI_COMPAT_BASE_URL, "").await {
        if let Some(model) = models.into_iter().next() {
            let engine = OpenAiCompatInferenceEngine::new(
                LOCAL_OPENAI_COMPAT_BASE_URL.to_string(),
                String::new(),
                model.clone(),
            );
            return Some((Arc::new(engine) as Arc<dyn ChatInferenceEngine>, model));
        }
    }

    let gguf = GgufModelManager::new().ok()?;
    for id in ["gemma-4-e4b-q4km", "ministral-3b-q4km"] {
        let Ok(path) = gguf.model_path(id) else {
            continue;
        };
        if !path.exists() {
            continue;
        }
        let family = if id.starts_with("gemma") {
            ModelFamily::Gemma4
        } else {
            ModelFamily::Ministral
        };
        let path_str = path.to_string_lossy().to_string();
        let engine = tokio::task::spawn_blocking(move || {
            LlamaChatInferenceEngine::load(&path_str, family, ChatConfig::default())
        })
        .await
        .ok()?
        .ok()?;
        return Some((
            Arc::new(engine) as Arc<dyn ChatInferenceEngine>,
            id.to_string(),
        ));
    }
    None
}

/// Executor whose only variable is whether routing is on. Retrieval is served
/// from fixed candidates so the measurement isolates the extra model turn and
/// the prompt-injection divergence, not embedding-search time (which is
/// unchanged by this work — the same `find_skills` ran before, as a tool).
struct BenchExecutor {
    routing: bool,
    /// When false, candidates carry no instruction text — isolating "tool
    /// surface was scoped" from "procedural instructions were injected".
    with_instructions: bool,
}

fn stub_tools() -> Vec<ToolDefinition> {
    ["create_node", "search_nodes", "get_node", "create_schema"]
        .iter()
        .map(|n| ToolDefinition {
            name: (*n).to_string(),
            description: format!("{n} operation"),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "node_type": {"type": "string"},
                    "query": {"type": "string"},
                    "content": {"type": "string"}
                }
            }),
        })
        .collect()
}

#[async_trait]
impl AgentToolExecutor for BenchExecutor {
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        Ok(stub_tools())
    }

    async fn execute(&self, name: &str, _args: serde_json::Value) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            tool_call_id: format!("call_{name}"),
            name: name.to_string(),
            result: json!({"id": "node-1", "title": "Done"}),
            is_error: false,
        })
    }

    async fn routing_available(&self) -> bool {
        self.routing
    }

    async fn retrieve_skills(
        &self,
        _query: &str,
        limit: usize,
    ) -> Result<SkillRetrieval, ToolError> {
        let mut candidates = vec![
            SkillCandidate {
                id: "skill-node-creation".into(),
                name: "Node Creation".into(),
                description: "Create new records, entries, or instances of a type".into(),
                score: 0.72,
                tools: vec!["create_node".into(), "search_nodes".into()],
                instructions: if self.with_instructions {
                    "Call create_node with the values from the user message. \
                     Set node_type to the type id, copied exactly. Do not ask for \
                     confirmation when the type already exists."
                        .into()
                } else {
                    String::new()
                },
                schema_metadata: json!([{
                    "type_id": "task",
                    "fields": [
                        {"name": "title", "type": "text"},
                        {"name": "due_date", "type": "date"}
                    ]
                }]),
            },
            SkillCandidate {
                id: "skill-research".into(),
                name: "Research & Search".into(),
                description: "Search and explore the knowledge graph".into(),
                score: 0.55,
                tools: vec!["search_nodes".into(), "get_node".into()],
                instructions: if self.with_instructions {
                    "Call search_nodes with a query drawn from the request.".into()
                } else {
                    String::new()
                },
                schema_metadata: json!([]),
            },
        ];
        candidates.truncate(limit);
        Ok(SkillRetrieval { candidates })
    }
}

struct ArmStats {
    latencies: Vec<u128>,
    tool_rounds: Vec<usize>,
    prompt_tokens: Vec<u32>,
    completion_tokens: Vec<u32>,
}

async fn time_arm(engine: Arc<dyn ChatInferenceEngine>, routing: bool) -> ArmStats {
    let mut stats = ArmStats {
        latencies: Vec::new(),
        tool_rounds: Vec::new(),
        prompt_tokens: Vec::new(),
        completion_tokens: Vec::new(),
    };
    for prompt in PROMPTS {
        for _ in 0..RUNS {
            let service = LocalAgentService::new(
                engine.clone(),
                Arc::new(BenchExecutor {
                    routing,
                    with_instructions: true,
                }) as Arc<dyn AgentToolExecutor>,
            );
            let session = service.create_session(None, Vec::new()).await;
            let started = Instant::now();
            let result = service.send_message(&session, prompt, |_| {}, |_| {}).await;
            stats.latencies.push(started.elapsed().as_millis());
            // Rounds and tokens explain the wall-clock delta. Without them a
            // surprising number is unattributable — and an unattributable
            // benchmark number is how false conclusions get recorded.
            match result {
                Ok(r) => {
                    if r.tool_calls_made.is_empty() {
                        println!(
                            "[{}] NO TOOL CALL for {prompt:?} -> {:?}",
                            if routing { "routed" } else { "baseline" },
                            r.response.chars().take(220).collect::<String>()
                        );
                    }
                    stats.tool_rounds.push(r.tool_calls_made.len());
                    stats.prompt_tokens.push(r.usage.prompt_tokens);
                    stats.completion_tokens.push(r.usage.completion_tokens);
                }
                Err(e) => println!(
                    "[{}] ERROR for {prompt:?}: {e}",
                    if routing { "routed" } else { "baseline" }
                ),
            }
        }
    }
    stats
}

fn mean_u32(v: &[u32]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|x| *x as f64).sum::<f64>() / v.len() as f64
}

fn mean_usize(v: &[usize]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().map(|x| *x as f64).sum::<f64>() / v.len() as f64
}

fn mean(v: &[u128]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<u128>() as f64 / v.len() as f64
}

fn median(v: &[u128]) -> u128 {
    if v.is_empty() {
        return 0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    s[s.len() / 2]
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a local inference backend; run explicitly for a latency number"]
async fn two_stage_routing_latency_vs_single_turn() {
    let Some((engine, model)) = resolve_backend().await else {
        eprintln!("no inference backend available — skipping routing latency benchmark");
        return;
    };

    // Warm the engine so the first arm does not absorb load/compile cost.
    let _ = time_arm(engine.clone(), false).await;

    let baseline = time_arm(engine.clone(), false).await;
    let routed = time_arm(engine.clone(), true).await;

    let (b_mean, r_mean) = (mean(&baseline.latencies), mean(&routed.latencies));
    let overhead = r_mean - b_mean;

    println!("\n=== ADR-038 two-stage routing latency ===");
    println!("model: {model}");
    println!("prompts: {}  runs each: {RUNS}", PROMPTS.len());
    println!(
        "single-turn (baseline): mean {:.0} ms   median {} ms   \
         tool rounds {:.2}   tokens {:.0} in / {:.0} out",
        b_mean,
        median(&baseline.latencies),
        mean_usize(&baseline.tool_rounds),
        mean_u32(&baseline.prompt_tokens),
        mean_u32(&baseline.completion_tokens),
    );
    println!(
        "two-stage (routed):     mean {:.0} ms   median {} ms   \
         tool rounds {:.2}   tokens {:.0} in / {:.0} out",
        r_mean,
        median(&routed.latencies),
        mean_usize(&routed.tool_rounds),
        mean_u32(&routed.prompt_tokens),
        mean_u32(&routed.completion_tokens),
    );
    println!(
        "routing overhead:       {:+.0} ms  ({:+.1}%)",
        overhead,
        if b_mean > 0.0 {
            overhead / b_mean * 100.0
        } else {
            0.0
        }
    );
    println!("=========================================\n");
}

/// One routed turn, with the assembled Stage-2 prompt and the model's raw
/// reply printed. Diagnostic for "the routed arm makes no tool calls" — the
/// aggregate benchmark can show that it happened but not why.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "diagnostic; requires a local inference backend"]
async fn dump_one_routed_turn() {
    let Some((engine, model)) = resolve_backend().await else {
        eprintln!("no backend");
        return;
    };
    println!("model: {model}");

    for (label, routing, with_instructions) in [
        ("baseline (no routing)", false, false),
        ("routed, scoped tools only", true, false),
        ("routed + injected instructions", true, true),
    ] {
        let service = LocalAgentService::new(
            engine.clone(),
            Arc::new(BenchExecutor {
                routing,
                with_instructions,
            }) as Arc<dyn AgentToolExecutor>,
        );
        let session = service.create_session(None, Vec::new()).await;
        let r = service
            .send_message(
                &session,
                "add a task to review the billing docs tomorrow",
                |_| {},
                |_| {},
            )
            .await;
        match r {
            Ok(res) => println!(
                "\n--- {label}: tools_called={:?} ---\nreply: {}\n",
                res.tool_calls_made
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>(),
                res.response.chars().take(400).collect::<String>()
            ),
            Err(e) => println!("\n--- {label} ERROR: {e} ---\n"),
        }
    }
}
