//! Skill and tool node seeding templates.
//!
//! Provides the default skill and tool nodes seeded on first run. Each
//! [`NodeTemplate`] produces one root node plus any children. Use
//! [`nodespace_core::markdown::prepare_nodes_from_template`] to expand a
//! template into a flat list of `PreparedNode`s before inserting them via
//! `NodeService::bulk_create_hierarchy`.
//!
//! The previous push-based [`SkillPipeline`] (pre-LLM intent
//! routing with confidence thresholds + tool whitelist scoping) has been
//! removed. Skill discovery is now LLM-orchestrated through the
//! `search_skills` tool exposed by [`crate::local_agent::tools`], so the
//! agent loop no longer needs a pipeline object — only the seeded skill
//! nodes themselves remain.
//!
//! Tool nodes (`node_type='tool'`) bridge graph storage to
//! deterministic Rust handlers. Each tool is seeded as a node carrying its
//! handler key, typed parameter schema, description, and `source` provenance.

use crate::skill_rules::{
    AMBIGUITY_CLARIFY, BULK_IMPORT_NO_FOLLOWUP_SEARCH, EDIT_DONT_RECREATE, ENUM_FORMAT,
    FIND_THEN_ACT, NAME_PLACEHOLDER_EXCEPTION, NO_NAME_TITLE_FIELD, ONE_SCHEMA_PER_REQUEST,
    ORG_NEEDS_EXISTING_COLLECTION, RELATIONSHIP_VS_FIELD, SCHEMA_ALREADY_EXISTS,
    SINGLE_ITEM_PER_CALL, SUCCESS_NO_REVERIFY, TARGET_TYPE_MUST_EXIST, TASK_STATUS_DEDICATED_VERB,
    TITLE_TEMPLATE_PLACEHOLDERS,
};
use nodespace_core::markdown::NodeTemplate;

/// Builds the Schema Creation skill's markdown_content, interpolating the
/// shared rules from [`crate::skill_rules`] (imperative form) so this text
/// cannot silently drift from the "Schema inspection and management" section
/// of `packages/skill/SKILL.md`, which renders the same rules in prose form
/// via `bin/gen_skill_md.rs`. Only the worked EXAMPLE blocks and structural
/// headers are unique to this LLM-prompt rendering.
fn schema_creation_guidance() -> String {
    format!(
        r#"# Schema Creation & Editing Guidance

CREATING A SCHEMA — call create_schema:

{one_schema_per_request}

{schema_already_exists}

EDITING A SCHEMA — call update_schema:

{edit_dont_recreate}

FIELDS: Only define type-specific fields. {no_name_title_field} {name_placeholder_exception} A 'description' field is acceptable when it adds value beyond the title. Good fields: status (enum), due_date (date), priority (enum), budget (number), owner (text).

{enum_format}

{relationship_vs_field} {target_type_must_exist}

{title_template_placeholders} Examples:
- Customer with fields [first_name, last_name]: title_template = "{{first_name}} {{last_name}}"
- Invoice with fields [invoice_number, ...]: title_template = "Invoice #{{invoice_number}}"
- Project with fields [name, status, ...]: title_template = "{{name}} ({{status}})"

EXAMPLE — Invoice schema (references existing 'customer' type):
{{
  "name": "Invoice",
  "description": "A billing invoice linked to a customer",
  "title_template": "Invoice #{{invoice_number}}",
  "fields": [
    {{"name": "invoice_number", "type": "text", "required": true}},
    {{"name": "issue_date", "type": "date", "required": true}},
    {{"name": "due_date", "type": "date"}},
    {{"name": "amount", "type": "number", "required": true}},
    {{"name": "status", "type": "enum", "required": true, "coreValues": [
      {{"value": "draft", "label": "Draft"}},
      {{"value": "sent", "label": "Sent"}},
      {{"value": "paid", "label": "Paid"}},
      {{"value": "overdue", "label": "Overdue"}}
    ]}}
  ],
  "relationships": [
    {{"name": "billed_to", "targetType": "customer", "direction": "out", "cardinality": "one"}}
  ]
}}

EXAMPLE — Project schema (title_template uses {{name}} AND {{status}}, so BOTH are in fields):
{{
  "name": "Project",
  "description": "A tracked project with status and timeline",
  "title_template": "{{name}} ({{status}})",
  "fields": [
    {{"name": "name", "type": "text", "required": true}},
    {{"name": "status", "type": "enum", "required": true, "coreValues": [
      {{"value": "planning", "label": "Planning"}},
      {{"value": "active", "label": "Active"}},
      {{"value": "on_hold", "label": "On Hold"}},
      {{"value": "completed", "label": "Completed"}}
    ]}},
    {{"name": "start_date", "type": "date"}},
    {{"name": "due_date", "type": "date"}},
    {{"name": "budget", "type": "number"}},
    {{"name": "owner", "type": "text"}}
  ],
  "relationships": [
    {{"name": "has_task", "targetType": "task", "direction": "out", "cardinality": "many"}}
  ]
}}"#,
        one_schema_per_request = ONE_SCHEMA_PER_REQUEST.imperative,
        schema_already_exists = SCHEMA_ALREADY_EXISTS.imperative,
        edit_dont_recreate = EDIT_DONT_RECREATE.imperative,
        no_name_title_field = NO_NAME_TITLE_FIELD.imperative,
        name_placeholder_exception = NAME_PLACEHOLDER_EXCEPTION.imperative,
        enum_format = ENUM_FORMAT.imperative,
        relationship_vs_field = RELATIONSHIP_VS_FIELD.imperative,
        target_type_must_exist = TARGET_TYPE_MUST_EXIST.imperative,
        title_template_placeholders = TITLE_TEMPLATE_PLACEHOLDERS.imperative,
    )
}

