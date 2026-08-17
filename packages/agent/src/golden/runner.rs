//! Executes a [`GoldenCase`] against the real inference path.
//!
//! ## What is and is not in the path
//!
//! In: `LlamaChatInferenceEngine` → `nlp-engine::ChatEngine`, which applies
//! the model's own chat template, resolves `chat_format` (Gemma 4 →
//! `PEG_GEMMA4`), runs llama.cpp's tool-call parsing, and — via
//! [`repair_tool_call_arguments`] applied here at the same parse boundary
//! `agent_loop.rs` applies it — production's argument repairs.
//!
//! Out: the daemon, the database, gRPC, `LocalAgentLoop`, `PromptAssembler`,
//! routing, and the chat-node lifecycle. None of them touch what the model
//! sees, and all of them cost minutes per iteration.
//!
//! That split is the point. "Does this exact prompt text get the right tool
//! call?" is answerable in seconds here; "does the real pipeline assemble that
//! exact prompt?" is a separate, deterministic, zero-model-call question.
//!
//! The fidelity line is drawn at the *template*, not below it: tuning against
//! a hand-rolled llama.cpp call or `llama-server` HTTP would apply a different
//! template and a different parser. ADR-038's findings were gathered that way
//! on `llama-server` build 8660, and ADR-046 recorded that they did not
//! transfer through NodeSpace's own inference path.

use std::path::Path;
use std::sync::{Arc, Mutex};

use nodespace_nlp_engine::chat::{ChatConfig, ChatMessage, Role, ToolCallRaw};

use crate::agent_types::{
    ChatInferenceEngine, InferenceRequest, ModelFamily, StreamingChunk, ToolDefinition,
};
use crate::local_agent::agent_loop::repair_tool_call_arguments;
use crate::local_agent::inference::LlamaChatInferenceEngine;

use super::case::{
    evaluate, CaseError, GoldenCase, ObservedCall, Outcome, TurnOutput, DEFAULT_TOOL_RESULT,
};

/// The ADR-056 locked native model, at the path the existing goldens use.
pub fn default_model_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.nodespace/models/gemma-4-E4B-it-Q4_K_M.gguf")
}

/// Context window for the runner's engine. Matches the goldens', which is
/// well above what any single case needs — a case that overflows it would be
/// measuring truncation rather than the prompt.
pub const RUNNER_N_CTX: u32 = 32768;

/// Load the engine once per process. Loading is slow (a ~5GB GGUF plus Metal
/// kernel compilation), so every rep of every turn shares one.
pub fn load_engine(model_path: &Path) -> Result<LlamaChatInferenceEngine, CaseError> {
    let config = ChatConfig {
        n_ctx: RUNNER_N_CTX,
        default_temperature: 0.1,
        ..Default::default()
    };
    let path = model_path.to_string_lossy().to_string();
    LlamaChatInferenceEngine::load(&path, ModelFamily::Gemma4, config)
        .map_err(|e| CaseError::Io(format!("could not load model at {path}: {e}")))
}

/// One turn's result within one rep.
#[derive(Debug, Clone)]
pub struct TurnRun {
    /// The turn's label.
    pub label: String,
    /// What the model produced.
    pub output: TurnOutput,
    /// The verdict.
    pub outcome: Outcome,
}

/// One full pass through every turn of a case.
#[derive(Debug, Clone)]
pub struct RepRun {
    /// 1-based rep index.
    pub index: u32,
    /// Per-turn results, in order. Short of the full turn count when an
    /// earlier turn failed and the chain was cut.
    pub turns: Vec<TurnRun>,
}

impl RepRun {
    /// A rep passes only when every turn it ran passed *and* none were cut.
    pub fn is_pass(&self, expected_turns: usize) -> bool {
        self.turns.len() == expected_turns && self.turns.iter().all(|t| t.outcome.is_pass())
    }
}

/// The whole case, across all reps.
#[derive(Debug, Clone)]
pub struct CaseRun {
    /// The case's name.
    pub name: String,
    /// Each rep, in order.
    pub reps: Vec<RepRun>,
    /// How many turns the case declares.
    pub turn_count: usize,
}

