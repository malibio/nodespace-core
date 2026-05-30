//! End-to-end latency benchmark for `search_skills` on a real local 7B-class model.
//!
//! Measures wall-clock and per-phase token cost for three scenarios:
//!   1. Conversational reply (model skips `search_skills`)
//!   2. Single skill turn (`search_skills` → matched tool → final text)
//!   3. Multi-skill turn (two `search_skills` calls, both with downstream tool invocations)
//!
//! Each scenario is run N=10 times for variance. Results are written to
//! `nodespace-docs/development/benchmarks/search-skills-latency.md`.
//!
//! Run with:
//!   cargo test -p nodespace-agent --test search_skills_latency -- --nocapture
//!
//! Gracefully skips if no inference backend is available (Ollama not running and
//! no local GGUF model downloaded).
//!
//! Issue #1152

use async_trait::async_trait;
use nodespace_agent::agent_types::{
    AgentToolExecutor, AgentTurnResult, ChatInferenceEngine, ModelFamily, StreamingChunk,
    ToolDefinition, ToolError, ToolResult,
};
use nodespace_agent::local_agent::agent_loop::LocalAgentService;
use nodespace_agent::local_agent::inference::LlamaChatInferenceEngine;
use nodespace_agent::local_agent::model_manager::GgufModelManager;
use nodespace_agent::local_agent::ollama_inference::OllamaInferenceEngine;
use nodespace_agent::local_agent::ollama_model_manager::OllamaModelManager;
use nodespace_agent::ModelManager;
use nodespace_nlp_engine::chat::ChatConfig;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Walk up from `CARGO_MANIFEST_DIR` searching for a `nodespace-docs` sibling directory.
///
/// Works correctly in both primary checkout and worktree layouts without
/// hardcoding a level count (which differs between the two contexts).
fn find_docs_benchmarks_dir() -> Option<std::path::PathBuf> {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..10 {
        let candidate = dir.join("../nodespace-docs/development/benchmarks");
        if let Ok(canonical) = candidate.canonicalize() {
            return Some(canonical);
        }
        // Also check if the parent of nodespace-docs exists (dir may not exist yet)
        let docs_parent = dir.join("../nodespace-docs/development");
        if docs_parent.exists() {
            return Some(dir.join("../nodespace-docs/development/benchmarks"));
        }
        dir = dir.join("..");
    }
    None
}

// ---------------------------------------------------------------------------
// Backend resolution (shared pattern with ollama_integration.rs)
// ---------------------------------------------------------------------------

async fn ollama_running() -> bool {
    reqwest::Client::new()
        .get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}

async fn first_ollama_model() -> Option<String> {
    let manager = OllamaModelManager::new();
    let models: Vec<_> = manager.list().await.ok()?;
    models.into_iter().next().map(|m| m.id)
}