/// Builds the Graph Editing skill's markdown_content, interpolating shared
/// interaction rules from [`crate::skill_rules`] (find-then-act, ambiguity
/// clarification, dedicated task-status verb, success-means-stop).
fn graph_editing_guidance() -> String {
    format!(
        r#"# Graph Editing Guidance

When updating an existing node:

FIND THEN UPDATE: {find_then_act} Then call update_node with the ID and only the fields that need changing.

AMBIGUITY: {ambiguity_clarify} Examples:
- 0 results: "I couldn't find an invoice matching that description. Are you looking for the invoice with amount $500?"
- Multiple results: "I found 3 invoices — which one did you mean: Invoice #001 ($500), Invoice #002 ($750), or Invoice #003 ($500 overdue)?"

{task_status_dedicated_verb}

update_node FIELDS: Pass only the properties that need to change. Omit properties that should stay the same. Use the exact property key from the node's schema.

CONTENT vs PROPERTIES: Use the content field to update the node's title/main text. Use properties for typed fields (status, due_date, amount, etc.).

SUCCESS: {success_no_reverify}"#,
        find_then_act = FIND_THEN_ACT.imperative,
        ambiguity_clarify = AMBIGUITY_CLARIFY.imperative,
        task_status_dedicated_verb = TASK_STATUS_DEDICATED_VERB.imperative,
        success_no_reverify = SUCCESS_NO_REVERIFY.imperative,
    )
}

/// Builds the Relationship Management skill's markdown_content, interpolating
/// the shared find-then-act and success-means-stop rules.
fn relationship_management_guidance() -> String {
    format!(
        r#"# Relationship Management Guidance

When linking nodes or exploring connections:

CREATING A RELATIONSHIP: Call create_relationship with the source node ID, target node ID, and a relation_type label (e.g. "has_task", "billed_to", "related_to"). Both node IDs must exist — search for them first if you don't have them.

TRAVERSING RELATIONSHIPS: Call get_related_nodes with a node ID to fetch its connected nodes. Use the direction parameter ("out", "in", or "both") to control traversal direction. Filter by relation_type to narrow results.

FIND BEFORE LINK: If the user says "link X to Y" and you don't have both IDs, call search_semantic or search_nodes once per entity to resolve them, then call create_relationship.

SUCCESS: {success_no_reverify}"#,
        success_no_reverify = SUCCESS_NO_REVERIFY.imperative,
    )
}