impl CaseRun {
    /// How many reps passed end to end.
    pub fn passes(&self) -> usize {
        self.reps
            .iter()
            .filter(|r| r.is_pass(self.turn_count))
            .count()
    }

    /// Failure counts by [`Outcome::tag`], so the tally says *how* a case
    /// failed rather than only how often. `no-tool-call` and `wrong-tool`
    /// point at different layers and this is where that stays visible.
    pub fn failure_tags(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for rep in &self.reps {
            for turn in &rep.turns {
                if !turn.outcome.is_pass() {
                    *counts.entry(turn.outcome.tag().to_string()).or_default() += 1;
                }
            }
        }
        counts.into_iter().collect()
    }

    /// Whether the case as a whole is a pass: every rep passed.
    ///
    /// An `observe`-only case is N/N by construction — its outcomes are
    /// [`Outcome::Observed`], which counts as a pass — so it reports and
    /// never fails, exactly as intended.
    pub fn is_pass(&self) -> bool {
        !self.reps.is_empty() && self.passes() == self.reps.len()
    }
}

/// Run every rep of a case, printing per-rep outcomes as they complete.
///
/// Printing as it goes rather than at the end matters at this cadence: a
/// 3-rep multi-turn case is minutes of wall clock, and the first rep's
/// outcome is usually enough to tell whether an edit went the right way.
pub async fn run_case(engine: &LlamaChatInferenceEngine, case: &GoldenCase) -> CaseRun {
    let mut reps = Vec::with_capacity(case.reps as usize);
    for index in 1..=case.reps {
        let rep = run_rep(engine, case, index).await;
        reps.push(rep);
    }
    CaseRun {
        name: case.name.clone(),
        reps,
        turn_count: case.turns.len(),
    }
}

async fn run_rep(engine: &LlamaChatInferenceEngine, case: &GoldenCase, index: u32) -> RepRun {
    // Messages accumulated from earlier turns of THIS rep: each turn's real
    // user message, its real assistant output, and the tool results paired to
    // the calls it actually made. That is the chaining the sequence golden
    // left as a TODO — a later turn reads what the model really said, not a
    // hand-picked stand-in for it.
    let mut chained: Vec<ChatMessage> = Vec::new();
    let mut turns = Vec::new();

    for (i, turn) in case.turns.iter().enumerate() {
        let label = case.turn_label(i);
        let tools: Vec<ToolDefinition> = turn
            .tools
            .iter()
            .filter_map(|t| t.to_tool_definition().ok())
            .collect();

        let mut messages = vec![ChatMessage::text(Role::System, turn.system.clone())];
        messages.extend(turn.history.iter().map(|h| h.to_chat_message()));
        if turn.chain {
            messages.extend(chained.iter().cloned());
        }
        messages.push(ChatMessage::text(Role::User, turn.user.clone()));

        let output = generate(engine, messages, tools, case.temperature, case.max_tokens).await;
        let outcome = evaluate(&turn.expect, &output);

        println!(
            "[{}] rep {}/{} {}: {}",
            case.name, index, case.reps, label, outcome
        );

        let failed = !outcome.is_pass();
        turns.push(TurnRun {
            label,
            output: output.clone(),
            outcome,
        });

        // A failed turn invalidates every turn that reads its output: the
        // chain would feed them a history the case never described, and their
        // results would measure the recovery rather than the prompt. Cut the
        // rep and say so, rather than reporting downstream noise as data.
        //
        // Only turns that actually chain are cut. When every remaining turn
        // is isolated, nothing downstream depends on this one, and stopping
        // would discard independent arms over an unrelated failure.
        if failed {
            if cuts_the_chain(case, i) {
                println!(
                    "[{}] rep {}/{}: chain cut after {} — {} dependent turn(s) not run",
                    case.name,
                    index,
                    case.reps,
                    case.turn_label(i),
                    case.turns.len() - i - 1
                );
                break;
            }
            continue;
        }

        chained.push(ChatMessage::text(Role::User, turn.user.clone()));
        chained.push(assistant_turn(&output));
        for (call_index, call) in output.calls.iter().enumerate() {
            let result = turn
                .tool_results
                .get(&call.name)
                .cloned()
                .unwrap_or_else(|| DEFAULT_TOOL_RESULT.to_string());
            chained.push(tool_result_message(call_index, call, result));
        }
    }

    RepRun { index, turns }
}

