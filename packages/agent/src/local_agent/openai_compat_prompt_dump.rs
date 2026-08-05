//! Dev-only prompt/response dump for the OpenAI-compatible HTTP path.
//!
//! `OpenAiCompatInferenceEngine` never touches `nlp-engine::ChatEngine` — it
//! is a pure HTTP client — so it cannot use `nlp_engine::chat::prompt_dump`,
//! which covers the native (GGUF) path's single chokepoint instead. This is
//! the equivalent capture point for the OpenAI-compat path: same env var,
//! same file, same record shape, so a session mixing both paths (or a
//! provider swap mid-conversation) produces one continuous timeline.
//!
//! When `NODESPACE_PROMPT_DUMP` is set to a file path, every call appends the
//! exact outgoing request JSON and the raw HTTP response body to that file as
//! line-delimited JSON. When the env var is unset this is a zero-cost no-op.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

const ENV_DUMP_PATH: &str = "NODESPACE_PROMPT_DUMP";

/// Shared with `nlp_engine::chat::prompt_dump`'s own counter only in name, not
/// value -- each engine implementation has its own process-wide sequence, so
/// a `seq` alone does not disambiguate which engine produced a record. Every
/// record from this module also carries `"engine": "openai_compat"`.
static SEQ: AtomicU64 = AtomicU64::new(0);

fn dump_path() -> Option<String> {
    match std::env::var(ENV_DUMP_PATH) {
        Ok(p) if !p.trim().is_empty() => Some(p),
        _ => None,
    }
}

pub fn enabled() -> bool {
    dump_path().is_some()
}

fn append(value: &serde_json::Value) {
    let Some(path) = dump_path() else { return };
    let line = match serde_json::to_string(value) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "openai_compat prompt_dump: serialize failed");
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
                tracing::debug!(error = %e, path = %path, "openai_compat prompt_dump: write failed");
            }
        }
        Err(e) => tracing::debug!(error = %e, path = %path, "openai_compat prompt_dump: open failed"),
    }
}

/// Dump the exact outgoing request (already-serialized JSON value). Returns
/// the sequence number to pass to [`dump_response`].
pub fn dump_request(base_url: &str, request_json: &serde_json::Value) -> u64 {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    if enabled() {
        append(&serde_json::json!({
            "kind": "prompt",
            "engine": "openai_compat",
            "seq": seq,
            "base_url": base_url,
            "request": request_json,
        }));
    }
    seq
}

/// Dump the raw HTTP response body for the call identified by `seq`.
pub fn dump_response(seq: u64, raw_body: &str) {
    if !enabled() {
        return;
    }
    append(&serde_json::json!({
        "kind": "response",
        "engine": "openai_compat",
        "seq": seq,
        "raw_response": raw_body,
        "raw_response_len": raw_body.len(),
    }));
}
