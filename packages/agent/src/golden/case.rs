//! The golden case-file format: everything the model sees, as data.
//!
//! A case file is TOML. It declares the *pre-template* inputs — system
//! prompt, tool definitions, history, user message — plus an expectation and
//! a rep count. Nothing here knows how to run a model; [`super::runner`] owns
//! that. Keeping the format and its expectation evaluation model-free is what
//! lets both be unit-tested in the default test run while the runner itself
//! stays out of it.
//!
//! ## Why pre-template
//!
//! The four ADR-064 instruction channels all live above the chat template:
//! resident prose is the system message, tool selection and argument shape are
//! the tool definitions, skill instructions arrive as an injected block or a
//! tool result, and history is history. Those are the surfaces worth
//! shortening and rewording. The template below them is fixed machinery — the
//! runner drives it through the same `LlamaChatInferenceEngine` production
//! uses, so post-template fidelity comes free rather than being restated here.
//!
//! ## Shape
//!
//! ```toml
//! name = "scenario6-zero-history-control"
//! reps = 3
//!
//! [[turn]]
//! system = """
//! You are a graph-editing assistant.
//! """
//! user = "The 2400 one came back — set it to returned"
//!
//!   [[turn.tool]]
//!   name = "resolve_query"
//!   description = "Resolve an indirect reference to the node it refers to."
//!   parameters_schema = { type = "object", properties = { request = { type = "string" } } }
//!
//!   [turn.expect]
//!   kind = "tool"
//!   tool = "resolve_query"
//!   arguments = { node_type = "equipment_checkout_record" }
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent_types::ToolDefinition;
use nodespace_nlp_engine::chat::{ChatMessage, Role};

/// A parsed case file: one or more turns, run `reps` times as a whole.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenCase {
    /// Identifier for this case, used in output. Defaults to the file stem.
    #[serde(default)]
    pub name: String,
    /// Free-text note recording what the case is probing. Never sent to the
    /// model — it documents the case for whoever reads the file next.
    #[serde(default)]
    pub notes: String,
    /// How many independent runs of the whole case to perform.
    ///
    /// Deliberately not `Option`: a case that omits it still gets
    /// [`DEFAULT_REPS`], because a single run is not decision-grade. Identical
    /// code has scored 6, 7, and 7 out of 12 on the agent matrix across three
    /// runs, and scenario 6's `resolve_query` behavior measured 3/3 one way
    /// and 0/8 the other depending on carried history.
    #[serde(default = "default_reps")]
    pub reps: u32,
    /// Sampling temperature for every turn. Matches the goldens' 0.1.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Cap on generated tokens per turn.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// The turns, in order. A single-turn case has exactly one.
    #[serde(rename = "turn")]
    pub turns: Vec<Turn>,
}

/// Reps applied when a case file omits `reps`. Three is the smallest count
/// that distinguishes "reliable" from "got lucky once" at a tolerable cost;
/// the existing goldens recorded their confirmations at 3/3.
pub const DEFAULT_REPS: u32 = 3;

fn default_reps() -> u32 {
    DEFAULT_REPS
}

fn default_temperature() -> f32 {
    0.1
}

fn default_max_tokens() -> u32 {
    512
}

fn default_chain() -> bool {
    true
}

/// One turn: what the model is shown, and what it is expected to do.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    /// Optional label for this turn in output. Defaults to `turn<N>`.
    #[serde(default)]
    pub label: String,
    /// The system prompt for this turn. Each turn carries its own, because
    /// the production loop's two stages do not share one.
    pub system: String,
    /// Literal history messages prepended to this turn, after the system
    /// prompt and before any messages chained in from earlier turns.
    #[serde(default, rename = "history")]
    pub history: Vec<HistoryMessage>,
    /// Whether this turn sees what earlier turns produced.
    ///
    /// `true` (the default) is the sequence case: the turn's history is
    /// extended with the real user/assistant/tool exchange of every preceding
    /// turn, so a later turn reads what the model actually said rather than a
    /// hand-picked stand-in for it.
    ///
    /// `false` isolates the turn — it sees only its own `system`, `history`,
    /// and `user`. That is what lets one case file hold several *arms* of one
    /// experiment: independent probes that vary one variable and must not
    /// contaminate each other. Without it, arms would have to be split across
    /// files and could drift apart in the parts meant to be identical.
    #[serde(default = "default_chain")]
    pub chain: bool,
    /// The user message for this turn.
    pub user: String,
    /// The tool surface offered on this turn.
    #[serde(default, rename = "tool")]
    pub tools: Vec<CaseTool>,
    /// Canned results for the tool calls this turn makes, keyed by tool name.
    ///
    /// When the next turn runs, each tool call this turn emitted is replayed
    /// into history as an assistant tool-call turn paired with its result, so
    /// the following turn sees a well-formed exchange rather than an orphan
    /// tool result. A tool with no entry here gets [`DEFAULT_TOOL_RESULT`].
    #[serde(default)]
    pub tool_results: BTreeMap<String, String>,
    /// What this turn is expected to produce.
    pub expect: Expectation,
}

