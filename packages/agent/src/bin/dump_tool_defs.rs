//! Dump the model-facing tool definitions as JSON.
//!
//! Investigation aid for tool-surface fidelity. The golden
//! corpus hand-authors 1-2 minimal tools per case while production Stage 2
//! sends 9 with full parameter schemas, so every corpus measurement ran on a
//! prompt missing ~15KB of declarations and never exercised tool *selection*.
//! This emits the real definitions so a production-fidelity arm can be built
//! from them rather than transcribed by hand.
//!
//! Uses `model_facing_tool_definitions()` — the same accessor `agent_loop.rs`
//! and the prompt-assembly snapshot gate call — so what this prints is what
//! production scopes from, not a parallel list that can drift.
//!
//! ```text
//! cargo run --release -p nodespace-agent --bin dump_tool_defs
//! ```

use nodespace_agent::local_agent::tools::model_facing_tool_definitions;

fn main() {
    let defs: Vec<_> = model_facing_tool_definitions()
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "description": d.description,
                "parameters_schema": d.parameters_schema,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&defs).expect("tool definitions must serialize")
    );
}
