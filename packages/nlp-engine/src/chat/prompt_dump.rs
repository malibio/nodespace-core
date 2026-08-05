//! Dev-only prompt/response dump — the single chokepoint every native-path
//! LLM call passes through (`ChatEngine::generate_blocking`), so this covers
//! every caller (Stage 1 routing, Stage 2 ReAct turns, `resolve_query`, the
//! routing probe, and anything added later) with no per-caller wiring.
//!
//! When `NODESPACE_PROMPT_DUMP` is set to a file path, every call appends the
//! *exact* rendered prompt string handed to the tokenizer and the raw
//! (pre-normalization) model response to that file as line-delimited JSON.
//! When the env var is unset this is a zero-cost no-op.
//!
//! This is the same env var `agent::local_agent::prompt_dump` uses for its
//! own (Stage-2-only, message-list-shaped) dump — the two are complementary
//! views of the same turns, not a replacement for each other. This one is
//! the literal string sent to `llama_decode`, correlated by nothing but call
//! order; the agent-crate one is per-session, per-ReAct-iteration, and
//! structured as a message list. Reach for this one when the agent-crate
//! dump is silent for a call (Stage 1, `resolve_query`) or when you need the
//! exact rendered text rather than a reconstructed message list.
//!
//! Usage:
//! ```sh
//! NODESPACE_PROMPT_DUMP=/tmp/dump.jsonl <run the daemon>
//! # then inspect /tmp/dump.jsonl — one JSON object per line:
//! #   {"kind":"prompt","seq":0,"prompt":"<full rendered text>"}
//! #   {"kind":"response","seq":0,"raw_response":"<full>"}
//! ```

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

/// Env var holding the dump file path. Unset → no-op.
const ENV_DUMP_PATH: &str = "NODESPACE_PROMPT_DUMP";

/// Monotonic counter correlating a prompt record with its response record.
/// `InferenceRequest` carries no session/turn id at this layer (it is the
/// engine's own request type, not the agent crate's), so call order is the
/// only correlation key available here.
static SEQ: AtomicU64 = AtomicU64::new(0);

fn dump_path() -> Option<String> {
    match std::env::var(ENV_DUMP_PATH) {
        Ok(p) if !p.trim().is_empty() => Some(p),
        _ => None,
    }
}

/// True when dumping is enabled — lets callers skip building the JSON payload.
pub fn enabled() -> bool {
    dump_path().is_some()
}

fn append(value: &serde_json::Value) {
    let Some(path) = dump_path() else { return };
    let line = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "nlp-engine prompt_dump: serialize failed");
            return;
        }
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::debug!(error = %e, path = %path, "nlp-engine prompt_dump: write failed");
            }
        }
        Err(e) => tracing::debug!(error = %e, path = %path, "nlp-engine prompt_dump: open failed"),
    }
}

/// Dump the exact rendered prompt string handed to the tokenizer for this
/// call. Returns the sequence number to pass to [`dump_response`] so the two
/// records can be joined.
pub fn dump_prompt(prompt: &str) -> u64 {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    if enabled() {
        append(&serde_json::json!({
            "kind": "prompt",
            "seq": seq,
            "prompt": prompt,
            "prompt_len": prompt.len(),
        }));
    }
    seq
}

/// Dump the raw (pre-normalization) model response for the call identified
/// by `seq` (the value [`dump_prompt`] returned for this same call).
pub fn dump_response(seq: u64, raw_response: &str) {
    if !enabled() {
        return;
    }
    append(&serde_json::json!({
        "kind": "response",
        "seq": seq,
        "raw_response": raw_response,
        "raw_response_len": raw_response.len(),
    }));
}
