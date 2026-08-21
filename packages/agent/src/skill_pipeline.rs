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
    AMBIGUITY_CLARIFY, BULK_IMPORT_NO_FOLLOWUP_SEARCH, EDIT_DONT_RECREATE, FIND_THEN_ACT,
    ONE_SCHEMA_PER_REQUEST, ORG_NEEDS_EXISTING_COLLECTION, RELATIONSHIP_VS_FIELD,
    RENAME_VS_RELABEL, SCHEMA_ALREADY_EXISTS, SCHEMA_VALIDATION_ERROR_RETRY, SINGLE_ITEM_PER_CALL,
    SUCCESS_NO_REVERIFY, TARGET_TYPE_MUST_EXIST, TASK_STATUS_DEDICATED_VERB,
    TITLE_TEMPLATE_PLACEHOLDERS, UNIQUE_FIELD_FLAGS,
};
use nodespace_core::markdown::{NodeTemplate, SeedTier};

/// Builds the Schema Creation skill's markdown_content.
///
/// Argument shape (which fields to omit, enum value casing, the
/// `title_template` placeholder rule) now lives on `create_schema`'s own
/// tool-schema descriptions (`local_agent/tools.rs`) — ADR-064 rule 1 — so
/// the rules that state only that shape (`NO_NAME_TITLE_FIELD`,
/// `NAME_PLACEHOLDER_EXCEPTION`, `FIELDS_FROM_REQUEST_ONLY`, `ENUM_FORMAT`)
/// are no longer interpolated here: duplicating a schema-stated rule into
/// prose is how the two drifted before (the `coreValues`/`core_values`
/// contradiction ADR-064 records). The worked JSON examples moved the same
/// way, onto `create_schema`'s own description, per ADR-038 Finding 2 (the
/// model reproduces the worked example's pattern, so the example belongs
/// where the model reads it right before calling).
///
/// What remains here is procedure: which tool to call, in what order, and
/// facts about the API's response contract (already-exists, validation-error
/// retry) that a schema cannot express. These rules are still interpolated
/// from [`crate::skill_rules`] so they cannot drift from the "Schema
/// inspection and management" section of `packages/skill/SKILL.md`
/// (`bin/gen_skill_md.rs` renders the same source in prose form for that,
/// separate, consumer).
fn schema_creation_guidance() -> String {
    format!(
        r#"# Schema Creation & Editing Guidance

CREATING A SCHEMA — call create_schema:

CALL create_schema NOW: your next action is the tool call, not planning text.

{one_schema_per_request}

{schema_already_exists}

{schema_validation_error_retry}

{edit_dont_recreate}

{rename_vs_relabel}

{relationship_vs_field} {target_type_must_exist}

{title_template_placeholders}

{unique_field_flags}"#,
        one_schema_per_request = ONE_SCHEMA_PER_REQUEST.imperative,
        schema_already_exists = SCHEMA_ALREADY_EXISTS.imperative,
        schema_validation_error_retry = SCHEMA_VALIDATION_ERROR_RETRY.imperative,
        edit_dont_recreate = EDIT_DONT_RECREATE.imperative,
        rename_vs_relabel = RENAME_VS_RELABEL.imperative,
        relationship_vs_field = RELATIONSHIP_VS_FIELD.imperative,
        target_type_must_exist = TARGET_TYPE_MUST_EXIST.imperative,
        title_template_placeholders = TITLE_TEMPLATE_PLACEHOLDERS.imperative,
        unique_field_flags = UNIQUE_FIELD_FLAGS.imperative,
    )
}