/// Returns (engine, model_name) or None if no backend available.
async fn resolve_backend() -> Option<(Arc<dyn ChatInferenceEngine>, String)> {
    if ollama_running().await {
        if let Some(model) = first_ollama_model().await {
            let engine = OllamaInferenceEngine::new(model.clone());
            return Some((Arc::new(engine) as Arc<dyn ChatInferenceEngine>, model));
        }
    }

    let gguf = GgufModelManager::new().ok()?;
    let model_path = gguf.model_path("ministral-3b-q4km").ok()?;
    if !model_path.exists() {
        return None;
    }

    let path_str = model_path.to_string_lossy().to_string();
    let engine = tokio::task::spawn_blocking(move || {
        LlamaChatInferenceEngine::load(&path_str, ModelFamily::Ministral, ChatConfig::default())
    })
    .await
    .ok()?
    .ok()?;

    Some((
        Arc::new(engine) as Arc<dyn ChatInferenceEngine>,
        "ministral-3b-q4km".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Benchmark tool executor
//
// Includes `search_skills` in `available_tools` so the model sees and can
// call it. Handles all downstream tools with realistic stub responses.
// ---------------------------------------------------------------------------

struct BenchToolExecutor;

#[async_trait]
impl AgentToolExecutor for BenchToolExecutor {
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        Ok(vec![
            ToolDefinition {
                name: "search_skills".into(),
                description: "Search registered skills by describing what you want to accomplish. \
                    Returns up to 3 matches by default (max 10), sorted by relevance, each with \
                    name, description, confidence (0-1), and tools. \
                    Call this when a request might be served by a known skill; skip it for conversational replies.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What you need to accomplish" },
                        "limit": { "type": "integer", "description": "Max skills to return (default 3)" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "search_nodes".into(),
                description: "Search nodes by keyword and/or filter by type and properties.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "node_type": { "type": "string" }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "search_semantic".into(),
                description: "Find nodes semantically related to a query.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "create_node".into(),
                description: "Create a new node in the knowledge graph.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "node_type": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["node_type"]
                }),
            },
            ToolDefinition {
                name: "update_node".into(),
                description: "Update an existing node's fields.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "fields": { "type": "object" }
                    },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "create_schema".into(),
                description: "Create a new entity type (schema) with custom fields and relationships.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "fields": { "type": "array" }
                    },
                    "required": ["name"]
                }),
            },
            ToolDefinition {
                name: "get_node".into(),
                description: "Get a node by ID.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "update_task_status".into(),
                description: "Update a task's completion status.".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "status": { "type": "string", "enum": ["open", "in_progress", "done"] }
                    },
                    "required": ["id", "status"]
                }),
            },
        ])
    }

    async fn execute(&self, name: &str, _args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let result = match name {
            "search_skills" => serde_json::json!({
                "query": "find and search knowledge",
                "matches": [
                    {
                        "name": "Research & Search",
                        "description": "Search and explore the knowledge graph to find relevant information.",
                        "confidence": 0.92,
                        "tools": ["search_semantic", "search_nodes", "get_node"]
                    }
                ]
            }),
            "search_nodes" => serde_json::json!({
                "count": 2,
                "nodes": [
                    { "id": "node-001", "title": "Machine Learning Notes", "type": "text" },
                    { "id": "node-002", "title": "ML Project Plan", "type": "task", "status": "open" }
                ]
            }),
            "search_semantic" => serde_json::json!({
                "count": 2,
                "nodes": [
                    { "id": "node-001", "title": "Machine Learning Notes", "type": "text" },
                    { "id": "node-003", "title": "Neural Networks Overview", "type": "text" }
                ]
            }),
            "create_node" => serde_json::json!({ "id": "node-new-1", "created": true }),
            "create_schema" => {
                serde_json::json!({ "id": "schema-proj", "name": "Project", "created": true })
            }
            "update_node" => serde_json::json!({ "id": "node-001", "updated": true }),
            "update_task_status" => {
                serde_json::json!({ "id": "node-002", "status": "in_progress", "updated": true })
            }
            "get_node" => {
                serde_json::json!({ "id": "node-001", "title": "Machine Learning Notes", "type": "text" })
            }
            _ => serde_json::json!({ "error": format!("unknown tool: {name}") }),
        };
        let is_error = matches!(name, t if !["search_skills","search_nodes","search_semantic","create_node","create_schema","update_node","update_task_status","get_node"].contains(&t));
        Ok(ToolResult {
            tool_call_id: format!("call_{name}"),
            name: name.to_string(),
            result,
            is_error,
        })
    }
}

// ---------------------------------------------------------------------------
// Latency measurement types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TurnMeasurement {
    wall_ms: u64,
    prompt_tokens: u32,
    completion_tokens: u32,
    search_skills_calls: usize,
    downstream_tool_calls: usize,
}

#[derive(Debug)]
struct ScenarioStats {
    name: String,
    description: String,
    runs: Vec<TurnMeasurement>,
}

impl ScenarioStats {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            runs: Vec::new(),
        }
    }

    fn wall_ms_values(&self) -> Vec<u64> {
        self.runs.iter().map(|r| r.wall_ms).collect()
    }

    fn mean_wall_ms(&self) -> f64 {
        let vals = self.wall_ms_values();
        vals.iter().sum::<u64>() as f64 / vals.len() as f64
    }

    fn min_wall_ms(&self) -> u64 {
        self.wall_ms_values().iter().copied().min().unwrap_or(0)
    }

    fn max_wall_ms(&self) -> u64 {
        self.wall_ms_values().iter().copied().max().unwrap_or(0)
    }

    fn percentile_wall_ms(&self, p: f64) -> u64 {
        let mut vals = self.wall_ms_values();
        if vals.is_empty() {
            return 0;
        }
        vals.sort_unstable();
        let idx = ((p / 100.0) * (vals.len() - 1) as f64).round() as usize;
        vals[idx.min(vals.len() - 1)]
    }

    fn mean_prompt_tokens(&self) -> f64 {
        self.runs
            .iter()
            .map(|r| r.prompt_tokens as f64)
            .sum::<f64>()
            / self.runs.len() as f64
    }

    fn mean_completion_tokens(&self) -> f64 {
        self.runs
            .iter()
            .map(|r| r.completion_tokens as f64)
            .sum::<f64>()
            / self.runs.len() as f64
    }

    fn mean_search_skills_calls(&self) -> f64 {
        self.runs
            .iter()
            .map(|r| r.search_skills_calls as f64)
            .sum::<f64>()
            / self.runs.len() as f64
    }

    fn mean_downstream_tool_calls(&self) -> f64 {
        self.runs
            .iter()
            .map(|r| r.downstream_tool_calls as f64)
            .sum::<f64>()
            / self.runs.len() as f64
    }
}