/// Whether a failure at turn `index` should abandon the rest of the rep.
///
/// It should exactly when some later turn chains, because that turn would
/// otherwise read a history the case never described. When every later turn
/// is isolated they are independent arms of one experiment, and discarding
/// them over an unrelated arm's failure throws away the data the case exists
/// to collect.
fn cuts_the_chain(case: &GoldenCase, index: usize) -> bool {
    case.turns[index + 1..].iter().any(|t| t.chain)
}

/// Rebuild the assistant turn the model just produced, tool calls included.
///
/// Carrying the calls (rather than only the text) is what keeps the replayed
/// history well-formed: the template renders them into the assistant turn so
/// the `tool` messages that follow have a call to pair with. Orphaned tool
/// results destabilize generation on some models, which would show up as a
/// prompt finding that is really a harness artifact.
fn assistant_turn(output: &TurnOutput) -> ChatMessage {
    if output.calls.is_empty() {
        return ChatMessage::text(Role::Assistant, output.text.clone());
    }
    let calls = output
        .calls
        .iter()
        .enumerate()
        .map(|(i, c)| ToolCallRaw {
            id: call_id(i),
            function_name: c.name.clone(),
            arguments_json: c.arguments_json.clone(),
        })
        .collect();
    ChatMessage::assistant_with_tool_calls(output.text.clone(), calls)
}

/// A tool result paired to the call at `index` of the assistant turn above.
///
/// `tool_call_id` must match that call's id or the result is an orphan the
/// template cannot pair, which is the shape known to destabilize generation.
fn tool_result_message(index: usize, call: &ObservedCall, content: String) -> ChatMessage {
    ChatMessage {
        role: Role::Tool,
        content,
        tool_calls: Vec::new(),
        tool_call_id: Some(call_id(index)),
        name: Some(call.name.clone()),
        reasoning: None,
    }
}

/// Deterministic ids for the replayed calls. The engine's own ids are not
/// reused because they are only meaningful within the turn that produced
/// them, and a stable scheme keeps a dumped prompt diffable across reps.
fn call_id(index: usize) -> String {
    format!("call_{index}")
}

/// One generation, collected and parsed the way production parses it.
async fn generate(
    engine: &LlamaChatInferenceEngine,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    temperature: f32,
    max_tokens: u32,
) -> TurnOutput {
    let request = InferenceRequest {
        messages,
        tools: if tools.is_empty() { None } else { Some(tools) },
        temperature: Some(temperature),
        max_tokens: Some(max_tokens),
    };

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = chunks.clone();
    let generated = engine
        .generate(
            request,
            Box::new(move |c| {
                if let Ok(mut g) = sink.lock() {
                    g.push(c);
                }
            }),
        )
        .await;

    let collected = {
        let guard = chunks.lock().unwrap_or_else(|p| p.into_inner());
        guard.clone()
    };

    if let Err(e) = generated {
        // Surfaced as text rather than a panic so one bad rep does not
        // discard the reps that already ran — the tally is the deliverable.
        return TurnOutput {
            text: format!("<inference error: {e}>"),
            calls: Vec::new(),
        };
    }

    parse_output(&collected)
}

