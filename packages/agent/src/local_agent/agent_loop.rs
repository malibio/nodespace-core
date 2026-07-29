//! ReAct (Reason + Act) loop and session management for the local agent.
//!
//! Orchestrates the conversation cycle: build prompts, call inference,
//! parse tool calls, execute tools, feed results back, and repeat until
//! the model produces a final response or hits iteration limits.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use opentelemetry::trace::{Span, TraceContextExt, Tracer};
use opentelemetry::KeyValue;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::agent_types::{
    AgentSession, AgentToolExecutor, AgentTurnResult, ChatInferenceEngine, ChatMessage,
    ChatModelSpec, InferenceError, InferenceRequest, InferenceUsage, LocalAgentStatus, Role,
    StreamingChunk, ToolCallRaw, ToolExecutionRecord,
};
use crate::local_agent::otlp_tracer::TRACER_NAME;
use crate::local_agent::prompt_templates;
use crate::local_agent::response_processing::{normalize_response, normalize_response_traced};
use crate::local_agent::tools::is_cross_turn_guarded_tool;
use crate::prompt_assembler::{PromptAssembler, TemplateContext, EMERGENCY_FALLBACK_PROMPT};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of tool-call iterations per turn.
const MAX_TOOL_ITERATIONS: usize = 5;

/// Consecutive tool calls with unparseable JSON arguments tolerated before the
/// turn gives up and produces a final response from what it already has.
///
/// Two, not one: a single malformed call followed by a correct retry is normal
/// recovery and observed in practice, so tripping on the first would abort turns
/// that were about to succeed.
const MAX_CONSECUTIVE_PARSE_FAILURES: usize = 2;

/// Longest canonical-args string persisted as a completed write's identity.
///
/// `create_nodes_from_markdown` carries an entire import in its arguments, so
/// storing them verbatim would write that content a second time into the chat
/// node's own message history and grow it without bound. Past this length the
/// identity is dropped rather than truncated: a truncated string could compare
/// equal to a *different* call sharing a long prefix, turning a size limit into
/// a wrong-suppression bug. Dropping it only costs a redundant re-execution.
pub const CANONICAL_ARGS_MAX_CHARS: usize = 4096;

/// Normalise a tool call's raw JSON arguments so equal calls compare equal.
///
/// Round-tripping through serde sorts nothing by itself, but it does normalise
/// whitespace and re-serialises `serde_json::Value`'s `BTreeMap`-backed objects
/// in sorted key order, so `{"b":1,"a":2}` and `{"a":2,"b":1}` produce the same
/// string. Unparseable arguments are returned unchanged: they cannot be
/// normalised, and an exact-match comparison on the raw text is still correct.
///
/// Shared by the per-turn duplicate detector and the cross-turn write guard, so
/// the normalisation rules cannot drift apart. Note the two feed it different
/// inputs: the loop-breaker canonicalises the raw emitted text, while the guard
/// canonicalises the parsed arguments to match what the write record persists.
/// That difference is deliberate — the loop-breaker only needs to recognise a
/// stuck model, whereas the guard's comparison must be exact.
pub fn canonical_args(args_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_json)
        .map(|v| v.to_string())
        .unwrap_or_else(|_| args_json.to_owned())
}

/// Build the tool result returned in place of a refused duplicate write.
///
/// Deliberately informative rather than a bare failure. A user genuinely
/// re-asking for the same node is rare but real, so the model must be able to
/// read this, tell the user the thing already exists, and name it — or, if the
/// repeat is intended, proceed deliberately with a call that differs.
fn duplicate_write_result(prior: &crate::agent_types::PriorWrite) -> serde_json::Value {
    let mut what = prior.tool.clone();
    if let Some(ref s) = prior.summary {
        what.push_str(&format!(" \"{s}\""));
    }
    serde_json::json!({
        "skipped": "duplicate_write",
        "id": prior.node_id,
        "message": format!(
            "Not executed: an identical {what} call already completed earlier in this \
             conversation, so this would create a second copy. The result of that write \
             still stands{}. Tell the user it already exists rather than repeating the \
             write. If they explicitly want another, separate copy, say so and issue a \
             call that differs from the original.",
            match prior.node_id {
                Some(ref id) => format!(" ({id})"),
                None => String::new(),
            }
        ),
    })
}

/// Maximum tokens any single inference round may generate.
///
/// Small local models (e.g. Gemma-4-E4B) occasionally open an empty
/// assistant turn with nothing to say and then run away to the model's
/// hard ceiling, producing a multi-minute hang that surfaces to the user
/// as "no reply". A tight cap bounds any such runaway to a couple of
/// seconds while still leaving ample room for a normal chat reply or a
/// tool-call argument blob.
///
/// Maximum tokens for the final text-only response (no tools). Keeps
/// user-facing replies concise. Tool-calling iterations use `max_tokens: None`
/// so argument JSON is never truncated mid-field.
const MAX_RESPONSE_TOKENS: u32 = 2_048;

/// Total token budget for the context window.
const TOTAL_TOKEN_BUDGET: u32 = 32_000;

/// Tokens reserved for the system prompt and tool definitions.
const SYSTEM_PROMPT_BUDGET: u32 = 4_000;

/// Tokens available for conversation history.
const HISTORY_TOKEN_BUDGET: u32 = TOTAL_TOKEN_BUDGET - SYSTEM_PROMPT_BUDGET;

/// Last-resort user-facing message when a turn produces nothing usable — no
/// tool executions to summarize and no (non-empty, non-pseudo-code) text from
/// the model. Guarantees the chat UI always shows an honest failure notice
/// rather than an empty bubble.
const EMPTY_RESPONSE_FALLBACK: &str =
    "⚠️ I wasn't able to produce a response for that. Please try again.";

/// Shared confirmation request used when a guard suppresses a response the model
/// should not have produced — a fabricated action claim, or a tool call the
/// model narrated as text instead of invoking. Kept in one place so the two
/// guards stay in sync.
const CONFIRMATION_REQUEST: &str =
    "I'd like to help with that. Could you confirm what you'd like me to do? I want to make sure I take the right action.";

fn session_prompt_override(session: &AgentSession) -> Option<&str> {
    session.system_prompt_override.as_deref()
}

// ---------------------------------------------------------------------------
// Tool name humanization
// ---------------------------------------------------------------------------

/// Convert an internal tool identifier into user-facing prose.
///
/// Used by fallback responses that surface tool activity to the chat UI when
/// the model fails to produce its own text. The display label is derived from
/// the tool registry ([`crate::local_agent::tools::Tool`]); names not in the
/// registry fall back to a generic phrase so a stray tool name never reaches
/// the user.
fn humanize_tool_name(tool_name: &str) -> &'static str {
    crate::local_agent::tools::Tool::from_name(tool_name)
        .map(|t| t.humanized())
        .unwrap_or("the requested action")
}

/// Detect whether a response text contains claims of completed actions.
///
/// Used by the anti-fabrication guard to catch responses where the model
/// narrates actions it never took via tool calls. Checks for common patterns
/// like "I created", "I updated", "I found", etc. The check is deliberately
/// conservative — it only fires on strong first-person action phrases to avoid
/// false positives on legitimate conversational text.
fn contains_action_claim(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // First-person past/present action verbs that imply the model performed a
    // side-effecting operation. "I can" / "I would" / "I could" intentionally
    // excluded — those are capability expressions, not claims of past action.
    const ACTION_PHRASES: &[&str] = &[
        "i created",
        "i've created",
        "i updated",
        "i've updated",
        "i found",
        "i've found",
        "i added",
        "i've added",
        "i deleted",
        "i've deleted",
        "i removed",
        "i've removed",
        "i marked",
        "i've marked",
        "i set ",
        "i've set",
        "i made ",
        "i've made",
        "i completed",
        "i've completed",
        "successfully created",
        "successfully updated",
        "successfully added",
        "successfully deleted",
        "successfully removed",
        "has been created",
        "has been updated",
        "has been added",
        "has been deleted",
    ];
    ACTION_PHRASES.iter().any(|p| lower.contains(p))
}

