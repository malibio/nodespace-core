//! Single source of truth for agent guidance rules.
//!
//! These constants define the shared rules injected into both the local
//! agent's seeded prompt nodes ([`crate::prompt_assembler`]) and the context
//! files produced for external agent sessions ([`crate::acp::context_assembly`]).
//! Changing a rule here propagates to every code path that composes agent
//! guidance — including the local Ollama agent (next time prompt nodes are
//! reseeded) and the `CLAUDE.md` / `AGENTS.md` files written under ADR-032.
//!
//! Issue #1089.

/// Schema creation guidance.
///
/// Covers the node-vs-schema mental model: when to call `create_schema` vs.
/// `create_node`, and how custom types relate to built-in types. More detailed
/// `title_template` token / field alignment guidance currently lives in
/// `skill_pipeline.rs` (used only by the skill-based schema-creation path) and
/// should be consolidated here when that path is unified — tracked separately
/// from #1089.
pub const SCHEMA_CREATION_RULES: &str = "NODE MODEL: Everything is a node. Built-in types: task, text, date. Custom types need a schema first (create_schema). Once a schema exists, create instances with create_node(node_type=<schema_id>). Never call create_schema for a type already in ENTITY TYPES.";

/// Tool strategy guidance.
///
/// Compressed "TOOL STRATEGY:" bulleted list. Covers the search-first mandate
/// (never invent placeholder IDs), tool selection hints for search_nodes vs.
/// search_semantic, and canonical create/update/connect patterns. Parameter-level
/// detail is intentionally deferred to tool schemas to avoid duplication.
pub const TOOL_STRATEGY_RULES: &str = "TOOL STRATEGY:\n\
    - Before any non-conversational action: call search_skills(query) to find a matching skill. Empty result = no skill, proceed with general tools. Skip for greetings/small talk.\n\
    - ALWAYS search first before updating or getting a node. NEVER use placeholder IDs like \"abc-123\".\n\
    - By keyword/type/property: search_nodes(query, node_type, filters). By meaning: search_semantic(query, node_types, scope, threshold, graph_boost).\n\
    - search_semantic result: if 'markdown' is non-empty, summarize from it directly — skip get_node.\n\
    - To get full content: get_node(id, format=markdown). To get connections: get_related_nodes(id).\n\
    - To update task status: search_nodes for it, then update_task_status with the real ID. To update node content: search_nodes first, then update_node with the real ID.\n\
    - To create a node: create_node(content, node_type). Pass 'properties' only if ENTITY TYPES shows fields. Include title_template fields in properties.\n\
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
        assert!(TOOL_STRATEGY_RULES.contains("ALWAYS search first"));
        assert!(TOOL_STRATEGY_RULES.contains("NEVER use placeholder IDs"));
    }

    #[test]
    fn node_reference_format_specifies_bare_uri() {
        assert!(NODE_REFERENCE_FORMAT.contains("nodespace://"));
        assert!(NODE_REFERENCE_FORMAT.contains("no markdown links"));
        assert!(NODE_REFERENCE_FORMAT.contains("no backticks"));
    }
}