/// The stand-in result for a tool call the case file did not supply one for.
///
/// Deliberately contentless: a case that depends on what a tool returned
/// should say so in `tool_results` rather than inherit a fact from the
/// harness. This exists only so the replayed exchange is structurally
/// well-formed.
pub const DEFAULT_TOOL_RESULT: &str = "{\"ok\": true}";

/// A history message as written in the case file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryMessage {
    /// `system`, `user`, `assistant`, or `tool`.
    pub role: CaseRole,
    /// The message text.
    pub content: String,
}

/// Message roles as spelled in a case file. Mirrors [`Role`] rather than
/// reusing it so the case format is not silently redefined by a change to the
/// engine's enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaseRole {
    /// A system message.
    System,
    /// A user message.
    User,
    /// An assistant message.
    Assistant,
    /// A tool result message.
    Tool,
}

impl From<CaseRole> for Role {
    fn from(r: CaseRole) -> Self {
        match r {
            CaseRole::System => Role::System,
            CaseRole::User => Role::User,
            CaseRole::Assistant => Role::Assistant,
            CaseRole::Tool => Role::Tool,
        }
    }
}

impl HistoryMessage {
    /// Convert to the engine's message type.
    pub fn to_chat_message(&self) -> ChatMessage {
        ChatMessage::text(self.role.into(), self.content.clone())
    }
}

/// A tool definition as written in a case file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseTool {
    /// Tool name the model calls.
    pub name: String,
    /// Tool description — under ADR-064 rule 2 this channel owns tool
    /// selection, so it is one of the things a case exists to tune.
    pub description: String,
    /// JSON Schema for the tool's parameters, written as an inline TOML table.
    #[serde(default = "empty_object_schema")]
    pub parameters_schema: toml::Value,
}

fn empty_object_schema() -> toml::Value {
    let mut t = toml::map::Map::new();
    t.insert("type".into(), toml::Value::String("object".into()));
    toml::Value::Table(t)
}

impl CaseTool {
    /// Convert to the inference boundary's tool type.
    ///
    /// The schema round-trips TOML → JSON via serde rather than being
    /// hand-walked, so any JSON Schema construct TOML can express survives
    /// without this function knowing the schema's shape.
    pub fn to_tool_definition(&self) -> Result<ToolDefinition, CaseError> {
        let parameters_schema = serde_json::to_value(&self.parameters_schema)
            .map_err(|e| CaseError::Schema(self.name.clone(), e.to_string()))?;
        Ok(ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters_schema,
        })
    }
}

/// What a turn is expected to produce.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Expectation {
    /// A named tool was called, optionally with argument assertions.
    Tool {
        /// The tool the model must call.
        tool: String,
        /// Argument assertions, matched as a recursive subset of the parsed
        /// arguments — never full-blob equality. A case asserting one field
        /// should not break when the model also fills a second, correct one.
        #[serde(default)]
        arguments: Option<toml::Value>,
    },
    /// An ordered chain of tool calls within this one turn.
    Sequence {
        /// Tool names in the order they must appear. Extra calls interleaved
        /// between them are tolerated; the named ones must appear in order.
        tools: Vec<String>,
        /// Per-tool minimum count of populated `properties` keys, so a
        /// sequence can require the write actually carried properties rather
        /// than merely that the tool name appeared.
        #[serde(default)]
        min_properties_on: BTreeMap<String, usize>,
    },
    /// The model asked the user to disambiguate.
    ///
    /// A first-class pass, not a failure mode: ADR-038's clarification
    /// contract makes "ask the user" the correct answer for a genuinely
    /// ambiguous request, and `route_clarify` is a typed tool.
    Clarify,
    /// A text reply with no tool call.
    Text {
        /// Optional substrings the reply must contain, matched
        /// case-insensitively.
        #[serde(default)]
        contains: Vec<String>,
    },
    /// Run the reps, print the outcomes, never fail.
    ///
    /// For exploratory probes taken before a target is known — the printed
    /// result is the finding. Three of the five committed scenario-6 arms are
    /// exactly this: they were authored as discriminating experiments with no
    /// asserted answer.
    Observe,
}

/// The tool name that satisfies [`Expectation::Clarify`].
pub const CLARIFY_TOOL: &str = crate::local_agent::routing::ROUTE_CLARIFY_TOOL;

/// A tool call as observed from a turn, after production's argument repairs.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedCall {
    /// The tool the model named.
    pub name: String,
    /// The repaired arguments JSON, exactly as it would reach the tool.
    pub arguments_json: String,
}