/// Accumulate streamed chunks into text plus repaired tool calls.
///
/// This mirrors `LocalAgentLoop::parse_chunks` followed by the
/// [`repair_tool_call_arguments`] pass at `agent_loop.rs`'s parse boundary.
/// The accumulation is duplicated rather than shared because `parse_chunks`
/// is a private associated function on the loop, and widening the loop's
/// surface to reach twenty lines of chunk folding would couple this runner to
/// a type it deliberately keeps out of the path. The repair itself is *not*
/// duplicated — it is the same public function production calls, which is the
/// part that would actually change the measurement if it drifted.
fn parse_output(chunks: &[StreamingChunk]) -> TurnOutput {
    let mut text = String::new();
    // (id, name, args) — a Vec, not a map, so emission order is preserved;
    // a sequence expectation is an assertion about that order.
    let mut pending: Vec<(String, String, String)> = Vec::new();

    for chunk in chunks {
        match chunk {
            StreamingChunk::Token { text: t } => text.push_str(t),
            StreamingChunk::ToolCallStart { id, name } => {
                pending.push((id.clone(), name.clone(), String::new()));
            }
            StreamingChunk::ToolCallArgs { id, args_json } => {
                if let Some(call) = pending.iter_mut().rev().find(|(cid, _, _)| cid == id) {
                    call.2.push_str(args_json);
                }
            }
            // Reasoning is intentionally dropped: it is not part of what a
            // case asserts, and folding it into `text` would make a `text`
            // expectation match against chain-of-thought.
            StreamingChunk::Reasoning { .. }
            | StreamingChunk::Done { .. }
            | StreamingChunk::Error { .. } => {}
        }
    }

    let calls = pending
        .into_iter()
        .map(|(_, name, mut arguments_json)| {
            repair_tool_call_arguments(&mut arguments_json);
            ObservedCall {
                name,
                arguments_json,
            }
        })
        .collect();

    TurnOutput { text, calls }
}

