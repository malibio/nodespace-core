//! Two-stage skill-mediated routing (ADR-038).
//!
//! The model reaches capabilities through skills discovered by semantic
//! retrieval, with the LLM judging at two gates and an explicit clarification
//! fallback. Retrieval itself is a deterministic system step the model does
//! not perform, which is where the candidate count is bounded and the trust
//! filter applies.
//!
//! ```text
//! Stage 1 (model):  emit EITHER a search query OR a clarification request
//!                   — a structural choice, not a confidence number
//! [system step]:    semantic retrieval over the skill registry → top-K,
//!                   score-gated. NOT a model tool call.
//! Stage 2 (model):  judge the candidates against the intent → pick one and
//!                   act, or clarify with the candidates as concrete options
//! ```
//!
//! ADR-038 rejects the single-turn pull (handing the model a `search_skills`
//! tool and letting it call retrieval itself), because that removes the
//! system's ability to bound K and enforce the trust boundary.

use crate::agent_types::{SkillCandidate, ToolDefinition};
use serde::Deserialize;

use super::tools::Tool;

/// How many skill candidates retrieval may return.
///
/// This is the bound ADR-038 requires the system — not the model — to own.
/// Three is enough to offer a real choice at Stage 2 (and to name concrete
/// options in a clarification) while keeping the injected instruction payload
/// small, since each candidate carries its full instruction subtree.
pub const RETRIEVAL_TOP_K: usize = 3;

/// Minimum retrieval score for a **read-only** skill to be actionable.
///
/// The mechanical half of the Stage-2 gate. The model's yes/no on the
/// candidate is the other, independently-sourced half; both must pass.
///
/// **Deliberately permissive, and not yet tuned.** Tuning requires an eval
/// that can distinguish a model failure from a harness failure, which the
/// agent matrix currently cannot do. Until that lands, a stricter value would
/// be tightening against an untested target: it would silently hide the long
/// tail, and no measurement would show the loss. Raise this only alongside a
/// measurement that demonstrates the tail being hidden is genuinely noise.
///
/// Note this is a different constant from `skill_ops::SKILL_SEARCH_THRESHOLD`,
/// which stays at `0.0` on purpose: retrieval still *returns* the long tail so
/// the judged gate can see it, and this bar decides what is *actionable*.
/// One gate filters, the other judges.
pub const READ_SKILL_SCORE_BAR: f32 = 0.15;

/// Minimum retrieval score for a **mutating** skill to be actionable.
///
/// ADR-038: "the Stage-2 bar is a property of the matched skill, not a single
/// global constant. Mutating skills warrant a higher bar than read-only ones,
/// because the expensive error — firing the wrong *mutating* tool — changes
/// the graph." Bias the gate against that error.
///
/// **Deliberately permissive, and not yet tuned** — see
/// [`READ_SKILL_SCORE_BAR`]. The ordering (mutating strictly above read-only)
/// is the load-bearing property here; the absolute values are placeholders
/// awaiting a diagnostic eval.
pub const MUTATING_SKILL_SCORE_BAR: f32 = 0.30;

/// Stage-1's structural choice, recovered from which tool the model called.
///
/// ADR-038 rejects gating on a self-reported confidence number: a numeric
/// self-rating from a small model is not calibrated and invites anchoring.
/// The choice is therefore *which of two typed tools* the model calls, which
/// is also the channel measured strongest for structured output — a tool
/// schema, rather than prose the model must be trusted to follow.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// The model formed a search query; run retrieval on it.
    Query(String),
    /// The model could not form a query and asked to clarify.
    Clarify {
        /// The specific question to put to the user.
        question: String,
        /// Concrete options to offer. ADR-038: a bare "what do you mean?" is
        /// the failure mode to avoid.
        options: Vec<String>,
    },
}

