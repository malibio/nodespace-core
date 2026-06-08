//! Skill and tool node seeding templates.
//!
//! Provides the default skill and tool nodes seeded on first run. Each
//! [`NodeTemplate`] produces one root node plus any children. Use
//! [`nodespace_core::markdown::prepare_nodes_from_template`] to expand a
//! template into a flat list of `PreparedNode`s before inserting them via
//! `NodeService::bulk_create_hierarchy`.
//!
//! Issue #1130: The previous push-based [`SkillPipeline`] (pre-LLM intent
//! routing with confidence thresholds + tool whitelist scoping) has been
//! removed. Skill discovery is now LLM-orchestrated through the
//! `search_skills` tool exposed by [`crate::local_agent::tools`], so the
//! agent loop no longer needs a pipeline object — only the seeded skill
//! nodes themselves remain.
//!
//! Issue #1353: Tool nodes (`node_type='tool'`) bridge graph storage to
//! deterministic Rust handlers. Each tool is seeded as a node carrying its
//! handler key, typed parameter schema, description, and `source` provenance.

use nodespace_core::markdown::NodeTemplate;

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
                "tool_whitelist": ["search_semantic", "search_nodes", "execute_query", "get_node"],
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

LISTING BY TYPE: To list all nodes of a type, use search_nodes with an empty query. Examples:
- "list all tasks" → search_nodes(query="", node_type="task")
- "list all customers" → search_nodes(query="", node_type="<customer-schema-id>")

STRUCTURED PROPERTY QUERIES: For filtering by property values (status, due_date, etc.) or using comparison operators (gt, lt, gte, lte, in), use execute_query — NOT search_nodes. Examples:
- "find all my open tasks" → execute_query(target_type="task", filters=[{"type":"property","operator":"equals","property":"status","value":"open"}])
- "tasks due tomorrow" → execute_query(target_type="task", filters=[{"type":"property","operator":"equals","property":"due_date","value":"2026-06-09"}], sorting=[{"field":"due_date","direction":"asc"}])
- "tasks due this week" → execute_query(target_type="task", filters=[{"type":"property","operator":"gte","property":"due_date","value":"2026-06-08"},{"type":"property","operator":"lte","property":"due_date","value":"2026-06-14"}])
- "find tasks for Acme" → execute_query(target_type="task", filters=[{"type":"property","operator":"equals","property":"company","value":"Acme"}])
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
            markdown_content: r#"# Schema Creation & Editing Guidance

CREATING A SCHEMA — call create_schema:

ONE SCHEMA PER REQUEST: Create exactly the type the user named, in a single create_schema call, then stop and report it. Do NOT proactively invent or create related types (e.g. asked for "Invoice", do not also create "Customer" or "Product"), and do NOT follow up with update_schema to wire relationships unless the user explicitly asked for them. A relationship's targetType must already exist in ENTITY TYPES; if it doesn't, omit the relationship rather than creating the other type.

SUCCESS: After create_schema returns a schema object (with fields, type_id, etc.), the schema was created. Respond to the user immediately — do NOT call create_schema again. If create_schema returns an error saying the schema already exists, stop and tell the user the type already exists and they can create instances with create_node.

EDITING A SCHEMA — call update_schema:

When the user wants to add a field, remove a field, rename a field, or change a relationship on an existing schema, call update_schema with the schema_id and only the fields that need changing. Do NOT re-create the whole schema. Use add_fields, remove_fields, rename_fields, or update the description/title_template as needed.

FIELDS: Only define type-specific fields. Do NOT add a 'name' or 'title' field — every node already has a built-in content/title field. EXCEPTION: if you use a 'name' placeholder in title_template (e.g. "{name} ({status})"), you MUST define 'name' as a text field so title generation works. A 'description' field is acceptable when it adds value beyond the title. Good fields: status (enum), due_date (date), priority (enum), budget (number), owner (text).

ENUMS: Use lowercase values with readable labels, e.g. {"value": "in_progress", "label": "In Progress"}.

RELATIONSHIPS: Use relationships (not fields) when a field references another node type. The targetType MUST be an existing schema ID from the ENTITY TYPES list in the system prompt — do NOT invent types that aren't listed. If the target type doesn't exist yet, omit the relationship entirely. Examples:
- Invoice billed_to customer (one): {"name": "billed_to", "targetType": "customer", "direction": "out", "cardinality": "one"}
- Project has_task task (many): {"name": "has_task", "targetType": "task", "direction": "out", "cardinality": "many"}

TITLE TEMPLATE: Set title_template when a node's identity comes from its fields rather than free-form content. Use {field_name} placeholders. CRITICAL: every placeholder in title_template MUST be defined as a field in the fields array. Examples:
- Customer with fields [first_name, last_name]: title_template = "{first_name} {last_name}"
- Invoice with fields [invoice_number, ...]: title_template = "Invoice #{invoice_number}"
- Project with fields [name, status, ...]: title_template = "{name} ({status})"
Omit title_template if the content/title field alone identifies the node.