/// Builds the Node Deletion skill's markdown_content, interpolating the
/// shared find-then-act, single-item-per-call, and success-means-stop rules.
fn node_deletion_guidance() -> String {
    format!(
        r#"# Node Deletion Guidance

When deleting a node:

FIND THEN DELETE: {find_then_act} Confirm the title matches what the user described, then call delete_node with the ID.

{single_item_per_call}

SUCCESS: {success_no_reverify}"#,
        find_then_act = FIND_THEN_ACT.imperative,
        single_item_per_call = SINGLE_ITEM_PER_CALL.imperative,
        success_no_reverify = SUCCESS_NO_REVERIFY.imperative,
    )
}

/// Builds the Bulk Import skill's markdown_content, interpolating the shared
/// no-followup-search success rule.
fn bulk_import_guidance() -> String {
    format!(
        r#"# Bulk Import Guidance

When importing a document or creating multiple nodes from markdown:

CALL create_nodes_from_markdown ONCE: Pass the markdown content directly. The tool parses headings into a node hierarchy — top-level headings become root nodes, sub-headings become children.

COLLECTION: If the user specifies a collection or folder name, pass it as the collection parameter.

NODE TYPE: Default to node_type="text" for general documents. Use a specific type if the user names one.

SUCCESS: {bulk_import_no_followup_search}"#,
        bulk_import_no_followup_search = BULK_IMPORT_NO_FOLLOWUP_SEARCH.imperative,
    )
}

/// Builds the Organization skill's markdown_content, interpolating the shared
/// find-then-act, collection-must-preexist, and success-means-stop rules.
fn organization_guidance() -> String {
    format!(
        r#"# Organization Guidance

When organizing nodes into collections or categories:

FIND THE NODE: {find_then_act}

ADD TO COLLECTION: Call create_relationship with the node ID as source, the collection node ID as target, and relation_type="member_of". {org_needs_existing_collection}

SUCCESS: After create_relationship returns, confirm to the user that the node has been organized into the collection."#,
        find_then_act = FIND_THEN_ACT.imperative,
        org_needs_existing_collection = ORG_NEEDS_EXISTING_COLLECTION.imperative,
    )
}