/// Everything a single turn produced.
#[derive(Debug, Clone, Default)]
pub struct TurnOutput {
    /// The assistant's text, if any.
    pub text: String,
    /// Tool calls in emission order.
    pub calls: Vec<ObservedCall>,
}

/// The verdict for one turn of one rep.
///
/// "No tool call parsed" and "wrong tool called" are separate variants
/// because they have opposite fixes: the first points at the template or the
/// parser, the second at the prompt. Collapsing them into a bare failure
/// count destroys exactly the distinction the runner exists to surface.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The expectation held.
    Pass,
    /// The model called a tool, but not the expected one.
    WrongTool {
        /// The tool the expectation named.
        expected: String,
        /// What the model actually called.
        actual: String,
    },
    /// The model produced no tool call at all where one was expected.
    NoToolCall {
        /// The tool the expectation named.
        expected: String,
        /// The text the model produced instead, for diagnosis.
        text: String,
    },
    /// The right tool was called with arguments that failed an assertion.
    BadArguments {
        /// The tool that was called.
        tool: String,
        /// Human-readable description of the first mismatch.
        detail: String,
    },
    /// A `sequence` expectation's tools did not all appear in order.
    BadSequence {
        /// The expected order.
        expected: Vec<String>,
        /// What was actually called, in order.
        actual: Vec<String>,
    },
    /// A `text` expectation saw a tool call, or missed a required substring.
    BadText {
        /// What went wrong.
        detail: String,
    },
    /// An `observe` expectation — never a failure, records what happened.
    Observed {
        /// A one-line summary of the turn's output.
        summary: String,
    },
}

impl Outcome {
    /// Whether this outcome counts toward the N-of-N tally.
    ///
    /// [`Outcome::Observed`] counts as a pass: an `observe` case asserts
    /// nothing, so every rep of it "passes" and the tally is N/N by
    /// construction. The printed per-rep summaries carry the finding.
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass | Outcome::Observed { .. })
    }

    /// Short tag for tallying failures by kind.
    pub fn tag(&self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::WrongTool { .. } => "wrong-tool",
            Outcome::NoToolCall { .. } => "no-tool-call",
            Outcome::BadArguments { .. } => "bad-arguments",
            Outcome::BadSequence { .. } => "bad-sequence",
            Outcome::BadText { .. } => "bad-text",
            Outcome::Observed { .. } => "observed",
        }
    }
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Pass => write!(f, "PASS"),
            Outcome::WrongTool { expected, actual } => {
                write!(f, "WRONG TOOL: expected {expected}, called {actual}")
            }
            Outcome::NoToolCall { expected, text } => {
                write!(
                    f,
                    "NO TOOL CALL: expected {expected}, model replied with text: {}",
                    truncate(text, 200)
                )
            }
            Outcome::BadArguments { tool, detail } => {
                write!(f, "BAD ARGUMENTS on {tool}: {detail}")
            }
            Outcome::BadSequence { expected, actual } => {
                write!(
                    f,
                    "BAD SEQUENCE: expected {} in order, got {}",
                    expected.join(" -> "),
                    if actual.is_empty() {
                        "no tool calls".to_string()
                    } else {
                        actual.join(" -> ")
                    }
                )
            }
            Outcome::BadText { detail } => write!(f, "BAD TEXT: {detail}"),
            Outcome::Observed { summary } => write!(f, "OBSERVED: {summary}"),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max).collect();
    format!("{cut}…")
}

/// Errors from loading or interpreting a case file.
#[derive(Debug)]
pub enum CaseError {
    /// The file could not be read.
    Io(String),
    /// The file was not valid TOML, or did not match the case schema.
    Parse(String),
    /// A tool's `parameters_schema` could not be converted to JSON.
    Schema(String, String),
    /// The case is structurally invalid (e.g. no turns).
    Invalid(String),
}

impl std::fmt::Display for CaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaseError::Io(m) => write!(f, "could not read case file: {m}"),
            CaseError::Parse(m) => write!(f, "case file is not a valid golden case: {m}"),
            CaseError::Schema(tool, m) => {
                write!(
                    f,
                    "tool '{tool}' has an unconvertible parameters_schema: {m}"
                )
            }
            CaseError::Invalid(m) => write!(f, "case is invalid: {m}"),
        }
    }
}

impl std::error::Error for CaseError {}

impl GoldenCase {
    /// Parse a case from TOML text, defaulting `name` to `default_name`.
    pub fn from_toml(text: &str, default_name: &str) -> Result<Self, CaseError> {
        let mut case: GoldenCase =
            toml::from_str(text).map_err(|e| CaseError::Parse(e.to_string()))?;
        if case.name.is_empty() {
            case.name = default_name.to_string();
        }
        case.validate()?;
        Ok(case)
    }