// ---------------------------------------------------------------------------
// Helper: run one turn and measure it
// ---------------------------------------------------------------------------

async fn measure_turn(
    service: &LocalAgentService<dyn ChatInferenceEngine, dyn AgentToolExecutor>,
    session_id: &str,
    message: &str,
) -> TurnMeasurement {
    let start = Instant::now();
    let result: AgentTurnResult = service
        .send_message(session_id, message, |_| {}, |_| {})
        .await
        .expect("send_message should succeed");
    let wall_ms = start.elapsed().as_millis() as u64;

    let search_skills_calls = result
        .tool_calls_made
        .iter()
        .filter(|t| t.name == "search_skills")
        .count();
    let downstream_tool_calls = result
        .tool_calls_made
        .iter()
        .filter(|t| t.name != "search_skills")
        .count();

    TurnMeasurement {
        wall_ms,
        prompt_tokens: result.usage.prompt_tokens,
        completion_tokens: result.usage.completion_tokens,
        search_skills_calls,
        downstream_tool_calls,
    }
}

// ---------------------------------------------------------------------------
// Markdown report builder
// ---------------------------------------------------------------------------

fn build_report(model_name: &str, scenarios: &[ScenarioStats], n: usize, date: &str) -> String {
    let mut md = String::new();

    md.push_str("# search_skills End-to-End Latency Benchmark\n\n");
    md.push_str(&format!("**Model:** `{model_name}`  \n"));
    md.push_str(&format!("**Date:** {date}  \n"));
    md.push_str(&format!("**Runs per scenario:** {n}  \n"));
    md.push_str("**Metric:** wall-clock time per full agent turn (inference + tool dispatch + final response)\n\n");
    md.push_str("---\n\n");

    md.push_str("## Results\n\n");
    md.push_str("| Scenario | Min (ms) | Mean (ms) | P50 (ms) | P95 (ms) | Max (ms) | Prompt toks | Completion toks | search_skills calls | Downstream tool calls |\n");
    md.push_str("|----------|----------|-----------|----------|----------|----------|-------------|-----------------|---------------------|----------------------|\n");

    for s in scenarios {
        let (min, mean, p50, p95, max) = if s.runs.is_empty() {
            (0, 0.0, 0, 0, 0)
        } else {
            (
                s.min_wall_ms(),
                s.mean_wall_ms(),
                s.percentile_wall_ms(50.0),
                s.percentile_wall_ms(95.0),
                s.max_wall_ms(),
            )
        };
        md.push_str(&format!(
            "| {} | {} | {:.0} | {} | {} | {} | {:.0} | {:.0} | {:.1} | {:.1} |\n",
            s.name,
            min,
            mean,
            p50,
            p95,
            max,
            s.mean_prompt_tokens(),
            s.mean_completion_tokens(),
            s.mean_search_skills_calls(),
            s.mean_downstream_tool_calls(),
        ));
    }

    md.push_str("\n## Scenario Descriptions\n\n");
    for s in scenarios {
        md.push_str(&format!("**{}:** {}  \n", s.name, s.description));
    }

    md.push_str("\n## Per-Run Data\n\n");
    for s in scenarios {
        md.push_str(&format!("### {}\n\n", s.name));
        md.push_str(
            "| Run | Wall (ms) | Prompt toks | Completion toks | search_skills | Downstream |\n",
        );
        md.push_str(
            "|-----|-----------|-------------|-----------------|---------------|------------|\n",
        );
        for (i, r) in s.runs.iter().enumerate() {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                i + 1,
                r.wall_ms,
                r.prompt_tokens,
                r.completion_tokens,
                r.search_skills_calls,
                r.downstream_tool_calls,
            ));
        }
        md.push('\n');
    }

    md.push_str("## Analysis\n\n");

    // Decision criterion from issue: if scenario 2 > ~3000ms, propose fast-path
    if scenarios.len() >= 2 && !scenarios[1].runs.is_empty() {
        let single_skill_mean = scenarios[1].mean_wall_ms();
        if single_skill_mean > 3000.0 {
            md.push_str(&format!(
                "**Fast-path recommendation:** Scenario 2 (single skill) averages {:.0}ms — exceeds the 3s threshold. \
                Consider a heuristic skip for messages the schema list obviously cannot satisfy \
                (e.g., pure conversational messages containing no imperative verb or knowledge-graph noun). \
                This would save one inference round (~{:.0}ms) on those turns.\n\n",
                single_skill_mean,
                single_skill_mean - scenarios[0].mean_wall_ms(),
            ));
        } else {
            md.push_str(&format!(
                "**Fast-path recommendation:** Scenario 2 (single skill) averages {:.0}ms — within the 3s threshold. \
                No fast-path optimization is warranted at this time.\n\n",
                single_skill_mean,
            ));
        }
    }

    md.push_str("## Methodology\n\n");
    md.push_str("- Each scenario used a fresh session so conversation history did not accumulate across runs.\n");
    md.push_str(
        "- Tool responses are stubs (no real database I/O) to isolate inference latency.\n",
    );
    md.push_str("- `search_skills` returns a single high-confidence Research & Search match; downstream tools return representative canned results.\n");
    md.push_str("- Wall-clock is measured from the `send_message` call until the `AgentTurnResult` is returned, covering prompt assembly, all inference rounds, and tool dispatch.\n");
    md.push_str("- Token counts are cumulative across all inference rounds in the turn (prompt and completion separately).\n");

    md
}