/// Default skill node templates seeded on first run.
///
/// Each template produces one skill root node plus optional `prompt` children
/// with guidance markdown. Tool whitelists and max_iterations are still stored
/// as properties on the skill node — they're consumed by external (ACP) agents
/// that prefer the older skill-scoped flow. The local agent ignores them and
/// just uses the description/name returned by `search_skills`.
pub fn seed_skill_nodes() -> Vec<NodeTemplate> {
    vec![
        NodeTemplate {
            title: "Research & Search".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Search and explore the knowledge graph to find relevant information, discover connections, and answer questions about stored knowledge.",
                "tool_whitelist": ["search_semantic", "search_nodes", "get_node"],
                "max_iterations": 4,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: r#"# Research & Search Guidance

When answering questions about stored knowledge:

SEARCH FIRST: When the user is looking for information or wants to act on a specific node, call search_semantic with a natural language query. Results are ordered by relevance — the first result is the best match. Skip search for conversational messages or capability questions.

RESULT STRUCTURE: Each result contains:
- id: node ID (use this for follow-up get_node calls)
- title: document title
- score: similarity score (0-1, higher = more relevant)
- snippet: short content preview
- markdown: full document content (present for top N results based on include_markdown, default 1)

USE MARKDOWN DIRECTLY: If the top result has a non-empty 'markdown' field, that is the complete document. Summarize or answer from it immediately — do NOT call get_node or search_nodes again.

FETCH ADDITIONAL CONTENT: Only call get_node with format=markdown if you need full content for a lower-ranked result that did not include markdown.

PARAMETER GUIDANCE:
- Use 'collection' to narrow search to a namespace/folder (e.g. collection="Architecture").
- Use 'node_types' to filter by type (e.g. node_types=["task"]) — prefer over 'collection' for type-based filtering.
- Use 'scope'="conversations" when the user asks about past chats or conversation history.
- Use 'threshold' to tune precision: default 0.3. Lower to 0.1-0.2 for broader recall when results are sparse.
- Use 'include_archived'=true only when the user explicitly asks for archived or historical content.
- Use 'exclude_collections' to suppress noisy collections (e.g. exclude_collections=["Archived"]).
- Use 'include_edges'=true to get relationship data (outgoing 'mentions' edges) with each result — saves a separate get_related_nodes call.
- Use 'graph_boost'=true to rank well-connected nodes higher (blends similarity with graph connectivity). Useful when the user wants the most referenced/central node on a topic.
- Use 'property_filters' for simple key-value filtering (e.g. property_filters={"status": "done"}). Prefer 'node_types' for type filtering.

MULTIPLE DOCUMENTS: If the user asks about multiple topics, call search_semantic once per topic rather than searching broadly and fetching each result individually.

search_nodes is the single tool for finding, listing, and filtering nodes — by title, by type, and by typed property. It returns each node's properties.

LISTING BY TYPE: To list all nodes of a type, use search_nodes with an empty query. Examples:
- "list all tasks" → search_nodes(query="", node_type="task")
- "list all customers" → search_nodes(query="", node_type="<customer-schema-id>")

STRUCTURED PROPERTY QUERIES: To filter by property values (status, due_date, etc.) or comparison operators (gt, lt, gte, lte, in), pass filters to search_nodes. Examples:
- "find all my open tasks" → search_nodes(node_type="task", filters=[{"type":"property","operator":"equals","property":"status","value":"open"}])
- "tasks due tomorrow" → search_nodes(node_type="task", filters=[{"type":"property","operator":"equals","property":"due_date","value":"<tomorrow's date in YYYY-MM-DD>"}], sorting=[{"field":"due_date","direction":"asc"}])
- "tasks due this week" → search_nodes(node_type="task", filters=[{"type":"property","operator":"gte","property":"due_date","value":"<today's date in YYYY-MM-DD>"},{"type":"property","operator":"lte","property":"due_date","value":"<end of week in YYYY-MM-DD>"}])
- "find tasks for Acme" → search_nodes(node_type="task", filters=[{"type":"property","operator":"equals","property":"company","value":"Acme"}])
- Date format: always YYYY-MM-DD. Operators: equals, contains, gt, lt, gte, lte, in, exists."#.to_string(),
        },
        NodeTemplate {
            title: "Node Creation".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Create new nodes, records, entries, or instances of any type — tasks, text notes, or custom types like Project, Customer, Invoice. Use when user wants to add, create, or insert a new item, record, entry, or example of an existing type.",
                "tool_whitelist": ["create_node", "search_semantic", "search_nodes", "get_node"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: r#"# Node Creation Guidance

⚡ IMMEDIATE ACTION REQUIRED: Call create_node NOW with all values from the user message. Do NOT output any text — your response to receiving these instructions must be the create_node tool call.

CALL create_node NOW: You received this instruction because search_skills was called. Your NEXT action MUST be create_node — do not output any planning text. Gather all needed values from the user message and call create_node immediately.

TYPE MAPPING FROM schema_metadata: When search_skills returned schema_metadata for this skill, set node_type to the type_id from that metadata (e.g. "invoice", "customer", "project"). For generic text notes use node_type="text". For tasks use node_type="task".

REQUIRED FIELDS: Read the fields array from schema_metadata. Required fields (required=true) MUST be included in the properties map. Optional fields should be included if the user provided a value for them.

TITLE: The node title is the content field. If schema_metadata has a title_template, the title is auto-generated from properties — set content to a brief descriptive label (e.g. the most identifying property value). If there is no title_template, set content to the best human-readable name the user provided.

PROPERTY KEYS: Use the field name exactly as it appears in schema_metadata fields[].name. Do NOT add namespace prefixes for schema-defined fields.

EXAMPLE — create an invoice for $500 due next Friday (schema_metadata type_id="invoice"):
{
  "node_type": "invoice",
  "content": "Invoice #001",
  "properties": {
    "amount": 500,
    "due_date": "2026-06-14",
    "status": "draft"
  }
}

SUCCESS: After create_node returns a node ID, confirm to the user what was created and STOP. Do NOT call search_skills, get_node, or any other tool — the create response is sufficient. The task is complete."#.to_string(),
        },
        NodeTemplate {
            title: "Schema Creation".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Define a new entity type or schema with custom fields, enums, and relationships, or modify an existing schema. Use when user says 'new type', 'node type', 'define fields', 'create schema', 'update schema', 'add a field', 'rename a field', or wants to design or change a kind of entity like Project, Customer, or Invoice.",
                "tool_whitelist": ["create_schema", "update_schema", "get_node"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: schema_creation_guidance(),
        },
        NodeTemplate {
            title: "Graph Editing".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Modify existing nodes in the knowledge graph - update content, properties, titles, and metadata. For tasks, use update_task_status to change status.",
                "tool_whitelist": ["update_node", "update_task_status", "get_node", "search_nodes", "search_semantic"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: graph_editing_guidance(),
        },
        NodeTemplate {
            title: "Relationship Management".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Create connections between nodes, explore relationships, and traverse the knowledge graph.",
                "tool_whitelist": ["create_relationship", "get_related_nodes", "get_node", "search_semantic", "search_nodes"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: relationship_management_guidance(),
        },
        NodeTemplate {
            title: "Node Deletion".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Delete nodes from the knowledge graph. Use when user wants to remove, delete, or trash a node or record.",
                "tool_whitelist": ["delete_node", "get_node", "search_semantic", "search_nodes"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: node_deletion_guidance(),
        },
        NodeTemplate {
            title: "Bulk Import".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Import documents and create node hierarchies from markdown. Use when user wants to import, bulk create, or create nodes from a markdown document.",
                "tool_whitelist": ["create_nodes_from_markdown"],
                "max_iterations": 2,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: bulk_import_guidance(),
        },
        NodeTemplate {
            title: "Organization".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Organize nodes into collections and categories. Use when user wants to add to a collection, categorize, or group nodes.",
                "tool_whitelist": ["create_relationship", "get_node", "search_semantic", "search_nodes"],
                "max_iterations": 3,
            }),
            child_node_type: Some("prompt".to_string()),
            child_properties: None,
            markdown_content: organization_guidance(),
        },
    ]
}

/// Default tool node templates seeded on first run.
///
/// Each template produces one `node_type='tool'` node bridging graph storage to
/// a deterministic Rust handler. Properties:
/// - `handler`: stable key into the handler registry (matches `Tool::name()`)
/// - `description`: embedded for semantic tool discovery
/// - `parameter_schema`: typed JSON Schema the model uses when calling the tool
/// - `source`: `"internal"` for all built-in tools
/// - `enabled`: `true` for internal tools (external tools require explicit enablement)
pub fn seed_tool_nodes() -> Vec<NodeTemplate> {
    use crate::local_agent::tools::Tool;
    Tool::ALL
        .iter()
        .map(|tool| {
            let def = tool.definition();
            NodeTemplate {
                title: def.name.clone(),
                content: None,
                root_node_type: "tool".to_string(),
                // Pre-namespace under "tool" so normalize_flat_properties_to_namespace
                // detects the existing namespace key and returns early (crud.rs:1633),
                // preserving the nested parameter_schema object. Without pre-namespacing
                // the normalizer misclassifies parameter_schema (an object) as a dormant
                // namespace, hoisting it out of the "tool" key so flatten_properties_for_api
                // later silently drops it.
                root_properties: serde_json::json!({
                    "tool": {
                        "handler": def.name,
                        "description": def.description,
                        "parameter_schema": def.parameters_schema,
                        "source": "internal",
                        "enabled": true,
                    }
                }),
                child_node_type: None,
                child_properties: None,
                markdown_content: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use nodespace_core::markdown::prepare_nodes_from_template;

    use super::*;

    fn tmpl_tool_whitelist(tmpl: &NodeTemplate) -> Vec<String> {
        tmpl.root_properties
            .get("tool_whitelist")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn tmpl_max_iterations(tmpl: &NodeTemplate) -> usize {
        tmpl.root_properties
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    }

    #[test]
    fn seed_skills_have_valid_properties() {
        let seeds = seed_skill_nodes();
        assert_eq!(seeds.len(), 8, "Should have 8 seed skills");

        for seed in &seeds {
            assert!(!seed.title.is_empty());
            assert!(
                seed.root_properties
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "Skill '{}' must have a non-empty description",
                seed.title
            );
            assert!(
                !tmpl_tool_whitelist(seed).is_empty(),
                "Skill '{}' must have tools",
                seed.title
            );
            assert!(
                tmpl_max_iterations(seed) > 0,
                "Skill '{}' must have max_iterations > 0",
                seed.title
            );
            assert!(
                !seed.markdown_content.is_empty(),
                "Skill '{}' must have non-empty markdown_content (instructions for the model)",
                seed.title
            );
        }
    }

    #[test]
    fn seed_skill_template_produces_skill_node() {
        let seeds = seed_skill_nodes();
        for seed in &seeds {
            let nodes = prepare_nodes_from_template(seed)
                .unwrap_or_else(|e| panic!("Template '{}' failed: {:?}", seed.title, e));
            assert!(
                !nodes.is_empty(),
                "Template '{}' produced no nodes",
                seed.title
            );
            let root = &nodes[0];
            assert_eq!(root.node_type, "skill");
            assert_eq!(root.id.len(), 36, "Node ID should be a UUID");
            assert_eq!(root.id.chars().filter(|c| *c == '-').count(), 4);
            assert_eq!(root.content, seed.title);
        }
    }

    // -- Skill whitelist / registry validation ---------------------------

    /// Every tool named in a seed skill's `tool_whitelist` must resolve to a
    /// real entry in the tool registry. This is the registry-backed replacement
    /// for the former per-skill drift detectors: a typo or a reference to a
    /// removed tool fails here instead of silently producing a whitelist entry
    /// that can never match a dispatchable tool.
    #[test]
    fn all_skill_whitelist_tools_are_registered() {
        use crate::local_agent::tools::Tool;
        for seed in seed_skill_nodes() {
            for tool_name in tmpl_tool_whitelist(&seed) {
                assert!(
                    Tool::from_name(&tool_name).is_some(),
                    "Skill '{}' whitelists unknown tool '{}' — it is not in the tool registry \
                     (Tool enum). Fix the name or add the tool to the registry.",
                    seed.title,
                    tool_name
                );
            }
        }
    }

    #[test]
    fn update_schema_is_reachable_via_search_skills() {
        let seeds = seed_skill_nodes();
        let schema_skill = seeds
            .iter()
            .find(|s| s.title == "Schema Creation")
            .expect("Schema Creation skill must exist");
        assert!(
            tmpl_tool_whitelist(schema_skill).contains(&"update_schema".to_string()),
            "update_schema must be in Schema Creation tool_whitelist so the model can reach it"
        );
    }

    // -- Tool node seeding tests --------------------------------

    #[test]
    fn seed_tool_nodes_covers_all_registered_tools() {
        use crate::local_agent::tools::Tool;
        let tool_seeds = seed_tool_nodes();
        assert_eq!(
            tool_seeds.len(),
            Tool::ALL.len(),
            "seed_tool_nodes() must produce one node per Tool::ALL entry"
        );
    }

    #[test]
    fn seed_tool_nodes_have_required_properties() {
        for seed in seed_tool_nodes() {
            assert_eq!(seed.root_node_type, "tool");

            // Properties are pre-namespaced under "tool" to survive the normalizer
            let ns = seed
                .root_properties
                .get("tool")
                .expect("tool namespace must be present");

            let handler = ns.get("handler").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                !handler.is_empty(),
                "Tool '{}' must have a handler key",
                seed.title
            );

            let source = ns.get("source").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(
                source, "internal",
                "Built-in tool '{}' must have source='internal'",
                seed.title
            );

            let enabled = ns.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            assert!(
                enabled,
                "Built-in tool '{}' must be enabled=true",
                seed.title
            );

            assert!(
                ns.get("parameter_schema")
                    .map(|v| v.is_object())
                    .unwrap_or(false),
                "Tool '{}' must have a parameter_schema object",
                seed.title
            );

            let desc = ns.get("description").and_then(|v| v.as_str()).unwrap_or("");
            assert!(
                !desc.is_empty(),
                "Tool '{}' must have a non-empty description",
                seed.title
            );
        }
    }

    #[test]
    fn seed_tool_nodes_handler_keys_match_registry() {
        use crate::local_agent::tools::Tool;
        for seed in seed_tool_nodes() {
            let handler = seed
                .root_properties
                .get("tool")
                .and_then(|ns| ns.get("handler"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert!(
                Tool::from_name(handler).is_some(),
                "Tool node '{}' has handler '{}' not in registry",
                seed.title,
                handler
            );
        }
    }

    #[test]
    fn seed_tool_nodes_produce_valid_prepared_nodes() {
        for seed in seed_tool_nodes() {
            let nodes = prepare_nodes_from_template(&seed)
                .unwrap_or_else(|e| panic!("Template '{}' failed: {:?}", seed.title, e));
            assert!(!nodes.is_empty(), "Tool '{}' produced no nodes", seed.title);
            let root = &nodes[0];
            assert_eq!(root.node_type, "tool");
        }
    }
}

#[cfg(test)]
mod full_seed_db_tests {
    use super::*;
    use crate::local_agent::tools::Tool;
    use crate::prompt_assembler::PromptAssembler;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::markdown::prepare_nodes_from_template;
    use nodespace_core::services::NodeService;
    use std::sync::Arc;

    /// End-to-end regression for the missing-tools half of the seed bug.
    ///
    /// `seed_nodes_from_templates` inserts every template group with a single
    /// `?` error path: if any node fails validation, the whole remaining batch
    /// is aborted. Previously the `create_schema` tool node (the 6th tool) was
    /// rejected by the external-tool `parameter_schema` depth guard, which
    /// silently dropped it AND every tool seeded after it (update_schema,
    /// update_task_status, create_relationship, get_related_nodes,
    /// search_skills, delete_node, create_nodes_from_markdown). The model was
    /// then offered only the first 5 tools — no create_schema, no search_skills.
    ///
    /// This seeds exactly the way the daemon does (prompts + skills + tools in
    /// one batch) and asserts every registered tool lands in the DB.
    #[tokio::test]
    async fn full_daemon_seed_inserts_all_tool_nodes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = tmp.path().join("t.db");
        let mut store = Arc::new(SqliteStore::new(db).await.unwrap());
        let ns = Arc::new(NodeService::new(&mut store).await.unwrap());

        let prompts = PromptAssembler::seed_prompt_nodes();
        let skills = seed_skill_nodes();
        let tools = seed_tool_nodes();
        let mut groups = Vec::new();
        for t in prompts.iter().chain(skills.iter()).chain(tools.iter()) {
            groups.push(prepare_nodes_from_template(t).expect("template expands"));
        }
        ns.seed_nodes_from_templates(groups)
            .await
            .expect("full daemon seed must succeed (no tool dropped by validation)");

        let q = nodespace_core::ops::node_ops::query_nodes(
            &ns,
            nodespace_core::ops::node_ops::QueryNodesInput {
                node_type: Some("tool".to_string()),
                parent_id: None,
                root_id: None,
                limit: Some(256),
                offset: None,
                collection_id: None,
                collection: None,
                filters: None,
            },
        )
        .await
        .unwrap();
        let names: Vec<String> = q
            .nodes
            .iter()
            .filter_map(|n| n.get("content").and_then(|v| v.as_str()).map(String::from))
            .collect();

        assert_eq!(
            names.len(),
            Tool::ALL.len(),
            "all {} tool nodes should be seeded, got {:?}",
            Tool::ALL.len(),
            names
        );
        // The two tools the seed bug previously dropped must be present.
        assert!(
            names.iter().any(|n| n == "create_schema"),
            "create_schema must seed"
        );
        assert!(
            names.iter().any(|n| n == "search_skills"),
            "search_skills must seed"
        );
    }
}