/// Wire name of the Stage-1 tool that emits a search query.
pub const ROUTE_QUERY_TOOL: &str = "route_query";
/// Wire name of the Stage-1 tool that requests clarification.
pub const ROUTE_CLARIFY_TOOL: &str = "route_clarify";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteQueryParams {
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteClarifyParams {
    question: String,
    #[serde(default)]
    options: Vec<String>,
}

/// The two tools offered at Stage 1, and only these two.
///
/// Offering exactly two makes the model's tool choice a discriminated output:
/// there is no third thing it can call, and no free text to parse. The
/// alternative — asking in prose for a `QUERY:`/`CLARIFY:` prefix — relies on
/// the weakest measured channel and needs a parser whose failures are
/// indistinguishable from model failures.
pub fn stage1_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: ROUTE_QUERY_TOOL.to_string(),
            description: "Use when you understand what the user wants. Provide a short search \
                 query describing the capability needed to fulfil it — describe the task, not \
                 the user's exact words."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A short description of the capability needed, in plain language."
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: ROUTE_CLARIFY_TOOL.to_string(),
            description: "Use ONLY when the request is too ambiguous to describe as a capability. \
                 Ask one specific question and offer concrete alternatives — never a bare \
                 'what do you mean?'."
                .to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "One specific question that would resolve the ambiguity."
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Two or more concrete interpretations the user can pick between."
                    }
                },
                "required": ["question"]
            }),
        },
    ]
}

/// Recover Stage-1's decision from the model's tool call.
///
/// Returns `None` when the model called neither routing tool or emitted
/// unparseable arguments. The caller treats that as "no routing decision" and
/// falls through to the general tool surface rather than failing the turn:
/// a routing step that cannot decide must not cost the user their request.
pub fn parse_route_decision(tool_name: &str, arguments_json: &str) -> Option<RouteDecision> {
    match tool_name {
        ROUTE_QUERY_TOOL => {
            let p: RouteQueryParams = serde_json::from_str(arguments_json).ok()?;
            let q = p.query.trim();
            if q.is_empty() {
                return None;
            }
            Some(RouteDecision::Query(q.to_string()))
        }
        ROUTE_CLARIFY_TOOL => {
            let p: RouteClarifyParams = serde_json::from_str(arguments_json).ok()?;
            let question = p.question.trim();
            if question.is_empty() {
                return None;
            }
            Some(RouteDecision::Clarify {
                question: question.to_string(),
                options: p.options,
            })
        }
        _ => None,
    }
}

/// Whether a skill can change graph state, derived from the tools it may fire.
///
/// Blast radius is not a stored property. It is computed from the skill's
/// `tool_whitelist` through the tool registry's own write classification, so
/// it cannot drift from what the skill is actually able to do — adding a
/// mutating tool to a whitelist raises that skill's bar automatically, with
/// no second field to keep in sync.
///
/// An unrecognised tool name counts as mutating. Resolving a name that is not
/// in the registry — a typo, a renamed tool, or a future externally-registered
/// one — yields no write classification at all, and treating that absence as
/// read-only would put the *lower* bar on a skill whose blast radius is
/// unknown. ADR-038 says to bias against the expensive error, so the unknown
/// case belongs on the restrictive side.
///
/// This gates a safety bar, not availability. `stage2_tools` fails open in the
/// other direction on purpose: there, an unknown name simply is not offered,
/// and stranding the model with no tools would be the worse outcome.
pub fn skill_is_mutating(candidate: &SkillCandidate) -> bool {
    candidate
        .tools
        .iter()
        .any(|t| Tool::from_name(t).is_none_or(Tool::is_write))
}

/// The retrieval score a candidate must clear to be actionable.
///
/// Scales with blast radius per ADR-038.
pub fn score_bar_for(candidate: &SkillCandidate) -> f32 {
    if skill_is_mutating(candidate) {
        MUTATING_SKILL_SCORE_BAR
    } else {
        READ_SKILL_SCORE_BAR
    }
}

/// Whether a candidate clears the **mechanical** half of the Stage-2 gate.
///
/// Passing this is necessary, not sufficient: ADR-038 requires two
/// independently-sourced signals, and the model's judgment supplies the other.
/// A candidate that clears this bar is offered to the model to judge; one that
/// does not is never actionable regardless of what the model says.
pub fn clears_score_gate(candidate: &SkillCandidate) -> bool {
    candidate.score >= score_bar_for(candidate)
}