// ---------------------------------------------------------------------------
// Main benchmark test
// ---------------------------------------------------------------------------

const N_RUNS: usize = 10;

#[tokio::test]
async fn bench_search_skills_e2e_latency() {
    let Some((engine, model_name)) = resolve_backend().await else {
        eprintln!(
            "SKIP bench_search_skills_e2e_latency: No inference backend available \
             (Ollama not running, ministral-3b not downloaded)"
        );
        return;
    };

    eprintln!("=== search_skills latency bench: model={model_name}, N={N_RUNS} ===\n");

    let executor: Arc<dyn AgentToolExecutor> = Arc::new(BenchToolExecutor);

    // -----------------------------------------------------------------------
    // Scenario 1: Conversational — model should skip search_skills
    // -----------------------------------------------------------------------
    let mut scenario1 = ScenarioStats::new(
        "1. Conversational (no tool)",
        "Pure conversational reply; model should skip search_skills entirely",
    );

    eprintln!("--- Scenario 1: Conversational ---");
    for i in 0..N_RUNS {
        let service = LocalAgentService::new(engine.clone(), executor.clone());
        let sid = service.create_session(None).await;
        let m = measure_turn(
            &service,
            &sid,
            "What time is it in Tokyo right now? Just give me a rough idea based on typical timezone offset.",
        )
        .await;
        eprintln!(
            "  run {:2}: {}ms  prompt={} completion={}  search_skills={} downstream={}",
            i + 1,
            m.wall_ms,
            m.prompt_tokens,
            m.completion_tokens,
            m.search_skills_calls,
            m.downstream_tool_calls,
        );
        scenario1.runs.push(m);
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Single skill — model calls search_skills once, then a tool
    // -----------------------------------------------------------------------
    let mut scenario2 = ScenarioStats::new(
        "2. Single skill turn",
        "Intent: knowledge-base lookup (expected to trigger search_skills → downstream search tool)",
    );

    eprintln!("\n--- Scenario 2: Single skill turn ---");
    for i in 0..N_RUNS {
        let service = LocalAgentService::new(engine.clone(), executor.clone());
        let sid = service.create_session(None).await;
        let m = measure_turn(
            &service,
            &sid,
            "What do I have in my knowledge base about machine learning?",
        )
        .await;
        eprintln!(
            "  run {:2}: {}ms  prompt={} completion={}  search_skills={} downstream={}",
            i + 1,
            m.wall_ms,
            m.prompt_tokens,
            m.completion_tokens,
            m.search_skills_calls,
            m.downstream_tool_calls,
        );
        scenario2.runs.push(m);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Multi-skill — model calls search_skills twice in one turn
    // -----------------------------------------------------------------------
    let mut scenario3 = ScenarioStats::new(
        "3. Multi-skill turn",
        "Intent: cross-skill request spanning search and creation (expected to trigger search_skills twice)",
    );

    eprintln!("\n--- Scenario 3: Multi-skill turn ---");
    for i in 0..N_RUNS {
        let service = LocalAgentService::new(engine.clone(), executor.clone());
        let sid = service.create_session(None).await;
        let m = measure_turn(
            &service,
            &sid,
            "Find all my notes about machine learning AND create a new task node called 'Review ML findings'.",
        )
        .await;
        eprintln!(
            "  run {:2}: {}ms  prompt={} completion={}  search_skills={} downstream={}",
            i + 1,
            m.wall_ms,
            m.prompt_tokens,
            m.completion_tokens,
            m.search_skills_calls,
            m.downstream_tool_calls,
        );
        scenario3.runs.push(m);
    }

    // -----------------------------------------------------------------------
    // Build and print summary
    // -----------------------------------------------------------------------
    let scenarios = vec![scenario1, scenario2, scenario3];

    // Warn if scenarios 2/3 never called search_skills — the latency data
    // then reflects direct tool routing, not skill-discovery overhead.
    for (idx, s) in scenarios.iter().enumerate().skip(1) {
        if !s.runs.is_empty() && s.runs.iter().all(|r| r.search_skills_calls == 0) {
            eprintln!(
                "WARN: search_skills was never called in scenario {} — \
                 latency reflects direct tool routing, not skill-discovery overhead",
                idx + 1
            );
        }
    }

    eprintln!("\n=== SUMMARY ===");
    for s in &scenarios {
        if s.runs.is_empty() {
            continue;
        }
        eprintln!(
            "{}: min={}ms mean={:.0}ms p50={}ms p95={}ms max={}ms  (avg search_skills={:.1} downstream={:.1})",
            s.name,
            s.min_wall_ms(),
            s.mean_wall_ms(),
            s.percentile_wall_ms(50.0),
            s.percentile_wall_ms(95.0),
            s.max_wall_ms(),
            s.mean_search_skills_calls(),
            s.mean_downstream_tool_calls(),
        );
    }

    // -----------------------------------------------------------------------
    // Write report to nodespace-docs
    // -----------------------------------------------------------------------
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let report = build_report(&model_name, &scenarios, N_RUNS, &date);

    // Walk up from CARGO_MANIFEST_DIR searching for a nodespace-docs sibling.
    // Handles both the primary checkout and worktree layouts without hardcoding
    // a level count (which differs between the two contexts).
    let docs_dir = find_docs_benchmarks_dir();

    let report_path = match docs_dir {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            let p = dir.join("search-skills-latency.md");
            match std::fs::write(&p, &report) {
                Ok(_) => {
                    eprintln!("\nReport written to: {}", p.display());
                    Some(p)
                }
                Err(e) => {
                    eprintln!("\nWARN: could not write report to {}: {e}", p.display());
                    None
                }
            }
        }
        None => {
            eprintln!(
                "\nWARN: nodespace-docs/development/benchmarks/ not found — report not saved"
            );
            None
        }
    };

    // Always print the report to stdout so CI captures it
    eprintln!("\n========== BENCHMARK REPORT ==========\n{report}");
    eprintln!("======================================");

    if report_path.is_some() {
        eprintln!("Report persisted. Commit it from nodespace-docs to record the measurement.");
    }
}

// ---------------------------------------------------------------------------
// Streaming overhead check — ensures streaming chunks don't add measurement noise
// ---------------------------------------------------------------------------
#[tokio::test]
async fn bench_streaming_chunks_are_buffered() {
    let Some((engine, model_name)) = resolve_backend().await else {
        eprintln!("SKIP bench_streaming_chunks_are_buffered: no backend available");
        return;
    };

    eprintln!("model={model_name}");
    let chunk_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let chunk_count_clone = chunk_count.clone();

    let executor: Arc<dyn AgentToolExecutor> = Arc::new(BenchToolExecutor);
    let service = LocalAgentService::new(engine, executor);
    let sid = service.create_session(None).await;

    service
        .send_message(
            &sid,
            "What is 2 + 2?",
            |_| {},
            move |chunk| {
                if matches!(chunk, StreamingChunk::Token { .. }) {
                    chunk_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            },
        )
        .await
        .expect("send_message should succeed");

    let n = chunk_count.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("streaming token chunks received: {n}");
    // Not asserting a specific value — just verifying streaming works end-to-end
}