/// Render the N-of-N report for a finished case.
pub fn render_report(run: &CaseRun) -> String {
    let mut out = String::new();
    let total = run.reps.len();
    let passes = run.passes();
    out.push_str(&format!(
        "\n{}: {}/{} reps passed\n",
        run.name, passes, total
    ));

    for rep in &run.reps {
        let verdict = if rep.is_pass(run.turn_count) {
            "pass"
        } else {
            "FAIL"
        };
        out.push_str(&format!("  rep {} [{verdict}]\n", rep.index));
        for turn in &rep.turns {
            out.push_str(&format!("    {}: {}\n", turn.label, turn.outcome));
        }
        let cut = run.turn_count - rep.turns.len();
        if cut > 0 {
            out.push_str(&format!("    ({cut} later turn(s) not run — chain cut)\n"));
        }
    }

    let tags = run.failure_tags();
    if !tags.is_empty() {
        let summary: Vec<String> = tags.iter().map(|(t, n)| format!("{t}={n}")).collect();
        out.push_str(&format!("  failures by kind: {}\n", summary.join(" ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_types::InferenceUsage;
    use crate::golden::case::Expectation;

    fn tool_start(id: &str, name: &str) -> StreamingChunk {
        StreamingChunk::ToolCallStart {
            id: id.into(),
            name: name.into(),
        }
    }

    fn tool_args(id: &str, args: &str) -> StreamingChunk {
        StreamingChunk::ToolCallArgs {
            id: id.into(),
            args_json: args.into(),
        }
    }

    #[test]
    fn parse_output_reassembles_streamed_argument_fragments() {
        let chunks = vec![
            tool_start("a", "resolve_query"),
            tool_args("a", r#"{"request":"#),
            tool_args("a", r#""the 2400 one"}"#),
            StreamingChunk::Done {
                usage: InferenceUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                },
            },
        ];
        let out = parse_output(&chunks);
        assert_eq!(out.calls.len(), 1);
        assert_eq!(out.calls[0].name, "resolve_query");
        assert_eq!(out.calls[0].arguments_json, r#"{"request":"the 2400 one"}"#);
    }

    #[test]
    fn parse_output_preserves_emission_order_across_interleaved_calls() {
        let chunks = vec![
            tool_start("a", "resolve_query"),
            tool_start("b", "update_node"),
            tool_args("b", r#"{"id":"x"}"#),
            tool_args("a", r#"{"request":"y"}"#),
        ];
        let out = parse_output(&chunks);
        let names: Vec<&str> = out.calls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["resolve_query", "update_node"],
            "a sequence expectation asserts emission order, so it must survive interleaving"
        );
        assert_eq!(out.calls[1].arguments_json, r#"{"id":"x"}"#);
    }

    #[test]
    fn parse_output_applies_productions_argument_repairs() {
        // Over-quoted keys: the model wrote `"\"name\""` where `name` was
        // meant. Production repairs this at the parse boundary, so the runner
        // must measure the repaired call — otherwise a case reports a failure
        // no real turn would have had, and the prompt gets "fixed" for a
        // defect that lives in the parser.
        let chunks = vec![
            tool_start("a", "create_schema"),
            tool_args(
                "a",
                r#"{"fields":[{"\"name\"":"status","\"type\"":"text"}]}"#,
            ),
        ];
        let out = parse_output(&chunks);
        let parsed: serde_json::Value =
            serde_json::from_str(&out.calls[0].arguments_json).expect("repaired args parse");
        assert_eq!(
            parsed["fields"][0]["name"], "status",
            "the runner must apply the same repairs agent_loop.rs applies, or it measures \
             something no production turn would see"
        );
        assert_eq!(parsed["fields"][0]["type"], "text");
    }

    #[test]
    fn parse_output_leaves_a_well_formed_call_byte_identical() {
        // The repair pass rewrites only when it changed something, so a clean
        // call must not be silently re-serialised into serde's key order —
        // a case asserting on raw argument text would otherwise drift.
        let raw = r#"{"request":"the 2400 one","node_type":"equipment"}"#;
        let out = parse_output(&[tool_start("a", "resolve_query"), tool_args("a", raw)]);
        assert_eq!(out.calls[0].arguments_json, raw);
    }

    #[test]
    fn parse_output_keeps_reasoning_out_of_the_text_a_text_expectation_matches() {
        let chunks = vec![
            StreamingChunk::Reasoning {
                text: "I should call update_node".into(),
            },
            StreamingChunk::Token {
                text: "Which item did you mean?".into(),
            },
        ];
        let out = parse_output(&chunks);
        assert_eq!(out.text, "Which item did you mean?");
    }

    #[test]
    fn assistant_turn_carries_the_tool_calls_into_replayed_history() {
        let output = TurnOutput {
            text: String::new(),
            calls: vec![ObservedCall {
                name: "resolve_query".into(),
                arguments_json: r#"{"request":"x"}"#.into(),
            }],
        };
        let msg = assistant_turn(&output);
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].function_name, "resolve_query");
        assert_eq!(
            msg.tool_calls[0].id,
            call_id(0),
            "ids must pair with the tool result messages that follow"
        );
    }

    #[test]
    fn a_replayed_tool_result_pairs_with_its_originating_call() {
        let output = TurnOutput {
            text: String::new(),
            calls: vec![
                ObservedCall {
                    name: "resolve_query".into(),
                    arguments_json: "{}".into(),
                },
                ObservedCall {
                    name: "update_node".into(),
                    arguments_json: "{}".into(),
                },
            ],
        };
        let assistant = assistant_turn(&output);
        let second = tool_result_message(1, &output.calls[1], "{}".into());
        assert_eq!(
            second.tool_call_id.as_deref(),
            Some(assistant.tool_calls[1].id.as_str()),
            "an unpaired tool result is a harness artifact that would read as a prompt finding"
        );
        assert_eq!(second.name.as_deref(), Some("update_node"));
    }

    fn case_with_chain_flags(flags: &[bool]) -> GoldenCase {
        let turns: String = flags
            .iter()
            .map(|chain| {
                format!(
                    r#"
[[turn]]
chain = {chain}
system = "s"
user = "u"
  [[turn.tool]]
  name = "t"
  description = "d"
  [turn.expect]
  kind = "tool"
  tool = "t"
"#
                )
            })
            .collect();
        GoldenCase::from_toml(&turns, "c").expect("case must parse")
    }

    #[test]
    fn a_failure_cuts_the_rep_only_when_a_later_turn_reads_its_output() {
        let chained = case_with_chain_flags(&[true, true]);
        assert!(
            cuts_the_chain(&chained, 0),
            "turn 2 reads turn 1's output, so turn 1 failing makes turn 2's result meaningless"
        );

        let arms = case_with_chain_flags(&[false, false, false]);
        assert!(
            !cuts_the_chain(&arms, 0),
            "independent arms must not be discarded because a sibling arm failed"
        );

        let mixed = case_with_chain_flags(&[false, true]);
        assert!(cuts_the_chain(&mixed, 0));

        let last = case_with_chain_flags(&[true, true]);
        assert!(
            !cuts_the_chain(&last, 1),
            "nothing follows the last turn, so there is no chain to cut"
        );
    }

    #[test]
    fn a_rep_missing_turns_is_not_a_pass() {
        // The chain-cut case: turn 1 passed, turn 2 never ran. Counting that
        // as a pass because "everything that ran passed" would report a
        // two-turn case as green off one turn's evidence.
        let rep = RepRun {
            index: 1,
            turns: vec![TurnRun {
                label: "turn1".into(),
                output: TurnOutput::default(),
                outcome: Outcome::Pass,
            }],
        };
        assert!(rep.is_pass(1));
        assert!(!rep.is_pass(2));
    }

    #[test]
    fn failure_tags_break_the_tally_down_by_kind() {
        let run = CaseRun {
            name: "c".into(),
            turn_count: 1,
            reps: vec![
                RepRun {
                    index: 1,
                    turns: vec![TurnRun {
                        label: "turn1".into(),
                        output: TurnOutput::default(),
                        outcome: Outcome::NoToolCall {
                            expected: "resolve_query".into(),
                            text: "give me an id".into(),
                        },
                    }],
                },
                RepRun {
                    index: 2,
                    turns: vec![TurnRun {
                        label: "turn1".into(),
                        output: TurnOutput::default(),
                        outcome: Outcome::WrongTool {
                            expected: "resolve_query".into(),
                            actual: "search_nodes".into(),
                        },
                    }],
                },
                RepRun {
                    index: 3,
                    turns: vec![TurnRun {
                        label: "turn1".into(),
                        output: TurnOutput::default(),
                        outcome: Outcome::Pass,
                    }],
                },
            ],
        };
        assert_eq!(run.passes(), 1);
        assert!(!run.is_pass());
        assert_eq!(
            run.failure_tags(),
            vec![
                ("no-tool-call".to_string(), 1),
                ("wrong-tool".to_string(), 1)
            ],
            "two failures with opposite fixes must not collapse into '2 failed'"
        );
    }

    #[test]
    fn an_observe_only_case_reports_and_never_fails() {
        let run = CaseRun {
            name: "probe".into(),
            turn_count: 1,
            reps: vec![RepRun {
                index: 1,
                turns: vec![TurnRun {
                    label: "turn1".into(),
                    output: TurnOutput::default(),
                    outcome: Outcome::Observed {
                        summary: "no tool call".into(),
                    },
                }],
            }],
        };
        assert!(run.is_pass());
        assert!(run.failure_tags().is_empty());
    }

    #[test]
    fn render_report_states_the_tally_and_the_failure_kinds() {
        let run = CaseRun {
            name: "scenario6".into(),
            turn_count: 1,
            reps: vec![RepRun {
                index: 1,
                turns: vec![TurnRun {
                    label: "turn1".into(),
                    output: TurnOutput::default(),
                    outcome: Outcome::WrongTool {
                        expected: "resolve_query".into(),
                        actual: "search_nodes".into(),
                    },
                }],
            }],
        };
        let text = render_report(&run);
        assert!(text.contains("0/1 reps passed"), "{text}");
        assert!(text.contains("wrong-tool=1"), "{text}");
    }

    #[test]
    fn expectation_and_runner_agree_on_what_a_clarify_pass_looks_like() {
        // Guards the seam between the two modules: the runner produces
        // ObservedCall names straight from the stream, and Clarify matches on
        // the routing constant. A rename on either side breaks here.
        let chunks = vec![tool_start("a", crate::golden::case::CLARIFY_TOOL)];
        let out = parse_output(&chunks);
        assert_eq!(evaluate(&Expectation::Clarify, &out), Outcome::Pass);
    }
}
