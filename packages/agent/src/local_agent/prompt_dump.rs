//! Dev-only prompt/response dump.
//!
//! When `NODESPACE_PROMPT_DUMP` is set to a file path, every agent turn appends
//! the *exact* assembled system prompt, the full message list sent to the model,
//! the tools offered, and the raw (pre-normalization) model response to that
//! file as line-delimited JSON. When the env var is unset this is a zero-cost
//! no-op.
//!
//! This gives a 100%-reliable view of what actually reached the model — the
//! daemon log only records the system-prompt *length* and a short preview, which
//! is insufficient for diagnosing prompt/tool-call issues. Reach for this when
//! you need to see the verbatim prompt and the raw model output.
//!
//! Usage:
//! ```sh
//! NODESPACE_PROMPT_DUMP=/tmp/dump.jsonl <run the daemon>
//! # then inspect /tmp/dump.jsonl — one JSON object per line:
//! #   {"kind":"turn","iteration":0,"system_prompt":"<full>","messages":[...],"tools":[...]}
//! #   {"kind":"response","iteration":0,"raw_response":"<full>","tool_calls":[...]}
//! ```

use std::io::Write;

/// Env var holding the dump file path. Unset → no-op.
const ENV_DUMP_PATH: &str = "NODESPACE_PROMPT_DUMP";

/// Returns the configured dump path, or `None` when dumping is disabled.
fn dump_path() -> Option<String> {
    match std::env::var(ENV_DUMP_PATH) {
        Ok(p) if !p.trim().is_empty() => Some(p),
        _ => None,
    }
}

/// Append one JSON value as a line to the dump file. Best-effort: any IO error
/// is logged at debug and otherwise ignored — dumping must never affect a turn.
fn append(value: &serde_json::Value) {
    let Some(path) = dump_path() else { return };
    let line = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "prompt_dump: serialize failed");
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
                tracing::debug!(error = %e, path = %path, "prompt_dump: write failed");
            }
        }
        Err(e) => tracing::debug!(error = %e, path = %path, "prompt_dump: open failed"),
    }
}

/// True when dumping is enabled — lets callers skip building the JSON payload.
pub fn enabled() -> bool {
    dump_path().is_some()
}

/// Dump the exact prompt sent to the model on one ReAct iteration: the system
/// prompt, the complete (untruncated) message list — which on later iterations
/// includes accumulated tool results — and the tools offered (iteration 0 only,
/// empty thereafter to avoid repeating the schema each step).
pub fn dump_turn_iteration(
    session_id: &str,
    iteration: usize,
    user_message: &str,
    system_prompt: &str,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) {
    if !enabled() {
        return;
    }
    append(&serde_json::json!({
        "kind": "turn",
        "session_id": session_id,
        "iteration": iteration,
        "user_message": user_message,
        "system_prompt": system_prompt,
        "system_prompt_len": system_prompt.len(),
        "messages": messages,
        "tools": tools,
    }));
}

/// Dump the raw (pre-normalization) model response and parsed tool calls for one
/// ReAct iteration.
pub fn dump_response(
    session_id: &str,
    iteration: usize,
    raw_response: &str,
    tool_calls: &[serde_json::Value],
) {
    if !enabled() {
        return;
    }
    append(&serde_json::json!({
        "kind": "response",
        "session_id": session_id,
        "iteration": iteration,
        "raw_response": raw_response,
        "raw_response_len": raw_response.len(),
        "tool_calls": tool_calls,
    }));
}