/// Builds the Graph Editing skill's markdown_content, interpolating shared
/// interaction rules from [`crate::skill_rules`] (find-then-act, ambiguity
/// clarification, dedicated task-status verb, success-means-stop).
///
/// The allowed-values / id-provenance rules below are stated in prose here
/// as well as on `update_node`'s own schema (`local_agent/tools.rs`) — not a
/// duplication ADR-064 forbids, because this text is procedure (WHEN to
/// treat a description as already-resolved vs. needing `resolve_query`),
/// not argument shape. The two invoice worked examples formerly here are
/// deleted outright rather than reworded: no case exercising this skill
/// needs a worked clarification example, and a business-domain one taught
/// nothing this text's own principle didn't already state.
fn graph_editing_guidance() -> String {
    format!(
        r#"# Graph Editing Guidance

When updating an existing node:

CALL update_node NOW: your next action is the tool call, not planning text.

FIND THEN UPDATE: {find_then_act} Then call update_node with the ID and only the fields that need changing.

ALREADY IN THIS CONVERSATION: an indirect reference like "the auth one", "that one", or "the 2400 one" can still name a record already returned by a prior tool result in this conversation — match the description against those records and use that record's id, matching on what the description says, not on which record was discussed last. Never ask the user to supply an id that's already in the conversation.

INDIRECT AND NOT YET FOUND: If the request identifies the target indirectly and no matching record has appeared in this conversation yet — a bare value without naming its field (an amount, a code), a relative date or status word (a weekday, "overdue", "recent"), or a paraphrased description — call resolve_query(request=<the request verbatim>, node_type) FIRST instead of hand-writing a search_nodes query yourself. resolve_query performs the search itself: if it returns resolved:true, act on the returned id directly (e.g. pass it straight to update_node) — do not call search_nodes afterward. If it returns resolved:false with reason:"no_match", tell the user nothing matched. If it returns reason:"multiple_matches", call route_clarify with one specific question naming the candidates as options — do not ask which one in prose.

AN ID ALONE CHANGES NOTHING: a call carrying only an id is a no-op that reports success — every call must also carry the change itself in `field_values`, using a field the type defines, copied character for character. When a field lists allowed values, use one of those values exactly — never a paraphrase of the user's wording, never a capitalised or spaced form of the value.

{ambiguity_clarify}

{task_status_dedicated_verb}

CONTENT vs FIELD VALUES: Use the content field only when the user is renaming the node. Use field_values for typed fields (status, assignee, due_date, etc.).

SUCCESS: {success_no_reverify}"#,
        find_then_act = FIND_THEN_ACT.imperative,
        ambiguity_clarify = AMBIGUITY_CLARIFY.imperative,
        task_status_dedicated_verb = TASK_STATUS_DEDICATED_VERB.imperative,
        success_no_reverify = SUCCESS_NO_REVERIFY.imperative,
    )
}

/// Builds the Relationship Management skill's markdown_content, interpolating
/// the shared find-then-act and success-means-stop rules.
/// The DIRECTION rule below is stated in prose here as well as on
/// `create_relationship`'s own `from_id`/`to_id` parameter descriptions
/// (`local_agent/tools.rs`) — a deliberate duplication, not a drift risk left
/// unguarded: `create_relationship` is whitelisted by BOTH this skill and
/// Organization (`seed_skill_nodes`'s "Organization" entry), and Organization's
/// own guidance does not restate it. A turn routed to Organization instead of
/// here would see `create_relationship` with only the tool schema's copy of
/// the rule, so the tool schema alone must already carry it — the same
/// reachability argument `graph_editing_guidance`'s doc comment makes for its
/// id-provenance rules.
fn relationship_management_guidance() -> String {
    format!(
        r#"# Relationship Management Guidance

When linking nodes or exploring connections:

CALL create_relationship NOW: your next action is the tool call, not planning text.

CREATING A RELATIONSHIP: both ids come from a prior tool result — copy each exactly, do not ask the user for either. The relation_type must be one declared on the source record's own type (e.g. "supersedes", "has_task"), or one of the four universal names legal between any two records: member_of, has_child, mentions, has_role. Any other name is rejected — when no declared relation fits, use "mentions".

DIRECTION: from_id is the record that ACTS, to_id is the record acted upon. "A supersedes B" is from_id=A, to_id=B. Reversing them records the opposite fact and still reports success.

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
/// Each template produces one skill root node plus ordinary markdown children
/// (header/text, inferred from the guidance markdown's structure) carrying the
/// guidance body. Tool whitelists and max_iterations are still stored
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
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
- Use 'threshold' to tune precision: default 0.3. Lower to 0.1-0.2 for broader recall when results are sparse.
- Use 'include_archived'=true only when the user explicitly asks for archived or historical content.
- Use 'exclude_collections' to suppress noisy collections (e.g. exclude_collections=["Archived"]).
- Use 'include_edges'=true to get relationship data (outgoing 'mentions' edges) with each result — saves a separate get_related_nodes call.
- Use 'graph_boost'=true to rank well-connected nodes higher (blends similarity with graph connectivity). Useful when the user wants the most referenced/central node on a topic.
- Use 'property_filters' for simple key-value filtering (e.g. property_filters={"status": "done"}). Prefer 'node_types' for type filtering.

MULTIPLE DOCUMENTS: If the user asks about multiple topics, call search_semantic once per topic rather than searching broadly and fetching each result individually.

search_nodes is the single tool for finding, listing, and filtering nodes — by title, by type, and by typed property. It returns each node's properties.

LISTING BY TYPE: To list all nodes of a type, use search_nodes with an empty query. Examples:
- "list all tasks" → `search_nodes(query="", node_type="task")`
- "show me our ADRs" → `search_nodes(query="", node_type="<adr-schema-id>")`

STRUCTURED PROPERTY QUERIES: To filter by property values (status, due_date, etc.) or comparison operators (gt, lt, gte, lte, in), pass filters to search_nodes. Copy the type id and the field name exactly as they appear in EXISTING SCHEMAS, and use the exact enum member for a status filter — never a paraphrase. Examples:
- "which tickets are still in dev?" → `search_nodes(node_type="ticket", filters=[{"type":"property","operator":"equals","property":"status","value":"in_dev"}])`
- "tasks due tomorrow" → `search_nodes(node_type="task", filters=[{"type":"property","operator":"equals","property":"due_date","value":"<tomorrow's date in YYYY-MM-DD>"}], sorting=[{"field":"due_date","direction":"asc"}])`
- "tasks due this week" → `search_nodes(node_type="task", filters=[{"type":"property","operator":"gte","property":"due_date","value":"<today's date in YYYY-MM-DD>"},{"type":"property","operator":"lte","property":"due_date","value":"<end of week in YYYY-MM-DD>"}])`
- Date format: always YYYY-MM-DD. Operators: equals, contains, gt, lt, gte, lte, in, exists."#.to_string(),
        },
        NodeTemplate {
            title: "Node Creation".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Create new nodes, records, entries, or instances of any type — tasks, text notes, or custom types like Spec, ADR, Ticket. Use when user wants to add, create, or insert a new item, record, entry, or example of an existing type.",
                "tool_whitelist": ["create_node", "search_semantic", "search_nodes", "get_node"],
                "max_iterations": 3,
            }),
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
            markdown_content: r#"# Node Creation Guidance

