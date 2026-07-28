//! Single source of truth for the local in-app agent's guidance rules.
//!
//! These constants define the rules injected into the local agent's seeded
//! prompt nodes ([`crate::prompt_assembler`]). Changing a rule here propagates
//! to every code path that composes local-agent guidance — including the
//! local Ollama agent, next time prompt nodes are reseeded.
//!
//! External PTY-spawned agent sessions ([`crate::acp::context_assembly`]) do
//! not use these constants — they get all tool/capability guidance from
//! `packages/skill/SKILL.md`, the CLI-vocabulary companion doc, not from this
//! module's tool-call-vocabulary prose.

/// Schema creation guidance.
///
/// Covers the node-vs-schema mental model: when to call `create_schema` vs.
/// `create_node`, and how custom types relate to built-in types. More detailed
/// `title_template` token / field alignment guidance currently lives in
/// `skill_pipeline.rs` (used only by the skill-based schema-creation path) and
/// should be consolidated here when that path is unified.
pub const SCHEMA_CREATION_RULES: &str = "NODE MODEL: Everything is a node. Built-in types: task, text, date. Custom types need a schema first (create_schema). Once a schema exists, create instances with create_node(node_type=<schema_id>). Never call create_schema for a type already in RELEVANT ENTITY TYPES.\n\
    \"DATABASE\" = SCHEMA: A request to start tracking a kind of thing — worded as a database, a tracker, a list, or \"track X\" — means call create_schema IMMEDIATELY: no confirmation, no search_skills, no planning text. Name the schema after the single entity being tracked, in singular form, stripped of the tracking wording itself.\n\
    INSTANCE vs TYPE: A request that supplies the particulars of ONE record — a name, an amount, a title, a date — asks for an INSTANCE of a type that already exists, not a new type. Call search_skills, then create_node(node_type=<id from RELEVANT ENTITY TYPES>, ...) with those particulars as properties. Never ask for confirmation — just execute. Only a request to start tracking a KIND of thing calls for create_schema.\n\
    NO CONFIRMATION FOR KNOWN TYPES: If a type appears in RELEVANT ENTITY TYPES, you have all the information you need. Do NOT say \"Could you confirm\" or \"I want to make sure\" or \"Would you like me to\" — just call search_skills then create_node immediately. Confirmation is NEVER required when the schema already exists.\n\
    SCHEMA SUCCESS/FAILURE: After create_schema returns a schema object, respond to the user and STOP — do NOT call create_schema again. If create_schema returns an \"already exists\" error, stop immediately and tell the user the type already exists.\n\
    TITLE TEMPLATE: Every {field_name} in title_template MUST appear in the fields array. If you want {invoice_number} in a template, you MUST add a field named invoice_number. Never reference a placeholder that is not in fields — this causes a validation error.\n\
    FIELD RULES: Every field object MUST have both \"name\" AND \"type\". Missing either causes a validation error that will never self-correct — stop retrying and fix the field. Valid: {\"name\":\"amount\",\"type\":\"number\",\"required\":true}.\n\
    ENUM FIELDS: type=\"enum\" requires a non-empty \"core_values\" array: [{\"value\":\"pending\",\"label\":\"Pending\"},{\"value\":\"paid\",\"label\":\"Paid\"}]. An enum with empty core_values always fails. If you don't have predefined values ready, use type=\"text\" instead — you can always add enum values later with update_schema.";