EXAMPLE — Invoice schema (references existing 'customer' type):
{
  "name": "Invoice",
  "description": "A billing invoice linked to a customer",
  "title_template": "Invoice #{invoice_number}",
  "fields": [
    {"name": "invoice_number", "type": "text", "required": true},
    {"name": "issue_date", "type": "date", "required": true},
    {"name": "due_date", "type": "date"},
    {"name": "amount", "type": "number", "required": true},
    {"name": "status", "type": "enum", "required": true, "coreValues": [
      {"value": "draft", "label": "Draft"},
      {"value": "sent", "label": "Sent"},
      {"value": "paid", "label": "Paid"},
      {"value": "overdue", "label": "Overdue"}
    ]}
  ],
  "relationships": [
    {"name": "billed_to", "targetType": "customer", "direction": "out", "cardinality": "one"}
  ]
}

EXAMPLE — Project schema (title_template uses {name} AND {status}, so BOTH are in fields):
{
  "name": "Project",
  "description": "A tracked project with status and timeline",
  "title_template": "{name} ({status})",
  "fields": [
    {"name": "name", "type": "text", "required": true},
    {"name": "status", "type": "enum", "required": true, "coreValues": [
      {"value": "planning", "label": "Planning"},
      {"value": "active", "label": "Active"},
      {"value": "on_hold", "label": "On Hold"},
      {"value": "completed", "label": "Completed"}
    ]},
    {"name": "start_date", "type": "date"},
    {"name": "due_date", "type": "date"},
    {"name": "budget", "type": "number"},
    {"name": "owner", "type": "text"}
  ],
  "relationships": [
    {"name": "has_task", "targetType": "task", "direction": "out", "cardinality": "many"}
  ]
}"#.to_string(),
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
            markdown_content: r#"# Graph Editing Guidance

When updating an existing node:

FIND THEN UPDATE: If you don't have the node's ID, call search_semantic or search_nodes first to locate it. Then call update_node with the ID and only the fields that need changing.

AMBIGUITY: If search returns 0 results or multiple results that don't clearly match what the user described, ask one specific clarifying question rather than retrying. Examples:
- 0 results: "I couldn't find an invoice matching that description. Are you looking for the invoice with amount $500?"
- Multiple results: "I found 3 invoices — which one did you mean: Invoice #001 ($500), Invoice #002 ($750), or Invoice #003 ($500 overdue)?"

TASK STATUS: To change a task's status (open, in_progress, done, cancelled), call update_task_status with the task ID and the new status string. Do NOT use update_node for task status changes.

update_node FIELDS: Pass only the properties that need to change. Omit properties that should stay the same. Use the exact property key from the node's schema.

CONTENT vs PROPERTIES: Use the content field to update the node's title/main text. Use properties for typed fields (status, due_date, amount, etc.).

SUCCESS: After update_node or update_task_status returns, confirm the change to the user. Do NOT re-fetch the node to confirm — the update response confirms success."#.to_string(),
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
            markdown_content: r#"# Relationship Management Guidance

When linking nodes or exploring connections:

CREATING A RELATIONSHIP: Call create_relationship with the source node ID, target node ID, and a relation_type label (e.g. "has_task", "billed_to", "related_to"). Both node IDs must exist — search for them first if you don't have them.

TRAVERSING RELATIONSHIPS: Call get_related_nodes with a node ID to fetch its connected nodes. Use the direction parameter ("out", "in", or "both") to control traversal direction. Filter by relation_type to narrow results.

FIND BEFORE LINK: If the user says "link X to Y" and you don't have both IDs, call search_semantic or search_nodes once per entity to resolve them, then call create_relationship.

SUCCESS: After create_relationship returns, confirm the link to the user. Do NOT call get_related_nodes to verify — the create response confirms success."#.to_string(),
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
            markdown_content: r#"# Node Deletion Guidance

When deleting a node:

FIND THEN DELETE: If you don't have the node's ID, call search_semantic or search_nodes to locate it. Confirm the title matches what the user described, then call delete_node with the ID.

SINGLE DELETE: Call delete_node once per node. Confirm each deletion before proceeding to the next.

SUCCESS: After delete_node returns, confirm to the user what was deleted. Do NOT search again to verify deletion."#.to_string(),
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
            markdown_content: r#"# Bulk Import Guidance

When importing a document or creating multiple nodes from markdown:

CALL create_nodes_from_markdown ONCE: Pass the markdown content directly. The tool parses headings into a node hierarchy — top-level headings become root nodes, sub-headings become children.

COLLECTION: If the user specifies a collection or folder name, pass it as the collection parameter.

NODE TYPE: Default to node_type="text" for general documents. Use a specific type if the user names one.

SUCCESS: After create_nodes_from_markdown returns, report the number of nodes created. Do NOT follow up with search calls."#.to_string(),
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
            markdown_content: r#"# Organization Guidance

When organizing nodes into collections or categories:

FIND THE NODE: If you don't have the node's ID, call search_semantic to locate it.

ADD TO COLLECTION: Call create_relationship with the node ID as source, the collection node ID as target, and relation_type="member_of". If the collection doesn't exist as a node yet, ask the user to create it first using the Node Creation skill.

SUCCESS: After create_relationship returns, confirm to the user that the node has been organized into the collection."#.to_string(),
        },
    ]
}

/// Default tool node templates seeded on first run (issue #1353).
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

    // -- Tool node seeding tests (issue #1353) --------------------------------

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