CALL create_node NOW: your next action is the tool call, not planning text.

THE TYPE: set node_type to the id shown in EXISTING SCHEMAS, copied exactly.

THE VALUES: put every particular the user supplied into field_values. Work through their message value by value and check each against the type's field list before calling. field_values is the ONLY way any value is stored — a value left out is lost silently while the record still reports as saved.

VALUES WITH NO MATCHING FIELD: If the user supplies a particular the listed fields do not cover, still put it in field_values under a key of your own — lowercase, singular, snake_case, named after the user's own noun for it. NEVER drop a value because the type has no field for it: a dropped value is gone silently and the user was told the record was saved. Bare on a type from EXISTING SCHEMAS; `custom:`-prefixed on a built-in type — text, task, date — where unprefixed names are reserved for built-in fields. Do NOT call create_schema or update_schema to add the field first; put the value in this create_node call.

SUCCESS: After create_node returns a node ID, confirm to the user what was created and STOP. Do NOT call get_node or any other tool — the create response is sufficient. The task is complete."#.to_string(),
        },
        NodeTemplate {
            title: "Schema Creation".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Set up a structured way to keep track of, log, or maintain records for a kind of thing the user hasn't stored before — specs, sprints, releases, tickets, or any recurring category of item with its own details to fill in. Also covers defining a new entity type or schema with custom fields, enums, and relationships, or modifying an existing schema. Use when the user wants a place to record or organize instances of something new, or says 'new type', 'node type', 'define fields', 'create schema', 'update schema', 'add a field', 'rename a field', or wants to design or change a kind of entity like Spec, Ticket, or ADR.",
                "tool_whitelist": ["create_schema", "update_schema", "get_node"],
                "max_iterations": 3,
            }),
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
            markdown_content: schema_creation_guidance(),
        },
        NodeTemplate {
            title: "Graph Editing".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: serde_json::json!({
                "description": "Modify existing nodes in the knowledge graph - update content, properties, titles, and metadata. For tasks, use update_task_status to change status.",
                "tool_whitelist": ["update_node", "update_task_status", "get_node", "search_nodes", "search_semantic", "resolve_query", "route_clarify"],
                "max_iterations": 3,
            }),
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
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
                tier: SeedTier::System,
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

    /// The Node Creation skill must tell the model what to do with a user value
    /// that no listed schema field covers.
    ///
    /// Without this, the guidance only ever covered values that MATCH a listed
    /// field, and a supplied particular with no home was silently discarded
    /// while the user was told the record saved — the agent-matrix scenario-4
    /// failure ("replacement cost 2400" against a schema with no cost field,
    /// where the model replied that the schema "does not currently support
    /// logging replacement costs" and persisted zero properties). Storage
    /// accepts undeclared keys (proved by
    /// `core/tests/create_node_property_persistence_test.rs`), so this gap was
    /// purely in what the model was told.
    #[test]
    fn node_creation_guidance_covers_values_with_no_matching_field() {
        let seeds = seed_skill_nodes();
        let node_creation = seeds
            .iter()
            .find(|s| s.title == "Node Creation")
            .expect("Node Creation skill must exist");
        let md = &node_creation.markdown_content;

        assert!(
            md.contains("VALUES WITH NO MATCHING FIELD"),
            "Node Creation guidance must address values the listed fields don't cover"
        );
        // The two failure modes actually observed, both named explicitly so a
        // future reword that drops either one fails here rather than in an eval.
        assert!(
            md.contains("NEVER drop a value"),
            "guidance must forbid discarding an unmatched value"
        );
        // Pins the RULE, not the quoted symptom: a reword that keeps the
        // prohibition but drops the exact phrase should still pass.
        assert!(
            md.contains("NEVER drop a value") && md.contains("gone silently"),
            "guidance must forbid discarding a value and say why it is harmful"
        );
        // The value belongs in THIS call, not behind a schema round-trip that
        // the skill's own tool whitelist cannot make anyway.
        assert!(
            md.contains("Do NOT call create_schema or update_schema"),
            "guidance must route the value into this create_node call"
        );
        let whitelist = tmpl_tool_whitelist(node_creation);
        assert!(
            !whitelist.contains(&"create_schema".to_string())
                && !whitelist.contains(&"update_schema".to_string()),
            "whitelist must not offer a schema escape hatch the guidance forbids"
        );
    }

    /// The same rule must hold on the tool schema itself, which is what the
    /// model sees when routing lands on a skill other than Node Creation — the
    /// scenario-4 traces show routing varying across arms while the empty-call
    /// outcome stayed identical, so guidance alone would leave the gap open on
    /// exactly the paths that were failing.
    #[test]
    fn create_node_tool_description_admits_keys_beyond_listed_fields() {
        let def = crate::local_agent::tools::Tool::CreateNode.definition();
        let props = def.parameters_schema["properties"]["field_values"]["description"]
            .as_str()
            .expect("field_values field must document itself");
        assert!(
            props.contains("Not limited to the listed fields"),
            "create_node must tell the model extra keys are allowed, got: {props}"
        );
    }

    /// Both surfaces state the SAME rule, so neither can be reworded into
    /// contradicting the other while its own test stays green.
    ///
    /// The rule is duplicated deliberately — routing does not always land on
    /// Node Creation, and on those turns the tool schema is the only surface
    /// carrying it — but deliberate duplication silently becomes divergence
    /// without something pinning the two together.
    ///
    /// The invariant pinned is ADR-063's: a value with no matching field is
    /// keyed bare on a user-defined type, and `custom:`-prefixed on a core
    /// type, where unprefixed names are reserved. Getting this wrong writes a
    /// bare key onto a core type — the collision `update_schema` rejects, and
    /// which `create_node` does NOT currently validate.
    #[test]
    fn both_surfaces_agree_on_the_undeclared_key_rule() {
        let node_creation_md = seed_skill_nodes()
            .into_iter()
            .find(|s| s.title == "Node Creation")
            .expect("Node Creation skill must exist")
            .markdown_content;
        let tool_desc = crate::local_agent::tools::Tool::CreateNode
            .definition()
            .parameters_schema["properties"]["field_values"]["description"]
            .as_str()
            .expect("field_values field must document itself")
            .to_string();

        for (surface, text) in [
            ("skill guidance", &node_creation_md),
            ("create_node tool schema", &tool_desc),
        ] {
            // Assert the PAIRING, not the mere presence of "custom:" — the
            // token appears in the worked example too, so a surface that
            // dropped the rule while keeping the example would still pass a
            // bare `contains("custom:")`. Verified by mutation: flipping the
            // tool schema to "bare on a built-in type" must fail this.
            let states_prefix_rule = text.contains("`custom:`-prefixed on a built-in type")
                || text.contains("prefix with `custom:`");
            assert!(
                states_prefix_rule,
                "{surface} must tie the custom: prefix to built-in types, not merely mention it"
            );
            for core_type in ["text", "task", "date"] {
                assert!(
                    text.contains(core_type),
                    "{surface} must name the core type '{core_type}' the prefix rule applies to"
                );
            }
            assert!(
                text.contains("reserved"),
                "{surface} must say why bare keys on core types are disallowed"
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

    /// Skill guidance children must come out as real markdown types (`header`,
    /// `text`, ...), not the retired `prompt` type — the entire point of
    /// `child_node_type: None` on every seed. Every seed's `markdown_content`
    /// starts with a `# Heading` line, so this also confirms `header` nodes
    /// are actually produced, not just `text`.
    #[test]
    fn seed_skill_children_are_real_markdown_types_not_prompt() {
        let seeds = seed_skill_nodes();
        for seed in &seeds {
            let nodes = prepare_nodes_from_template(seed)
                .unwrap_or_else(|e| panic!("Template '{}' failed: {:?}", seed.title, e));
            assert!(
                nodes.len() > 1,
                "Skill '{}' must have guidance children, not just the root",
                seed.title
            );

            let children = &nodes[1..];
            assert!(
                children.iter().any(|c| c.node_type == "header"),
                "Skill '{}' guidance starts with a markdown heading and must \
                 produce at least one 'header' child",
                seed.title
            );
            for child in children {
                assert_ne!(
                    child.node_type, "prompt",
                    "Skill '{}' child must not be typed 'prompt' — that type is retired; \
                     children must be ordinary markdown types",
                    seed.title
                );
                assert!(
                    matches!(child.node_type.as_str(), "header" | "text" | "code-block"),
                    "Skill '{}' child has unexpected node_type '{}' — expected 'header', \
                     'text', or 'code-block'",
                    seed.title,
                    child.node_type
                );
            }
        }
    }

    /// Skills whose guidance carries a worked JSON example must produce real
    /// `code-block` children, not JSON flattened into a prose `text` node.
    ///
    /// The seeding pipeline is a genuine markdown import — `prepare_nodes_from_markdown`
    /// turns ` ``` ` fences into `code-block` nodes — so an unfenced JSON example
    /// silently degrades to paragraph text. Fencing is what makes these examples
    /// round-trip as structured markdown (`flatten_subtree_content` re-emits each
    /// node's content verbatim, fence markers included).
    ///
    /// Neither seed carries a multi-line JSON example any more: both moved
    /// their worked example onto their tool's own description
    /// (`create_node`/`create_schema` in `local_agent/tools.rs`), per
    /// ADR-064 rule 1 and Finding 2 — the model reads the tool schema right
    /// before calling, so that is where the pattern it reproduces should
    /// live, not duplicated into the skill's prose too. This test now pins
    /// the negative: a future edit that re-adds a fenced JSON block to
    /// either skill's markdown_content should be a deliberate, reviewed
    /// choice, not a silent reintroduction of the duplication ADR-064
    /// removed.
    #[test]
    fn seed_skill_json_examples_produce_code_block_children() {
        // Neither seed carries a multi-line JSON worked example any more —
        // see the moved-to-tool-description note above. The rest use inline
        // code spans for short call shapes, which stay in `text`.
        let expected: &[(&str, usize)] = &[("Node Creation", 0), ("Schema Creation", 0)];

        for (title, want_blocks) in expected {
            let seed = seed_skill_nodes()
                .into_iter()
                .find(|s| s.title == *title)
                .unwrap_or_else(|| panic!("No seeded skill titled '{title}'"));
            let nodes = prepare_nodes_from_template(&seed)
                .unwrap_or_else(|e| panic!("Template '{title}' failed: {e:?}"));

            let code_blocks: Vec<&str> = nodes[1..]
                .iter()
                .filter(|n| n.node_type == "code-block")
                .map(|n| n.content.as_str())
                .collect();

            assert_eq!(
                code_blocks.len(),
                *want_blocks,
                "Skill '{title}' should produce {want_blocks} 'code-block' child(ren) from its \
                 fenced JSON example(s), got {}. Unfenced JSON is parsed as prose instead.",
                code_blocks.len()
            );

            for block in &code_blocks {
                assert!(
                    block.starts_with("```json"),
                    "Skill '{title}' code-block should keep its ```json fence verbatim so it \
                     round-trips as markdown, got: {}",
                    block.chars().take(40).collect::<String>()
                );
                let body = block
                    .trim_start_matches("```json")
                    .trim_end_matches("```")
                    .trim();
                serde_json::from_str::<serde_json::Value>(body).unwrap_or_else(|e| {
                    panic!("Skill '{title}' fenced example is not valid JSON: {e}\n{body}")
                });
            }
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

    /// Blast radius is derived from each seeded skill's `tool_whitelist`
    /// (ADR-038), so the derivation must agree with what the seeds actually
    /// declare. This pins the classification against the real seed data rather
    /// than a stub, and fails if a seed's whitelist gains or loses a write tool
    /// without that being an intentional change to its Stage-2 bar.
    #[test]
    fn seeded_skills_classify_by_blast_radius_as_expected() {
        let expected_mutating = [
            ("Node Creation", true),
            ("Schema Creation", true),
            ("Graph Editing", true),
            ("Relationship Management", true),
            ("Node Deletion", true),
            ("Bulk Import", true),
            ("Organization", true),
            ("Research & Search", false),
        ];

        for (title, should_mutate) in expected_mutating {
            let seed = seed_skill_nodes()
                .into_iter()
                .find(|t| t.title == title)
                .unwrap_or_else(|| panic!("seed skill '{title}' must exist"));
            let tools = tmpl_tool_whitelist(&seed);
            // Exercise the production classifier rather than restating its
            // rule: a reimplementation here would keep passing while
            // `skill_is_mutating` regressed, which is the opposite of what
            // this test is for.
            let candidate = crate::agent_types::SkillCandidate {
                id: format!("seed-{title}"),
                name: title.to_string(),
                description: String::new(),
                score: 1.0,
                tools: tools.clone(),
                instructions: String::new(),
                schema_metadata: serde_json::json!([]),
            };
            let mutates = crate::local_agent::routing::skill_is_mutating(&candidate);
            assert_eq!(
                mutates, should_mutate,
                "skill '{title}' blast radius changed; whitelist is {tools:?}"
            );
        }
    }

    /// No seeded skill declares `node_types`, so every candidate takes
    /// `find_skills`' unscoped branch — all non-core schemas, capped at
    /// `MAX_UNSCOPED_SCHEMA_METADATA`, UNLESS the query itself names exactly
    /// one non-core type (`skill_ops::schema_named_in_query`), in which case
    /// that one type's schema is all that's attached instead. On a query that
    /// doesn't resolve to a single named type, every candidate still carries
    /// an *identical* schema list, which is what makes the repeated
    /// `EXISTING SCHEMAS` copies in one Stage-2 prompt genuinely
    /// redundant rather than differently-scoped.
    ///
    /// This pins the premise of that finding: a seed gaining `node_types`
    /// would scope its copy to a subset, and the de-duplication argument would
    /// need re-measuring rather than silently ceasing to hold. (The
    /// query-named-type narrowing above is a separate, per-query mechanism —
    /// it does not give any seed a static `node_types` list, so this test's
    /// own premise, "no seed declares node_types", is unaffected by it.)
    #[test]
    fn no_seeded_skill_scopes_its_schema_metadata() {
        let scoped: Vec<String> = seed_skill_nodes()
            .into_iter()
            // Mirrors `find_skills`' predicate: an absent key and an empty
            // array both fall through to the unscoped branch, so only a
            // non-empty list actually scopes a candidate's schema_metadata.
            .filter(|seed| {
                seed.root_properties
                    .get("node_types")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty())
            })
            .map(|seed| seed.title)
            .collect();

        assert!(
            scoped.is_empty(),
            "these seeds now scope schema_metadata via node_types: {scoped:?} — the entity-block \
             duplication measurement assumed every candidate carries the same unscoped schema \
             list, so re-measure before relying on that finding"
        );
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

    /// `UNIQUE_FIELD_FLAGS` is interpolated into `schema_creation_guidance()`
    /// via a format-string placeholder — a future edit to that format string
    /// could silently drop the `{unique_field_flags}` slot with no compiler
    /// error, since the value is still a valid `String` either way. The
    /// `SCHEMA_RULES` structural tests (`no_rule_text_is_empty` etc.) only
    /// check the constant in isolation, not that it actually reaches the
    /// seeded markdown a model sees — this pins the advisory-only claim
    /// specifically, since a model that thinks `unique` is an enforced
    /// constraint could tell a user it will block duplicates, which is false
    /// (see `NodeService::find_duplicate_for`).
    ///
    /// The worked example demonstrating a unique-flagged field lives on
    /// `create_schema`'s own tool description now, per ADR-064 rule 1 and
    /// Finding 2 (the example the model reads right before calling is the
    /// one it reproduces) — checked below by
    /// `create_schema_tool_description_keeps_dev_domain_worked_example`, not
    /// here.
    #[test]
    fn schema_creation_guidance_covers_unique_field_flags() {
        let seeds = seed_skill_nodes();
        let schema_skill = seeds
            .iter()
            .find(|s| s.title == "Schema Creation")
            .expect("Schema Creation skill must exist");
        let md = &schema_skill.markdown_content;

        assert!(
            md.contains("unique_case_insensitive"),
            "Schema Creation guidance must mention unique_case_insensitive"
        );
        assert!(
            md.to_lowercase().contains("advisory only"),
            "Schema Creation guidance must state the unique flag is advisory only, \
             not an enforced constraint"
        );
    }

    /// The worked example that replaced Schema Creation's inline "EXAMPLE —
    /// Customer schema" block now lives on `create_schema`'s tool
    /// description (ADR-064 rule 1 / Finding 2). Pins its presence and its
    /// dev-workflow domain, so a future edit can't silently drop it or drift
    /// back to a business-domain example.
    #[test]
    fn create_schema_tool_description_keeps_dev_domain_worked_example() {
        let desc = crate::local_agent::tools::Tool::CreateSchema
            .definition()
            .description;
        assert!(
            desc.contains("Example call:") && desc.contains("\"coreValues\""),
            "create_schema's description must keep a worked example with a coreValues enum, got: {desc:?}"
        );
        assert!(
            desc.contains("Ticket") && desc.contains("ready_for_dev"),
            "create_schema's worked example must be dev-workflow domain (Ticket/status enum), got: {desc:?}"
        );
        for business_word in ["Invoice", "Customer", "Product"] {
            assert!(
                !desc.contains(business_word),
                "create_schema's description regressed to a business-domain example: found {business_word:?}"
            );
        }
    }

    /// Schema Creation's description must lead with the natural phrasing users
    /// actually use for wanting a new kind of thing tracked, not only technical
    /// schema vocabulary.
    ///
    /// Agent-matrix scenario 3 traced "I want to keep a record of the equipment
    /// my team checks out..." to semantic retrieval never surfacing this skill
    /// in the top-3 candidates — the description was a keyword list ("new
    /// type", "create schema") that this kind of request doesn't lexically
    /// match, so `create_schema` was unreachable and the model fell back to a
    /// bare `create_node` or split the request across two schemas trying to
    /// recover. This test can't validate retrieval outcome (that needs the
    /// real embedding model — see `bun run eval:agent`), but it pins the
    /// wording itself against an accidental future trim or revert, which
    /// would silently regress scenario 3 with no other signal in `cargo test`
    /// or `bun run test:all`.
    #[test]
    fn schema_creation_description_covers_natural_tracking_phrasing() {
        let seeds = seed_skill_nodes();
        let schema_skill = seeds
            .iter()
            .find(|s| s.title == "Schema Creation")
            .expect("Schema Creation skill must exist");
        let description = schema_skill
            .root_properties
            .get("description")
            .and_then(|v| v.as_str())
            .expect("Schema Creation must have a description");

        let natural_phrases = ["keep track of", "log", "maintain records for"];
        assert!(
            natural_phrases.iter().any(|p| description.contains(p)),
            "Schema Creation description must contain at least one natural-language \
             tracking phrase ({natural_phrases:?}) alongside the technical keyword list, \
             or semantic retrieval will miss requests phrased like \
             'I want to keep a record of X' (agent-matrix scenario 3). Got: {description:?}"
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

        let prompts = PromptAssembler::seed_agent_guidance_nodes();
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