/// Tool strategy guidance.
///
/// Compressed "TOOL STRATEGY:" bulleted list. Covers the search-first mandate
/// (never invent placeholder IDs), tool selection hints for search_nodes vs.
/// search_semantic, and canonical create/update/connect patterns. Parameter-level
/// detail is intentionally deferred to tool schemas to avoid duplication.
pub const TOOL_STRATEGY_RULES: &str = "TOOL STRATEGY:\n\
    - NODE_TYPE COMES FROM RELEVANT ENTITY TYPES: Every node_type argument (search_nodes, create_node, update_node, resolve_query) MUST be an id copied exactly from the RELEVANT ENTITY TYPES block in this prompt — character for character, including any underscores. Never shorten, singularize, paraphrase, or guess it from the user's wording: the user's noun is usually NOT the id, because the id was derived from whatever the type was named when its schema was created. If the type you need is not listed in RELEVANT ENTITY TYPES, it does not exist yet — do not invent an id for it.\n\
    - CONVERSATIONAL TURNS USE NO TOOLS. Greetings, thanks, small talk, questions about your own capabilities or limits, and other meta questions about yourself — answer directly in text. Do NOT call any tool: nothing in the user's graph needs to be read to answer them.\n\
    - META QUESTIONS (\"how did you check?\", \"what tool did you use?\", \"did you look up X?\"): answer ONLY from what is visible in this conversation's tool call history. Do NOT fabricate tool names, arguments, or results. If you cannot see a tool call in the history that matches the claim, say so honestly — \"I did not make that search\" or \"I don't see a record of that in this conversation.\"\n\
    - SCHEMA CLAIMS WITHOUT VERIFICATION: Never state that a node type has or lacks a specific property (e.g. \"task has no due_date field\") without first calling a tool to verify. If you have not called get_node or search_nodes on the schema in this turn, you do not know its fields — say so.\n\
    - IDENTICAL TOOL CALLS: Never call the same tool with the exact same arguments twice in one turn. If a tool returned a result and you are about to call it again identically, you already have the answer — produce your response using the result you have.\n\
    - For create_node (adding an instance): call search_skills(query) THEN immediately call create_node in the SAME turn — no text between them. After search_skills returns, your next step MUST be create_node, not a planning message.\n\
    - For create_schema (new entity type): call create_schema DIRECTLY — no search_skills needed. See DATABASE=SCHEMA rule above.\n\
    - SKILL COMPLETION: Once create_node, update_node, create_schema, or delete_node returns successfully, respond to the user immediately. Do NOT call search_skills again or call create_schema again — the task is done.\n\
    - NEVER CLAIM ACTION WITHOUT TOOL RESULT: Never tell the user a node was created, updated, or deleted without a successful tool result in this turn. If no tool was called, no action happened — call the tool.\n\
    - CLARIFICATION CONTRACT: at most one clarification per intent. If the user clarifies and the request is still ambiguous, fall through to semantic_search and answer with what's available. Never clarify twice.\n\
    - AMBIGUITY: If a search returns 0 results or multiple results that don't clearly match, ask the user one specific clarifying question (e.g. \"Are you looking for the invoice with amount $500?\") rather than retrying the search.\n\
    - BLAST-RADIUS GATE: deletion is irreversible — only call delete_node or delete_schema when the user explicitly and unambiguously asks to delete. Never clarify before create_schema, create_node, or update operations. \"Could you confirm?\" and \"I want to make sure\" are FORBIDDEN before any non-delete operation.\n\
    - ALWAYS search_nodes first before update_node or update_task_status — even if a node ID appeared earlier in the conversation. Never skip the search step.\n\
    - READ-THEN-WRITE TURN COMPLETION: an instruction to change something (\"mark X as Y\", \"update X\", \"set X's status to Y\") is not finished when the search that finds X returns — it is finished when the write (update_node or update_task_status) succeeds. Once search_nodes returns a clear single match for a write instruction, your NEXT action MUST be that write call in the SAME turn — not a summary, not a question, not stopping. Searching and then stopping is a FAILED turn, not a completed one.\n\
    - search_nodes is the ONE tool for finding, listing, and filtering nodes. By keyword/title: search_nodes(query, node_type). To LIST EVERY NODE OF A TYPE — any request naming a type with no narrowing condition — call search_nodes(query=\"\", node_type=<type>); the empty query is what makes it return the whole type, so do NOT invent a keyword to put there. To FILTER BY A TYPED PROPERTY (status, due_date, amount, operators like gt/lt): add filters, e.g. search_nodes(node_type=<type>, filters=[{\"type\":\"property\",\"operator\":\"equals\",\"property\":\"status\",\"value\":\"open\"}]). search_nodes returns each node's properties. Use search_semantic(query, node_types, scope, threshold, graph_boost) ONLY for meaning-based / fuzzy questions.\n\
    - search_nodes filter \"type\" values: use \"property\" for schema/node fields (e.g. status, due_date, priority — anything defined on the node type). Use \"metadata\" ONLY for created_at, modified_at, node_type, or content. Using \"metadata\" for a property field (e.g. status) always fails with \"Invalid metadata field\".\n\
    - search_semantic result: if 'markdown' is non-empty, summarize from it directly — skip get_node.\n\
    - To get full content: get_node(id, format=markdown). To get connections: get_related_nodes(id).\n\
    - To update a CUSTOM schema node's property: if the request identifies the target indirectly — by a paraphrased description, an implicit property reference (a bare value such as an amount or code, without naming which field it belongs to), or a relative date/status word (a weekday, \"overdue\", \"recent\") — call resolve_query(request=<the user's request verbatim>, node_type=<id from RELEVANT ENTITY TYPES>) FIRST, then pass its returned \"query\" and \"filters\" directly into search_nodes, then update_node(id=<found_id>, properties={...}). Do NOT hand-write the search_nodes query yourself in these cases — resolve_query has looked up the schema's real fields and today's date, which you have not. Skip resolve_query only when the target is already named directly and unambiguously (a proper name or exact title you can pass straight to search_nodes as query). Use update_task_status ONLY for built-in task nodes — for every custom schema type, use update_node.\n\
    - To create a node: call search_skills first to get schema_metadata, then call create_node(content, node_type=<type_id>, properties=<fields from schema_metadata>). For built-in types (task, text, date), call create_node directly with no properties unless the user provides them.\n\
    - To add/modify an entity type: create_schema or update_schema(schema_id).\n\
    - To connect nodes: create_relationship with names from schemas above.\n\
    - Tool arguments must be valid JSON. No comments (#) in JSON.";