    /// Load and parse a case file from disk.
    pub fn load(path: &Path) -> Result<Self, CaseError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CaseError::Io(format!("{}: {e}", path.display())))?;
        let default_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("case")
            .to_string();
        Self::from_toml(&text, &default_name)
    }

    fn validate(&self) -> Result<(), CaseError> {
        if self.turns.is_empty() {
            return Err(CaseError::Invalid(
                "a case must declare at least one [[turn]]".into(),
            ));
        }
        if self.reps == 0 {
            return Err(CaseError::Invalid(
                "reps must be at least 1 — the runner reports N/N, never a bare pass/fail".into(),
            ));
        }
        for (i, turn) in self.turns.iter().enumerate() {
            // Every tool schema is converted eagerly so a malformed one is
            // reported before the 5GB model load, not several minutes into it.
            for tool in &turn.tools {
                tool.to_tool_definition()?;
            }
            let expected_tools: Vec<&str> = match &turn.expect {
                Expectation::Tool { tool, .. } => vec![tool.as_str()],
                Expectation::Sequence { tools, .. } => tools.iter().map(|t| t.as_str()).collect(),
                Expectation::Clarify => vec![CLARIFY_TOOL],
                Expectation::Text { .. } | Expectation::Observe => vec![],
            };
            // A tool the model was never offered cannot be called, so an
            // expectation naming one is a typo in the case, not a finding
            // about the model. Catching it here turns a guaranteed N-of-N
            // failure into an immediate error.
            for name in expected_tools {
                if !turn.tools.iter().any(|t| t.name == name) {
                    return Err(CaseError::Invalid(format!(
                        "turn {} expects tool '{}' but does not offer it in its [[turn.tool]] set",
                        i + 1,
                        name
                    )));
                }
            }
            if let Expectation::Sequence {
                tools,
                min_properties_on,
            } = &turn.expect
            {
                if tools.is_empty() {
                    return Err(CaseError::Invalid(format!(
                        "turn {}: a sequence expectation must name at least one tool",
                        i + 1
                    )));
                }
                for name in min_properties_on.keys() {
                    if !tools.contains(name) {
                        return Err(CaseError::Invalid(format!(
                            "turn {}: min_properties_on names '{}', which is not in the sequence",
                            i + 1,
                            name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// The label for turn `index`, defaulting to `turn<N>`.
    pub fn turn_label(&self, index: usize) -> String {
        let turn = &self.turns[index];
        if turn.label.is_empty() {
            format!("turn{}", index + 1)
        } else {
            turn.label.clone()
        }
    }
}

/// Evaluate a turn's output against its expectation.
///
/// Model-free by construction, so the whole expectation surface is covered by
/// ordinary unit tests in the default run while the runner that produces
/// [`TurnOutput`] stays out of it.
pub fn evaluate(expect: &Expectation, output: &TurnOutput) -> Outcome {
    match expect {
        Expectation::Tool { tool, arguments } => {
            let Some(call) = output.calls.first() else {
                return Outcome::NoToolCall {
                    expected: tool.clone(),
                    text: output.text.clone(),
                };
            };
            if &call.name != tool {
                return Outcome::WrongTool {
                    expected: tool.clone(),
                    actual: call.name.clone(),
                };
            }
            match arguments {
                None => Outcome::Pass,
                Some(expected) => check_arguments(tool, expected, &call.arguments_json),
            }
        }
        Expectation::Sequence {
            tools,
            min_properties_on,
        } => {
            let actual: Vec<String> = output.calls.iter().map(|c| c.name.clone()).collect();
            if output.calls.is_empty() {
                // A sequence that saw nothing at all is the no-tool-call
                // failure, not a mis-ordering — same opposite-fixes reason.
                return Outcome::NoToolCall {
                    expected: tools.join(" -> "),
                    text: output.text.clone(),
                };
            }
            if !is_subsequence(tools, &actual) {
                return Outcome::BadSequence {
                    expected: tools.clone(),
                    actual,
                };
            }
            for (tool, min) in min_properties_on {
                if let Some(detail) = properties_shortfall(output, tool, *min) {
                    return Outcome::BadArguments {
                        tool: tool.clone(),
                        detail,
                    };
                }
            }
            Outcome::Pass
        }
        Expectation::Clarify => match output.calls.first() {
            Some(call) if call.name == CLARIFY_TOOL => Outcome::Pass,
            Some(call) => Outcome::WrongTool {
                expected: CLARIFY_TOOL.to_string(),
                actual: call.name.clone(),
            },
            None => Outcome::NoToolCall {
                expected: CLARIFY_TOOL.to_string(),
                text: output.text.clone(),
            },
        },
        Expectation::Text { contains } => {
            if let Some(call) = output.calls.first() {
                return Outcome::BadText {
                    detail: format!("expected a text reply, but the model called {}", call.name),
                };
            }
            let haystack = output.text.to_lowercase();
            for needle in contains {
                if !haystack.contains(&needle.to_lowercase()) {
                    return Outcome::BadText {
                        detail: format!(
                            "reply does not contain {needle:?}; got: {}",
                            truncate(&output.text, 200)
                        ),
                    };
                }
            }
            Outcome::Pass
        }
        Expectation::Observe => Outcome::Observed {
            summary: summarize(output),
        },
    }
}

fn summarize(output: &TurnOutput) -> String {
    if output.calls.is_empty() {
        return format!("no tool call; text: {}", truncate(&output.text, 200));
    }
    output
        .calls
        .iter()
        .map(|c| format!("{}({})", c.name, truncate(&c.arguments_json, 300)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Count the populated `properties` keys on the last call to `tool`.
///
/// Returns a description of the shortfall, or `None` when the bar is met.
/// The *last* matching call is the one checked: a model that retries a write
/// should be judged on the call it settled on.
fn properties_shortfall(output: &TurnOutput, tool: &str, min: usize) -> Option<String> {
    let call = output.calls.iter().rev().find(|c| c.name == tool)?;
    let parsed: serde_json::Value = match serde_json::from_str(&call.arguments_json) {
        Ok(v) => v,
        Err(e) => {
            return Some(format!(
                "arguments are not valid JSON ({e}), so the write carried no properties"
            ))
        }
    };
    let count = parsed
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|o| o.iter().filter(|(_, v)| !v.is_null()).count())
        .unwrap_or(0);
    if count >= min {
        None
    } else {
        Some(format!(
            "expected at least {min} populated properties, the call carried {count} — \
             the tool name appeared but the write was empty"
        ))
    }
}

/// Whether `needles` appears in `haystack` in order, allowing other calls
/// between them.
fn is_subsequence(needles: &[String], haystack: &[String]) -> bool {
    let mut iter = haystack.iter();
    needles.iter().all(|n| iter.any(|h| h == n))
}

fn check_arguments(tool: &str, expected: &toml::Value, arguments_json: &str) -> Outcome {
    let expected_json = match serde_json::to_value(expected) {
        Ok(v) => v,
        Err(e) => {
            return Outcome::BadArguments {
                tool: tool.to_string(),
                detail: format!("the case's `arguments` table is not convertible to JSON: {e}"),
            }
        }
    };
    let actual: serde_json::Value = match serde_json::from_str(arguments_json) {
        Ok(v) => v,
        Err(e) => {
            return Outcome::BadArguments {
                tool: tool.to_string(),
                detail: format!("the model's arguments are not valid JSON ({e}): {arguments_json}"),
            }
        }
    };
    match subset_mismatch(&expected_json, &actual, "") {
        None => Outcome::Pass,
        Some(detail) => Outcome::BadArguments {
            tool: tool.to_string(),
            detail,
        },
    }
}

/// Recursive subset match: every key/value in `expected` must be present and
/// equal in `actual`. Extra keys in `actual` are fine — a case asserting one
/// argument should not break because the model correctly filled another.
///
/// Arrays are compared element-wise with the same subset rule per element and
/// require equal length, since an array's length is usually the assertion.
fn subset_mismatch(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    path: &str,
) -> Option<String> {
    use serde_json::Value;
    let here = if path.is_empty() { "<root>" } else { path };
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (key, want) in e {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match a.get(key) {
                    None => return Some(format!("missing key `{child_path}` (expected {want})")),
                    Some(got) => {
                        if let Some(m) = subset_mismatch(want, got, &child_path) {
                            return Some(m);
                        }
                    }
                }
            }
            None
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Some(format!(
                    "`{here}` has {} elements, expected {}",
                    a.len(),
                    e.len()
                ));
            }
            for (i, (want, got)) in e.iter().zip(a).enumerate() {
                if let Some(m) = subset_mismatch(want, got, &format!("{here}[{i}]")) {
                    return Some(m);
                }
            }
            None
        }
        // Numbers cross the TOML→JSON boundary as integers or floats
        // depending on how they were written, so `2400` in a case file must
        // match `2400.0` from the model. Every other type compares directly.
        (Value::Number(e), Value::Number(a)) => match (e.as_f64(), a.as_f64()) {
            (Some(x), Some(y)) if (x - y).abs() < f64::EPSILON => None,
            _ => Some(format!("`{here}` is {actual}, expected {expected}")),
        },
        _ => {
            if expected == actual {
                None
            } else {
                Some(format!("`{here}` is {actual}, expected {expected}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: &str) -> ObservedCall {
        ObservedCall {
            name: name.to_string(),
            arguments_json: args.to_string(),
        }
    }

    fn output(calls: Vec<ObservedCall>, text: &str) -> TurnOutput {
        TurnOutput {
            text: text.to_string(),
            calls,
        }
    }

    const MINIMAL_CASE: &str = r#"
reps = 2

[[turn]]
system = "You are a test assistant."
user = "do the thing"

  [[turn.tool]]
  name = "do_thing"
  description = "Does the thing."
  parameters_schema = { type = "object", properties = { what = { type = "string" } } }

  [turn.expect]
  kind = "tool"
  tool = "do_thing"
"#;

    #[test]
    fn parses_a_minimal_case_and_defaults_its_name_to_the_file_stem() {
        let case = GoldenCase::from_toml(MINIMAL_CASE, "my-case").expect("case must parse");
        assert_eq!(case.name, "my-case");
        assert_eq!(case.reps, 2);
        assert_eq!(case.turns.len(), 1);
        assert_eq!(case.turn_label(0), "turn1");
    }

    #[test]
    fn reps_defaults_to_three_when_omitted() {
        let text = MINIMAL_CASE.replace("reps = 2\n", "");
        let case = GoldenCase::from_toml(&text, "c").expect("case must parse");
        assert_eq!(
            case.reps, DEFAULT_REPS,
            "a case that omits reps must still run more than once — single runs are not \
             decision-grade"
        );
    }

    #[test]
    fn turns_chain_by_default_and_can_be_isolated_explicitly() {
        let case = GoldenCase::from_toml(MINIMAL_CASE, "c").expect("case must parse");
        assert!(
            case.turns[0].chain,
            "the sequence case is the common one, so chaining is the default"
        );

        let isolated = MINIMAL_CASE.replace("[[turn]]", "[[turn]]\nchain = false");
        let case = GoldenCase::from_toml(&isolated, "c").expect("case must parse");
        assert!(!case.turns[0].chain);
    }

    #[test]
    fn tool_schema_round_trips_from_toml_to_json() {
        let case = GoldenCase::from_toml(MINIMAL_CASE, "c").expect("case must parse");
        let def = case.turns[0].tools[0]
            .to_tool_definition()
            .expect("schema must convert");
        assert_eq!(def.name, "do_thing");
        assert_eq!(def.parameters_schema["type"], "object");
        assert_eq!(
            def.parameters_schema["properties"]["what"]["type"],
            "string"
        );
    }

    #[test]
    fn rejects_a_case_with_no_turns() {
        let err = GoldenCase::from_toml("reps = 1\n", "c").expect_err("must reject");
        // `turns` is a required field, so this fails at parse rather than in
        // validate() — either way the case is refused before any model load.
        assert!(matches!(err, CaseError::Parse(_) | CaseError::Invalid(_)));
    }

    #[test]
    fn rejects_zero_reps() {
        let text = MINIMAL_CASE.replace("reps = 2", "reps = 0");
        let err = GoldenCase::from_toml(&text, "c").expect_err("must reject");
        assert!(matches!(err, CaseError::Invalid(m) if m.contains("reps")));
    }

    #[test]
    fn rejects_an_expectation_naming_a_tool_the_turn_never_offers() {
        let text = MINIMAL_CASE.replace(r#"tool = "do_thing""#, r#"tool = "not_offered""#);
        let err = GoldenCase::from_toml(&text, "c").expect_err("must reject");
        assert!(
            matches!(&err, CaseError::Invalid(m) if m.contains("not_offered")),
            "got {err}"
        );
    }

    #[test]
    fn rejects_min_properties_on_a_tool_outside_the_sequence() {
        let text = r#"
[[turn]]
system = "s"
user = "u"

  [[turn.tool]]
  name = "a"
  description = "d"

  [[turn.tool]]
  name = "b"
  description = "d"

  [turn.expect]
  kind = "sequence"
  tools = ["a", "b"]
  min_properties_on = { c = 1 }
"#;
        let err = GoldenCase::from_toml(text, "c").expect_err("must reject");
        assert!(matches!(&err, CaseError::Invalid(m) if m.contains("min_properties_on")));
    }

    #[test]
    fn rejects_an_unknown_field_rather_than_silently_ignoring_it() {
        // A typo'd key in a case file would otherwise read as "the tuning
        // change had no effect", which is the worst possible failure mode for
        // an instrument whose whole job is measuring prompt edits.
        let text = MINIMAL_CASE.replace("reps = 2", "reps = 2\nrepetitions = 5");
        let err = GoldenCase::from_toml(&text, "c").expect_err("must reject");
        assert!(matches!(&err, CaseError::Parse(m) if m.contains("repetitions")));
    }

    #[test]
    fn tool_expectation_passes_on_the_named_tool() {
        let expect = Expectation::Tool {
            tool: "resolve_query".into(),
            arguments: None,
        };
        let out = output(vec![call("resolve_query", "{}")], "");
        assert_eq!(evaluate(&expect, &out), Outcome::Pass);
    }

    #[test]
    fn wrong_tool_and_no_tool_call_are_distinct_outcomes() {
        let expect = Expectation::Tool {
            tool: "resolve_query".into(),
            arguments: None,
        };

        let wrong = evaluate(&expect, &output(vec![call("search_nodes", "{}")], ""));
        assert!(
            matches!(&wrong, Outcome::WrongTool { actual, .. } if actual == "search_nodes"),
            "got {wrong}"
        );
        assert_eq!(wrong.tag(), "wrong-tool");

        let none = evaluate(
            &expect,
            &output(vec![], "Could you give me the node id please?"),
        );
        assert!(
            matches!(&none, Outcome::NoToolCall { text, .. } if text.contains("node id")),
            "got {none}"
        );
        assert_eq!(none.tag(), "no-tool-call");

        assert_ne!(
            wrong.tag(),
            none.tag(),
            "the two have opposite fixes and must never collapse into one count"
        );
    }

    #[test]
    fn argument_assertions_are_subset_matches_not_full_blob_equality() {
        let mut expected = toml::map::Map::new();
        expected.insert(
            "node_type".into(),
            toml::Value::String("equipment_checkout_record".into()),
        );
        let expect = Expectation::Tool {
            tool: "resolve_query".into(),
            arguments: Some(toml::Value::Table(expected)),
        };

        let out = output(
            vec![call(
                "resolve_query",
                r#"{"node_type":"equipment_checkout_record","request":"the 2400 one","extra":1}"#,
            )],
            "",
        );
        assert_eq!(
            evaluate(&expect, &out),
            Outcome::Pass,
            "unasserted arguments the model filled correctly must not fail the case"
        );
    }

    #[test]
    fn argument_assertion_reports_the_mismatching_path() {
        let mut props = toml::map::Map::new();
        props.insert("status".into(), toml::Value::String("returned".into()));
        let mut expected = toml::map::Map::new();
        expected.insert("properties".into(), toml::Value::Table(props));
        let expect = Expectation::Tool {
            tool: "update_node".into(),
            arguments: Some(toml::Value::Table(expected)),
        };

        let out = output(
            vec![call(
                "update_node",
                r#"{"properties":{"status":"checked_out"}}"#,
            )],
            "",
        );
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(&outcome, Outcome::BadArguments { detail, .. }
                if detail.contains("properties.status")),
            "got {outcome}"
        );
    }

    #[test]
    fn argument_assertion_matches_an_integer_against_a_float() {
        // TOML writes 2400; the model may emit 2400.0. Same number.
        let mut expected = toml::map::Map::new();
        expected.insert("replacementCost".into(), toml::Value::Integer(2400));
        let expect = Expectation::Tool {
            tool: "create_node".into(),
            arguments: Some(toml::Value::Table(expected)),
        };
        let out = output(
            vec![call("create_node", r#"{"replacementCost":2400.0}"#)],
            "",
        );
        assert_eq!(evaluate(&expect, &out), Outcome::Pass);
    }

    #[test]
    fn argument_assertion_flags_a_missing_key() {
        let mut expected = toml::map::Map::new();
        expected.insert("node_type".into(), toml::Value::String("task".into()));
        let expect = Expectation::Tool {
            tool: "create_node".into(),
            arguments: Some(toml::Value::Table(expected)),
        };
        let out = output(vec![call("create_node", r#"{"properties":{}}"#)], "");
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(&outcome, Outcome::BadArguments { detail, .. } if detail.contains("node_type")),
            "got {outcome}"
        );
    }

    #[test]
    fn sequence_tolerates_extra_calls_between_the_named_ones() {
        let expect = Expectation::Sequence {
            tools: vec!["resolve_query".into(), "update_node".into()],
            min_properties_on: BTreeMap::new(),
        };
        let out = output(
            vec![
                call("resolve_query", "{}"),
                call("search_nodes", "{}"),
                call("update_node", "{}"),
            ],
            "",
        );
        assert_eq!(evaluate(&expect, &out), Outcome::Pass);
    }

    #[test]
    fn sequence_fails_when_the_order_is_reversed() {
        let expect = Expectation::Sequence {
            tools: vec!["resolve_query".into(), "update_node".into()],
            min_properties_on: BTreeMap::new(),
        };
        let out = output(
            vec![call("update_node", "{}"), call("resolve_query", "{}")],
            "",
        );
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(outcome, Outcome::BadSequence { .. }),
            "got {outcome}"
        );
    }

    #[test]
    fn sequence_with_no_calls_at_all_reports_no_tool_call() {
        let expect = Expectation::Sequence {
            tools: vec!["resolve_query".into()],
            min_properties_on: BTreeMap::new(),
        };
        let outcome = evaluate(&expect, &output(vec![], "I need the node id."));
        assert_eq!(outcome.tag(), "no-tool-call", "got {outcome}");
    }

    #[test]
    fn min_properties_on_catches_a_write_that_carried_nothing() {
        let mut min = BTreeMap::new();
        min.insert("update_node".to_string(), 1usize);
        let expect = Expectation::Sequence {
            tools: vec!["resolve_query".into(), "update_node".into()],
            min_properties_on: min,
        };

        let empty = output(
            vec![
                call("resolve_query", "{}"),
                call("update_node", r#"{"id":"abc","properties":{}}"#),
            ],
            "",
        );
        let outcome = evaluate(&expect, &empty);
        assert!(
            matches!(&outcome, Outcome::BadArguments { detail, .. } if detail.contains("carried 0")),
            "the tool name appearing is not the same as the write landing; got {outcome}"
        );

        let populated = output(
            vec![
                call("resolve_query", "{}"),
                call(
                    "update_node",
                    r#"{"id":"abc","properties":{"status":"returned"}}"#,
                ),
            ],
            "",
        );
        assert_eq!(evaluate(&expect, &populated), Outcome::Pass);
    }

    #[test]
    fn min_properties_ignores_null_valued_keys() {
        let mut min = BTreeMap::new();
        min.insert("update_node".to_string(), 1usize);
        let expect = Expectation::Sequence {
            tools: vec!["update_node".into()],
            min_properties_on: min,
        };
        let out = output(
            vec![call("update_node", r#"{"properties":{"status":null}}"#)],
            "",
        );
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(outcome, Outcome::BadArguments { .. }),
            "a null-valued key is not a carried property"
        );
    }

    #[test]
    fn min_properties_checks_the_last_matching_call() {
        let mut min = BTreeMap::new();
        min.insert("update_node".to_string(), 1usize);
        let expect = Expectation::Sequence {
            tools: vec!["update_node".into()],
            min_properties_on: min,
        };
        let out = output(
            vec![
                call("update_node", r#"{"properties":{}}"#),
                call("update_node", r#"{"properties":{"status":"returned"}}"#),
            ],
            "",
        );
        assert_eq!(
            evaluate(&expect, &out),
            Outcome::Pass,
            "a model that retried a write should be judged on the call it settled on"
        );
    }

    #[test]
    fn clarify_is_a_pass_not_a_failure_mode() {
        let out = output(
            vec![call(
                CLARIFY_TOOL,
                r#"{"question":"Which one?","options":["a","b"]}"#,
            )],
            "",
        );
        assert_eq!(evaluate(&Expectation::Clarify, &out), Outcome::Pass);
    }

    #[test]
    fn clarify_fails_when_the_model_guessed_instead_of_asking() {
        let out = output(vec![call("update_node", "{}")], "");
        let outcome = evaluate(&Expectation::Clarify, &out);
        assert!(
            matches!(&outcome, Outcome::WrongTool { expected, .. } if expected == CLARIFY_TOOL),
            "got {outcome}"
        );
    }

    #[test]
    fn text_expectation_fails_on_a_tool_call() {
        let expect = Expectation::Text { contains: vec![] };
        let out = output(vec![call("search_nodes", "{}")], "");
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(&outcome, Outcome::BadText { detail } if detail.contains("search_nodes")),
            "got {outcome}"
        );
    }

    #[test]
    fn text_expectation_matches_substrings_case_insensitively() {
        let expect = Expectation::Text {
            contains: vec!["Laser Cutter".into()],
        };
        let hit = output(vec![], "You have one item: the laser cutter.");
        assert_eq!(evaluate(&expect, &hit), Outcome::Pass);

        let miss = output(vec![], "You have one item.");
        assert!(matches!(evaluate(&expect, &miss), Outcome::BadText { .. }));
    }

    #[test]
    fn observe_never_fails_and_records_what_happened() {
        let with_call = evaluate(
            &Expectation::Observe,
            &output(vec![call("resolve_query", r#"{"request":"x"}"#)], ""),
        );
        assert!(with_call.is_pass());
        assert!(
            matches!(&with_call, Outcome::Observed { summary } if summary.contains("resolve_query")),
            "got {with_call}"
        );

        let without = evaluate(&Expectation::Observe, &output(vec![], "no thanks"));
        assert!(
            without.is_pass(),
            "an observe probe is exploratory — it records, it does not judge"
        );
    }

    #[test]
    fn malformed_model_arguments_are_reported_as_bad_arguments_not_a_panic() {
        let mut expected = toml::map::Map::new();
        expected.insert("id".into(), toml::Value::String("x".into()));
        let expect = Expectation::Tool {
            tool: "update_node".into(),
            arguments: Some(toml::Value::Table(expected)),
        };
        let out = output(vec![call("update_node", "{not json")], "");
        let outcome = evaluate(&expect, &out);
        assert!(
            matches!(&outcome, Outcome::BadArguments { detail, .. } if detail.contains("not valid JSON")),
            "got {outcome}"
        );
    }
}