/// Names of the candidates that clear the score gate, comma-separated, for the
/// `routed_skills` log field.
///
/// The same set `render_candidates_for_prompt` writes into Stage 2's prompt and
/// `stage2_tools` scopes the tool surface from, so a log reader can tell which
/// skill a turn was actually routed to rather than only how many candidates
/// retrieval returned.
///
/// A function rather than an inline expression at the log site specifically so
/// the rendered text is testable. The scraper consuming this field parses to
/// end of line — skill names contain spaces and commas, and `tracing` emits the
/// value unquoted — so its exact shape is a contract, and the first version of
/// that scraper silently matched nothing because the shape was assumed rather
/// than asserted.
pub fn routed_skill_names(candidates: &[SkillCandidate]) -> String {
    candidates
        .iter()
        .filter(|c| clears_score_gate(c))
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render retrieved candidates for injection into the Stage-2 prompt.
///
/// Delivered **in the prompt** rather than as a tool result. ADR-064 rule 4
/// reserves tool results for resolved facts rather than procedures, and the
/// supporting measurement for skill instructions was taken on prompt-rendered
/// text; the same payload returned as a tool result was observed to suppress
/// tool-calling substantially.
///
/// Only candidates clearing the score gate are rendered — the mechanical gate
/// runs before the judged one, so the model is never asked to judge a
/// candidate the system has already ruled out.
pub fn render_candidates_for_prompt(candidates: &[SkillCandidate]) -> Option<String> {
    let eligible: Vec<&SkillCandidate> =
        candidates.iter().filter(|c| clears_score_gate(c)).collect();
    if eligible.is_empty() {
        return None;
    }

    // Phrased as a direct instruction to act, not to deliberate. An earlier
    // wording ("Pick the ONE that fits and carry out its instructions") framed
    // the turn as a selection exercise, and the model answered it in kind:
    // it narrated which tool it would use — in one case emitting the tool-call
    // JSON inside a code block — instead of calling it. Measured on
    // mistral:7b, that wording produced zero tool calls where the unrouted
    // control produced one.
    let mut out = String::from(
        "REFERENCE — procedures relevant to this request. Use whichever applies and IGNORE the \
         rest. Do not describe, quote, or summarise any of it. Your reply must be the tool call \
         itself. If none applies, answer the user normally.\n",
    );
    for (i, c) in eligible.iter().enumerate() {
        out.push_str(&format!("\n--- Candidate {}: {}\n", i + 1, c.name));
        if !c.description.is_empty() {
            out.push_str(&format!("Purpose: {}\n", c.description));
        }
        if !c.instructions.is_empty() {
            out.push_str(&format!("\n{}\n", c.instructions));
        }
        if let Some(meta) = render_schema_metadata(&c.schema_metadata) {
            out.push_str(&format!("\nRELEVANT ENTITY TYPES:\n{meta}\n"));
        }
    }
    Some(out)
}

/// Render a candidate's `schema_metadata` into the compact form the model
/// already sees elsewhere, or `None` when there is nothing to show.
fn render_schema_metadata(meta: &serde_json::Value) -> Option<String> {
    let arr = meta.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for entry in arr {
        // Skip a malformed entry rather than propagating out of the loop: `?`
        // here would discard every remaining type because one lacked an id.
        let Some(type_id) = entry.get("type_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let fields: Vec<String> = entry
            .get("fields")
            .and_then(|f| f.as_array())
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| {
                        let name = f.get("name").and_then(|v| v.as_str())?;
                        let ty = f.get("type").and_then(|v| v.as_str()).unwrap_or("text");
                        // Same `name: type` notation the workspace-context
                        // block uses. Both are emitted into a single system
                        // prompt under the same heading, so two spellings of
                        // one concept would read as two different things.
                        let mut descriptor = match f.get("enum_values").and_then(|v| v.as_array())
                        {
                            Some(vals) if !vals.is_empty() => {
                                let vs: Vec<&str> =
                                    vals.iter().filter_map(|v| v.as_str()).collect();
                                format!("{name}: {ty} ({})", vs.join(", "))
                            }
                            _ => format!("{name}: {ty}"),
                        };
                        if f.get("required").and_then(|v| v.as_bool()).unwrap_or(false) {
                            descriptor.push_str(", required");
                        }
                        Some(descriptor)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut line = format!("- {type_id}: {}", fields.join("; "));
        // The create_node tool description tells the model the template is
        // "shown in ENTITY TYPES" and to include its fields; that promise
        // needs a referent here.
        if let Some(tmpl) = entry.get("title_template").and_then(|v| v.as_str()) {
            line.push_str(&format!(" [title_template: {tmpl}]"));
        }
        lines.push(line);
    }
    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

/// The tools Stage 2 may offer, restricted to the union of the eligible
/// candidates' whitelists.
///
/// This is the trust boundary ADR-038 places on the system side: the model
/// judges *among* what retrieval surfaced, and can only fire what the matched
/// skills permit. A skill's `tool_whitelist` was previously read only by
/// external ACP agents; this gives it a local-agent consumer.
///
/// Falls back to the full surface when nothing was retrieved, so an
/// unavailable embedding service degrades to today's behaviour rather than
/// leaving the model with no tools at all.
pub fn stage2_tools(candidates: &[SkillCandidate], all: &[ToolDefinition]) -> Vec<ToolDefinition> {
    let permitted: std::collections::HashSet<&str> = candidates
        .iter()
        .filter(|c| clears_score_gate(c))
        .flat_map(|c| c.tools.iter().map(|t| t.as_str()))
        .collect();

    if permitted.is_empty() {
        return all.to_vec();
    }
    let scoped: Vec<ToolDefinition> = all
        .iter()
        .filter(|t| permitted.contains(t.name.as_str()))
        .cloned()
        .collect();

    // A whitelist naming only tools this build does not register would strand
    // the model with nothing to call. Fail open to the full surface.
    if scoped.is_empty() {
        return all.to_vec();
    }
    scoped
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn candidate(name: &str, score: f32, tools: &[&str]) -> SkillCandidate {
        SkillCandidate {
            id: format!("skill-{name}"),
            name: name.to_string(),
            description: format!("{name} description"),
            score,
            tools: tools.iter().map(|t| t.to_string()).collect(),
            instructions: format!("{name} instructions"),
            schema_metadata: json!([]),
        }
    }

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: String::new(),
            parameters_schema: json!({}),
        }
    }

    #[test]
    fn stage1_offers_exactly_the_two_routing_tools() {
        let defs = stage1_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec![ROUTE_QUERY_TOOL, ROUTE_CLARIFY_TOOL]);
    }

    #[test]
    fn route_query_parses_into_a_query_decision() {
        let d = parse_route_decision(ROUTE_QUERY_TOOL, r#"{"query":"create a schema"}"#);
        assert_eq!(d, Some(RouteDecision::Query("create a schema".into())));
    }

    #[test]
    fn route_clarify_parses_question_and_options() {
        let d = parse_route_decision(
            ROUTE_CLARIFY_TOOL,
            r#"{"question":"Which did you mean?","options":["Track debts","Search notes"]}"#,
        );
        assert_eq!(
            d,
            Some(RouteDecision::Clarify {
                question: "Which did you mean?".into(),
                options: vec!["Track debts".into(), "Search notes".into()],
            })
        );
    }

    #[test]
    fn clarify_without_options_still_parses() {
        // `options` is optional on the wire so a model omitting it does not
        // cost the turn; the contract's "be specific" requirement is enforced
        // by the tool description, not by rejecting the call.
        let d = parse_route_decision(ROUTE_CLARIFY_TOOL, r#"{"question":"Which one?"}"#);
        assert!(matches!(d, Some(RouteDecision::Clarify { .. })));
    }

    #[test]
    fn unknown_tool_or_blank_query_yields_no_decision() {
        assert!(parse_route_decision("search_nodes", r#"{"query":"x"}"#).is_none());
        assert!(parse_route_decision(ROUTE_QUERY_TOOL, r#"{"query":"   "}"#).is_none());
        assert!(parse_route_decision(ROUTE_QUERY_TOOL, "not json").is_none());
    }

    #[test]
    fn blast_radius_derives_from_the_tool_whitelist() {
        assert!(!skill_is_mutating(&candidate(
            "research",
            0.5,
            &["search_nodes", "get_node"]
        )));
        assert!(skill_is_mutating(&candidate(
            "schema",
            0.5,
            &["create_schema", "get_node"]
        )));
        // A single write tool among reads is enough to raise the bar.
        assert!(skill_is_mutating(&candidate(
            "deletion",
            0.5,
            &["search_nodes", "delete_node"]
        )));
    }

    #[test]
    fn an_unrecognised_whitelist_tool_is_treated_as_mutating() {
        // Fail-safe: a typo, a renamed tool, or a future externally-registered
        // one is of unknown blast radius, so it takes the higher bar rather
        // than defaulting to read-only.
        let ghost = candidate("ghost", 0.2, &["delete_everything_v2"]);
        assert!(skill_is_mutating(&ghost));
        assert!(
            !clears_score_gate(&ghost),
            "0.2 clears the read bar but must not clear the mutating one"
        );
    }

    #[test]
    fn mutating_skills_carry_a_strictly_higher_bar() {
        // Compile-time: ADR-038 requires the expensive error — firing the
        // wrong *mutating* tool — to be gated harder than a wrong read. Tuning
        // the values is expected; inverting the ordering is not, so it fails
        // the build rather than a test run.
        const _: () = assert!(MUTATING_SKILL_SCORE_BAR > READ_SKILL_SCORE_BAR);
        let read = candidate("research", 0.2, &["search_nodes"]);
        let write = candidate("schema", 0.2, &["create_schema"]);
        // Identical score, different verdict — the bar is a property of the
        // matched skill, not a single global constant.
        assert!(clears_score_gate(&read));
        assert!(!clears_score_gate(&write));
    }

    #[test]
    fn candidates_below_their_bar_are_never_rendered() {
        let cands = vec![
            candidate("weak", 0.01, &["search_nodes"]),
            candidate("strong", 0.9, &["search_nodes"]),
        ];
        let rendered = render_candidates_for_prompt(&cands).expect("one candidate is eligible");
        assert!(rendered.contains("strong"));
        assert!(!rendered.contains("weak instructions"));
    }

    #[test]
    fn routed_skill_names_lists_only_gate_clearing_candidates() {
        let cands = vec![
            candidate("Research & Search", 0.9, &["search_nodes"]),
            candidate("Below The Bar", 0.01, &["search_nodes"]),
            candidate("Node Creation", 0.9, &["search_nodes"]),
        ];
        assert_eq!(
            routed_skill_names(&cands),
            "Research & Search, Node Creation"
        );
    }

    #[test]
    fn routed_skill_names_is_empty_when_nothing_clears_the_gate() {
        // Distinct from "retrieval returned nothing" only in the candidate
        // count logged alongside it; the scraper omits the marker either way.
        let cands = vec![candidate("weak", 0.001, &["create_schema"])];
        assert_eq!(routed_skill_names(&cands), "");
    }

    #[test]
    fn routed_skill_names_matches_what_is_rendered_into_the_prompt() {
        // The field exists to answer "which skill was this turn routed to", so
        // it has to name the same set the model actually sees. Asserting the
        // two agree keeps the log honest if either filter later changes.
        let cands = vec![
            candidate("Graph Editing", 0.9, &["search_nodes"]),
            candidate("Excluded", 0.001, &["search_nodes"]),
        ];
        let rendered = render_candidates_for_prompt(&cands).expect("one is eligible");
        for name in routed_skill_names(&cands).split(", ") {
            assert!(
                rendered.contains(name),
                "{name} logged as routed but absent from the prompt block"
            );
        }
        assert!(!rendered.contains("Excluded"));
    }

    #[test]
    fn no_eligible_candidates_renders_nothing() {
        let cands = vec![candidate("weak", 0.001, &["create_schema"])];
        assert!(render_candidates_for_prompt(&cands).is_none());
    }

    #[test]
    fn rendered_candidates_carry_their_instruction_subtree() {
        let cands = vec![candidate("Schema Creation", 0.9, &["create_schema"])];
        let rendered = render_candidates_for_prompt(&cands).unwrap();
        assert!(rendered.contains("Schema Creation instructions"));
        assert!(rendered.contains("Purpose: Schema Creation description"));
    }

    #[test]
    fn stage2_tools_are_scoped_to_eligible_candidate_whitelists() {
        let all = vec![
            tool("search_nodes"),
            tool("get_node"),
            tool("create_schema"),
            tool("delete_node"),
        ];
        let cands = vec![candidate("research", 0.9, &["search_nodes", "get_node"])];
        let scoped = stage2_tools(&cands, &all);
        let names: Vec<&str> = scoped.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["search_nodes", "get_node"]);
        assert!(
            !names.contains(&"delete_node"),
            "a read-only skill must not put a destructive tool in reach"
        );
    }

    #[test]
    fn an_ineligible_candidate_does_not_widen_the_tool_surface() {
        let all = vec![tool("search_nodes"), tool("delete_node")];
        // Deletion is mutating, so 0.2 is below its bar though above the read bar.
        let cands = vec![
            candidate("research", 0.9, &["search_nodes"]),
            candidate("deletion", 0.2, &["delete_node"]),
        ];
        let scoped = stage2_tools(&cands, &all);
        let names: Vec<&str> = scoped.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["search_nodes"]);
    }

    #[test]
    fn empty_retrieval_falls_back_to_the_full_tool_surface() {
        let all = vec![tool("search_nodes"), tool("create_schema")];
        assert_eq!(stage2_tools(&[], &all).len(), 2);
    }

    #[test]
    fn whitelist_naming_only_unregistered_tools_fails_open() {
        let all = vec![tool("search_nodes")];
        let cands = vec![candidate("ghost", 0.9, &["tool_that_does_not_exist"])];
        assert_eq!(
            stage2_tools(&cands, &all).len(),
            1,
            "stranding the model with zero tools is worse than a wide surface"
        );
    }

    #[test]
    fn a_malformed_metadata_entry_does_not_discard_the_others() {
        let mut c = candidate("Node Creation", 0.9, &["create_node"]);
        c.schema_metadata = json!([
            {"fields": [{"name": "orphan", "type": "text"}]},
            {"type_id": "task", "fields": [{"name": "title", "type": "text"}]}
        ]);
        let rendered = render_candidates_for_prompt(&[c]).unwrap();
        assert!(
            rendered.contains("- task: title: text"),
            "an entry missing type_id must not take the valid ones with it: {rendered}"
        );
    }

    #[test]
    fn schema_metadata_marks_required_and_renders_title_template() {
        let mut c = candidate("Node Creation", 0.9, &["create_node"]);
        c.schema_metadata = json!([{
            "type_id": "invoice",
            "title_template": "{reference} - {amount}",
            "fields": [
                {"name": "reference", "type": "string"},
                {"name": "amount", "type": "number", "required": true}
            ]
        }]);
        let rendered = render_candidates_for_prompt(&[c]).unwrap();
        // Same notation the workspace-context block uses; both land in one prompt.
        assert!(
            rendered.contains("- invoice: reference: string; amount: number, required"),
            "got: {rendered}"
        );
        // create_node's description promises the template is shown here.
        assert!(
            rendered.contains("[title_template: {reference} - {amount}]"),
            "got: {rendered}"
        );
    }

    #[test]
    fn metadata_with_no_usable_entries_renders_no_section() {
        let mut c = candidate("Node Creation", 0.9, &["create_node"]);
        c.schema_metadata = json!([{"fields": []}]);
        let rendered = render_candidates_for_prompt(&[c]).unwrap();
        assert!(!rendered.contains("RELEVANT ENTITY TYPES"));
    }

    #[test]
    fn schema_metadata_renders_enum_values() {
        let mut c = candidate("Node Creation", 0.9, &["create_node"]);
        c.schema_metadata = json!([{
            "type_id": "task",
            "fields": [
                {"name": "title", "type": "text"},
                {"name": "status", "type": "enum", "enum_values": ["todo", "done"]}
            ]
        }]);
        let rendered = render_candidates_for_prompt(&[c]).unwrap();
        assert!(rendered.contains("- task: title: text; status: enum (todo, done)"));
    }
}