/// Synthesize a user-facing bullet summary from the tool executions of a turn.
///
/// Last-resort fallback for when the model produced no usable final text
/// (empty, or a leaked tool call). Repeated calls to the same tool collapse to
/// one bullet with a retry count so the diagnostic signal — the agent looped on
/// the same operation — survives. Executions that errored are labelled
/// "failed" rather than "completed" so a turn that only ever failed is never
/// summarized as a success (e.g. the looping-`search_nodes` case).
fn summarize_executions(executions: &[ToolExecutionRecord]) -> String {
    // (label, total, errored) preserving first-seen order.
    let mut counts: Vec<(&'static str, usize, usize)> = Vec::new();
    for t in executions {
        let label = humanize_tool_name(&t.name);
        if let Some(entry) = counts.iter_mut().find(|(l, _, _)| *l == label) {
            entry.1 += 1;
            if t.is_error {
                entry.2 += 1;
            }
        } else {
            counts.push((label, 1, usize::from(t.is_error)));
        }
    }
    counts
        .into_iter()
        .map(|(label, count, errored)| {
            // "failed" if every call to this tool errored; otherwise "completed".
            let verb = if errored == count {
                "failed"
            } else {
                "completed"
            };
            if count > 1 {
                format!("• {label} {verb} ({count}×)")
            } else {
                format!("• {label} {verb}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Detect a tool call that the model emitted as plain text instead of invoking.
///
/// Some locally-hosted models (e.g. `mistral:7b` via Ollama) don't reliably use
/// the structured `tool_calls` response field — they instead print the call as
/// prose, e.g. `search_nodes(node_type='task', ...)`. No tool ever executes,
/// yet the pseudo-code is persisted verbatim as the assistant's answer, so the
/// user sees a raw code snippet with no indication anything went wrong.
///
/// This is deliberately narrow: it only matches a registered tool name
/// immediately followed by `(` (allowing whitespace between). Matching a general
/// `snake_case(` shape would false-positive on legitimate prose that references
/// functions. Tool names are taken from the registry ([`crate::local_agent::tools::Tool::ALL`])
/// so the detector stays in sync as tools are added or removed.
fn looks_like_narrated_tool_call(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    crate::local_agent::tools::Tool::ALL.iter().any(|tool| {
        let name = tool.name();
        // Find each occurrence of the tool name and check the next
        // non-whitespace character is an opening paren.
        lower.match_indices(name).any(|(idx, _)| {
            let after = &lower[idx + name.len()..];
            after.trim_start().starts_with('(')
        })
    })
}

// ---------------------------------------------------------------------------
// LocalAgentLoop
// ---------------------------------------------------------------------------

/// Core ReAct loop implementation.
///
/// Stateless: operates on a provided session and delegates to the injected
/// inference engine and tool executor. The caller (`LocalAgentService`)
/// manages session state and persistence.
pub struct LocalAgentLoop<E: ChatInferenceEngine + ?Sized, T: AgentToolExecutor + ?Sized> {
    engine: Arc<E>,
    tool_executor: Arc<T>,
    prompt_assembler: Option<Arc<PromptAssembler>>,
}

impl<E: ChatInferenceEngine + ?Sized, T: AgentToolExecutor + ?Sized> LocalAgentLoop<E, T> {
    pub fn new(engine: Arc<E>, tool_executor: Arc<T>) -> Self {
        Self {
            engine,
            tool_executor,
            prompt_assembler: None,
        }
    }

    /// The inference engine backing this loop, so callers that need to report
    /// the loaded model's real geometry (id, granted context window) can ask it
    /// without threading a second handle through every construction site.
    pub fn engine(&self) -> &Arc<E> {
        &self.engine
    }

    pub fn with_prompt_assembler(mut self, assembler: Arc<PromptAssembler>) -> Self {
        self.prompt_assembler = Some(assembler);
        self
    }

    /// Execute one full agent turn: inference + tool loop.
    ///
    /// Appends the user message to the session, builds the prompt, runs
    /// inference (potentially multiple rounds of tool calls), and returns
    /// the final response. The session is mutated in place with all
    /// intermediate messages.
    ///
    /// `on_status` is called for each status transition.
    /// `on_chunk` forwards streaming tokens to the caller.
    /// `cancel` can be used to abort mid-generation.
    pub async fn run_turn(
        &self,
        session: &mut AgentSession,
        user_message: &str,
        on_status: impl Fn(LocalAgentStatus) + Send + Sync + 'static,
        on_chunk: impl Fn(StreamingChunk) + Send + Sync + 'static,
        cancel: CancellationToken,
    ) -> Result<AgentTurnResult, InferenceError> {
        // Wrap on_chunk in Arc so it can be cloned into each iteration's callback
        let on_chunk = Arc::new(on_chunk);

        // Root OTLP span for this full agent turn. No-op when tracing is disabled.
        let tracer = opentelemetry::global::tracer(TRACER_NAME);
        let mut turn_span = tracer.start("agent_turn");
        turn_span.set_attribute(KeyValue::new("session_id", session.id.clone()));
        turn_span.set_attribute(KeyValue::new(
            "model_id",
            session.model_id.clone().unwrap_or_default(),
        ));
        turn_span.set_attribute(KeyValue::new("user_message", user_message.to_string()));
        let turn_cx = opentelemetry::Context::current_with_span(turn_span);

        // Append user message
        session
            .messages
            .push(ChatMessage::text(Role::User, user_message.to_string()));

        // Full tool list including search_skills — the model calls search_skills
        // itself to discover capabilities (ADR-036 pull model). No pre-filtering:
        // the model receives all tools and judges which skill applies after retrieval.
        // Degraded path: if embedding service is unavailable, search_skills returns
        // an error result and the model falls through to general tools — acceptable.
        let tools = self
            .tool_executor
            .available_tools()
            .await
            .unwrap_or_default();

        let dynamic_ctx = session.dynamic_context.as_deref().unwrap_or("");
        let model_name = session.model_id.as_deref().unwrap_or("unknown");
        let template_ctx = TemplateContext {
            current_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            model_name: model_name.to_string(),
            workspace_context: dynamic_ctx.to_string(),
        };

        // Build the system prompt: test override > graph assembler > emergency.
        // `session_prompt_override` returns `None` in production builds (see
        // the `testing` feature on the agent crate). Production inference always
        // wires a `PromptAssembler`; the emergency arm only fires for the
        // daemon's no-op/idle service, which never reaches live inference.
        let system_content = if let Some(override_prompt) = session_prompt_override(session) {
            override_prompt.to_string()
        } else if let Some(ref assembler) = self.prompt_assembler {
            assembler
                .assemble(&template_ctx, tools.clone())
                .await
                .system_prompt
        } else {
            EMERGENCY_FALLBACK_PROMPT.to_string()
        };

        // prompt_assembly child span: records full assembled system prompt and tools offered.
        {
            let mut span = tracer.start_with_context("prompt_assembly", &turn_cx);
            span.set_attribute(KeyValue::new("system_prompt", system_content.clone()));
            span.set_attribute(KeyValue::new("workspace_context", dynamic_ctx.to_string()));
            let tools_json: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| serde_json::json!({"name": t.name, "description": t.description}))
                .collect();
            span.set_attribute(KeyValue::new(
                "tools_offered",
                serde_json::to_string(&tools_json).unwrap_or_default(),
            ));
        }

        let effective_max_iterations = MAX_TOOL_ITERATIONS;

        tracing::info!(
            tools_count = tools.len(),
            tool_names = %tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "),
            system_prompt_len = system_content.len(),
            "Agent turn: system prompt and tools prepared"
        );

        let mut all_tool_executions: Vec<ToolExecutionRecord> = Vec::new();
        // Reasoning (chain-of-thought) accumulated across every ReAct iteration of
        // this turn, joined into a single section on the final assistant message.
        let mut accumulated_reasoning = String::new();
        // Tracks whether any iteration in this turn has produced at least one
        // tool call. Used by the anti-fabrication guard: once the model has done
        // real work, a fabricated final summary is a different (harder) problem
        // than a pure hallucination with zero tool calls.
        let mut any_real_tool_calls = false;
        // Duplicate tool-call detector: (tool_name, canonical_args_json) pairs
        // seen so far this turn. When the model issues an identical call again
        // (same tool + identical args after round-tripping through serde to
        // normalise key order), we break the loop immediately rather than burning
        // an iteration executing the same query and getting the same result.
        let mut seen_calls: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        // Consecutive tool calls whose arguments would not parse as JSON. The
        // duplicate detector above cannot catch this class: each attempt is
        // malformed *differently*, so no two canonical arg strings match and the
        // model can burn every iteration without executing a single tool. Reset
        // on any successful parse, so only an unbroken run trips it.
        let mut consecutive_parse_failures = 0usize;
        let mut total_usage = InferenceUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };

        // ReAct loop: iterate up to effective_max_iterations (skill-specific or global fallback)
        for iteration in 0..effective_max_iterations {
            if cancel.is_cancelled() {
                return Err(InferenceError::Engine("cancelled".into()));
            }

            // Maybe summarize history if over budget
            self.maybe_summarize_history(session, &system_content)
                .await?;

            // Build message list: system + history
            let mut messages = vec![ChatMessage::text(Role::System, system_content.clone())];
            messages.extend(session.messages.clone());

            // react_iteration_N child span — records full messages sent, raw response, tool calls.
            let iter_span_name = format!("react_iteration_{iteration}");
            let mut iter_span = tracer.start_with_context(iter_span_name, &turn_cx);
            iter_span.set_attribute(KeyValue::new("iteration", iteration as i64));
            // Serialize messages sent (truncated per message to avoid huge spans).
            let messages_json: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| {
                    let role = format!("{:?}", m.role).to_lowercase();
                    let preview: String = m.content.chars().take(2000).collect();
                    serde_json::json!({"role": role, "content": preview})
                })
                .collect();
            iter_span.set_attribute(KeyValue::new(
                "messages_sent",
                serde_json::to_string(&messages_json).unwrap_or_default(),
            ));

            // Dev-only file dump (NODESPACE_PROMPT_DUMP): record the EXACT prompt
            // sent to the model on THIS iteration — full system prompt + the
            // complete (untruncated) message list, which on later iterations
            // includes the accumulated tool results fed back in. Tools are dumped
            // once (iteration 0). Reliable local-disk view of exactly what
            // reaches the model at every step of the loop.
            if crate::local_agent::prompt_dump::enabled() {
                let full_messages: Vec<serde_json::Value> = messages
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": format!("{:?}", m.role).to_lowercase(),
                            "content": m.content,
                        })
                    })
                    .collect();
                let tools_full: Vec<serde_json::Value> = if iteration == 0 {
                    tools
                        .iter()
                        .map(|t| {
                            serde_json::json!({
                                "name": t.name,
                                "description": t.description,
                                "parameters": t.parameters_schema,
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                crate::local_agent::prompt_dump::dump_turn_iteration(
                    &session.id,
                    iteration,
                    user_message,
                    &system_content,
                    &full_messages,
                    &tools_full,
                );
            }

            // Status: Thinking
            on_status(LocalAgentStatus::Thinking);
            session.status = LocalAgentStatus::Thinking;

            // Collect chunks to parse tool calls from the response.
            // Uses std::sync::Mutex (not tokio) because the callback runs on
            // a blocking thread inside spawn_blocking.
            let collected_chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
                Arc::new(std::sync::Mutex::new(Vec::new()));
            let collected_for_cb = Arc::clone(&collected_chunks);
            let on_chunk_clone = Arc::clone(&on_chunk);

            // Wrap on_chunk so we can also collect
            let chunk_callback: Box<dyn Fn(StreamingChunk) + Send> =
                Box::new(move |chunk: StreamingChunk| {
                    // Forward to caller
                    on_chunk_clone(chunk.clone());
                    // Collect for parsing
                    if let Ok(mut guard) = collected_for_cb.lock() {
                        guard.push(chunk);
                    }
                });

            let request = InferenceRequest {
                messages,
                tools: Some(tools.clone()),
                temperature: Some(0.1),
                max_tokens: None, // No cap on tool-calling iterations — truncated args produce invalid JSON
            };

            // Run inference
            let usage = self.engine.generate(request, chunk_callback).await?;

            total_usage.prompt_tokens += usage.prompt_tokens;
            total_usage.completion_tokens += usage.completion_tokens;

            // Parse collected chunks into text + tool calls.
            // Poison recovery is safe here: chunks are append-only, so partial
            // data after a panic is acceptable (we just get fewer chunks).
            let chunks: Vec<StreamingChunk> = {
                let guard = collected_chunks.lock().unwrap_or_else(|p| p.into_inner());
                guard.clone()
            };
            let (response_text, iteration_reasoning, tool_calls) = Self::parse_chunks(&chunks);

            iter_span.set_attribute(KeyValue::new("raw_response", response_text.clone()));
            let tool_calls_json: Vec<serde_json::Value> = tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "name": tc.function_name,
                        "arguments": tc.arguments_json,
                    })
                })
                .collect();
            iter_span.set_attribute(KeyValue::new(
                "tool_calls_parsed",
                serde_json::to_string(&tool_calls_json).unwrap_or_default(),
            ));

            // Dev-only file dump (NODESPACE_PROMPT_DUMP): the RAW model response
            // (pre-normalization) and parsed tool calls for THIS iteration. One
            // record per ReAct iteration, so multi-iteration tool loops are fully
            // captured in order.
            crate::local_agent::prompt_dump::dump_response(
                &session.id,
                iteration,
                &response_text,
                &tool_calls_json,
            );

            iter_span.set_attribute(KeyValue::new("prompt_tokens", usage.prompt_tokens as i64));
            iter_span.set_attribute(KeyValue::new(
                "completion_tokens",
                usage.completion_tokens as i64,
            ));

            // Accumulate this iteration's reasoning into the turn-wide section,
            // separating iterations with a blank line.
            if !iteration_reasoning.trim().is_empty() {
                if !accumulated_reasoning.is_empty() {
                    accumulated_reasoning.push_str("\n\n");
                }
                accumulated_reasoning.push_str(iteration_reasoning.trim());
            }

            tracing::info!(
                iteration,
                tool_calls = tool_calls.len(),
                response_len = response_text.len(),
                response_preview = %response_text.chars().take(200).collect::<String>(),
                "Agent loop: inference round completed"
            );

            if tool_calls.is_empty() {
                // No tool calls — final response
                on_status(LocalAgentStatus::Streaming);
                session.status = LocalAgentStatus::Streaming;

                // response_processing child span: records raw input, normalized output,
                // and which strippers fired. Uses normalize_response_traced so we get
                // the stripper list without running normalization twice.
                let raw_for_span = response_text.clone();
                let (normalized, strippers_fired) = normalize_response_traced(&response_text);
                {
                    let mut span = tracer.start_with_context("response_processing", &turn_cx);
                    span.set_attribute(KeyValue::new("raw_input", raw_for_span));
                    span.set_attribute(KeyValue::new("normalized_output", normalized.clone()));
                    span.set_attribute(KeyValue::new(
                        "strippers_fired",
                        strippers_fired.join(", "),
                    ));
                }

                // Anti-fabrication guard: if the model claims an action it never
                // executed (no tool calls in any iteration this turn), suppress the
                // fabricated claim and ask the user to confirm instead.
                // This catches models (e.g. 12B@8K) that narrate fictional successes
                // like "I created invoice 104" without calling any tool.
                // Uses any_real_tool_calls rather than all_tool_executions.is_empty()
                // so that the guard correctly fires in the final iteration even when
                // zero-tool-call turns precede it in the same ReAct loop.
                let normalized = if normalized.is_empty() {
                    normalized
                } else if !any_real_tool_calls && contains_action_claim(&normalized) {
                    tracing::warn!(
                        session_id = %session.id,
                        model = %session.model_id.as_deref().unwrap_or("unknown"),
                        iteration = iteration,
                        response_preview = %normalized.chars().take(120).collect::<String>(),
                        "Anti-fabrication: model claimed action with zero tool calls — converting to confirmation request"
                    );
                    CONFIRMATION_REQUEST.to_string()
                } else {
                    normalized
                };

                // Narrated-tool-call guard: some local models emit a tool call as
                // plain text (e.g. `search_nodes(...)`) instead of using the
                // structured tool_calls field, so nothing executes and the raw
                // pseudo-code would be persisted as the answer. Detect that shape
                // and replace it with a confirmation request rather than leaking
                // internal call syntax to the user. Fires independently of
                // any_real_tool_calls: even after a real call earlier in the turn,
                // a leaked pseudo-call in the final text is still not a valid answer.
                let normalized = if !normalized.is_empty()
                    && looks_like_narrated_tool_call(&normalized)
                {
                    tracing::warn!(
                        session_id = %session.id,
                        model = %session.model_id.as_deref().unwrap_or("unknown"),
                        iteration = iteration,
                        response_preview = %normalized.chars().take(120).collect::<String>(),
                        "Narrated tool call: model printed a tool call as text instead of invoking it — converting to confirmation request"
                    );
                    CONFIRMATION_REQUEST.to_string()
                } else {
                    normalized
                };

                // Tool-failure surfacing: if any tool failed and the model's response
                // doesn't acknowledge the error, replace the response with an honest
                // error message. Appending to a success claim would produce contradictory
                // output ("The node was updated. ⚠️ Note: ... encountered an error.").
                let failed_tools: Vec<&str> = all_tool_executions
                    .iter()
                    .filter(|r| r.is_error)
                    .map(|r| r.name.as_str())
                    .collect();
                let normalized = if !failed_tools.is_empty() && !normalized.is_empty() {
                    let lower = normalized.to_ascii_lowercase();
                    let mentions_error = lower.contains("error")
                        || lower.contains("fail")
                        || lower.contains("couldn't")
                        || lower.contains("could not")
                        || lower.contains("unable");
                    if !mentions_error {
                        let unique_labels: Vec<String> = {
                            let mut seen = std::collections::HashSet::new();
                            failed_tools
                                .iter()
                                .map(|n| humanize_tool_name(n).to_string())
                                .filter(|l| seen.insert(l.clone()))
                                .collect()
                        };
                        tracing::warn!(
                            session_id = %session.id,
                            failed_tools = %failed_tools.join(", "),
                            "Tool failures not surfaced in model response — replacing with error message"
                        );
                        format!(
                            "⚠️ {} encountered an error. Please try again or check the details and retry.",
                            unique_labels.join(", ")
                        )
                    } else {
                        normalized
                    }
                } else {
                    normalized
                };

                // If the model produced no text after tool calls, synthesize a
                // brief confirmation so the UI always shows something meaningful.
                let final_response = if normalized.is_empty() && !all_tool_executions.is_empty() {
                    let tool_name = &all_tool_executions.last().unwrap().name;
                    format!(
                        "Done — {} completed successfully.",
                        humanize_tool_name(tool_name)
                    )
                } else if normalized.is_empty() {
                    // Model returned nothing at all — no tools, no text. This is
                    // an inference bug, not a routing decision: the model should
                    // either call a tool or produce text. Surface as an error so
                    // it shows up in logs/metrics rather than being masked by a
                    // canned UX string. Structured fields below let the
                    // production dashboards group these by model and surface
                    // session/iteration for replay.
                    tracing::error!(
                        session_id = %session.id,
                        model = %session.model_id.as_deref().unwrap_or("unknown"),
                        iteration = iteration,
                        prompt_tokens = total_usage.prompt_tokens,
                        completion_tokens = total_usage.completion_tokens,
                        user_message_preview = %session
                            .messages
                            .iter()
                            .rev()
                            .find(|m| matches!(m.role, Role::User))
                            .map(|m| m.content.chars().take(80).collect::<String>())
                            .unwrap_or_default(),
                        "Agent returned empty response with no tool calls"
                    );
                    return Err(InferenceError::Engine(
                        "model produced empty response with no tool calls".into(),
                    ));
                } else {
                    normalized
                };

                // Collapse the accumulated reasoning into the final option.
                let reasoning = (!accumulated_reasoning.trim().is_empty())
                    .then(|| accumulated_reasoning.trim().to_string());

                // Append assistant response to history, carrying the reasoning so
                // it persists and round-trips on reload.
                let mut assistant_msg = ChatMessage::text(Role::Assistant, final_response.clone());
                assistant_msg.reasoning = reasoning.clone();
                session.messages.push(assistant_msg);

                on_status(LocalAgentStatus::Idle);
                session.status = LocalAgentStatus::Idle;

                return Ok(AgentTurnResult {
                    response: final_response,
                    reasoning,
                    tool_calls_made: all_tool_executions,
                    usage: total_usage,
                });
            }

            // Append the assistant message that issued these tool calls, carrying
            // the structured tool_calls so the next re-prompt produces a
            // well-formed turn (assistant tool_calls → matching tool results).
            //
            // Drop any prose the model emitted alongside the tool call. Small
            // models (Gemma 4) tend to narrate ("Let me search…") in the same
            // turn as a tool call; when re-prompted, the chat template collapses
            // that prose + the tool_call + the following tool result into a
            // single malformed assistant turn, then opens an empty turn the
            // model fills by running away to the token cap. Persisting only the
            // tool_calls keeps the turn structurally clean: the assistant turn
            // is purely the call, and the answer is produced in a later turn
            // once the tool results are in hand.
            session
                .messages
                .push(ChatMessage::assistant_with_tool_calls(
                    String::new(),
                    tool_calls.clone(),
                ));

            // Record that at least one real tool call has been made this turn.
            any_real_tool_calls = true;

            // Duplicate-call guard: if every tool call in this iteration is
            // identical to one already executed this turn, the model is stuck in
            // a loop. Break out now so the final-inference path can produce a
            // response from the results already in session history, rather than
            // burning iterations re-executing the same query.
            //
            // Args are round-tripped through serde to normalise JSON key order
            // so {"b":1,"a":2} and {"a":2,"b":1} are treated as the same call.
            let all_duplicate = tool_calls.iter().all(|tc| {
                seen_calls.contains(&(tc.function_name.clone(), canonical_args(&tc.arguments_json)))
            });
            if all_duplicate {
                tracing::warn!(
                    session_id = %session.id,
                    iteration = iteration,
                    tool_names = %tool_calls.iter().map(|tc| tc.function_name.as_str()).collect::<Vec<_>>().join(", "),
                    "Duplicate tool-call loop detected — breaking to force final response"
                );
                // Remove the assistant tool-call message we just pushed (it has no
                // matching tool results and would produce a malformed turn).
                session.messages.pop();
                break;
            }
            // Register each call in the seen set so later iterations can detect repeats.
            for tc in &tool_calls {
                seen_calls.insert((tc.function_name.clone(), canonical_args(&tc.arguments_json)));
            }

            // Execute each tool call
            let mut tool_results_for_span: Vec<serde_json::Value> = Vec::new();
            for tc in &tool_calls {
                if cancel.is_cancelled() {
                    return Err(InferenceError::Engine("cancelled".into()));
                }

                on_status(LocalAgentStatus::ToolExecution {
                    tool_name: tc.function_name.clone(),
                });
                session.status = LocalAgentStatus::ToolExecution {
                    tool_name: tc.function_name.clone(),
                };

                let start = Instant::now();

                // Unparseable arguments must be reported as such. Substituting an
                // empty object here (the previous behaviour) sends the tool a
                // payload the model never wrote, so the failure surfaces as a
                // missing required field — describing the substitute rather than
                // the malformed JSON that actually caused it, and pointing the
                // model's repair attempt at the wrong problem.
                let parsed_args = if tc.arguments_json.trim().is_empty() {
                    // No arguments emitted at all: an empty object is the faithful
                    // reading, and the tool's own required-field error is correct.
                    Ok(serde_json::json!({}))
                } else {
                    serde_json::from_str::<serde_json::Value>(&tc.arguments_json)
                };

                let (args, tool_result) = match parsed_args {
                    Ok(args) => {
                        consecutive_parse_failures = 0;
                        // Cross-turn duplicate guard. The per-turn `seen_calls`
                        // set above cannot see this: the session is rebuilt from
                        // persisted messages every turn, so a repeat of a write
                        // that landed in an *earlier* turn arrives here looking
                        // brand new. Comparison is on the same canonical form
                        // `seen_calls` uses, against writes replayed from the
                        // conversation record.
                        let already_written = if is_cross_turn_guarded_tool(&tc.function_name) {
                            // Canonicalised from the *parsed* arguments, not the
                            // raw text, so this derives from exactly what the
                            // record side stores (`ToolExecutionRecord.args`).
                            // Canonicalising the raw string instead would differ
                            // wherever the two are not textually identical — most
                            // concretely when empty arguments are read as `{}`
                            // above, which yields "" here against a stored "{}"
                            // and a write that could never match itself.
                            let incoming = canonical_args(&args.to_string());
                            session.prior_writes.iter().find(|w| {
                                w.tool == tc.function_name && w.canonical_args == incoming
                            })
                        } else {
                            None
                        };
                        if let Some(prior) = already_written {
                            tracing::warn!(
                                session_id = %session.id,
                                tool = %tc.function_name,
                                iteration = iteration,
                                "Cross-turn duplicate write refused — identical call already completed in an earlier turn"
                            );
                            // An informative result, not a silent block. A user
                            // genuinely re-asking for the same node is rare but
                            // real, so the model needs to be able to tell them it
                            // already exists — or, if the repeat is deliberate,
                            // proceed by varying the call.
                            (
                                args,
                                Some(Ok(crate::agent_types::ToolResult {
                                    tool_call_id: tc.id.clone(),
                                    name: tc.function_name.clone(),
                                    result: duplicate_write_result(prior),
                                    // Not an error: nothing went wrong, and the
                                    // requested state already holds. Flagging it
                                    // as a failure would invite a repair retry —
                                    // the exact loop this guard exists to stop.
                                    is_error: false,
                                })),
                            )
                        } else {
                            let result = self
                                .tool_executor
                                .execute(&tc.function_name, args.clone())
                                .await;
                            (args, Some(result))
                        }
                    }
                    Err(parse_err) => {
                        consecutive_parse_failures += 1;
                        tracing::warn!(
                            session_id = %session.id,
                            tool = %tc.function_name,
                            iteration = iteration,
                            error = %parse_err,
                            consecutive_parse_failures,
                            args_preview = %tc.arguments_json.chars().take(300).collect::<String>(),
                            "Model emitted unparseable tool arguments — reporting to model instead of substituting an empty object"
                        );
                        (serde_json::json!({}), None)
                    }
                };

                let duration_ms = start.elapsed().as_millis() as u64;

                let (result_value, is_error) = match tool_result {
                    Some(Ok(tr)) => (tr.result, tr.is_error),
                    Some(Err(e)) => (serde_json::json!({"error": e.to_string()}), true),
                    None => (
                        serde_json::json!({
                            "error": format!(
                                "The arguments for {} were not valid JSON, so the call could not \
                                 be made. Re-send the call with the same intent and syntactically \
                                 valid JSON arguments.",
                                tc.function_name
                            )
                        }),
                        true,
                    ),
                };

                // Field count from the tool RESULT, not its arguments: the result is the
                // executor's report of what it persisted, while args are only the model's
                // report of what it asked for. Logged as a bare integer because both
                // previews truncate at 300 chars — a realistic create_schema payload
                // exceeds that, so a parser reading them would fail on exactly the
                // well-formed calls it is meant to pass.
                let result_field_count = result_value
                    .get("fields")
                    .and_then(|f| f.as_array())
                    .map(|a| a.len());

                tracing::info!(
                    tool = %tc.function_name,
                    is_error,
                    duration_ms,
                    result_field_count,
                    args_preview = %args.to_string().chars().take(300).collect::<String>(),
                    result_preview = %result_value.to_string().chars().take(300).collect::<String>(),
                    "Tool executed"
                );

                tool_results_for_span.push(serde_json::json!({
                    "tool": tc.function_name,
                    "is_error": is_error,
                    "duration_ms": duration_ms,
                    "result": result_value.to_string().chars().take(2000).collect::<String>(),
                }));

                let record = ToolExecutionRecord {
                    tool_call_id: tc.id.clone(),
                    name: tc.function_name.clone(),
                    args,
                    result: result_value.clone(),
                    is_error,
                    duration_ms,
                };

                session.tool_executions.push(record.clone());
                all_tool_executions.push(record);

                // Append tool result to history
                let tool_msg = prompt_templates::format_tool_result(
                    &tc.function_name,
                    &result_value,
                    is_error,
                );
                session.messages.push(ChatMessage::tool_result(
                    tool_msg,
                    tc.id.clone(),
                    tc.function_name.clone(),
                ));
            }

            // A model that cannot emit valid JSON is not making progress, and each
            // attempt is malformed differently so the duplicate guard never fires.
            // Break after a short unbroken run — the error results are already in
            // history, so the final-inference path can still answer from them.
            if consecutive_parse_failures >= MAX_CONSECUTIVE_PARSE_FAILURES {
                tracing::warn!(
                    session_id = %session.id,
                    iteration = iteration,
                    consecutive_parse_failures,
                    "Model repeatedly emitted unparseable tool arguments — breaking to force final response"
                );
                break;
            }

            // Set tool results on the iteration span and close it before the next iteration.
            iter_span.set_attribute(KeyValue::new(
                "tool_results",
                serde_json::to_string(&tool_results_for_span).unwrap_or_default(),
            ));

            // If this was the last allowed iteration, do one final inference
            // WITHOUT tools so the model must produce a text response.
            if iteration == effective_max_iterations - 1 {
                tracing::info!(
                    "Agent loop: max iterations reached, running final inference without tools"
                );
                on_status(LocalAgentStatus::Thinking);

                let mut messages = vec![ChatMessage::text(Role::System, system_content.clone())];
                messages.extend(session.messages.clone());

                let final_chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
                    Arc::new(std::sync::Mutex::new(Vec::new()));
                let final_for_cb = Arc::clone(&final_chunks);
                let on_chunk_final = Arc::clone(&on_chunk);

                let final_callback: Box<dyn Fn(StreamingChunk) + Send> =
                    Box::new(move |chunk: StreamingChunk| {
                        on_chunk_final(chunk.clone());
                        if let Ok(mut guard) = final_for_cb.lock() {
                            guard.push(chunk);
                        }
                    });

                let final_request = InferenceRequest {
                    messages,
                    tools: None, // No tools — force text response
                    temperature: Some(0.1),
                    max_tokens: Some(MAX_RESPONSE_TOKENS),
                };

                if let Ok(usage) = self.engine.generate(final_request, final_callback).await {
                    total_usage.prompt_tokens += usage.prompt_tokens;
                    total_usage.completion_tokens += usage.completion_tokens;

                    // Poison recovery safe: append-only chunk collection (see above).
                    let chunks: Vec<StreamingChunk> = {
                        let guard = final_chunks.lock().unwrap_or_else(|p| p.into_inner());
                        guard.clone()
                    };
                    let (final_text, final_reasoning, _) = Self::parse_chunks(&chunks);
                    if !final_reasoning.trim().is_empty() {
                        if !accumulated_reasoning.is_empty() {
                            accumulated_reasoning.push_str("\n\n");
                        }
                        accumulated_reasoning.push_str(final_reasoning.trim());
                    }
                    // Accept the final text only if it's real content — not empty
                    // and not a tool call the model printed as text instead of
                    // invoking. A narrated pseudo-call here falls through to the
                    // tool-result synthesis below rather than being persisted raw.
                    let normalized = normalize_response(&final_text);
                    if !normalized.is_empty() && !looks_like_narrated_tool_call(&normalized) {
                        let reasoning = (!accumulated_reasoning.trim().is_empty())
                            .then(|| accumulated_reasoning.trim().to_string());
                        let mut assistant_msg =
                            ChatMessage::text(Role::Assistant, normalized.clone());
                        assistant_msg.reasoning = reasoning.clone();
                        session.messages.push(assistant_msg);

                        on_status(LocalAgentStatus::Idle);
                        session.status = LocalAgentStatus::Idle;

                        return Ok(AgentTurnResult {
                            response: normalized,
                            reasoning,
                            tool_calls_made: all_tool_executions,
                            usage: total_usage,
                        });
                    }
                }

                on_status(LocalAgentStatus::Idle);
                session.status = LocalAgentStatus::Idle;

                // Both final inference and last iteration returned empty —
                // synthesize a summary from tool results so the UI always gets a
                // response.
                let fallback = if !all_tool_executions.is_empty() {
                    summarize_executions(&all_tool_executions)
                } else {
                    // No tool executions to summarize. Try the last iteration's
                    // text; if it's empty, pure internal plumbing, or a leaked
                    // tool call printed as text, fall back to an honest failure
                    // notice so the UI is never blank and never shows pseudo-code.
                    let normalized = normalize_response(&response_text);
                    if normalized.is_empty() || looks_like_narrated_tool_call(&normalized) {
                        EMPTY_RESPONSE_FALLBACK.to_string()
                    } else {
                        normalized
                    }
                };

                return Ok(AgentTurnResult {
                    response: fallback,
                    reasoning: (!accumulated_reasoning.trim().is_empty())
                        .then(|| accumulated_reasoning.trim().to_string()),
                    tool_calls_made: all_tool_executions,
                    usage: total_usage,
                });
            }

            // Otherwise loop back for another inference round
        }

        // Reached via either `break` above: the duplicate-call guard, or the
        // consecutive-parse-failure guard. The max-iteration path
        // (iteration == effective_max_iterations - 1) always returns early and
        // never falls through here. Run one final text-only inference so the
        // session always produces a response from the tool results already in
        // history.
        on_status(LocalAgentStatus::Thinking);

        let mut messages = vec![ChatMessage::text(Role::System, system_content.clone())];
        messages.extend(session.messages.clone());

        let final_chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let final_for_cb = Arc::clone(&final_chunks);
        let on_chunk_tail = Arc::clone(&on_chunk);
        let tail_callback: Box<dyn Fn(StreamingChunk) + Send> =
            Box::new(move |chunk: StreamingChunk| {
                on_chunk_tail(chunk.clone());
                if let Ok(mut guard) = final_for_cb.lock() {
                    guard.push(chunk);
                }
            });

        let tail_request = InferenceRequest {
            messages,
            tools: None,
            temperature: Some(0.1),
            max_tokens: Some(MAX_RESPONSE_TOKENS),
        };

        let final_response =
            if let Ok(usage) = self.engine.generate(tail_request, tail_callback).await {
                total_usage.prompt_tokens += usage.prompt_tokens;
                total_usage.completion_tokens += usage.completion_tokens;
                let chunks: Vec<StreamingChunk> = {
                    let guard = final_chunks.lock().unwrap_or_else(|p| p.into_inner());
                    guard.clone()
                };
                let (tail_text, tail_reasoning, _) = Self::parse_chunks(&chunks);
                if !tail_reasoning.trim().is_empty() {
                    if !accumulated_reasoning.is_empty() {
                        accumulated_reasoning.push_str("\n\n");
                    }
                    accumulated_reasoning.push_str(tail_reasoning.trim());
                }
                let normalized_tail = normalize_response(&tail_text);
                if normalized_tail.is_empty() || looks_like_narrated_tool_call(&normalized_tail) {
                    // Model returned nothing, leaked internal plumbing (e.g. a
                    // <tool_call> block) that stripped down to nothing, or printed
                    // a tool call as text instead of invoking it — synthesize a
                    // summary from the tool results instead of persisting a blank
                    // bubble or raw pseudo-code.
                    summarize_executions(&all_tool_executions)
                } else {
                    normalized_tail
                }
            } else {
                // Inference failed — synthesize from executions
                if !all_tool_executions.is_empty() {
                    let label = humanize_tool_name(&all_tool_executions.last().unwrap().name);
                    format!("Done — {} completed successfully.", label)
                } else {
                    String::new()
                }
            };

        // Final safety net: every branch above can, in principle, produce an
        // empty string (no tool executions to summarize and no usable model
        // text). Never persist a blank assistant bubble — surface an honest
        // failure notice instead.
        let final_response = if final_response.trim().is_empty() {
            EMPTY_RESPONSE_FALLBACK.to_string()
        } else {
            final_response
        };

        let reasoning = (!accumulated_reasoning.trim().is_empty())
            .then(|| accumulated_reasoning.trim().to_string());

        let mut assistant_msg = ChatMessage::text(Role::Assistant, final_response.clone());
        assistant_msg.reasoning = reasoning.clone();
        session.messages.push(assistant_msg);

        on_status(LocalAgentStatus::Idle);
        session.status = LocalAgentStatus::Idle;

        Ok(AgentTurnResult {
            response: final_response,
            reasoning,
            tool_calls_made: all_tool_executions,
            usage: total_usage,
        })
    }

    /// Parse collected streaming chunks into response text and tool calls.
    /// Parse a streamed chunk sequence into `(answer_text, reasoning_text, tool_calls)`.
    ///
    /// Reasoning chunks (the model's chain-of-thought, already separated from the
    /// answer at the nlp-engine parse layer) are accumulated independently of the
    /// answer text so the answer bubble stays clean.
    fn parse_chunks(chunks: &[StreamingChunk]) -> (String, String, Vec<ToolCallRaw>) {
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCallRaw> = Vec::new();
        // Accumulate tool call args by id
        // Use Vec to preserve tool call ordering (important for causal dependencies)
        let mut pending_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args_json)

        for chunk in chunks {
            match chunk {
                StreamingChunk::Token { text: t } => {
                    text.push_str(t);
                }
                StreamingChunk::Reasoning { text: r } => {
                    reasoning.push_str(r);
                }
                StreamingChunk::ToolCallStart { id, name } => {
                    pending_calls.push((id.clone(), name.clone(), String::new()));
                }
                StreamingChunk::ToolCallArgs { id, args_json } => {
                    if let Some(call) = pending_calls.iter_mut().rev().find(|(cid, _, _)| cid == id)
                    {
                        call.2.push_str(args_json);
                    }
                }
                StreamingChunk::Done { .. } | StreamingChunk::Error { .. } => {}
            }
        }

        // Convert accumulated tool calls into ToolCallRaw (order preserved)
        for (id, name, args_json) in pending_calls {
            tool_calls.push(ToolCallRaw {
                id,
                function_name: name,
                arguments_json: args_json,
            });
        }

        (text, reasoning, tool_calls)
    }

    /// Summarize older history turns if the conversation exceeds the token budget.
    ///
    /// Estimates token count for the full history. If it exceeds
    /// `HISTORY_TOKEN_BUDGET`, summarizes older messages (keeping the most
    /// recent 2-3 turns) and replaces them with a single summary message.
    async fn maybe_summarize_history(
        &self,
        session: &mut AgentSession,
        system_content: &str,
    ) -> Result<(), InferenceError> {
        if session.messages.len() <= 4 {
            // Too few messages to need summarization
            return Ok(());
        }

        // Estimate token count of the full conversation
        let mut history_text = String::new();
        for msg in &session.messages {
            history_text.push_str(&msg.content);
            history_text.push(' ');
        }

        let history_tokens = self.engine.token_count(&history_text).await?;
        let system_tokens = self.engine.token_count(system_content).await?;

        // Budget against the model's *effective* context window, which the
        // native path sizes to available memory at load time (a large model on
        // a constrained machine may get far less than 32K). Reserve room for the
        // reply so summarization triggers before the prompt fills the window and
        // the engine rejects it with ContextOverflow. Fall back to the static
        // budget when the window is unknown (model not loaded / remote backend).
        let (total_budget, history_budget) = match self.engine.model_info().await {
            Ok(Some(spec)) if spec.context_window > 0 => {
                let total = spec
                    .context_window
                    .saturating_sub(MAX_RESPONSE_TOKENS)
                    .max(SYSTEM_PROMPT_BUDGET + 1);
                (total, total.saturating_sub(SYSTEM_PROMPT_BUDGET))
            }
            _ => (TOTAL_TOKEN_BUDGET, HISTORY_TOKEN_BUDGET),
        };

        if history_tokens + system_tokens <= total_budget {
            return Ok(());
        }

        if history_tokens <= history_budget {
            return Ok(());
        }

        // Need to summarize. Keep the last 3 messages verbatim, summarize the rest.
        let keep_count = 3.min(session.messages.len());
        let mut split_point = session.messages.len() - keep_count;

        // Never split an assistant tool-call turn from its tool results. A naive
        // cut can leave the kept window starting with an orphan `Tool` message
        // (its preceding assistant `tool_calls` turn drained into the summary),
        // which is exactly the malformed sequence that makes Gemma's template
        // collapse turns and run away. Walk the cut back until the kept window
        // begins on a non-tool message, so every tool result stays paired with
        // the assistant turn that issued it.
        while split_point > 0 && matches!(session.messages[split_point].role, Role::Tool) {
            split_point -= 1;
        }

        // If the back-off consumed the whole prefix there is nothing older to
        // summarize — skip the summarization inference entirely rather than
        // spending a model call on empty input. (Rare: only when the entire
        // over-budget history is one unbroken tool-call chain.)
        if split_point == 0 {
            return Ok(());
        }

        let older_messages: Vec<ChatMessage> = session.messages.drain(..split_point).collect();

        // Build summarization text from older messages. A tool-call assistant
        // turn carries its signal in `tool_calls`, not `content` (content is
        // empty by construction), so render a synthetic line for it — otherwise
        // it collapses to a bare "Assistant:" and the summary loses what the
        // agent actually did.
        let mut summary_input = String::new();
        for msg in &older_messages {
            let role_str = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            let rendered = if msg.content.is_empty() && !msg.tool_calls.is_empty() {
                // Cap each argument blob: a single tool call can emit up to the
                // generation token cap of JSON, and the summary only needs the
                // gist of what was called, not the full payload.
                let calls = msg
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        let args: String = tc.arguments_json.chars().take(120).collect();
                        format!("{}({})", tc.function_name, args)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("called tools: {calls}")
            } else {
                msg.content.clone()
            };
            summary_input.push_str(&format!("{role_str}: {rendered}\n"));
        }

        let summary_prompt = prompt_templates::summarization_prompt(&summary_input);

        // Run a single-shot summarization inference (no tools).
        //
        // This cap is intentionally larger than `MAX_RESPONSE_TOKENS` (the chat
        // turn cap): a summary condenses many drained turns and is not part of
        // the runaway-prone ReAct loop (no tools, single shot, then discarded
        // into one message), so it can afford more room without risking a
        // multi-minute hang. Do NOT unify the two constants.
        let summary_request = InferenceRequest {
            messages: vec![ChatMessage::text(Role::User, summary_prompt)],
            tools: None,
            temperature: Some(0.1),
            max_tokens: Some(4096),
        };

        let summary_chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let summary_for_cb = Arc::clone(&summary_chunks);
        let cb: Box<dyn Fn(StreamingChunk) + Send> = Box::new(move |chunk: StreamingChunk| {
            if let Ok(mut guard) = summary_for_cb.lock() {
                guard.push(chunk);
            }
        });

        let _ = self.engine.generate(summary_request, cb).await?;

        let chunks: Vec<StreamingChunk> = {
            let guard = summary_chunks.lock().unwrap_or_else(|p| p.into_inner());
            guard.clone()
        };
        let (summary_text, _, _) = Self::parse_chunks(&chunks);

        let summary_content = if summary_text.is_empty() {
            // Fallback: just note that history was truncated
            "Previous conversation context was summarized due to token limits.".to_string()
        } else {
            format!("[Conversation summary]: {}", summary_text)
        };

        // Prepend summary as a system-like message at the start of remaining history
        session
            .messages
            .insert(0, ChatMessage::text(Role::System, summary_content));

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LocalAgentService
// ---------------------------------------------------------------------------

/// Session management facade for the local agent.
///
/// Manages active sessions and provides a high-level API for creating,
/// resuming, and ending conversations. Delegates the actual ReAct loop
/// to [`LocalAgentLoop`].
pub struct LocalAgentService<E: ChatInferenceEngine + ?Sized, T: AgentToolExecutor + ?Sized> {
    sessions: RwLock<HashMap<String, AgentSession>>,
    agent_loop: LocalAgentLoop<E, T>,
    /// Per-session cancellation tokens.
    cancel_tokens: RwLock<HashMap<String, CancellationToken>>,
}

impl<E: ChatInferenceEngine + ?Sized + 'static, T: AgentToolExecutor + ?Sized + 'static>
    LocalAgentService<E, T>
{
    pub fn new(engine: Arc<E>, tool_executor: Arc<T>) -> Self {
        Self::new_with_assembler(engine, tool_executor, None)
    }

    pub fn new_with_assembler(
        engine: Arc<E>,
        tool_executor: Arc<T>,
        prompt_assembler: Option<Arc<PromptAssembler>>,
    ) -> Self {
        let mut agent_loop = LocalAgentLoop::new(engine, tool_executor);
        if let Some(assembler) = prompt_assembler {
            agent_loop = agent_loop.with_prompt_assembler(assembler);
        }
        Self {
            sessions: RwLock::new(HashMap::new()),
            agent_loop,
            cancel_tokens: RwLock::new(HashMap::new()),
        }
    }

    /// The loaded model's spec (id and the context window actually granted at
    /// load time, not the configured ceiling). `None` when no model is loaded.
    ///
    /// Exposed so the daemon can report the real window over the wire: an eval
    /// or client that assumes a window larger than the one granted produces
    /// turns that die on context overflow before inference runs.
    pub async fn model_spec(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
        self.agent_loop.engine().model_info().await
    }

    /// Create a new conversation session.
    ///
    /// Returns the session ID. If a model_id is provided, it is recorded
    /// in the session metadata.
    pub async fn create_session(
        &self,
        model_id: Option<String>,
        history: Vec<ChatMessage>,
    ) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = AgentSession {
            id: session_id.clone(),
            model_id,
            messages: history,
            status: LocalAgentStatus::Idle,
            created_at: chrono::Utc::now(),
            tool_executions: Vec::new(),
            dynamic_context: None,
            system_prompt_override: None,
            prior_writes: Vec::new(),
        };

        let cancel = CancellationToken::new();
        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session);
        self.cancel_tokens
            .write()
            .await
            .insert(session_id.clone(), cancel);

        session_id
    }

    /// Set the dynamic workspace context for a session.
    ///
    /// Called after session creation once NodeService is available to
    /// populate schemas, collections, and playbooks for the system prompt.
    pub async fn set_session_context(&self, session_id: &str, context: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.dynamic_context = Some(context);
        }
    }

    /// Seed the writes completed by earlier turns of this conversation.
    ///
    /// The tool-execution path uses these to refuse a repeat of a write that
    /// already landed in a prior turn. Callers that do not persist conversation
    /// history simply never call this.
    pub async fn set_session_prior_writes(
        &self,
        session_id: &str,
        prior_writes: Vec<crate::agent_types::PriorWrite>,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.prior_writes = prior_writes;
        }
    }

    /// Override the full system prompt for a session.
    ///
    /// When set, this bypasses both `PromptAssembler` and the emergency fallback.
    /// Intended for integration tests that want to inject a pre-built prompt
    /// (constructed via `PromptAssembler::assemble_static`) without a live database.
    ///
    /// Gated by the `testing` Cargo feature so it does not leak into the
    /// production API surface.
    #[cfg(any(test, feature = "testing"))]
    pub async fn set_system_prompt(&self, session_id: &str, prompt: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.system_prompt_override = Some(prompt);
        }
    }

    /// Send a user message and run the agent turn.
    ///
    /// Returns the agent's response after potentially multiple rounds
    /// of tool execution. Streams chunks and status updates via callbacks.
    pub async fn send_message(
        &self,
        session_id: &str,
        message: &str,
        on_status: impl Fn(LocalAgentStatus) + Send + Sync + 'static,
        on_chunk: impl Fn(StreamingChunk) + Send + Sync + 'static,
    ) -> Result<AgentTurnResult, InferenceError> {
        let cancel = {
            let tokens = self.cancel_tokens.read().await;
            tokens
                .get(session_id)
                .cloned()
                .ok_or_else(|| InferenceError::Engine(format!("session not found: {session_id}")))?
        };

        // Take session out for mutation, put it back after
        let mut session = {
            let mut sessions = self.sessions.write().await;
            sessions
                .remove(session_id)
                .ok_or_else(|| InferenceError::Engine(format!("session not found: {session_id}")))?
        };

        let result = self
            .agent_loop
            .run_turn(&mut session, message, on_status, on_chunk, cancel)
            .await;

        // Put session back
        self.sessions
            .write()
            .await
            .insert(session_id.to_string(), session);

        result
    }

    /// Cancel an in-progress generation for the given session.
    pub async fn cancel(&self, session_id: &str) {
        let mut tokens = self.cancel_tokens.write().await;
        if let Some(token) = tokens.get(session_id) {
            token.cancel();
        }
        // Replace with a fresh token for future use
        tokens.insert(session_id.to_string(), CancellationToken::new());
    }

    /// End and remove a session, freeing all resources.
    pub async fn end_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
        if let Some(token) = self.cancel_tokens.write().await.remove(session_id) {
            token.cancel();
        }
    }

    /// List all active sessions (id + status).
    pub async fn get_sessions(&self) -> Vec<(String, LocalAgentStatus)> {
        self.sessions
            .read()
            .await
            .iter()
            .map(|(id, s)| (id.clone(), s.status.clone()))
            .collect()
    }

    /// Get a snapshot of a session's current state.
    pub async fn get_session(&self, session_id: &str) -> Option<AgentSession> {
        self.sessions.read().await.get(session_id).cloned()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_types::{
        ChatModelSpec, ModelFamily, PriorWrite, ToolDefinition, ToolError, ToolResult,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Compile-time coupling check: `multi_skill_turn_invokes_skill_tools_between_searches`
    // feeds exactly 5 inference rounds (search_skills → search_semantic →
    // search_skills → create_node → final text) and expects the loop to make
    // it through all of them without hitting the iteration-cap fallback.
    // If `MAX_TOOL_ITERATIONS` is ever reduced below 5, that test would
    // silently start asserting on the fallback path instead of the multi-skill
    // chain — fail loudly here at compile time rather than mysteriously at
    // test time.
    const _: () = assert!(
        MAX_TOOL_ITERATIONS >= 5,
        "multi_skill_turn_invokes_skill_tools_between_searches requires MAX_TOOL_ITERATIONS >= 5",
    );

    // -- Mock inference engine -------------------------------------------

    /// Mock engine that returns pre-configured responses.
    struct MockEngine {
        /// Responses to return for sequential calls to `generate`.
        /// Each entry is a list of chunks to emit.
        responses: tokio::sync::Mutex<Vec<Vec<StreamingChunk>>>,
        generate_count: AtomicUsize,
        /// Effective context window reported by `model_info` — the summarization
        /// gate budgets against this, so tests can exercise a reduced window.
        context_window: u32,
    }

    impl MockEngine {
        fn new(responses: Vec<Vec<StreamingChunk>>) -> Self {
            Self {
                responses: tokio::sync::Mutex::new(responses),
                generate_count: AtomicUsize::new(0),
                context_window: 8192,
            }
        }

        /// Same as `new` but with an explicit effective context window.
        fn with_context_window(responses: Vec<Vec<StreamingChunk>>, context_window: u32) -> Self {
            Self {
                responses: tokio::sync::Mutex::new(responses),
                generate_count: AtomicUsize::new(0),
                context_window,
            }
        }

        /// Create a mock that returns a single text response (no tools).
        fn single_text(text: &str) -> Self {
            Self::new(vec![vec![
                StreamingChunk::Token {
                    text: text.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ]])
        }

        /// Create a mock that first returns a tool call, then a text response.
        fn tool_then_text(tool_name: &str, tool_args: &str, final_text: &str) -> Self {
            Self::new(vec![
                // First call: tool call
                vec![
                    StreamingChunk::ToolCallStart {
                        id: "tc_1".to_string(),
                        name: tool_name.to_string(),
                    },
                    StreamingChunk::ToolCallArgs {
                        id: "tc_1".to_string(),
                        args_json: tool_args.to_string(),
                    },
                    StreamingChunk::Done {
                        usage: InferenceUsage {
                            prompt_tokens: 20,
                            completion_tokens: 10,
                        },
                    },
                ],
                // Second call: final text
                vec![
                    StreamingChunk::Token {
                        text: final_text.to_string(),
                    },
                    StreamingChunk::Done {
                        usage: InferenceUsage {
                            prompt_tokens: 30,
                            completion_tokens: 15,
                        },
                    },
                ],
            ])
        }
    }

    #[async_trait]
    impl ChatInferenceEngine for MockEngine {
        async fn generate(
            &self,
            _request: InferenceRequest,
            on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            let idx = self.generate_count.fetch_add(1, Ordering::SeqCst);
            let responses = self.responses.lock().await;

            if idx >= responses.len() {
                // Return empty response if we run out of pre-configured ones
                on_chunk(StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 0,
                        completion_tokens: 0,
                    },
                });
                return Ok(InferenceUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                });
            }

            let chunks = &responses[idx];
            let mut usage = InferenceUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            };
            for chunk in chunks {
                if let StreamingChunk::Done { usage: u } = chunk {
                    usage = *u;
                }
                on_chunk(chunk.clone());
            }
            Ok(usage)
        }

        async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
            Ok(Some(ChatModelSpec {
                model_id: "test-model".into(),
                family: ModelFamily::Ministral,
                context_window: self.context_window,
                default_temperature: 0.1,
                type_k: None,
                type_v: None,
            }))
        }

        async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
            // Rough estimate: ~4 chars per token
            Ok((text.len() as f32 / 4.0).ceil() as u32)
        }
    }

    // -- Mock tool executor ----------------------------------------------

    struct MockToolExecutor {
        tools: Vec<ToolDefinition>,
        /// Canned results keyed by tool name.
        results: HashMap<String, serde_json::Value>,
    }

    impl MockToolExecutor {
        fn new() -> Self {
            Self {
                tools: Vec::new(),
                results: HashMap::new(),
            }
            .with_tool(
                "search_nodes",
                json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
                json!({"count": 2, "nodes": [
                    {"id": "abc123", "title": "Billing Architecture", "type": "text"},
                    {"id": "def456", "title": "Payment Processing", "type": "text"},
                ]}),
            )
            .with_tool(
                "get_node",
                json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
                json!({"id": "abc123", "title": "Billing Architecture", "body": "Content here"}),
            )
        }

        /// Register an additional tool with its JSON schema and canned result.
        ///
        /// Lets tests express "I expect the agent to call X with shape Y, and
        /// when it does, return Z" in one call instead of poking at the
        /// internal `tools` / `results` fields directly.
        fn with_tool(
            mut self,
            name: &str,
            parameters_schema: serde_json::Value,
            result: serde_json::Value,
        ) -> Self {
            self.tools.push(ToolDefinition {
                name: name.into(),
                description: format!("Mock tool: {name}"),
                parameters_schema,
            });
            self.results.insert(name.to_string(), result);
            self
        }
    }

    #[async_trait]
    impl AgentToolExecutor for MockToolExecutor {
        async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
            Ok(self.tools.clone())
        }

        async fn execute(
            &self,
            name: &str,
            _args: serde_json::Value,
        ) -> Result<ToolResult, ToolError> {
            let result = self
                .results
                .get(name)
                .cloned()
                .unwrap_or(json!({"error": "unknown tool"}));
            let is_error = !self.results.contains_key(name);
            Ok(ToolResult {
                tool_call_id: format!("call_{name}"),
                name: name.to_string(),
                result,
                is_error,
            })
        }
    }

    /// Executor that records every call it actually performs.
    ///
    /// The duplicate guard's whole point is that a call never reaches the
    /// executor, so asserting on the *result* alone is not enough — a guard that
    /// executed the write and then relabelled the result would pass such a
    /// check. This records ground truth.
    struct RecordingToolExecutor {
        inner: MockToolExecutor,
        calls: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl RecordingToolExecutor {
        fn new(inner: MockToolExecutor) -> Self {
            Self {
                inner,
                calls: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn calls_handle(&self) -> Arc<std::sync::Mutex<Vec<String>>> {
            Arc::clone(&self.calls)
        }
    }

    #[async_trait]
    impl AgentToolExecutor for RecordingToolExecutor {
        async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
            self.inner.available_tools().await
        }

        async fn execute(
            &self,
            name: &str,
            args: serde_json::Value,
        ) -> Result<ToolResult, ToolError> {
            self.calls.lock().unwrap().push(name.to_string());
            self.inner.execute(name, args).await
        }
    }

    // -- Helper to create a fresh session --------------------------------

    fn new_session() -> AgentSession {
        AgentSession {
            id: "test-session".to_string(),
            model_id: Some("test-model".to_string()),
            messages: Vec::new(),
            status: LocalAgentStatus::Idle,
            created_at: chrono::Utc::now(),
            tool_executions: Vec::new(),
            dynamic_context: None,
            system_prompt_override: None,
            prior_writes: Vec::new(),
        }
    }

    // -- Tests -----------------------------------------------------------

    #[tokio::test]
    async fn single_turn_no_tools() {
        let engine = Arc::new(MockEngine::single_text("Hello! How can I help?"));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let statuses: Arc<std::sync::Mutex<Vec<LocalAgentStatus>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let statuses_cb = Arc::clone(&statuses);
        let chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let chunks_cb = Arc::clone(&chunks);

        let result = agent_loop
            .run_turn(
                &mut session,
                "Summarize the GitHub release notes for v1.2 in plain English",
                move |s| {
                    statuses_cb.lock().unwrap().push(s);
                },
                move |c| {
                    chunks_cb.lock().unwrap().push(c);
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.response, "Hello! How can I help?");
        assert!(result.tool_calls_made.is_empty());
        assert!(result.usage.prompt_tokens > 0);

        // Session should have 2 messages: user + assistant
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(session.status, LocalAgentStatus::Idle);
    }

    #[tokio::test]
    async fn tool_call_then_final_response() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "search_nodes",
            r#"{"query":"billing"}"#,
            "Found 2 nodes about billing.",
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Search GitHub for open release-blocker issues then summarize them",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.response, "Found 2 nodes about billing.");
        assert_eq!(result.tool_calls_made.len(), 1);
        assert_eq!(result.tool_calls_made[0].name, "search_nodes");

        // Session should have: user, assistant (tool call), tool result, assistant (final)
        assert_eq!(session.messages.len(), 4);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(session.messages[2].role, Role::Tool);
        assert_eq!(session.messages[3].role, Role::Assistant);
    }

    #[tokio::test]
    async fn multi_step_tool_chain() {
        // First: search_nodes, Second: get_node, Third: final text
        let engine = Arc::new(MockEngine::new(vec![
            // Round 1: search_nodes
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"architecture"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 10,
                    },
                },
            ],
            // Round 2: get_node
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_2".to_string(),
                    name: "get_node".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_2".to_string(),
                    args_json: r#"{"id":"abc123"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 40,
                        completion_tokens: 10,
                    },
                },
            ],
            // Round 3: final response
            vec![
                StreamingChunk::Token {
                    text: "The Billing Architecture node describes...".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 60,
                        completion_tokens: 20,
                    },
                },
            ],
        ]));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Look up the Billing Architecture node then fetch its referenced details",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(result
            .response
            .contains("Billing Architecture node describes"));
        assert_eq!(result.tool_calls_made.len(), 2);
        assert_eq!(result.tool_calls_made[0].name, "search_nodes");
        assert_eq!(result.tool_calls_made[1].name, "get_node");

        // Total usage should sum all rounds
        assert_eq!(result.usage.prompt_tokens, 120); // 20+40+60
        assert_eq!(result.usage.completion_tokens, 40); // 10+10+20
    }

    #[tokio::test]
    async fn max_iteration_limit() {
        // Each round returns a DISTINCT query so the duplicate-call guard does
        // NOT fire — this test exercises the pure iteration-cap path.
        let tool_round = |i: usize| {
            vec![
                StreamingChunk::ToolCallStart {
                    id: format!("tc_{i}"),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: format!("tc_{i}"),
                    args_json: format!(r#"{{"query":"test-{i}"}}"#),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ]
        };

        // Provide more rounds than the limit; the loop must stop at MAX_TOOL_ITERATIONS.
        // +1 extra for the final tool-less inference call.
        let rounds: Vec<_> = (0..MAX_TOOL_ITERATIONS + 2).map(tool_round).collect();
        let engine = Arc::new(MockEngine::new(rounds));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Keep running search_nodes forever — verify the iteration cap stops it",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // Should have executed exactly MAX_TOOL_ITERATIONS tool calls (the limit)
        assert_eq!(result.tool_calls_made.len(), MAX_TOOL_ITERATIONS);
        // All should be search_nodes
        for tc in &result.tool_calls_made {
            assert_eq!(tc.name, "search_nodes");
        }

        // The fallback response must encode the invariant:
        // no raw tool identifier reaches the UI.
        assert!(
            !result.response.contains("search_nodes"),
            "fallback response leaked raw tool name: {:?}",
            result.response
        );
    }

    /// Duplicate-call guard: when the model issues the same tool+args pair
    /// it already executed in this turn, the loop breaks immediately so the
    /// final-inference step can synthesise a response from what's in history.
    ///
    /// Engine call sequence:
    ///   Round 0: first call → executed normally (added to seen_calls)
    ///   Round 1: identical call → guard fires, loop breaks (round NOT executed)
    ///   Round 2: tail text-only inference → returns the real response
    #[tokio::test]
    async fn duplicate_tool_call_breaks_loop() {
        let dup_call = || {
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_dup".to_string(),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_dup".to_string(),
                    args_json: r#"{"node_type":"task","query":"Test Task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ]
        };

        let rounds = vec![
            // Round 0: first call — executes normally
            dup_call(),
            // Round 1: identical call — guard detects duplicate, breaks loop
            dup_call(),
            // Round 2: tail tool-less inference — returns text
            vec![
                StreamingChunk::Token {
                    text: "I found the task.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 5,
                    },
                },
            ],
        ];

        let engine = Arc::new(MockEngine::new(rounds));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Find the task named Test Task",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // Guard fires before executing round 1 — exactly 1 tool call recorded.
        assert_eq!(
            result.tool_calls_made.len(),
            1,
            "duplicate guard must break after first execution, got: {:?}",
            result
                .tool_calls_made
                .iter()
                .map(|t| &t.name)
                .collect::<Vec<_>>()
        );
        assert_eq!(result.tool_calls_made[0].name, "search_nodes");
        // Tail inference text is returned (not a synthesised fallback).
        assert!(
            result.response.contains("found the task"),
            "expected tail-inference response, got: {:?}",
            result.response
        );
    }

    // -- humanize_tool_name ------------------------------------------------

    #[test]
    fn humanize_tool_name_known_tools() {
        assert_eq!(humanize_tool_name("create_schema"), "schema creation");
        assert_eq!(humanize_tool_name("update_node"), "node update");
        assert_eq!(humanize_tool_name("search_semantic"), "semantic search");
        assert_eq!(humanize_tool_name("delete_node"), "node deletion");
    }

    #[test]
    fn humanize_tool_name_unknown_falls_back_to_generic() {
        // Unknown identifiers must NOT leak through verbatim — they map to a
        // generic phrase so the chat UI never displays an internal name.
        assert_eq!(
            humanize_tool_name("some_future_tool"),
            "the requested action"
        );
        assert_eq!(humanize_tool_name(""), "the requested action");
    }

    // The former `humanize_tool_name_covers_all_registered_tools` drift detector
    // is gone: `humanize_tool_name` now derives from `Tool`, whose `humanized()`
    // arm is exhaustive over the registry, so coverage holds by construction.

    #[tokio::test]
    async fn cancellation_stops_generation() {
        let engine = Arc::new(MockEngine::single_text("Should not complete"));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let cancel = CancellationToken::new();
        cancel.cancel(); // Cancel immediately

        let result = agent_loop
            .run_turn(
                &mut session,
                "Begin generating a long answer about the GitHub release process",
                |_| {},
                |_| {},
                cancel,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            InferenceError::Engine(msg) => assert_eq!(msg, "cancelled"),
            other => panic!("Expected Engine error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn parse_chunks_text_only() {
        let chunks = vec![
            StreamingChunk::Token {
                text: "Hello ".to_string(),
            },
            StreamingChunk::Token {
                text: "world".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                },
            },
        ];
        let (text, reasoning, tool_calls) =
            LocalAgentLoop::<MockEngine, MockToolExecutor>::parse_chunks(&chunks);
        assert_eq!(text, "Hello world");
        assert!(reasoning.is_empty());
        assert!(tool_calls.is_empty());
    }

    #[tokio::test]
    async fn parse_chunks_separates_reasoning_from_answer() {
        let chunks = vec![
            StreamingChunk::Reasoning {
                text: "The user said hi; ".to_string(),
            },
            StreamingChunk::Token {
                text: "Hello!".to_string(),
            },
            StreamingChunk::Reasoning {
                text: "I should greet back.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 3,
                    completion_tokens: 2,
                },
            },
        ];
        let (text, reasoning, tool_calls) =
            LocalAgentLoop::<MockEngine, MockToolExecutor>::parse_chunks(&chunks);
        assert_eq!(text, "Hello!");
        assert_eq!(reasoning, "The user said hi; I should greet back.");
        assert!(tool_calls.is_empty());
    }

    #[tokio::test]
    async fn parse_chunks_with_tool_calls() {
        let chunks = vec![
            StreamingChunk::Token {
                text: "Let me search".to_string(),
            },
            StreamingChunk::ToolCallStart {
                id: "tc_1".to_string(),
                name: "search_nodes".to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc_1".to_string(),
                args_json: r#"{"query":""#.to_string(),
            },
            StreamingChunk::ToolCallArgs {
                id: "tc_1".to_string(),
                args_json: r#"test"}"#.to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                },
            },
        ];
        let (text, _reasoning, tool_calls) =
            LocalAgentLoop::<MockEngine, MockToolExecutor>::parse_chunks(&chunks);
        assert_eq!(text, "Let me search");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function_name, "search_nodes");
        assert_eq!(tool_calls[0].arguments_json, r#"{"query":"test"}"#);
    }

    // -- LocalAgentService tests -----------------------------------------

    #[tokio::test]
    async fn service_create_and_list_sessions() {
        let engine = Arc::new(MockEngine::single_text("Hello"));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let id1 = service.create_session(Some("model-a".into()), vec![]).await;
        let id2 = service.create_session(None, vec![]).await;

        let sessions = service.get_sessions().await;
        assert_eq!(sessions.len(), 2);

        let session1 = service.get_session(&id1).await.unwrap();
        assert_eq!(session1.model_id, Some("model-a".to_string()));

        let session2 = service.get_session(&id2).await.unwrap();
        assert_eq!(session2.model_id, None);
    }

    #[tokio::test]
    async fn service_end_session() {
        let engine = Arc::new(MockEngine::single_text("Hello"));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let id = service.create_session(None, vec![]).await;
        assert!(service.get_session(&id).await.is_some());

        service.end_session(&id).await;
        assert!(service.get_session(&id).await.is_none());
        assert!(service.get_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn service_send_message() {
        let engine = Arc::new(MockEngine::single_text("I can help with that!"));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let id = service.create_session(None, vec![]).await;
        let result = service
            .send_message(
                &id,
                "Send this message to the agent and confirm a GitHub release reply comes back",
                |_| {},
                |_| {},
            )
            .await
            .unwrap();

        assert_eq!(result.response, "I can help with that!");

        // Session should still exist with messages
        let session = service.get_session(&id).await.unwrap();
        assert_eq!(session.messages.len(), 2);
    }

    #[tokio::test]
    async fn service_send_message_unknown_session() {
        let engine = Arc::new(MockEngine::single_text("Hello"));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let result = service
            .send_message("nonexistent", "Hello", |_| {}, |_| {})
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn service_cancel_session() {
        let engine = Arc::new(MockEngine::single_text("Hello"));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let id = service.create_session(None, vec![]).await;

        // Cancel should not panic even if nothing is in progress
        service.cancel(&id).await;

        // Session should still be usable after cancel
        let session = service.get_session(&id).await;
        assert!(session.is_some());
    }

    #[tokio::test]
    async fn status_transitions_single_turn() {
        let engine = Arc::new(MockEngine::single_text("Response"));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let statuses: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let statuses_cb = Arc::clone(&statuses);

        agent_loop
            .run_turn(
                &mut session,
                "Walk through each status transition while answering about GitHub releases",
                move |s| {
                    let label = match &s {
                        LocalAgentStatus::Idle => "Idle",
                        LocalAgentStatus::Thinking => "Thinking",
                        LocalAgentStatus::ToolExecution { .. } => "ToolExecution",
                        LocalAgentStatus::Streaming => "Streaming",
                        LocalAgentStatus::Error { .. } => "Error",
                    };
                    statuses_cb.lock().unwrap().push(label.to_string());
                },
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        let statuses = statuses.lock().unwrap();
        // Should be: Thinking, Streaming, Idle
        assert!(statuses.contains(&"Thinking".to_string()));
        assert!(statuses.contains(&"Streaming".to_string()));
        assert!(statuses.contains(&"Idle".to_string()));
    }

    #[tokio::test]
    async fn status_transitions_with_tool() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "search_nodes",
            r#"{"query":"test"}"#,
            "Done",
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let statuses: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let statuses_cb = Arc::clone(&statuses);

        agent_loop
            .run_turn(
                &mut session,
                "Search GitHub release notes and report each status transition along the way",
                move |s| {
                    let label = match &s {
                        LocalAgentStatus::Idle => "Idle",
                        LocalAgentStatus::Thinking => "Thinking",
                        LocalAgentStatus::ToolExecution { .. } => "ToolExecution",
                        LocalAgentStatus::Streaming => "Streaming",
                        LocalAgentStatus::Error { .. } => "Error",
                    };
                    statuses_cb.lock().unwrap().push(label.to_string());
                },
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        let statuses = statuses.lock().unwrap();
        // Should include: Thinking (round 1), ToolExecution, Thinking (round 2), Streaming, Idle
        assert!(statuses.contains(&"Thinking".to_string()));
        assert!(statuses.contains(&"ToolExecution".to_string()));
        assert!(statuses.contains(&"Idle".to_string()));
    }

    #[tokio::test]
    async fn history_summarization_trigger() {
        // Create an engine that always returns text (no tools) but we
        // pre-populate the session with enough history to trigger summarization.
        let engine = Arc::new(MockEngine::new(vec![
            // Summarization call
            vec![
                StreamingChunk::Token {
                    text: "Summary: user discussed billing and payments.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 50,
                        completion_tokens: 20,
                    },
                },
            ],
            // Actual response
            vec![
                StreamingChunk::Token {
                    text: "Here is your answer.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 30,
                        completion_tokens: 10,
                    },
                },
            ],
        ]));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();

        // Add enough history to exceed TOTAL_TOKEN_BUDGET (32000 tokens).
        // With ~4 chars/token estimate, we need > 32000*4 = 128000 chars.
        // 20 messages * 7000 chars = 140000 chars = ~35000 tokens > 32000 budget.
        for i in 0..20 {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            session.messages.push(ChatMessage::text(
                role,
                format!("Message {} with extensive content: {}", i, "x".repeat(7000)),
            ));
        }

        let messages_before = session.messages.len();

        let result = agent_loop
            .run_turn(
                &mut session,
                "Recap the prior Billing conversation after triggering history summarization",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // After summarization, older messages should be replaced with a summary.
        // Without summarization, count would be: 20 (pre-existing) + 1 (user) + 1 (assistant) = 22.
        // With summarization, it should be: 1 (summary) + 3 (kept) + 1 (user) + 1 (assistant) = 6 or similar.
        assert!(
            session.messages.len() < messages_before,
            "Expected summarization to reduce message count. Before: {}, After: {}",
            messages_before,
            session.messages.len()
        );

        // The first message should be the summary
        assert!(
            session.messages[0]
                .content
                .contains("[Conversation summary]"),
            "First message should be the summary, got: {}",
            session.messages[0].content
        );

        assert_eq!(result.response, "Here is your answer.");
    }

    /// A model whose effective context window was reduced to fit memory must
    /// summarize when history exceeds *that* window — not the old
    /// hardcoded 32K budget. Here history is ~10K tokens: comfortably under the
    /// former 32K constant, but well over a 4096-token effective window. Before
    /// budgeting against the effective window, this history would sail past the
    /// gate and then be rejected by the engine as ContextOverflow.
    #[tokio::test]
    async fn summarization_triggers_on_reduced_effective_window() {
        let engine = Arc::new(MockEngine::with_context_window(
            vec![
                // Summarization call
                vec![
                    StreamingChunk::Token {
                        text: "Summary: earlier discussion condensed.".to_string(),
                    },
                    StreamingChunk::Done {
                        usage: InferenceUsage {
                            prompt_tokens: 40,
                            completion_tokens: 10,
                        },
                    },
                ],
                // Final answer
                vec![
                    StreamingChunk::Token {
                        text: "Done.".to_string(),
                    },
                    StreamingChunk::Done {
                        usage: InferenceUsage {
                            prompt_tokens: 30,
                            completion_tokens: 10,
                        },
                    },
                ],
            ],
            4096, // effective window reduced to fit memory
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();

        // ~10K tokens (@ ~4 chars/token): under the old 32K constant, over 4096.
        // 20 messages * 2000 chars = 40000 chars = ~10000 tokens.
        for i in 0..20 {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            session.messages.push(ChatMessage::text(
                role,
                format!("Msg {}: {}", i, "y".repeat(2000)),
            ));
        }
        let messages_before = session.messages.len();

        agent_loop
            .run_turn(
                &mut session,
                "Continue the conversation",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(
            session.messages.len() < messages_before,
            "Reduced effective window must trigger summarization. Before: {}, After: {}",
            messages_before,
            session.messages.len()
        );
        assert!(
            session.messages[0]
                .content
                .contains("[Conversation summary]"),
            "First message should be the summary, got: {}",
            session.messages[0].content
        );
    }

    #[tokio::test]
    async fn summarization_never_orphans_a_tool_result() {
        // Build a history that exceeds the token budget and is arranged so the
        // naive "keep last 3" window would START on a Tool message — i.e. its
        // preceding assistant tool-call turn would be drained into the summary,
        // leaving an orphan tool result. The split must back off so the kept
        // window begins on the assistant tool-call turn instead.
        let engine = Arc::new(MockEngine::new(vec![vec![
            StreamingChunk::Token {
                text: "Summary of earlier turns.".to_string(),
            },
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                },
            },
        ]]));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        // Bulk filler to blow the budget (≈4 chars/token in the mock).
        for i in 0..18 {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            session
                .messages
                .push(ChatMessage::text(role, "x".repeat(8000)));
        }
        // CRITICAL fixture detail: the tail is
        //   [assistant tool-call, tool result, user, user]
        // so the naive "keep last 3" window is [tool result, user, user] — it
        // STARTS on the orphan tool result, with its assistant tool-call turn
        // sitting just before the cut. This is what actually exercises the
        // back-off; without it (e.g. a 3-element tail) the split lands on the
        // assistant turn and the loop never runs, making the test a tautology.
        session
            .messages
            .push(ChatMessage::assistant_with_tool_calls(
                String::new(),
                vec![ToolCallRaw {
                    id: "tc_1".into(),
                    function_name: "search_nodes".into(),
                    arguments_json: r#"{"query":"q"}"#.into(),
                }],
            ));
        session.messages.push(ChatMessage::tool_result(
            "result".to_string(),
            "tc_1".to_string(),
            "search_nodes".to_string(),
        ));
        session
            .messages
            .push(ChatMessage::text(Role::User, "first follow-up"));
        session
            .messages
            .push(ChatMessage::text(Role::User, "second follow-up"));

        agent_loop
            .maybe_summarize_history(&mut session, "system")
            .await
            .unwrap();

        // (1) No orphan: every retained Tool message is immediately preceded by
        // an assistant turn carrying tool_calls.
        for (i, m) in session.messages.iter().enumerate() {
            if matches!(m.role, Role::Tool) {
                assert!(
                    i > 0,
                    "tool result at index 0 has no preceding assistant turn"
                );
                let prev = &session.messages[i - 1];
                assert!(
                    matches!(prev.role, Role::Assistant) && !prev.tool_calls.is_empty(),
                    "tool result at index {i} is orphaned (preceding message is not an assistant tool-call turn)"
                );
            }
        }

        // (2) Back-off actually fired: the assistant tool-call turn (which a naive
        // cut would have drained into the summary) must be RETAINED in the kept
        // window, paired with its tool result. Without the back-off this assertion
        // fails — that is what makes this test exercise the fix rather than pass
        // vacuously.
        let retained_tool_call = session
            .messages
            .iter()
            .any(|m| matches!(m.role, Role::Assistant) && !m.tool_calls.is_empty());
        assert!(
            retained_tool_call,
            "assistant tool-call turn was drained, orphaning its tool result — back-off did not fire"
        );
    }

    // -- Additional coverage tests ------------------------------------------

    /// Mock engine that always fails on generate.
    struct FailingEngine;

    #[async_trait]
    impl ChatInferenceEngine for FailingEngine {
        async fn generate(
            &self,
            _request: InferenceRequest,
            _on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
        ) -> Result<InferenceUsage, InferenceError> {
            Err(InferenceError::Engine("model crashed".into()))
        }

        async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
            Ok(None)
        }

        async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
            Ok((text.len() as f32 / 4.0).ceil() as u32)
        }
    }

    /// When `run_turn` returns an error the session must still be in the
    /// sessions map (the "take-mutate-return" pattern reinserts on error).
    #[tokio::test]
    async fn session_persistence_after_inference_error() {
        let engine = Arc::new(FailingEngine);
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        let id = service
            .create_session(Some("test-model".into()), vec![])
            .await;

        // send_message should fail because FailingEngine errors
        let user_msg = "Trigger an inference error and confirm the GitHub session survives intact";
        let result = service.send_message(&id, user_msg, |_| {}, |_| {}).await;

        assert!(result.is_err(), "Expected inference error");

        // Session must still exist in the map despite the error
        let session = service.get_session(&id).await;
        assert!(
            session.is_some(),
            "Session should persist after inference error"
        );

        // The user message should have been appended before the error
        let session = session.unwrap();
        assert!(
            !session.messages.is_empty(),
            "User message should be in session history"
        );
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[0].content, user_msg);
    }

    /// When every turn produces tool calls the loop must stop after exactly
    /// MAX_TOOL_ITERATIONS and return without spinning forever. After
    /// reaching the limit, one final tool-less inference is run.
    #[tokio::test]
    async fn max_iteration_limit_enforced_exactly() {
        let call_count = Arc::new(AtomicUsize::new(0));

        // Build more rounds than MAX_TOOL_ITERATIONS of tool-call responses with
        // DISTINCT queries so the duplicate-call guard does NOT fire — this test
        // exercises the pure iteration-cap path.
        // Plus a final text response for the tool-less wrap-up call.
        let mut responses: Vec<Vec<StreamingChunk>> = Vec::new();
        for i in 0..8 {
            responses.push(vec![
                StreamingChunk::ToolCallStart {
                    id: format!("tc_{i}"),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: format!("tc_{i}"),
                    args_json: format!(r#"{{"query":"loop-{i}"}}"#),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ]);
        }

        /// Mock engine that counts how many times generate is called.
        struct CountingEngine {
            inner: MockEngine,
            count: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl ChatInferenceEngine for CountingEngine {
            async fn generate(
                &self,
                request: InferenceRequest,
                on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
            ) -> Result<InferenceUsage, InferenceError> {
                self.count.fetch_add(1, Ordering::SeqCst);
                self.inner.generate(request, on_chunk).await
            }

            async fn model_info(&self) -> Result<Option<ChatModelSpec>, InferenceError> {
                self.inner.model_info().await
            }

            async fn token_count(&self, text: &str) -> Result<u32, InferenceError> {
                self.inner.token_count(text).await
            }
        }

        let engine = Arc::new(CountingEngine {
            inner: MockEngine::new(responses),
            count: Arc::clone(&call_count),
        });
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Keep calling search_nodes past the iteration cap to verify enforcement",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // Tool calls made = MAX_TOOL_ITERATIONS (one per iteration)
        assert_eq!(result.tool_calls_made.len(), MAX_TOOL_ITERATIONS);

        // Engine called MAX_TOOL_ITERATIONS times + 1 final tool-less call
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            MAX_TOOL_ITERATIONS + 1,
            "generate should be called MAX_TOOL_ITERATIONS + 1 (final tool-less) times"
        );

        // Usage summed from all rounds (including final tool-less call)
        let total_rounds = MAX_TOOL_ITERATIONS + 1;
        assert_eq!(result.usage.prompt_tokens, 10 * total_rounds as u32);
        assert_eq!(result.usage.completion_tokens, 5 * total_rounds as u32);
    }

    /// Cancellation during tool execution should stop the loop promptly.
    #[tokio::test]
    async fn cancellation_during_tool_execution() {
        // Engine returns a tool call in the first round
        let engine = Arc::new(MockEngine::new(vec![
            // Round 1: tool call
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"test"}"#.to_string(),
                },
                // Also request a second tool call in the same round
                StreamingChunk::ToolCallStart {
                    id: "tc_2".to_string(),
                    name: "get_node".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_2".to_string(),
                    args_json: r#"{"id":"abc123"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 10,
                    },
                },
            ],
        ]));

        /// Executor that cancels the token after executing the first tool.
        struct CancellingExecutor {
            inner: MockToolExecutor,
            cancel: CancellationToken,
            call_count: AtomicUsize,
        }

        #[async_trait]
        impl AgentToolExecutor for CancellingExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                self.inner.available_tools().await
            }

            async fn execute(
                &self,
                name: &str,
                args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                let count = self.call_count.fetch_add(1, Ordering::SeqCst);
                let result = self.inner.execute(name, args).await;
                // Cancel after the first tool execution
                if count == 0 {
                    self.cancel.cancel();
                }
                result
            }
        }

        let cancel = CancellationToken::new();
        let executor = Arc::new(CancellingExecutor {
            inner: MockToolExecutor::new(),
            cancel: cancel.clone(),
            call_count: AtomicUsize::new(0),
        });
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Search the Billing Architecture documents and cancel mid-tool-execution",
                |_| {},
                |_| {},
                cancel,
            )
            .await;

        // Should have been cancelled
        assert!(result.is_err(), "Expected cancellation error");
        match result.unwrap_err() {
            InferenceError::Engine(msg) => assert_eq!(msg, "cancelled"),
            other => panic!("Expected Engine(cancelled), got {:?}", other),
        }
    }

    /// After summarization, the history token count should be below the budget.
    #[tokio::test]
    async fn history_summarization_reduces_token_count() {
        let engine = Arc::new(MockEngine::new(vec![
            // Summarization call — return a short summary
            vec![
                StreamingChunk::Token {
                    text: "User asked about billing.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 50,
                        completion_tokens: 10,
                    },
                },
            ],
            // Actual response
            vec![
                StreamingChunk::Token {
                    text: "Here you go.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 30,
                        completion_tokens: 5,
                    },
                },
            ],
        ]));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(Arc::clone(&engine), executor);

        let mut session = new_session();

        // Fill history with enough content to exceed HISTORY_TOKEN_BUDGET.
        // ~4 chars/token, budget is 6000 tokens => need > 24000 chars.
        for i in 0..30 {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            session.messages.push(ChatMessage::text(
                role,
                format!("Msg {}: {}", i, "a".repeat(2000)),
            ));
        }

        let _result = agent_loop
            .run_turn(
                &mut session,
                "Summarize the long Billing history to reduce tokens below the budget",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // Calculate token count of post-summarization history
        let mut total_text = String::new();
        for msg in &session.messages {
            total_text.push_str(&msg.content);
            total_text.push(' ');
        }
        let token_count = engine.token_count(&total_text).await.unwrap();

        assert!(
            token_count <= HISTORY_TOKEN_BUDGET,
            "After summarization, history tokens ({}) should be at or below budget ({})",
            token_count,
            HISTORY_TOKEN_BUDGET
        );
    }

    /// Tool calls with empty or invalid JSON args should be handled gracefully
    /// (defaulting to `{}` rather than panicking).
    #[tokio::test]
    async fn empty_tool_call_args_handled_gracefully() {
        // Engine returns a tool call with empty args, then a final text response
        let engine = Arc::new(MockEngine::new(vec![
            // Round 1: tool call with empty args string
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_nodes".to_string(),
                },
                // No ToolCallArgs chunks at all — args_json will be ""
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ],
            // Round 2: final text
            vec![
                StreamingChunk::Token {
                    text: "Done with empty args.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 10,
                    },
                },
            ],
        ]));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "Invoke a GitHub tool with empty args and verify the loop does not panic",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await;

        // Should not panic — empty args_json falls back to json!({})
        assert!(
            result.is_ok(),
            "Empty tool call args should not cause panic"
        );
        let result = result.unwrap();
        assert_eq!(result.response, "Done with empty args.");
        assert_eq!(result.tool_calls_made.len(), 1);
        // Args should have been defaulted to empty object
        assert_eq!(result.tool_calls_made[0].args, json!({}));
    }

    /// Two sessions can exist and operate independently without interference.
    #[tokio::test]
    async fn multiple_concurrent_sessions() {
        // Engine produces different responses based on call order:
        // calls 0,1 are for session A and session B respectively.
        let engine = Arc::new(MockEngine::new(vec![
            // Session A's response
            vec![
                StreamingChunk::Token {
                    text: "Response for session A".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ],
            // Session B's response
            vec![
                StreamingChunk::Token {
                    text: "Response for session B".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 15,
                        completion_tokens: 8,
                    },
                },
            ],
        ]));
        let executor = Arc::new(MockToolExecutor::new());
        let service = LocalAgentService::new(engine, executor);

        // Create two independent sessions
        let id_a = service.create_session(Some("model-a".into()), vec![]).await;
        let id_b = service.create_session(Some("model-b".into()), vec![]).await;

        assert_ne!(id_a, id_b, "Session IDs should be unique");

        // Send a message to session A
        let result_a = service
            .send_message(&id_a, "Hello from A", |_| {}, |_| {})
            .await
            .unwrap();

        // Send a message to session B
        let result_b = service
            .send_message(&id_b, "Hello from B", |_| {}, |_| {})
            .await
            .unwrap();

        // Verify responses are independent
        assert_eq!(result_a.response, "Response for session A");
        assert_eq!(result_b.response, "Response for session B");

        // Verify each session has its own history
        let session_a = service.get_session(&id_a).await.unwrap();
        let session_b = service.get_session(&id_b).await.unwrap();

        assert_eq!(session_a.messages.len(), 2); // user + assistant
        assert_eq!(session_b.messages.len(), 2); // user + assistant

        assert_eq!(session_a.messages[0].content, "Hello from A");
        assert_eq!(session_b.messages[0].content, "Hello from B");

        assert_eq!(session_a.model_id, Some("model-a".to_string()));
        assert_eq!(session_b.model_id, Some("model-b".to_string()));

        // Ending session A should not affect session B
        service.end_session(&id_a).await;
        assert!(service.get_session(&id_a).await.is_none());
        assert!(service.get_session(&id_b).await.is_some());

        // Session B should still be functional
        let sessions = service.get_sessions().await;
        assert_eq!(sessions.len(), 1);
    }

    // -- search_skills as a regular tool ---------------------------------

    /// The model decides when to search for skills by calling
    /// the `search_skills` tool, then invokes a matching skill (if any) like
    /// any other tool. There's no pre-LLM dispatch — the loop always runs the
    /// full tool set against the model.
    #[tokio::test]
    async fn model_calls_search_skills_then_invokes_matched_skill() {
        // Round 1: model calls search_skills
        // Round 2: model invokes create_node (a "skill" tool) after seeing the match
        // Round 3: model produces a text summary
        let engine = Arc::new(MockEngine::new(vec![
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_skills".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"create a new task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ],
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_2".to_string(),
                    name: "create_node".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_2".to_string(),
                    args_json: r#"{"content":"new task","node_type":"task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 8,
                    },
                },
            ],
            vec![
                StreamingChunk::Token {
                    text: "Created the task.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 30,
                        completion_tokens: 5,
                    },
                },
            ],
        ]));

        let executor = MockToolExecutor::new()
            .with_tool(
                "search_skills",
                json!({"type": "object"}),
                json!({
                    "query": "create a new task",
                    "matches": [
                        {"id": "skill-1", "name": "Node Creation", "confidence": 0.91,
                         "description": "Create new nodes", "tools": ["create_node"]}
                    ]
                }),
            )
            .with_tool(
                "create_node",
                json!({"type": "object"}),
                json!({"id": "nodespace://task-1"}),
            );

        let agent_loop = LocalAgentLoop::new(engine, Arc::new(executor));
        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "create a new task to review the GitHub release notes",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.response, "Created the task.");
        assert_eq!(result.tool_calls_made.len(), 2);
        assert_eq!(result.tool_calls_made[0].name, "search_skills");
        assert_eq!(result.tool_calls_made[1].name, "create_node");
    }

    /// When the model decides no skill is needed, it responds directly —
    /// no clarification short-circuit, no canned string.
    #[tokio::test]
    async fn model_can_respond_without_calling_search_skills() {
        let engine = Arc::new(MockEngine::single_text("Hi there — how can I help?"));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(&mut session, "hi", |_| {}, |_| {}, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(result.response, "Hi there — how can I help?");
        assert!(result.tool_calls_made.is_empty());
        // Crucially: the model was actually invoked (no pre-LLM short-circuit).
        assert!(result.usage.prompt_tokens > 0);
    }

    /// Multi-skill turn: the model calls `search_skills`
    /// for each sub-task, then invokes the matched skill's tool. This test
    /// exercises a full chain — search_skills (notes) → search_semantic →
    /// search_skills (task) → create_node — not just two back-to-back
    /// searches, so a regression that breaks tool dispatch after a second
    /// `search_skills` call is caught here.
    #[tokio::test]
    async fn multi_skill_turn_invokes_skill_tools_between_searches() {
        let engine = Arc::new(MockEngine::new(vec![
            // Round 1: search_skills for "find notes"
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_skills".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"find notes"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ],
            // Round 2: invoke search_semantic (the matched skill's tool)
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_2".to_string(),
                    name: "search_semantic".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_2".to_string(),
                    args_json: r#"{"query":"Q2 budget notes"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 15,
                        completion_tokens: 5,
                    },
                },
            ],
            // Round 3: search_skills for "create task"
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_3".to_string(),
                    name: "search_skills".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_3".to_string(),
                    args_json: r#"{"query":"create task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 18,
                        completion_tokens: 5,
                    },
                },
            ],
            // Round 4: invoke create_node (the matched skill's tool)
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_4".to_string(),
                    name: "create_node".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_4".to_string(),
                    args_json: r#"{"content":"Review Q2 notes","node_type":"task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 22,
                        completion_tokens: 6,
                    },
                },
            ],
            // Round 5: final summary
            vec![
                StreamingChunk::Token {
                    text: "Found the notes and created the review task.".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 25,
                        completion_tokens: 9,
                    },
                },
            ],
        ]));

        let executor = MockToolExecutor::new()
            .with_tool(
                "search_skills",
                json!({"type": "object"}),
                // Same canned response works for both calls; in production
                // the embeddings would distinguish them, but the agent loop
                // doesn't introspect the match payload — it just relays it
                // back to the model.
                json!({
                    "query": "x",
                    "matches": [
                        {"id": "skill-1", "name": "Match", "confidence": 0.9,
                         "description": "matched skill", "tools": ["search_semantic"]}
                    ]
                }),
            )
            .with_tool(
                "search_semantic",
                json!({"type": "object"}),
                json!({"count": 1, "results": [
                    {"id": "note-1", "title": "Q2 Budget", "score": 0.87,
                     "snippet": "Quarterly budget summary…"}
                ]}),
            )
            .with_tool(
                "create_node",
                json!({"type": "object"}),
                json!({"id": "nodespace://task-1"}),
            );

        let agent_loop = LocalAgentLoop::new(engine, Arc::new(executor));
        let mut session = new_session();
        // Loosen the iteration cap since this turn legitimately needs 4 tool
        // rounds (2 searches + 2 invocations) plus a text round. MAX_TOOL_ITERATIONS
        // is 5, so we're at the boundary on purpose.
        let result = agent_loop
            .run_turn(
                &mut session,
                "find my Q2 budget notes and create a task to review them",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            result.tool_calls_made.len(),
            4,
            "{:?}",
            result.tool_calls_made
        );
        assert_eq!(result.tool_calls_made[0].name, "search_skills");
        assert_eq!(result.tool_calls_made[1].name, "search_semantic");
        assert_eq!(result.tool_calls_made[2].name, "search_skills");
        assert_eq!(result.tool_calls_made[3].name, "create_node");
        assert_eq!(
            result.response,
            "Found the notes and created the review task."
        );
    }

    /// Empty `search_skills` matches → model judges and produces a contextual
    /// clarification (referencing what it searched), rather than the prior
    /// hardcoded `CLARIFYING_QUESTION` string. This is the "no relevant skill"
    /// path.
    #[tokio::test]
    async fn empty_search_skills_matches_let_model_clarify_with_context() {
        let engine = Arc::new(MockEngine::new(vec![
            // Round 1: model calls search_skills
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_skills".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"send carrier pigeons"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ],
            // Round 2: model produces a contextual clarification referencing
            // the search it just performed. The exact wording isn't checked —
            // only that the model gets to respond after seeing matches=[].
            vec![
                StreamingChunk::Token {
                    text: "I searched for skills related to that but didn't find anything relevant. Could you describe what you'd like to do?".to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 20,
                        completion_tokens: 18,
                    },
                },
            ],
        ]));

        let executor = MockToolExecutor::new().with_tool(
            "search_skills",
            json!({"type": "object"}),
            // Empty matches array — the meaningful "no skill applies" signal.
            json!({"query": "send carrier pigeons", "matches": []}),
        );

        let agent_loop = LocalAgentLoop::new(engine, Arc::new(executor));
        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "can you send carrier pigeons",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.tool_calls_made.len(), 1);
        assert_eq!(result.tool_calls_made[0].name, "search_skills");
        // Crucially: the response is the model's contextual text, not a
        // canned constant. Just check it's non-empty and acknowledges the
        // search — exact wording belongs to the model.
        assert!(!result.response.is_empty());
        assert!(
            result.response.to_lowercase().contains("search")
                || result.response.to_lowercase().contains("didn't find"),
            "model response should reference what it searched: {:?}",
            result.response
        );
    }

    // -- contains_action_claim tests -----------------------------------------

    #[test]
    fn action_claim_detects_creation_verbs() {
        assert!(contains_action_claim("I created a new node for this."));
        assert!(contains_action_claim(
            "I've created the invoice successfully."
        ));
        assert!(contains_action_claim("I updated the task status."));
        assert!(contains_action_claim("Successfully created the schema."));
        assert!(contains_action_claim("The node has been created."));
    }

    #[test]
    fn action_claim_does_not_fire_on_capability_statements() {
        assert!(!contains_action_claim("I can help you create a node."));
        assert!(!contains_action_claim(
            "I would create a node if you'd like."
        ));
        assert!(!contains_action_claim(
            "I could search for that information."
        ));
        assert!(!contains_action_claim("Sure, I'll look that up for you."));
        assert!(!contains_action_claim("Hello! How can I help?"));
    }

    #[test]
    fn action_claim_does_not_fire_on_conversational_text() {
        assert!(!contains_action_claim("What would you like to do?"));
        assert!(!contains_action_claim("Let me search for that."));
        assert!(!contains_action_claim(
            "The billing architecture node describes a system."
        ));
    }

    // -- looks_like_narrated_tool_call tests ---------------------------------

    #[test]
    fn narrated_tool_call_detects_registered_tool_invocation() {
        // The exact shape reproduced with mistral:7b via Ollama.
        assert!(looks_like_narrated_tool_call(
            r#"search_nodes(node_type='task', filters=[{"property":"status"}])"#
        ));
        assert!(looks_like_narrated_tool_call("create_node(title='Foo')"));
        // Whitespace between name and paren still counts.
        assert!(looks_like_narrated_tool_call(
            "update_node ({\"id\": \"x\"})"
        ));
        // Embedded in surrounding prose.
        assert!(looks_like_narrated_tool_call(
            "Let me run search_nodes(query='invoice') to find it."
        ));
    }

    #[test]
    fn narrated_tool_call_does_not_fire_on_normal_prose() {
        assert!(!looks_like_narrated_tool_call(
            "I can help you query your tasks. What status are you looking for?"
        ));
        // Tool name mentioned but not as a call (no following paren).
        assert!(!looks_like_narrated_tool_call(
            "You can use search_nodes to filter by property."
        ));
        // A generic function-call shape that is NOT a registered tool must not
        // trip the narrow detector.
        assert!(!looks_like_narrated_tool_call(
            "The helper foo(x) returns a value."
        ));
        assert!(!looks_like_narrated_tool_call(""));
    }

    // -- summarize_executions tests ------------------------------------------

    fn exec_record(name: &str, is_error: bool) -> ToolExecutionRecord {
        ToolExecutionRecord {
            tool_call_id: format!("tc_{name}"),
            name: name.to_string(),
            args: json!({}),
            result: json!({}),
            is_error,
            duration_ms: 1,
        }
    }

    #[test]
    fn summarize_executions_marks_all_failed_calls_as_failed() {
        // The looping-search case: every call errored → "failed", never
        // an optimistic "completed".
        let summary = summarize_executions(&[
            exec_record("search_nodes", true),
            exec_record("search_nodes", true),
        ]);
        assert_eq!(summary, "• node search failed (2×)");
    }

    #[test]
    fn summarize_executions_marks_successful_calls_as_completed() {
        let summary = summarize_executions(&[exec_record("create_node", false)]);
        assert_eq!(summary, "• node creation completed");
    }

    #[test]
    fn summarize_executions_treats_partial_failure_as_completed() {
        // A tool that succeeded at least once is not reported as outright failed.
        let summary = summarize_executions(&[
            exec_record("search_nodes", true),
            exec_record("search_nodes", false),
        ]);
        assert_eq!(summary, "• node search completed (2×)");
    }

    // -- Anti-fabrication guard tests ----------------------------------------

    /// Model claims action with zero tool calls → response should be converted
    /// to a confirmation request.
    #[tokio::test]
    async fn anti_fabrication_guard_fires_on_ungrounded_action_claim() {
        let engine = Arc::new(MockEngine::single_text(
            "I created invoice ID 104 and marked it as paid.",
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "create an invoice for $500",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(result.tool_calls_made.is_empty());
        // The fabricated claim should not reach the user verbatim.
        assert!(
            !result.response.to_lowercase().contains("created invoice"),
            "fabricated claim should be suppressed: {:?}",
            result.response
        );
        // Should ask for confirmation instead.
        assert!(
            result.response.to_lowercase().contains("confirm")
                || result.response.to_lowercase().contains("would you like"),
            "should ask for confirmation: {:?}",
            result.response
        );
    }

    /// Model produces legitimate conversational text with no tool calls →
    /// anti-fabrication guard must NOT fire.
    #[tokio::test]
    async fn anti_fabrication_guard_does_not_fire_on_conversational_response() {
        let engine = Arc::new(MockEngine::single_text(
            "I can help you create a node. What title would you like to give it?",
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "I want to create a note",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(result.tool_calls_made.is_empty());
        assert_eq!(
            result.response, "I can help you create a node. What title would you like to give it?",
            "conversational response should pass through unchanged"
        );
    }

    /// Model claims action AFTER successfully executing tool calls → guard
    /// must NOT fire (the claim is grounded in real tool executions).
    #[tokio::test]
    async fn anti_fabrication_guard_does_not_fire_after_real_tool_calls() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "search_nodes",
            r#"{"query":"invoice"}"#,
            "I found 2 invoice nodes in your workspace.",
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "find my invoice nodes",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.tool_calls_made.len(), 1);
        assert_eq!(
            result.response, "I found 2 invoice nodes in your workspace.",
            "grounded response after real tool call should pass through unchanged"
        );
    }

    // -- Tool-failure surfacing tests ----------------------------------------

    /// When a tool fails and the model doesn't mention the error, an error
    /// note should be appended to the response.
    #[tokio::test]
    async fn tool_failure_appended_when_model_ignores_error() {
        // Tool executor that always reports an error
        struct ErrorToolExecutor;

        #[async_trait]
        impl AgentToolExecutor for ErrorToolExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                Ok(vec![ToolDefinition {
                    name: "update_node".into(),
                    description: "Update a node".into(),
                    parameters_schema: json!({"type": "object"}),
                }])
            }

            async fn execute(
                &self,
                name: &str,
                _args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                Ok(ToolResult {
                    tool_call_id: "tc_1".into(),
                    name: name.into(),
                    result: json!({"error": "node not found"}),
                    is_error: true,
                })
            }
        }

        let engine = Arc::new(MockEngine::tool_then_text(
            "update_node",
            r#"{"id":"abc"}"#,
            // Model papers over the error with a success claim
            "All done! The node has been updated.",
        ));
        let executor = Arc::new(ErrorToolExecutor);
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "update node abc",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.tool_calls_made.len(), 1);
        assert!(
            result.tool_calls_made[0].is_error,
            "tool execution should be marked as error"
        );
        // Error note should be appended since model didn't mention the failure.
        assert!(
            result.response.contains("⚠️") || result.response.to_lowercase().contains("error"),
            "error note should be present: {:?}",
            result.response
        );
    }

    /// Unparseable tool arguments must be reported as unparseable — never
    /// silently replaced with `{}` and executed.
    ///
    /// Substituting an empty object sends the tool a payload the model never
    /// wrote, so the failure comes back as a missing required field. The model
    /// then "repairs" an argument it did not get wrong, which is how a single
    /// malformed call turns into a run of progressively worse retries.
    #[tokio::test]
    async fn malformed_tool_arguments_are_not_executed_as_empty_object() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct RecordingExecutor {
            calls: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AgentToolExecutor for RecordingExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                Ok(vec![ToolDefinition {
                    name: "create_node".into(),
                    description: "Create a node".into(),
                    parameters_schema: json!({"type": "object"}),
                }])
            }

            async fn execute(
                &self,
                name: &str,
                _args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(ToolResult {
                    tool_call_id: "tc_1".into(),
                    name: name.into(),
                    result: json!({"id": "nodespace://created"}),
                    is_error: false,
                })
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(MockEngine::tool_then_text(
            "create_node",
            // Truncated mid-object — a real shape observed from small models.
            r#"{"content":"Kind of Blue","node_type":"#,
            "Added it for you.",
        ));
        let executor = Arc::new(RecordingExecutor {
            calls: calls.clone(),
        });
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "put down Kind of Blue",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "the tool must not run at all when the model's arguments are not valid JSON"
        );
        assert_eq!(result.tool_calls_made.len(), 1);
        assert!(
            result.tool_calls_made[0].is_error,
            "the malformed call must be recorded as an error"
        );
    }

    /// A model emitting *differently*-malformed arguments each round must not be
    /// able to burn every iteration.
    ///
    /// The duplicate-call guard cannot catch this: it keys on canonical argument
    /// strings, and no two malformed attempts are identical, so nothing matches.
    #[tokio::test]
    async fn repeated_unparseable_arguments_break_the_turn() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct NeverCalledExecutor {
            calls: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AgentToolExecutor for NeverCalledExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                Ok(vec![ToolDefinition {
                    name: "create_node".into(),
                    description: "Create a node".into(),
                    parameters_schema: json!({"type": "object"}),
                }])
            }

            async fn execute(
                &self,
                name: &str,
                _args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(ToolResult {
                    tool_call_id: "tc".into(),
                    name: name.into(),
                    result: json!({}),
                    is_error: false,
                })
            }
        }

        // Each round is malformed in a DIFFERENT way, so canonical-args dedup
        // never matches. Supply more rounds than the guard should allow.
        let malformed = [
            r#"{"content":"a","node_type":"#,
            r#"{"content":"b",,}"#,
            r#"{"content":"c""#,
            r#"{"content":"d"}}"#,
        ];
        let rounds: Vec<Vec<StreamingChunk>> = malformed
            .iter()
            .enumerate()
            .map(|(i, args)| {
                vec![
                    StreamingChunk::ToolCallStart {
                        id: format!("tc_{i}"),
                        name: "create_node".to_string(),
                    },
                    StreamingChunk::ToolCallArgs {
                        id: format!("tc_{i}"),
                        args_json: (*args).to_string(),
                    },
                    StreamingChunk::Done {
                        usage: InferenceUsage {
                            prompt_tokens: 10,
                            completion_tokens: 5,
                        },
                    },
                ]
            })
            .collect();

        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(MockEngine::new(rounds));
        let executor = Arc::new(NeverCalledExecutor {
            calls: calls.clone(),
        });
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "put down Kind of Blue",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no tool should ever execute — every call had invalid JSON"
        );
        assert!(
            result.tool_calls_made.len() <= MAX_CONSECUTIVE_PARSE_FAILURES,
            "the turn must stop after {} consecutive parse failures, got {} attempts",
            MAX_CONSECUTIVE_PARSE_FAILURES,
            result.tool_calls_made.len()
        );
        assert!(
            result.tool_calls_made.iter().all(|r| r.is_error),
            "every recorded attempt must be marked as an error"
        );
    }

    /// A tool call carrying no arguments at all is a different case: `{}` is the
    /// faithful reading, so the call proceeds and the tool's own required-field
    /// error is the correct message.
    #[tokio::test]
    async fn absent_tool_arguments_still_reach_the_tool() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct RecordingExecutor {
            calls: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl AgentToolExecutor for RecordingExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                Ok(vec![ToolDefinition {
                    name: "create_node".into(),
                    description: "Create a node".into(),
                    parameters_schema: json!({"type": "object"}),
                }])
            }

            async fn execute(
                &self,
                name: &str,
                args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(args, json!({}), "empty arguments should arrive as {{}}");
                Ok(ToolResult {
                    tool_call_id: "tc_1".into(),
                    name: name.into(),
                    result: json!({"error": "missing field `node_type`"}),
                    is_error: true,
                })
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(MockEngine::tool_then_text("create_node", "", "Done."));
        let executor = Arc::new(RecordingExecutor {
            calls: calls.clone(),
        });
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        agent_loop
            .run_turn(
                &mut session,
                "put down Kind of Blue",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "an argument-less call is well-formed and must still reach the tool"
        );
    }

    /// An empty response with no tool calls is an inference bug
    /// (model should always produce text or a tool call), not a UX surface.
    /// Surface as an error so it lands in logs/metrics rather than being
    /// silently masked.
    #[tokio::test]
    async fn empty_model_response_with_no_tools_returns_error() {
        let engine = Arc::new(MockEngine::single_text(""));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(&mut session, "hi", |_| {}, |_| {}, CancellationToken::new())
            .await;

        let err = result.expect_err("empty model response should surface as an error");
        match err {
            InferenceError::Engine(msg) => assert!(msg.contains("empty response")),
            other => panic!("Expected Engine error, got {:?}", other),
        }
    }

    // -- Silent-failure guard regression tests -----------------------

    /// Scenario B: the model prints a tool call as plain text instead of using
    /// the structured tool_calls field. `tool_calls == 0`, so the tool-failure
    /// and (phrase-based) fabrication guards don't fire — the narrated-call guard
    /// must catch it so the raw pseudo-code is never persisted as the answer.
    #[tokio::test]
    async fn narrated_tool_call_as_text_is_not_persisted_verbatim() {
        let engine = Arc::new(MockEngine::single_text(
            r#"search_nodes(node_type='task', filters=[{"property":"status", "operator": "equals", "value": "open"}])"#,
        ));
        let executor = Arc::new(MockToolExecutor::new());
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "show my open tasks",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // No tool actually executed.
        assert!(result.tool_calls_made.is_empty());
        // The leaked pseudo-code must not reach the user.
        assert!(
            !result.response.contains("search_nodes("),
            "narrated tool call should be suppressed, not persisted: {:?}",
            result.response
        );
        // Never empty; should ask the user to confirm instead.
        assert!(!result.response.trim().is_empty());
        assert!(
            result.response.to_lowercase().contains("confirm")
                || result.response.to_lowercase().contains("would you like"),
            "should ask for confirmation: {:?}",
            result.response
        );
        // And the persisted assistant message matches the returned response.
        let last = session.messages.last().unwrap();
        assert!(!last.content.contains("search_nodes("));
        assert!(!last.content.trim().is_empty());
    }

    /// Scenario A: a tool fails, the model loops on the identical broken call
    /// (tripping the duplicate-call breaker), and the forced final inference
    /// returns empty text. The turn must still surface *something* user-visible
    /// — never a blank assistant bubble.
    #[tokio::test]
    async fn empty_final_inference_after_tool_failure_still_surfaces_response() {
        // Executor whose tool always errors.
        struct ErrorToolExecutor;

        #[async_trait]
        impl AgentToolExecutor for ErrorToolExecutor {
            async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
                Ok(vec![ToolDefinition {
                    name: "search_nodes".into(),
                    description: "Find, list, and filter nodes".into(),
                    parameters_schema: json!({"type": "object"}),
                }])
            }

            async fn execute(
                &self,
                name: &str,
                _args: serde_json::Value,
            ) -> Result<ToolResult, ToolError> {
                Ok(ToolResult {
                    tool_call_id: "tc_1".into(),
                    name: name.into(),
                    result: json!({"error": "Invalid metadata field: status"}),
                    is_error: true,
                })
            }
        }

        // Round 1: the broken tool call. Round 2: the identical call again (the
        // duplicate-call breaker fires here, popping the assistant turn and
        // breaking to the tail final-inference). Round 3 (tail, tools stripped):
        // empty text — nothing usable from the model.
        let broken_call = || {
            vec![
                StreamingChunk::ToolCallStart {
                    id: "tc_1".to_string(),
                    name: "search_nodes".to_string(),
                },
                StreamingChunk::ToolCallArgs {
                    id: "tc_1".to_string(),
                    args_json: r#"{"query":"","node_type":"task"}"#.to_string(),
                },
                StreamingChunk::Done {
                    usage: InferenceUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                    },
                },
            ]
        };
        let engine = Arc::new(MockEngine::new(vec![
            broken_call(),
            broken_call(),
            // Tail final inference: empty.
            vec![StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 5,
                    completion_tokens: 0,
                },
            }],
        ]));
        let executor = Arc::new(ErrorToolExecutor);
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        let result = agent_loop
            .run_turn(
                &mut session,
                "show my open tasks",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap();

        // The tool ran and failed at least once.
        assert!(!result.tool_calls_made.is_empty());
        assert!(result.tool_calls_made.iter().any(|r| r.is_error));
        // Never a blank bubble: the tool-result synthesis produces a bullet
        // summary from the (failing) execution.
        assert!(
            !result.response.trim().is_empty(),
            "empty final inference must not yield a blank response"
        );
        // And no leaked internal call syntax.
        assert!(!result.response.contains("search_nodes("));
        // The synthesized summary is honest about the failure — a turn that only
        // ever failed must not be reported as "completed".
        assert!(
            result.response.to_lowercase().contains("failed"),
            "failed-only executions should be summarized as failed: {:?}",
            result.response
        );
        assert!(!result.response.contains("completed"));
        let last = session.messages.last().unwrap();
        assert!(!last.content.trim().is_empty());
    }

    /// Guard the last-resort constant itself: the defensive nets that fire when
    /// there is genuinely nothing to synthesize (no tool executions, no usable
    /// model text) depend on it being a non-empty, honest notice.
    #[test]
    fn empty_response_fallback_is_never_blank() {
        assert!(!EMPTY_RESPONSE_FALLBACK.trim().is_empty());
        assert!(EMPTY_RESPONSE_FALLBACK.contains("try again"));
    }

    // -- Cross-turn duplicate-write guard --------------------------------

    /// Build a session that already carries one completed `create_node` write,
    /// as a rebuilt turn N+1 would after loading persisted history.
    fn session_with_prior_create(canonical: &str) -> AgentSession {
        let mut session = new_session();
        session.prior_writes = vec![PriorWrite {
            tool: "create_node".to_string(),
            canonical_args: canonical.to_string(),
            node_id: Some("nodespace://n1".to_string()),
            summary: Some("Buy milk".to_string()),
        }];
        session
    }

    fn create_node_executor() -> MockToolExecutor {
        MockToolExecutor::new().with_tool(
            "create_node",
            json!({"type": "object", "properties": {"content": {"type": "string"}}}),
            json!({"id": "nodespace://n2", "created": true}),
        )
    }

    /// The acceptance criterion: a repeated write with identical canonical args
    /// in a later turn must not execute a second time.
    #[tokio::test]
    async fn cross_turn_duplicate_write_is_not_executed() {
        let args = r#"{"content":"Buy milk"}"#;
        let engine = Arc::new(MockEngine::tool_then_text(
            "create_node",
            args,
            "It already exists.",
        ));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = session_with_prior_create(&canonical_args(args));
        let result = agent_loop
            .run_turn(
                &mut session,
                "add a task to buy milk",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(
            !calls.lock().unwrap().iter().any(|c| c == "create_node"),
            "the duplicate create must never reach the executor, got {:?}",
            calls.lock().unwrap()
        );

        // The model still receives a result, and it is not an error.
        let rec = result
            .tool_calls_made
            .iter()
            .find(|r| r.name == "create_node")
            .expect("a tool result must still be produced");
        assert!(!rec.is_error, "a refused duplicate is not a failure");
    }

    /// The result must name the already-written node, so the model can tell the
    /// user the thing exists instead of reporting an opaque refusal.
    #[tokio::test]
    async fn refused_duplicate_names_the_existing_node() {
        let args = r#"{"content":"Buy milk"}"#;
        let engine = Arc::new(MockEngine::tool_then_text("create_node", args, "Exists."));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = session_with_prior_create(&canonical_args(args));
        let result = agent_loop
            .run_turn(
                &mut session,
                "add it",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        let rec = result
            .tool_calls_made
            .iter()
            .find(|r| r.name == "create_node")
            .expect("tool result");
        let rendered = rec.result.to_string();
        assert!(
            rendered.contains("nodespace://n1"),
            "must name the existing node, got {rendered}"
        );
        assert!(
            rendered.contains("Buy milk"),
            "must describe what already exists, got {rendered}"
        );
    }

    /// Key order must not defeat the guard: the same call re-emitted with
    /// reordered keys is the same write.
    #[tokio::test]
    async fn guard_matches_regardless_of_argument_key_order() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "create_node",
            r#"{"node_type":"task","content":"Buy milk"}"#,
            "Exists.",
        ));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = session_with_prior_create(&canonical_args(
            r#"{"content":"Buy milk","node_type":"task"}"#,
        ));
        agent_loop
            .run_turn(
                &mut session,
                "add it",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(
            !calls.lock().unwrap().iter().any(|c| c == "create_node"),
            "reordered keys are the same call and must still be refused"
        );
    }

    /// A genuinely different write must still go through. The guard keys on the
    /// arguments, not merely the tool name.
    #[tokio::test]
    async fn a_different_write_still_executes() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "create_node",
            r#"{"content":"Buy bread"}"#,
            "Added.",
        ));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = session_with_prior_create(&canonical_args(r#"{"content":"Buy milk"}"#));
        agent_loop
            .run_turn(
                &mut session,
                "add bread",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(
            calls.lock().unwrap().iter().any(|c| c == "create_node"),
            "a distinct create must not be blocked"
        );
    }

    /// Idempotent repeats must survive the guard. Setting the same task status
    /// twice is a no-op, not a duplicate, and blocking it would break a user
    /// legitimately re-asserting a value.
    #[tokio::test]
    async fn idempotent_update_repeat_is_not_blocked() {
        let args = r#"{"id":"nodespace://t1","status":"done"}"#;
        let engine = Arc::new(MockEngine::tool_then_text(
            "update_task_status",
            args,
            "Marked done.",
        ));
        let executor = Arc::new(RecordingToolExecutor::new(
            MockToolExecutor::new().with_tool(
                "update_task_status",
                json!({"type": "object", "properties": {"status": {"type": "string"}}}),
                json!({"id": "nodespace://t1", "status": "done"}),
            ),
        ));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        // Even with an exact-match record present, an update must execute.
        session.prior_writes = vec![PriorWrite {
            tool: "update_task_status".to_string(),
            canonical_args: canonical_args(args),
            node_id: Some("nodespace://t1".to_string()),
            summary: Some("t1".to_string()),
        }];

        agent_loop
            .run_turn(
                &mut session,
                "mark it done",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(
            calls
                .lock()
                .unwrap()
                .iter()
                .any(|c| c == "update_task_status"),
            "idempotent updates must not be blocked by the cross-turn guard"
        );
    }

    /// A session with no prior writes — every caller that does not persist
    /// history — must behave exactly as before.
    #[tokio::test]
    async fn no_prior_writes_leaves_execution_untouched() {
        let engine = Arc::new(MockEngine::tool_then_text(
            "create_node",
            r#"{"content":"Buy milk"}"#,
            "Added.",
        ));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        agent_loop
            .run_turn(
                &mut session,
                "add milk",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(calls.lock().unwrap().iter().any(|c| c == "create_node"));
    }

    /// The guard's identity must derive from the *parsed* arguments on both
    /// sides. Empty arguments are read as `{}` before execution, so a
    /// raw-string comparison would store `"{}"` and compare `""` — a write that
    /// could never match itself, silently disarming the guard for that call.
    #[tokio::test]
    async fn empty_arguments_compare_against_their_parsed_form() {
        let engine = Arc::new(MockEngine::tool_then_text("create_node", "", "Exists."));
        let executor = Arc::new(RecordingToolExecutor::new(create_node_executor()));
        let calls = executor.calls_handle();
        let agent_loop = LocalAgentLoop::new(engine, executor);

        let mut session = new_session();
        // What the daemon would have persisted for an empty-args call: the
        // canonical form of the parsed `{}`, not the empty string.
        session.prior_writes = vec![PriorWrite {
            tool: "create_node".to_string(),
            canonical_args: canonical_args("{}"),
            node_id: Some("nodespace://n1".to_string()),
            summary: Some("Buy milk".to_string()),
        }];

        agent_loop
            .run_turn(
                &mut session,
                "add it",
                |_| {},
                |_| {},
                CancellationToken::new(),
            )
            .await
            .expect("turn should succeed");

        assert!(
            !calls.lock().unwrap().iter().any(|c| c == "create_node"),
            "an empty-args repeat must still be recognised as the same call"
        );
    }
}