/// Node reference formatting rule.
///
/// Single-line directive that nodes must be referenced as bare `nodespace://`
/// URIs in agent output — no markdown links, no backticks. Designed to be
/// inlined into a larger response-formatting rules section.
pub const NODE_REFERENCE_FORMAT: &str =
    "Reference nodes with bare URI: nodespace://abc-123 (no markdown links, no backticks)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creation_rules_non_empty() {
        assert!(SCHEMA_CREATION_RULES.contains("NODE MODEL:"));
        assert!(SCHEMA_CREATION_RULES.contains("create_schema"));
        assert!(SCHEMA_CREATION_RULES.contains("create_node"));
    }

    #[test]
    fn tool_strategy_rules_non_empty() {
        assert!(TOOL_STRATEGY_RULES.contains("TOOL STRATEGY:"));
        assert!(TOOL_STRATEGY_RULES.contains("ALWAYS search_nodes first"));
        assert!(TOOL_STRATEGY_RULES.contains("Never skip the search step"));
    }

    #[test]
    fn tool_strategy_rules_cover_meta_question_schema_and_duplicate_call_guidance() {
        // Meta-question accuracy (confabulation fix)
        assert!(
            TOOL_STRATEGY_RULES.contains("META QUESTIONS"),
            "must instruct agent to answer meta-questions from conversation history only"
        );
        assert!(
            TOOL_STRATEGY_RULES.contains("tool call history"),
            "must refer to tool call history as the source of truth"
        );
        // Schema verification (hallucination fix)
        assert!(
            TOOL_STRATEGY_RULES.contains("SCHEMA CLAIMS WITHOUT VERIFICATION"),
            "must prohibit unverified schema claims"
        );
        // Duplicate-call prevention (loop fix)
        assert!(
            TOOL_STRATEGY_RULES.contains("IDENTICAL TOOL CALLS"),
            "must prohibit identical repeated tool calls"
        );
    }

    #[test]
    fn tool_strategy_rules_cover_read_then_write_completion() {
        assert!(
            TOOL_STRATEGY_RULES.contains("READ-THEN-WRITE TURN COMPLETION"),
            "must instruct the model to continue to the write call after a successful search, not stop"
        );
        assert!(
            TOOL_STRATEGY_RULES.contains("FAILED turn"),
            "must frame search-then-stop as a failed turn"
        );
    }

    #[test]
    fn tool_strategy_rules_reference_resolve_query_for_ambiguous_updates() {
        assert!(
            TOOL_STRATEGY_RULES.contains("resolve_query"),
            "must instruct the model to call resolve_query for ambiguous update targets"
        );
        assert!(
            !TOOL_STRATEGY_RULES.contains("ONE WORD ONLY"),
            "the ineffective ONE-WORD-ONLY prompt workaround must be removed, not layered under resolve_query"
        );
    }

    /// `node_type` must be taught as a lookup into the injected RELEVANT ENTITY
    /// TYPES block, never as a literal. A hardcoded type id is wrong for
    /// essentially every real workspace, because the id is derived from whatever
    /// the schema happened to be named at creation time.
    #[test]
    fn tool_strategy_rules_bind_node_type_to_relevant_entity_types() {
        assert!(
            TOOL_STRATEGY_RULES.contains("NODE_TYPE COMES FROM RELEVANT ENTITY TYPES"),
            "must state where node_type values come from"
        );
        for tool in [
            "search_nodes",
            "create_node",
            "update_node",
            "resolve_query",
        ] {
            assert!(
                TOOL_STRATEGY_RULES.contains(tool),
                "node_type guidance must cover {tool}"
            );
        }
        assert!(
            !TOOL_STRATEGY_RULES.contains("node_type=\"invoice\""),
            "node_type must never be taught as a hardcoded literal"
        );
    }

    #[test]
    fn node_reference_format_specifies_bare_uri() {
        assert!(NODE_REFERENCE_FORMAT.contains("nodespace://"));
        assert!(NODE_REFERENCE_FORMAT.contains("no markdown links"));
        assert!(NODE_REFERENCE_FORMAT.contains("no backticks"));
    }

    // ------------------------------------------------------------------
    // Eval-contamination guard
    // ------------------------------------------------------------------

    /// Every guidance string that reaches the model, paired with its constant
    /// name for error reporting. Add new guidance constants here.
    fn guidance_corpus() -> Vec<(&'static str, String)> {
        let mut corpus: Vec<(&'static str, String)> = vec![
            ("SCHEMA_CREATION_RULES", SCHEMA_CREATION_RULES.to_string()),
            ("TOOL_STRATEGY_RULES", TOOL_STRATEGY_RULES.to_string()),
            ("NODE_REFERENCE_FORMAT", NODE_REFERENCE_FORMAT.to_string()),
        ];
        // skill_rules.rs `imperative` text is seeded into the DB as prompt
        // content by seed_skill_nodes(), so it is guidance too.
        for r in crate::skill_rules::SCHEMA_RULES {
            corpus.push(("skill_rules::SCHEMA_RULES", r.imperative.to_string()));
        }
        for r in crate::skill_rules::INTERACTION_RULES {
            corpus.push(("skill_rules::INTERACTION_RULES", r.imperative.to_string()));
        }
        // Every seeded skill's markdown_content is injected as skill
        // instructions when search_skills routes to it, so it is model-facing
        // guidance and equally contaminable. Pulling these from
        // seed_skill_nodes() rather than naming constants means a skill added
        // later is covered automatically — a hand-maintained file list is
        // exactly how the skill_pipeline.rs contamination went unnoticed.
        for t in crate::skill_pipeline::seed_skill_nodes() {
            if !t.markdown_content.is_empty() {
                corpus.push(("skill_pipeline::seed_skill_nodes", t.markdown_content));
            }
        }
        corpus
    }

    /// Extract the `prompt: "..."` / `prompt: '...'` literals from an eval
    /// script. Parsing the real file (rather than keeping a copy here) is the
    /// point: a reworded scenario is picked up automatically, so this guard
    /// cannot silently go stale against the eval it protects.
    /// Returns (parsed prompts, number of `prompt:` literal sites seen). The
    /// caller compares the two: a parser that silently skips a literal shape
    /// it does not understand fails OPEN — that prompt simply goes unchecked —
    /// so coverage is asserted rather than assumed.
    fn eval_prompts(source: &str) -> (Vec<String>, usize) {
        let mut prompts = Vec::new();
        let mut sites = 0usize;
        for (idx, _) in source.match_indices("prompt:") {
            // Only treat this as an object key. `prompt:` also appears inside
            // identifiers/field access (e.g. `fixture.prompt:`) and prose.
            if source[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                continue;
            }
            let rest = source[idx + "prompt:".len()..].trim_start();
            let quote = match rest.chars().next() {
                Some(q @ ('"' | '\'' | '`')) => q,
                // e.g. `prompt: fixture.prompt` — a reference, not a literal;
                // nothing to parse, so it is not counted as a site.
                _ => continue,
            };
            sites += 1;
            let body = &rest[quote.len_utf8()..];
            // Locate the closing quote, honoring backslash escapes so an
            // embedded \" does not truncate the literal early.
            let mut end = None;
            let mut escaped = false;
            for (i, c) in body.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match c {
                    '\\' => escaped = true,
                    c if c == quote => {
                        end = Some(i);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(end) = end else { continue };
            let literal = &body[..end];
            if !literal.is_empty() {
                prompts.push(literal.to_string());
            }
        }
        (prompts, sites)
    }

    fn normalize(s: &str) -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_string())
            .collect()
    }

    /// Longest run of consecutive words shared between an eval prompt and a
    /// guidance string. Catches near-verbatim reuse (a planted example lightly
    /// reworded), not just exact substring matches.
    fn longest_shared_run(prompt_words: &[String], guidance_words: &[String]) -> (usize, String) {
        let mut best = 0usize;
        let mut best_phrase = String::new();
        for i in 0..prompt_words.len() {
            for j in 0..guidance_words.len() {
                let mut k = 0;
                while i + k < prompt_words.len()
                    && j + k < guidance_words.len()
                    && prompt_words[i + k] == guidance_words[j + k]
                {
                    k += 1;
                }
                if k > best {
                    best = k;
                    best_phrase = prompt_words[i..i + k].join(" ");
                }
            }
        }
        (best, best_phrase)
    }

    /// The agent's guidance must not contain the eval's own scenario prompts.
    ///
    /// If a prompt is planted in guidance alongside the tool call it should
    /// produce, a passing scenario proves only that the model can copy a
    /// memorized example — the eval stops measuring generalization and prompt
    /// tuning acquires a degenerate solution (add more worked examples that
    /// match more scenarios). This is trivially easy to reintroduce while
    /// tuning prompts, which is exactly when it does the most damage.
    ///
    /// Fix a failure by rewording the GUIDANCE into a rule that teaches the
    /// shape ("pass the id exactly as shown in RELEVANT ENTITY TYPES") rather
    /// than by deleting the eval scenario.
    #[test]
    fn guidance_is_not_contaminated_by_eval_prompts() {
        // Longest run of consecutive shared words tolerated between an eval
        // prompt and a guidance rule. Ordinary instruction-following overlap
        // ("all my", "set it to") is short; a planted example is not.
        const MAX_SHARED_RUN: usize = 4;

        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let eval_scripts = ["scripts/aichat-matrix.ts", "scripts/routing-eval.ts"];

        let corpus = guidance_corpus();
        let tokenized: Vec<(&str, Vec<String>)> = corpus
            .iter()
            .map(|(name, text)| (*name, normalize(text)))
            .collect();

        let mut violations: Vec<String> = Vec::new();
        let mut checked = 0usize;

        for script in eval_scripts {
            let path = repo_root.join(script);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

            let (prompts, sites) = eval_prompts(&source);
            assert!(
                !prompts.is_empty(),
                "no `prompt:` literals parsed out of {script} — the parser has \
                 drifted from the eval's format and this guard is now vacuous; \
                 fix eval_prompts() rather than deleting this assertion"
            );
            // A parser that skips a literal shape it does not understand fails
            // OPEN: the prompt goes unchecked and the guard still passes. So
            // every site it saw must have produced a prompt.
            assert_eq!(
                prompts.len(),
                sites,
                "parsed only {} of {sites} `prompt:` literals in {script} — the \
                 unparsed ones are silently exempt from this guard; teach \
                 eval_prompts() the shape it missed",
                prompts.len()
            );

            for prompt in prompts {
                checked += 1;
                let prompt_words = normalize(&prompt);
                if prompt_words.is_empty() {
                    continue;
                }
                for (const_name, guidance_words) in &tokenized {
                    let (run, phrase) = longest_shared_run(&prompt_words, guidance_words);
                    // A short prompt can be quoted in full without ever
                    // exceeding a fixed run length, so also flag any prompt
                    // reproduced in its entirety.
                    let quoted_whole = run == prompt_words.len();
                    if run > MAX_SHARED_RUN || quoted_whole {
                        let why = if quoted_whole {
                            "the ENTIRE prompt appears"
                        } else {
                            "shares a long phrase"
                        };
                        violations.push(format!(
                            "  {const_name} {why} ({run} consecutive words) from \
                             {script} prompt {prompt:?}\n      shared phrase: {phrase:?}"
                        ));
                    }
                }
            }
        }

        assert!(checked > 0, "no eval prompts were checked");
        assert!(
            violations.is_empty(),
            "agent guidance contains eval scenario prompts — the eval would be \
             measuring recall of a planted example, not generalization:\n{}\n\n\
             Reword the GUIDANCE into a rule that teaches the shape rather than \
             a memorized literal; do not weaken this test or delete the scenario.",
            violations.join("\n")
        );
    }

    /// The guard above is only meaningful if it actually fires — this pins the
    /// detector against the real contamination that motivated it (the former
    /// `agent_guidance.rs:53` worked example built on scenario 6's prompt).
    #[test]
    fn contamination_guard_detects_a_planted_example() {
        let planted = normalize(
            "Example: \"Mark the $500 invoice as paid\" \
             → resolve_query(request=\"Mark the $500 invoice as paid\", node_type=\"invoice\")",
        );
        let prompt = normalize("Mark the $500 invoice as paid");
        let (run, _) = longest_shared_run(&prompt, &planted);
        assert!(
            run > 4,
            "detector failed to flag a known-contaminated example (run={run})"
        );
    }

    #[test]
    fn eval_prompt_parser_extracts_literals() {
        let src = r#"
            { scenario: "a", prompt: "Hi there", expect: x },
            { scenario: "b", prompt: 'Add a book called "Dune"', expect: y },
            { scenario: "c", prompt: fixture.prompt },
        "#;
        let (got, sites) = eval_prompts(src);
        // The single-quoted literal keeps its embedded double quotes; the
        // non-literal `fixture.prompt` reference is skipped and not counted
        // as a site, so coverage stays exact.
        assert_eq!(got, vec!["Hi there", "Add a book called \"Dune\""]);
        assert_eq!(sites, got.len(), "every counted site must be parsed");
    }

    #[test]
    fn eval_prompt_parser_handles_backticks_and_escapes() {
        let src = r#"
            { prompt: `a template literal prompt` },
            { prompt: "an \"escaped\" quote inside" },
        "#;
        let (got, sites) = eval_prompts(src);
        assert_eq!(sites, 2, "both literal shapes must be counted as sites");
        assert_eq!(
            got,
            vec![
                "a template literal prompt",
                "an \\\"escaped\\\" quote inside"
            ],
            "backticked and escaped literals must parse in full, not truncate"
        );
    }
}
