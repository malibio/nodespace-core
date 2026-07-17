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
    \"DATABASE\" = SCHEMA: When the user says \"create a database\", \"set up a tracker\", or \"track X\", call create_schema IMMEDIATELY — no confirmation, no search_skills, no planning text. The entity name is the schema (\"a contacts database\" → call create_schema with name=\"Contact\").\n\
    INSTANCE vs TYPE: \"Add an invoice for $500\", \"add a contact named Jane Doe\", \"log a project called X\" — these ask for a specific INSTANCE of an existing type. Call search_skills(query=\"add contact\") then call create_node(node_type=\"contact\", ...). Never ask for confirmation — just execute.\n\
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
    - CONVERSATIONAL TURNS USE NO TOOLS. Greetings (\"hi\", \"hello\"), thanks, small talk, capability questions (\"what can you do?\"), and meta questions about yourself — answer directly in text. Do NOT call any tool.\n\
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
    - search_nodes is the ONE tool for finding, listing, and filtering nodes. By keyword/title: search_nodes(query, node_type). To LIST ALL OF A TYPE (e.g. \"list all my invoices\", \"show all tasks\"): search_nodes(query=\"\", node_type=<type>) — empty query lists every node of that type. To FILTER BY A TYPED PROPERTY (status, due_date, amount, operators like gt/lt): add filters, e.g. search_nodes(node_type=<type>, filters=[{\"type\":\"property\",\"operator\":\"equals\",\"property\":\"status\",\"value\":\"open\"}]). search_nodes returns each node's properties. Use search_semantic(query, node_types, scope, threshold, graph_boost) ONLY for meaning-based / fuzzy questions.\n\
    - search_nodes filter \"type\" values: use \"property\" for schema/node fields (e.g. status, due_date, priority — anything defined on the node type). Use \"metadata\" ONLY for created_at, modified_at, node_type, or content. Using \"metadata\" for a property field (e.g. status) always fails with \"Invalid metadata field\".\n\
    - search_semantic result: if 'markdown' is non-empty, summarize from it directly — skip get_node.\n\
    - To get full content: get_node(id, format=markdown). To get connections: get_related_nodes(id).\n\
    - To update a CUSTOM schema node's property (e.g. mark invoice as paid): search_nodes(node_type=<type>, query=<ONE WORD ONLY>) then update_node(id=<found_id>, properties={\"status\": \"paid\"}). ONE WORD ONLY means exactly that — never pass the user's sentence as the query. Example: \"Mark the $500 invoice as paid\" → query=\"invoice\" (correct) — query=\"$500 invoice due next Friday\" or any other multi-word phrase is WRONG and returns zero results. Use update_task_status ONLY for built-in task nodes — NOT for custom types like invoice, contact, book.\n\
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
    fn node_reference_format_specifies_bare_uri() {
        assert!(NODE_REFERENCE_FORMAT.contains("nodespace://"));
        assert!(NODE_REFERENCE_FORMAT.contains("no markdown links"));
        assert!(NODE_REFERENCE_FORMAT.contains("no backticks"));
    }
}
