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
pub const SCHEMA_CREATION_RULES: &str = "NODE MODEL: Everything in NodeSpace is a node. Built-in types (task, text, date) are always available. Custom types (e.g. 'project', 'customer') require a schema node to exist first — the schema defines the type's fields and title template. Once a schema exists, create instances with create_node(node_type=<schema_id>). Use create_schema only to define a new type; use create_node to create data.";

/// Tool strategy guidance.
///
/// The full "TOOL STRATEGY:" bulleted list. Anchored on the rule that the
/// agent must always search before updating or fetching nodes — never invent
/// placeholder IDs — but also covers how to choose between search_nodes,
/// search_semantic, get_node, and get_related_nodes, plus the canonical
/// create_node / create_schema / create_relationship usage patterns.
pub const TOOL_STRATEGY_RULES: &str = "TOOL STRATEGY:\n\
    - To discover whether a registered skill matches the user's intent: call search_skills with a natural-language query describing what you want to do. Returns up to N matches with name, description, confidence (0-1), and tools. Empty matches mean no skill is related — judge whether to respond directly, ask the user, or proceed with general tools. Skip for purely conversational replies (greetings, thanks, small talk).\n\
    - ALWAYS search first before updating or getting a node. NEVER use placeholder IDs like \"abc-123\".\n\
    - To find nodes by exact title or keyword (when you know the name): use search_nodes with query=<keyword>. To filter by type (e.g. \"show all tasks\"), pass node_type=\"task\" with query=\"\". To filter by property (e.g. \"open tasks\"), pass filters={\"status\":\"open\"}.\n\
    - To find nodes by meaning/topic (when the exact name is unknown): use search_semantic (natural language query)\n\
    - search_semantic results are ordered by relevance. Each result has: id, title, score (0-1), snippet, and optionally markdown (full content).\n\
    - search_semantic parameters: use 'collection' to scope to a namespace/folder, 'node_types' to filter by type (e.g. [\"task\"]), 'scope'='conversations' to search chat history, 'threshold' to tune precision (lower = broader recall), 'include_archived'=true to include archived content, 'exclude_collections' to suppress noisy collections, 'include_edges'=true to get relationship data with results, 'graph_boost'=true to rank well-connected nodes higher.\n\
    - If a search_semantic result has a non-empty 'markdown' field, that IS the full document — summarize from it directly. Only call get_node for results that lack markdown.\n\
    - To get full content for a known node ID: use get_node with format=markdown.\n\
    - To find what nodes are connected to a node: use get_related_nodes with the node ID.\n\
    - To update a task status: search_nodes for the task by name, then use update_task_status with the real ID.\n\
    - To update a node's title or content: search_nodes for it by name, then use update_node with the real ID.\n\
    - To create a new entity type: use create_schema (not create_node). If the type already appears in ENTITY TYPES above, the schema already exists — do not call create_schema again.\n\
    - To modify an existing entity type (add/remove fields, change title_template): use update_schema with the schema_id\n\
    - To create any node: use create_node with content=<name or text> and node_type. Pass 'properties' only if the schema has fields (shown in ENTITY TYPES).\n\
    - If ENTITY TYPES shows a title template for the schema (e.g. title: \"{name} ({status})\"), include those template fields in 'properties' — the service composes the displayed title from them.\n\
    - To connect nodes: use create_relationship with relationship names from the schemas above\n\
    - Tool call arguments must be valid JSON. Do NOT include comments (#) in JSON.";

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
        assert!(!SCHEMA_CREATION_RULES.is_empty());
        assert!(SCHEMA_CREATION_RULES.contains("NODE MODEL:"));
        assert!(SCHEMA_CREATION_RULES.contains("create_schema"));
        assert!(SCHEMA_CREATION_RULES.contains("create_node"));
    }

    #[test]
    fn tool_strategy_rules_non_empty() {
        assert!(!TOOL_STRATEGY_RULES.is_empty());
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
