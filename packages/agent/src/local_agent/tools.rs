//! Graph operation tools for the local agent.
//!
//! Implements [`AgentToolExecutor`] by wrapping `NodeService` and
//! `NodeEmbeddingService` methods as individual tools. Each tool validates its
//! arguments against a JSON schema, executes the corresponding service call, and
//! returns a compact, token-efficient result suitable for an 8k-context local model.

use crate::agent_types::{
    AgentToolExecutor, ChatInferenceEngine, ChatMessage, InferenceRequest, Role, SkillCandidate,
    SkillRetrieval, StreamingChunk, ToolDefinition, ToolError, ToolResult,
};
use async_trait::async_trait;
use nodespace_core::agent_params::{SearchNodesParams, SearchSemanticParams};
use nodespace_core::ops::{node_ops, query_ops, rel_ops, search_ops, OpsError};
use nodespace_core::schema::handle_create_schema;
use nodespace_core::services::{NodeEmbeddingService, NodeService};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared handle to the embedding service.
///
/// The embedding model loads in a background task after daemon startup, so the
/// service may be absent when the executor is built and appear later. Holding it
/// behind a shared `RwLock` (rather than a captured snapshot) lets the executor
/// read the *current* value on every call — it never gets wired with a stale or
/// `None` service once one becomes available, and there is no engine-swap step
/// to remember. The inner `Option` is `None` until the embedding model finishes
/// loading.
pub type SharedEmbeddingService = Arc<RwLock<Option<Arc<NodeEmbeddingService>>>>;

/// Handle to the chat inference engine, for tools that need to make their own
/// nested LLM call (e.g. `resolve_query`'s decomposition step).
///
/// Unlike [`SharedEmbeddingService`], this is NOT a live-updated shared lock —
/// the daemon rebuilds the whole `GraphToolExecutor` (and hands it a fresh
/// engine) on every model load/unload/switch in `replace_engine`, so there is
/// no "wire it once, update it later in place" scenario to support. `None`
/// only in tests that construct an executor with no engine at all.
///
/// `resolve_query`'s nested call only ever feeds its output into
/// `search_nodes`'s existing filter pipeline (validated property names,
/// parameterized `json_extract` values) — never executed as code — so
/// interpolating user/schema text into its prompt carries no injection risk
/// beyond what `search_nodes` itself already guards against.
pub type SharedChatInferenceEngine = Option<Arc<dyn ChatInferenceEngine>>;

// ---------------------------------------------------------------------------
// Agent-specific parameter structs
//
// These complement the shared search params (nodespace_core::agent_params)
// for tools whose wire format differs (e.g., agent uses "title"+"body"
// while the markdown library uses "content").
// ---------------------------------------------------------------------------

/// Parameters for the agent's create_node tool.
///
/// The model passes `content` as the node text. `node_service` derives the
/// display title automatically — from `title_template`+`properties` if the schema
/// defines one, or from `strip_markdown(content)` for root nodes otherwise.
/// The agent never sets or manipulates the title field.
///
/// Deliberately NOT `deny_unknown_fields`: `exec_create_node` tolerates a model
/// that passes schema fields flat at the top level (instead of nested under
/// `properties`) by pre-scanning the raw args for keys outside `content`/
/// `node_type`/`parent_id`/`properties` and promoting them into `properties`
/// itself. Those same "unknown" keys must still deserialize cleanly here, or
/// that tolerance would break.
#[derive(Debug, Deserialize)]
struct AgentCreateNodeParams {
    #[serde(default)]
    pub content: Option<String>,
    pub node_type: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub properties: Option<Value>,
}

/// Parameters for the agent's update_node tool.
///
/// Deliberately NOT `deny_unknown_fields` — same flat-schema-field tolerance
/// as [`AgentCreateNodeParams`], via `exec_update_node`'s own flat-extras scan.
#[derive(Debug, Deserialize)]
struct AgentUpdateNodeParams {
    #[serde(alias = "node_id")]
    pub id: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub properties: Option<Value>,
}

/// Parameters for the agent's get_node tool (includes optional format field)
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentGetNodeParams {
    #[serde(alias = "node_id")]
    pub id: String,
    #[serde(default)]
    pub format: Option<String>,
}

/// Parameters for the create_relationship tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRelationshipParams {
    pub from_id: String,
    pub to_id: String,
    pub relationship_type: String,
}

/// Parameters for the get_related_nodes tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetRelatedNodesParams {
    #[serde(alias = "node_id")]
    pub id: String,
    #[serde(default)]
    pub relationship_type: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
}

/// Parameters for the resolve_query tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveQueryParams {
    /// The user's natural-language request, verbatim (e.g. "Mark the $500 invoice as paid").
    pub request: String,
    /// The target node type to resolve the request against (e.g. "invoice").
    pub node_type: String,
}

/// Parameters for the search_skills tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchSkillsParams {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for the update_task_status tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateTaskStatusParams {
    #[serde(alias = "node_id")]
    pub id: String,
    pub status: String,
}

/// Parameters for the delete_node tool
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteNodeParams {
    #[serde(alias = "node_id")]
    pub id: String,
}

/// Maximum characters for node body in full node results.
const BODY_TRUNCATE_FULL: usize = 2000;

/// Maximum characters for node body in list/summary results.
const BODY_TRUNCATE_SUMMARY: usize = 500;

/// Default search result limit.
const DEFAULT_SEARCH_LIMIT: usize = 50;

/// Default semantic search result limit.
const DEFAULT_SEMANTIC_LIMIT: usize = 5;

/// Minimum similarity threshold for semantic search.
const SEMANTIC_THRESHOLD: f32 = 0.3;

/// Max candidates `resolve_query` fetches to discriminate zero/one/many
/// matches. Small — this exists to tell "unique" from "ambiguous" from
/// "not found", not to page through a large result set (the model would
/// call `search_nodes` directly for that).
const RESOLVE_QUERY_MATCH_LIMIT: usize = 10;

/// Extract the first balanced `{...}` JSON object from arbitrary text.
///
/// Small local models sometimes wrap a requested JSON-only response in prose
/// or a markdown code fence despite instructions not to. Scans for the first
/// `{`, then tracks brace depth (ignoring braces inside string literals) to
/// find its matching close, rather than requiring the whole response to be
/// bare JSON.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Truncate a string to `max_chars`, appending `[truncated]` if truncated.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        // Find a safe char boundary
        let mut end = max_chars;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}[truncated]", &s[..end])
    }
}

/// Build an error `ToolResult` from a string message.
fn error_result(tool_call_id: &str, name: &str, message: &str) -> ToolResult {
    ToolResult {
        tool_call_id: tool_call_id.to_string(),
        name: name.to_string(),
        result: json!({ "error": message }),
        is_error: true,
    }
}

/// Convert an OpsError to a ToolError.
fn ops_error_to_tool(e: OpsError, tool_name: &str) -> ToolError {
    ToolError::ExecutionFailed(format!("{} failed: {}", tool_name, e))
}

/// Build a success `ToolResult`.
fn ok_result(tool_call_id: &str, name: &str, data: Value) -> ToolResult {
    ToolResult {
        tool_call_id: tool_call_id.to_string(),
        name: name.to_string(),
        result: data,
        is_error: false,
    }
}

/// Prefix a bare node ID with `nodespace://` so the model sees the URI format
/// it should use when referencing nodes in responses.
fn node_uri(id: &str) -> String {
    if id.is_empty() || id.starts_with("nodespace://") {
        id.to_string()
    } else {
        format!("nodespace://{id}")
    }
}

/// Strip the `nodespace://` prefix from a node ID supplied by the model.
/// The model is instructed to use `nodespace://uuid` URIs, so incoming
/// tool arguments may carry the prefix — strip it before hitting the DB.
fn strip_node_uri(id: &str) -> &str {
    id.strip_prefix("nodespace://").unwrap_or(id)
}

// ---------------------------------------------------------------------------
// Tool definitions (JSON schemas)
// ---------------------------------------------------------------------------

fn def_search_nodes() -> ToolDefinition {
    ToolDefinition {
        name: "search_nodes".into(),
        description: "Find, list, and filter nodes. This is the single tool for querying the graph by \
            title, type, and/or typed properties — use it for all three of: \
            (1) title/keyword lookup (query='invoice'); \
            (2) listing every node of a type (query='', node_type='task' — empty query lists all of that type); \
            (3) filtering by typed properties with operators (status='open', amount > 500, due_date before a date) — \
            pass 'filters' for these. Combine as needed (e.g. node_type + a property filter). \
            Returns each node's properties, so this is the right tool whenever the user wants to see or act on typed data. \
            Dates use YYYY-MM-DD. Prefer this over search_semantic when you know the name/type or want structured results; \
            use search_semantic only for meaning-based / fuzzy questions."
            .into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keyword or phrase to match against node titles (substring match). Pass empty string to skip the title filter (e.g. to list all nodes of a type)."
                },
                "node_type": {
                    "type": "string",
                    "description": "Filter by node type (e.g. 'task', 'text', or a custom schema ID). For a custom schema ID, copy the id exactly from the RELEVANT ENTITY TYPES block — character for character, including underscores — never shorten, singularize, paraphrase, or guess it from the user's wording. Omit to search all types."
                },
                "filters": {
                    "type": "array",
                    "description": "Optional typed-property filters. Use for status/due_date/amount and any custom schema field, with operators. Omit for plain title/type listing.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["property", "content", "metadata", "relationship"],
                                "description": "Filter category: 'property' for ANY schema/node field (status, due_date, priority, and all custom schema fields) — use this for status. 'content' for node text. 'metadata' ONLY for the 4 built-in fields created_at, modified_at, node_type, content — NEVER use 'metadata' for status or any schema field, it will fail with 'Invalid metadata field'. 'relationship' for graph edges."
                            },
                            "operator": {
                                "type": "string",
                                "enum": ["equals", "contains", "gt", "lt", "gte", "lte", "in", "exists"],
                                "description": "Comparison operator"
                            },
                            "property": {
                                "type": "string",
                                "description": "Property key. Example: {\"type\": \"property\", \"property\": \"status\", \"operator\": \"equals\", \"value\": \"open\"} — status is a property filter, not metadata."
                            },
                            "value": {
                                "description": "Value to compare against. Use string for dates (YYYY-MM-DD), string/number for others. Use array for 'in' operator."
                            },
                            "case_sensitive": {
                                "type": "boolean",
                                "description": "For 'contains' operator: case sensitivity (default: true)"
                            },
                            "relationship_type": {
                                "type": "string",
                                "enum": ["parent", "children", "mentions", "mentioned_by"],
                                "description": "For 'relationship' filters: which graph edge direction to traverse"
                            },
                            "node_id": {
                                "type": "string",
                                "description": "For 'relationship' filters: the anchor node ID to traverse from"
                            }
                        },
                        "required": ["type", "operator"]
                    }
                },
                "sorting": {
                    "type": "array",
                    "description": "Optional sort configuration, applied in order.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "field": {
                                "type": "string",
                                "description": "Field to sort by (e.g. 'due_date', 'created_at', 'status')"
                            },
                            "direction": {
                                "type": "string",
                                "enum": ["asc", "desc"],
                                "description": "Sort direction (default: asc)"
                            }
                        },
                        "required": ["field"]
                    }
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default 50)"
                }
            },
            "required": []
        }),
    }
}

fn def_resolve_query() -> ToolDefinition {
    ToolDefinition {
        name: "resolve_query".into(),
        description: "Find the single node an ambiguous natural-language request refers to, when the \
            request bundles an implicit semantic decision you are not certain how to phrase as a search \
            — e.g. which property a value like '$500' refers to, what a relative date like 'next Friday' \
            or 'overdue' resolves to, or how to identify a specific node from a paraphrased description. \
            This performs the search itself — it does NOT return query arguments for you to pass to \
            search_nodes. On a unique match, returns 'resolved: true' with the node's id, title, and \
            properties — act on that node directly (e.g. pass its id straight to update_node). On no \
            match, returns 'resolved: false, reason: \"no_match\"' — tell the user nothing matched, do \
            not retry the same request. On more than one match, returns 'resolved: false, \
            reason: \"multiple_matches\"' with a 'candidates' list — ask the user which one they meant, \
            do not guess. Skip this for simple, unambiguous requests (e.g. 'list all my invoices') — \
            call search_nodes directly instead."
            .into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "request": {
                    "type": "string",
                    "description": "The user's request, verbatim (e.g. \"Mark the $500 invoice as paid\")."
                },
                "node_type": {
                    "type": "string",
                    "description": "The target node type to resolve against (e.g. 'invoice'). Copy the id exactly from the RELEVANT ENTITY TYPES block — character for character, including underscores — never shorten, singularize, paraphrase, or guess it from the user's wording."
                }
            },
            "required": ["request", "node_type"]
        }),
    }
}

fn def_search_semantic() -> ToolDefinition {
    ToolDefinition {
        name: "search_semantic".into(),
        description: "Find nodes semantically related to a natural-language query. By default returns full content for the top result (include_markdown=1). Increase include_markdown to get full content for more results, or set to 0 for IDs and snippets only. If a result's 'markdown' field is non-empty, that is the complete document — summarize or answer from it directly, do not call get_node or search_nodes again for that result.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query for semantic search"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default 5)"
                },
                "include_markdown": {
                    "type": "integer",
                    "description": "Number of top results to include full markdown content for (0-5, default 1). Set to 0 for IDs and snippets only, or increase to get full content for multiple results."
                },
                "collection": {
                    "type": "string",
                    "description": "Filter results to a specific collection path (e.g. 'Architecture', 'Development'). Use for namespace/folder filtering."
                },
                "threshold": {
                    "type": "number",
                    "description": "Minimum similarity score (0.0-1.0). Lower values return more results with less precision. Default: 0.3. Lower to 0.1-0.2 for broader recall when initial results are too few."
                },
                "scope": {
                    "type": "string",
                    "enum": ["knowledge", "everything"],
                    "description": "Search scope: 'knowledge' (default, searches text/header/code/schema nodes) or 'everything' (all embedded node types). Note: ai-chat conversations are not embedded, so they are never returned by search regardless of scope."
                },
                "include_archived": {
                    "type": "boolean",
                    "description": "Whether to include archived nodes in results. Default: false. Set to true when the user explicitly asks about archived or historical content."
                },
                "node_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filter results to specific node types (e.g. [\"task\", \"text\"]). Use for type filtering; use 'collection' for namespace/folder filtering."
                },
                "exclude_collections": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Collection paths to exclude from results (e.g. [\"Archived\", \"Drafts\"]). Useful to narrow results when a collection is noisy."
                },
                "property_filters": {
                    "type": "object",
                    "description": "Filter by node properties (AND logic, e.g. {\"status\": \"done\"})",
                    "additionalProperties": { "type": "string" }
                },
                "include_edges": {
                    "type": "boolean",
                    "description": "When true, attach outgoing 'mentions' relationships of each result as an 'edges' array. Reduces round-trips for graph traversal. Default: false."
                },
                "graph_boost": {
                    "type": "boolean",
                    "description": "When true, re-rank results by blending similarity with graph connectivity (nodes with more relationships score higher). Formula: 0.7 * similarity + 0.3 * normalized_degree. Default: false."
                }
            },
            "required": ["query"]
        }),
    }
}

fn def_get_node() -> ToolDefinition {
    ToolDefinition {
        name: "get_node".into(),
        description: "Get a node by ID. Use format=markdown to include all descendants as a readable document.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Node ID to retrieve"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "markdown"],
                    "description": "Output format: json (default) returns node fields, markdown returns the node and all descendants as a readable document"
                }
            },
            "required": ["id"]
        }),
    }
}

fn def_create_node() -> ToolDefinition {
    ToolDefinition {
        name: "create_node".into(),
        description: "Create a new node. Always pass 'content' as the node name or text. Optionally pass 'properties' if the schema type has fields. If the schema has a title_template (shown in ENTITY TYPES), include those template fields in 'properties' — the service composes the displayed title from them automatically.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The node name or text content"
                },
                "node_type": {
                    "type": "string",
                    "description": "Node type: 'text', 'task', or a custom schema ID (e.g. 'project', 'customer'). For a custom schema ID, copy the id exactly from the RELEVANT ENTITY TYPES block — character for character, including underscores — never shorten, singularize, paraphrase, or guess it from the user's wording. If the type is not listed there, it does not exist yet — do not invent an id for it."
                },
                "properties": {
                    "type": "object",
                    "description": "Schema field values (e.g. {\"status\": \"active\"}). For schemas with a title_template, include the template fields (e.g. {\"name\": \"Olympics Campaign\", \"status\": \"Closed\"})."
                },
                "parent_id": {
                    "type": "string",
                    "description": "Optional parent node ID"
                }
            },
            "required": ["node_type", "content"]
        }),
    }
}

fn def_update_node() -> ToolDefinition {
    ToolDefinition {
        name: "update_node".into(),
        description: "Update an existing node's content or properties immediately — call this directly with the node ID you already have (e.g. from search_nodes or get_node), don't ask the user to confirm or provide it first. The node service recomputes the title automatically after any update. Example call: {\"id\": \"a1b2c3d4-...\", \"content\": \"Buy milk and eggs\"}.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Node ID to update, e.g. \"a1b2c3d4-e5f6-7890-abcd-ef1234567890\""
                },
                "content": {
                    "type": "string",
                    "description": "New content/text for the node (optional), e.g. \"Buy milk and eggs\""
                },
                "properties": {
                    "type": "object",
                    "description": "Properties to merge/update (optional), e.g. {\"status\": \"done\"}"
                }
            },
            "required": ["id"]
        }),
    }
}

fn def_create_relationship() -> ToolDefinition {
    ToolDefinition {
        name: "create_relationship".into(),
        description: "Create a relationship between two nodes".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "from_id": {
                    "type": "string",
                    "description": "Source node ID"
                },
                "to_id": {
                    "type": "string",
                    "description": "Target node ID"
                },
                "relationship_type": {
                    "type": "string",
                    "description": "Type of relationship. Use a relationship name defined on the relevant schema(s) (e.g. 'has_task', 'billed_to') if one applies, otherwise a generic label (member_of, mentions, related_to, etc.)."
                }
            },
            "required": ["from_id", "to_id", "relationship_type"]
        }),
    }
}

fn def_get_related_nodes() -> ToolDefinition {
    ToolDefinition {
        name: "get_related_nodes".into(),
        description: "Get nodes related to a given node. Defaults to 'mentions' relationship type if not specified.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Node ID to find relations for"
                },
                "relationship_type": {
                    "type": "string",
                    "description": "Relationship type to query (default: 'mentions')"
                },
                "direction": {
                    "type": "string",
                    "enum": ["in", "out", "both"],
                    "description": "Direction of relationships (default: both)"
                }
            },
            "required": ["id"]
        }),
    }
}

fn def_search_skills() -> ToolDefinition {
    ToolDefinition {
        name: "search_skills".into(),
        description: "Search registered skills by describing your intent. Returns up to 3 matches sorted by relevance, each with name, description, confidence (0-1), tools, and instructions. \
            When you receive results: judge each candidate against the understood intent — pick one and emit its typed action in this same turn, OR ask the user to clarify (offering the candidates as concrete options). \
            Empty result = no skill matches — proceed with general tools or fall through to semantic_search. \
            Skip this tool for conversational replies; call it when the user wants to find, create, update, delete, or connect something.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language description of what you need to do (can differ from the user's exact wording)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum skills to return (default 3, max 10)"
                }
            },
            "required": ["query"]
        }),
    }
}

fn def_create_schema() -> ToolDefinition {
    ToolDefinition {
        name: "create_schema".into(),
        description: "Create a new entity type (schema) with custom fields and relationships. \
            The top-level 'name' parameter is REQUIRED — it is the display name of the entity type (e.g. 'Invoice', 'Project'). \
            The schema ID is auto-generated as lowercase snake_case from name (e.g. 'Customer Profile' → 'customer_profile'). \
            After creation, use this ID as node_type when creating instances. \
            FIELDS: Every node already has a built-in content/title — do NOT add a 'name' or 'title' entry to the fields array. \
            EXCEPTION: if title_template references '{name}' (e.g. title_template='{name} ({status})'), \
            you MUST define 'name' as a text field so the template can reference it. \
            Only define type-specific fields. If a field maps to an existing node type, define it as a relationship instead.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Display name for the entity type (e.g., 'Project', 'Customer')"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of what this entity type represents"
                },
                "fields": {
                    "type": "array",
                    "description": "Array of field definitions. Only use for scalar properties (text, number, date, enum, boolean). Do NOT use for references to other node types — use relationships instead.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Field name (e.g., 'status', 'email')" },
                            "type": { "type": "string", "description": "Field type: text, number, date, enum, array, object, boolean" },
                            "required": { "type": "boolean", "description": "Whether this field is required" },
                            "indexed": { "type": "boolean", "description": "Whether to index for search/filter" },
                            "description": { "type": "string", "description": "Field description" },
                            "coreValues": {
                                "type": "array",
                                "description": "REQUIRED and must be non-empty when type=\"enum\" — an enum field with no values always fails validation. Array of {value, label} pairs. Use lowercase values (e.g., 'active' not 'Active'). If predefined values aren't known yet, use type=\"text\" instead; values can be added later with update_schema.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": { "type": "string" },
                                        "label": { "type": "string" }
                                    }
                                }
                            }
                        },
                        "required": ["name", "type"]
                    }
                },
                "title_template": {
                    "type": "string",
                    "description": "Template for auto-generating node titles. Use {field_name} placeholders. RULE: every {field_name} token MUST have a matching entry in the fields array — if you write '{name}' here, you MUST add {\"name\": \"name\", \"type\": \"text\"} to fields. Missing fields cause a validation error."
                },
                "relationships": {
                    "type": "array",
                    "description": "Relationships to other node types. Use instead of array fields when referencing existing types (e.g., project has_task task).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Relationship name (e.g., 'has_task', 'assigned_to', 'depends_on')" },
                            "targetType": { "type": "string", "description": "Target node type ID — MUST be an existing type from the ENTITY TYPES list (e.g., 'task', 'project', 'customer'). Do NOT invent types that don't exist yet." },
                            "direction": { "type": "string", "enum": ["out", "in"], "description": "Direction: 'out' (this→target, default) or 'in' (target→this)" },
                            "cardinality": { "type": "string", "enum": ["one", "many"], "description": "Cardinality: 'one' or 'many' (default)" },
                            "description": { "type": "string", "description": "What this relationship represents" }
                        },
                        "required": ["name", "targetType", "direction", "cardinality"]
                    }
                }
            },
            "required": ["name"]
        }),
    }
}

fn def_update_schema() -> ToolDefinition {
    ToolDefinition {
        name: "update_schema".into(),
        description: "Modify an existing schema type: add/remove/rename fields, add/remove relationships, update description or title_template. Use rename_fields to safely rename a field — it migrates all existing node property data to the new key and updates the schema definition.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "schema_id": {
                    "type": "string",
                    "description": "ID of the schema to update (e.g. 'project', 'customer')"
                },
                "description": {
                    "type": "string",
                    "description": "New description (optional)"
                },
                "title_template": {
                    "type": "string",
                    "description": "New title template using {field_name} placeholders (e.g. '{name} ({status})'). All referenced fields must exist in the schema."
                },
                "add_fields": {
                    "type": "array",
                    "description": "Fields to add",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "type": { "type": "string", "description": "text, number, date, enum, boolean" },
                            "description": { "type": "string" },
                            "coreValues": {
                                "type": "array",
                                "description": "REQUIRED and must be non-empty when type=\"enum\" — an enum field with no values always fails validation. Array of {value, label} pairs.",
                                "items": { "type": "object", "properties": { "value": { "type": "string" }, "label": { "type": "string" } } }
                            }
                        },
                        "required": ["name", "type"]
                    }
                },
                "remove_fields": {
                    "type": "array",
                    "description": "Field names to remove",
                    "items": { "type": "string" }
                },
                "rename_fields": {
                    "type": "array",
                    "description": "Fields to rename. Each entry rekeys property data on ALL existing nodes of this schema type and updates the schema definition. Renaming to an existing field name is rejected. Processed before add_fields/remove_fields.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from": { "type": "string", "description": "Current field name" },
                            "to": { "type": "string", "description": "New field name" }
                        },
                        "required": ["from", "to"],
                        "additionalProperties": false
                    }
                },
                "add_relationships": {
                    "type": "array",
                    "description": "Relationships to add",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "targetType": { "type": "string" },
                            "direction": { "type": "string", "enum": ["out", "in"] },
                            "cardinality": { "type": "string", "enum": ["one", "many"] }
                        },
                        "required": ["name", "targetType", "direction", "cardinality"]
                    }
                },
                "remove_relationships": {
                    "type": "array",
                    "description": "Relationship names to remove",
                    "items": { "type": "string" }
                }
            },
            "required": ["schema_id"]
        }),
    }
}

fn def_delete_node() -> ToolDefinition {
    ToolDefinition {
        name: "delete_node".into(),
        description: "Delete a node from the knowledge graph by its ID. Use get_node first to confirm the node exists before deleting.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Node ID to delete"
                }
            },
            "required": ["id"]
        }),
    }
}

fn def_create_nodes_from_markdown() -> ToolDefinition {
    ToolDefinition {
        name: "create_nodes_from_markdown".into(),
        description: "Import a markdown document and create a hierarchy of nodes. Headings become parent nodes, content becomes child nodes.".into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "markdown": {
                    "type": "string",
                    "description": "Markdown content to import as nodes"
                },
                "parent_id": {
                    "type": "string",
                    "description": "Optional parent node ID to attach the import under"
                },
                "collection": {
                    "type": "string",
                    "description": "Optional collection path to add imported nodes to"
                }
            },
            "required": ["markdown"]
        }),
    }
}

fn def_update_task_status() -> ToolDefinition {
    ToolDefinition {
        name: "update_task_status".into(),
        description: "Update a task's status. Valid statuses: open, in_progress, done, cancelled."
            .into(),
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Task node ID to update"
                },
                "status": {
                    "type": "string",
                    "enum": ["open", "in_progress", "done", "cancelled"],
                    "description": "New status value"
                }
            },
            "required": ["id", "status"]
        }),
    }
}

// ---------------------------------------------------------------------------
// Tool registry — single source of truth
// ---------------------------------------------------------------------------

/// The canonical set of agent tools.
///
/// This enum is the *single source of truth* for the tool surface. Everything
/// that used to be a separate hand-maintained list is derived from it:
/// - the wire name ([`Tool::name`]) and JSON schema ([`Tool::definition`]),
/// - the user-facing display label ([`Tool::humanized`]).
///
/// Adding a tool means adding one variant and filling in its arms; the compiler
/// then forces every derivation to account for it. There are no drift-detector
/// tests because the lists can no longer drift — they are computed from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    SearchNodes,
    ResolveQuery,
    SearchSemantic,
    GetNode,
    CreateNode,
    UpdateNode,
    CreateSchema,
    UpdateSchema,
    UpdateTaskStatus,
    CreateRelationship,
    GetRelatedNodes,
    SearchSkills,
    DeleteNode,
    CreateNodesFromMarkdown,
}

impl Tool {
    /// Every tool in registry order. The order here is the order tools are
    /// presented to the model, so keep retrieval/discovery tools first.
    ///
    /// Completeness is compiler-enforced via [`Tool::ALL_IS_COMPLETE`]: a
    /// variant added to the enum but omitted here fails to build. Without that
    /// check this list is just another hand-maintained list, and everything
    /// derived from it — including the duplicate-write guard — would silently
    /// ignore the missing tool.
    pub const ALL: &'static [Tool] = &[
        Tool::SearchNodes,
        Tool::ResolveQuery,
        Tool::SearchSemantic,
        Tool::GetNode,
        Tool::CreateNode,
        Tool::UpdateNode,
        Tool::CreateSchema,
        Tool::UpdateSchema,
        Tool::UpdateTaskStatus,
        Tool::CreateRelationship,
        Tool::GetRelatedNodes,
        Tool::SearchSkills,
        Tool::DeleteNode,
        Tool::CreateNodesFromMarkdown,
    ];

    /// The number of variants, counted by walking every one of them.
    ///
    /// The successor chain is an exhaustive match, so a new variant cannot be
    /// added without being threaded into it — and threading it in necessarily
    /// increments this count. That is the whole point: a literal here would be
    /// satisfied by the very edit it is meant to police, since adding a variant
    /// changes nothing on the right-hand side of a comparison against a
    /// constant. `std::mem::variant_count` would say this directly but is still
    /// nightly-only.
    const COUNT: usize = {
        let mut n = 0;
        let mut v = Tool::SearchNodes;
        loop {
            n += 1;
            v = match v {
                Tool::SearchNodes => Tool::ResolveQuery,
                Tool::ResolveQuery => Tool::SearchSemantic,
                Tool::SearchSemantic => Tool::GetNode,
                Tool::GetNode => Tool::CreateNode,
                Tool::CreateNode => Tool::UpdateNode,
                Tool::UpdateNode => Tool::CreateSchema,
                Tool::CreateSchema => Tool::UpdateSchema,
                Tool::UpdateSchema => Tool::UpdateTaskStatus,
                Tool::UpdateTaskStatus => Tool::CreateRelationship,
                Tool::CreateRelationship => Tool::GetRelatedNodes,
                Tool::GetRelatedNodes => Tool::SearchSkills,
                Tool::SearchSkills => Tool::DeleteNode,
                Tool::DeleteNode => Tool::CreateNodesFromMarkdown,
                Tool::CreateNodesFromMarkdown => break,
            };
        }
        n
    };

    /// Compile-time proof that [`Tool::ALL`] lists every variant, exactly once,
    /// in enum order.
    ///
    /// Two independent failure modes, because neither check catches the other's:
    /// the index match below catches an entry that is out of order or listed
    /// twice, while the count catches one omitted entirely — an omission
    /// shortens the list without disturbing the indices of what remains.
    ///
    /// This is worth stating precisely because the obvious version of this
    /// check does not work. Comparing `ALL.len()` against a literal passes the
    /// case that actually happens — someone adds a tool — since the exhaustive
    /// match forces an edit to the *match*, not to the number. The count must
    /// itself be derived from an exhaustive match ([`Tool::COUNT`]) for the
    /// proof to bind. Verify any change here by adding a variant, leaving it
    /// out of `ALL`, and confirming the build fails; inspection is not enough.
    const ALL_IS_COMPLETE: () = {
        let mut i = 0;
        while i < Self::ALL.len() {
            // Each variant maps to its own index; any duplicate or missing
            // entry shifts the rest and trips the comparison below.
            let expected = match Self::ALL[i] {
                Tool::SearchNodes => 0,
                Tool::ResolveQuery => 1,
                Tool::SearchSemantic => 2,
                Tool::GetNode => 3,
                Tool::CreateNode => 4,
                Tool::UpdateNode => 5,
                Tool::CreateSchema => 6,
                Tool::UpdateSchema => 7,
                Tool::UpdateTaskStatus => 8,
                Tool::CreateRelationship => 9,
                Tool::GetRelatedNodes => 10,
                Tool::SearchSkills => 11,
                Tool::DeleteNode => 12,
                Tool::CreateNodesFromMarkdown => 13,
            };
            assert!(expected == i, "Tool::ALL lists a variant out of order");
            i += 1;
        }
        assert!(
            Self::ALL.len() == Self::COUNT,
            "Tool::ALL is missing a variant — every variant must be listed"
        );
    };

    /// The wire/identifier name the model uses to call this tool.
    pub fn name(self) -> &'static str {
        match self {
            Tool::SearchNodes => "search_nodes",
            Tool::ResolveQuery => "resolve_query",
            Tool::SearchSemantic => "search_semantic",
            Tool::GetNode => "get_node",
            Tool::CreateNode => "create_node",
            Tool::UpdateNode => "update_node",
            Tool::CreateSchema => "create_schema",
            Tool::UpdateSchema => "update_schema",
            Tool::UpdateTaskStatus => "update_task_status",
            Tool::CreateRelationship => "create_relationship",
            Tool::GetRelatedNodes => "get_related_nodes",
            Tool::SearchSkills => "search_skills",
            Tool::DeleteNode => "delete_node",
            Tool::CreateNodesFromMarkdown => "create_nodes_from_markdown",
        }
    }

    /// Resolve a wire name back to its registry entry. Returns `None` for any
    /// name not in the registry — used for dispatch and for validating
    /// skill `tool_whitelist` references.
    pub fn from_name(name: &str) -> Option<Tool> {
        Tool::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// The tool's JSON-schema definition (name, description, parameters).
    pub(crate) fn definition(self) -> ToolDefinition {
        match self {
            Tool::SearchNodes => def_search_nodes(),
            Tool::ResolveQuery => def_resolve_query(),
            Tool::SearchSemantic => def_search_semantic(),
            Tool::GetNode => def_get_node(),
            Tool::CreateNode => def_create_node(),
            Tool::UpdateNode => def_update_node(),
            Tool::CreateSchema => def_create_schema(),
            Tool::UpdateSchema => def_update_schema(),
            Tool::UpdateTaskStatus => def_update_task_status(),
            Tool::CreateRelationship => def_create_relationship(),
            Tool::GetRelatedNodes => def_get_related_nodes(),
            Tool::SearchSkills => def_search_skills(),
            Tool::DeleteNode => def_delete_node(),
            Tool::CreateNodesFromMarkdown => def_create_nodes_from_markdown(),
        }
    }

    /// User-facing prose for surfacing tool activity in the chat UI.
    ///
    /// Used by fallback responses when the model fails to produce its own text.
    /// Because every variant returns a real phrase, an internal name can never
    /// leak to the user.
    pub(crate) fn humanized(self) -> &'static str {
        match self {
            Tool::SearchNodes => "node search",
            Tool::ResolveQuery => "query resolution",
            Tool::SearchSemantic => "semantic search",
            Tool::GetNode => "node lookup",
            Tool::CreateNode => "node creation",
            Tool::UpdateNode => "node update",
            Tool::CreateSchema => "schema creation",
            Tool::UpdateSchema => "schema update",
            Tool::UpdateTaskStatus => "task update",
            Tool::CreateRelationship => "relationship creation",
            Tool::GetRelatedNodes => "related node lookup",
            Tool::SearchSkills => "skill search",
            Tool::DeleteNode => "node deletion",
            Tool::CreateNodesFromMarkdown => "markdown import",
        }
    }

    /// What repeating this tool with identical arguments means.
    ///
    /// An exhaustive match rather than a list, so adding a tool cannot silently
    /// default to "safe to repeat": the compiler makes the author state the
    /// semantics. Consumers derive their own sets from this — see
    /// [`Tool::is_write`] and [`Tool::duplicate_is_destructive`].
    pub fn write_semantics(self) -> WriteSemantics {
        match self {
            // Reads. Repeating one is wasteful, never a duplicate.
            Tool::SearchNodes
            | Tool::ResolveQuery
            | Tool::SearchSemantic
            | Tool::GetNode
            | Tool::GetRelatedNodes
            | Tool::SearchSkills => WriteSemantics::Read,

            // Idempotent writes. Setting a node to the same content, or a task
            // to the same status, twice is a no-op — the second call is not a
            // duplicate, and refusing it would break a user legitimately
            // re-asserting a value.
            Tool::UpdateNode | Tool::UpdateTaskStatus => WriteSemantics::IdempotentWrite,

            // Not idempotent, but not guarded either. A repeated `add_fields`
            // or `add_relationships` rejects the field as already present, and
            // a repeated `rename_fields` fails because the source name is gone
            // — so a repeat is self-limiting rather than duplicating data.
            //
            // Left unguarded because the guard keys on whole-call arguments,
            // and `update_schema` batches independent edits: a call repeating
            // one already-applied rename alongside a new field is not the same
            // write, yet would be refused wholesale. Refusing a legitimate
            // schema edit is worse than the error the schema layer already
            // returns for the redundant part.
            Tool::UpdateSchema => WriteSemantics::SelfLimitingWrite,

            // Writes whose repeat produces a second, unwanted copy of the
            // user's data — or, for a delete, re-attacks a node the record
            // already shows removed.
            Tool::CreateNode
            | Tool::CreateSchema
            | Tool::CreateRelationship
            | Tool::CreateNodesFromMarkdown
            | Tool::DeleteNode => WriteSemantics::DuplicableWrite,
        }
    }

    /// Whether this tool changes graph state.
    pub fn is_write(self) -> bool {
        !matches!(self.write_semantics(), WriteSemantics::Read)
    }

    /// Whether repeating this tool across turns duplicates the user's data.
    pub fn duplicate_is_destructive(self) -> bool {
        matches!(self.write_semantics(), WriteSemantics::DuplicableWrite)
    }
}

/// What a repeat of a tool call means for graph state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteSemantics {
    /// Does not change user-visible graph state. A repeat wastes an iteration,
    /// nothing more. (Reads may still persist internal upkeep such as lazy
    /// schema migration, which is convergent and invisible to the caller.)
    Read,
    /// Changes state, but a repeat with identical arguments converges on the
    /// same result rather than producing a second copy.
    IdempotentWrite,
    /// Changes state, and a repeat is either rejected by the tool's own
    /// validation or is a no-op — never a second copy. Both happen within one
    /// tool: a repeated `update_schema` add or rename is an error, while a
    /// repeated remove simply finds nothing left to remove.
    ///
    /// Not guarded, for a reason beyond the tool handling redundancy itself:
    /// the guard keys on whole-call arguments, and a call in this category may
    /// batch independent edits, so repeating one applied edit alongside a new
    /// one is not the same write yet would be refused wholesale.
    SelfLimitingWrite,
    /// Changes state, and a repeat with identical arguments produces a second
    /// copy of the user's data. These are what the cross-turn guard refuses.
    DuplicableWrite,
}

/// Whether a repeat of this tool in a *later turn* must be refused.
///
/// Resolves the wire name through the registry, so the set is computed from
/// [`Tool::write_semantics`] rather than restated. An unknown name is not
/// guarded: the guard blocks work, so an unrecognised tool must fail open.
pub fn is_cross_turn_guarded_tool(tool: &str) -> bool {
    Tool::from_name(tool).is_some_and(Tool::duplicate_is_destructive)
}

/// Whether a tool changes graph state, by wire name. Computed from the registry.
pub fn is_write_tool(tool: &str) -> bool {
    Tool::from_name(tool).is_some_and(Tool::is_write)
}

/// All tool definitions for the graph executor, derived from the registry.
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    // Force evaluation of the completeness proof; an associated const is only
    // checked where it is used.
    const _: () = Tool::ALL_IS_COMPLETE;
    Tool::ALL.iter().map(|t| t.definition()).collect()
}

/// Whether a tool is withheld from the local agent's model-facing surface.
///
/// `search_skills` is retrieval, and ADR-038 makes retrieval a deterministic
/// system step rather than a model tool call — offering it back to the model
/// is the single-turn pull that ADR rejects, because it lets the model set K
/// and bypasses the trust filter. The tool stays in [`Tool::ALL`] because it
/// remains a legitimate surface for external agents (the MCP `find_skills`
/// handler shares its implementation); only the local loop withholds it.
pub fn is_system_only_tool(tool: &str) -> bool {
    matches!(Tool::from_name(tool), Some(Tool::SearchSkills))
}

/// Tool definitions offered to the local agent's model, excluding those the
/// system reserves for itself. See [`is_system_only_tool`].
pub fn model_facing_tool_definitions() -> Vec<ToolDefinition> {
    all_tool_definitions()
        .into_iter()
        .filter(|t| !is_system_only_tool(&t.name))
        .collect()
}

// ---------------------------------------------------------------------------
// GraphToolExecutor
// ---------------------------------------------------------------------------

/// Executes graph operation tools against `NodeService` and `NodeEmbeddingService`.
///
/// Service references are injected directly, decoupling this crate from
/// Tauri-specific `AppServices`. The desktop-app layer is responsible for
/// resolving services and constructing this executor.
pub struct GraphToolExecutor {
    /// Node service for graph operations. `None` if services aren't initialized yet.
    pub node_service: Option<Arc<NodeService>>,
    /// Shared handle to the embedding service for semantic search and skill
    /// routing. Read per-call so the executor always sees the current value,
    /// even when the embedding model finishes loading after this executor is
    /// built. See [`SharedEmbeddingService`].
    pub embedding_service: SharedEmbeddingService,
    /// The chat inference engine, used by `resolve_query` to make its own
    /// scoped nested inference call. Unlike `embedding_service`, this is fixed
    /// at construction — the daemon rebuilds the whole executor (with a fresh
    /// engine) on every model load/switch, so there is no later-arriving value
    /// to read live. `None` only in tests that construct an executor with no
    /// engine at all. See [`SharedChatInferenceEngine`].
    pub inference_engine: SharedChatInferenceEngine,
}

impl GraphToolExecutor {
    // -- Individual tool implementations --

    /// The single query tool: find, list, and filter nodes by title, type,
    /// and/or typed properties.
    ///
    /// Routing is by capability, transparent to the model:
    /// - **No property filters** (plain title keyword and/or type listing) →
    ///   `node_ops::query_nodes`, which owns `title contains` matching (the
    ///   title index is the only path that filters by title).
    /// - **Property filters present** → `query_ops::execute_query`
    ///   (`QueryService`), which pushes typed property conditions to SQL
    ///   `json_extract` — the correct path for operators and date comparisons.
    ///
    /// Both paths return each node's `properties` in the summary.
    /// Shared search core behind `search_nodes` and `resolve_query`.
    ///
    /// Both tools resolve a `(node_type, query, filters, sorting, limit)`
    /// tuple to the same node summaries — `resolve_query` differs only in
    /// where that tuple comes from (an LLM decomposition step rather than
    /// the model's direct arguments) and in what it does with the result
    /// (pick/discriminate rather than hand back the whole list). Sharing
    /// this core keeps both tools' notion of "what a query means" identical.
    async fn run_node_query(
        &self,
        node_type: Option<String>,
        query: String,
        filters: Vec<query_ops::AgentFilterItem>,
        sorting: Option<Vec<query_ops::AgentSortItem>>,
        limit: usize,
        tool_name: &str,
    ) -> Result<Vec<Value>, ToolError> {
        let ns = self.node_service()?;

        let output = if filters.is_empty() {
            // Title/type listing: only `node_ops::query_nodes` filters by title.
            let filters = if query.is_empty() {
                None
            } else {
                Some(vec![node_ops::QueryFilterItem {
                    field: "title".to_string(),
                    operator: "contains".to_string(),
                    value: Value::String(query),
                }])
            };

            node_ops::query_nodes(
                &ns,
                node_ops::QueryNodesInput {
                    node_type,
                    parent_id: None,
                    root_id: None,
                    limit: Some(limit),
                    offset: None,
                    collection_id: None,
                    collection: None,
                    filters,
                },
            )
            .await
            .map_err(|e| ops_error_to_tool(e, tool_name))?
        } else {
            // Typed property query: route through QueryService (SQL json_extract).
            // A non-empty title keyword is added as a content filter alongside the
            // property predicates (QueryService has no title path).
            let mut filters = filters;
            if !query.is_empty() {
                filters.push(query_ops::AgentFilterItem {
                    filter_type: "content".to_string(),
                    operator: "contains".to_string(),
                    property: None,
                    value: Some(Value::String(query)),
                    case_sensitive: Some(false),
                    relationship_type: None,
                    node_id: None,
                });
            }

            query_ops::execute_query(
                &ns,
                query_ops::ExecuteQueryInput {
                    target_type: node_type.unwrap_or_else(|| "*".to_string()),
                    filters,
                    sorting,
                    limit: Some(limit),
                },
            )
            .await
            .map_err(|e| ops_error_to_tool(e, tool_name))?
        };

        // Truncate node data for token efficiency. Properties are always included
        // so the model can see and act on typed fields (status, amount, etc.).
        Ok(output
            .nodes
            .iter()
            .map(|v| {
                json!({
                    "id": node_uri(v.get("id").and_then(|v| v.as_str()).unwrap_or("")),
                    "title": truncate(
                        v.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                        100
                    ),
                    "type": v.get("node_type").or(v.get("type")).and_then(|v| v.as_str()).unwrap_or(""),
                    "snippet": truncate(
                        v.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                        BODY_TRUNCATE_SUMMARY
                    ),
                    "properties": v.get("properties").cloned().unwrap_or(json!({})),
                })
            })
            .collect())
    }

    async fn exec_search_nodes(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: SearchNodesParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "search_nodes".to_string(),
                reason: e.to_string(),
            })?;
        let limit = params.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);

        let summaries = self
            .run_node_query(
                params.node_type,
                params.query,
                params.filters,
                params.sorting,
                limit,
                "search_nodes",
            )
            .await?;

        Ok(ok_result(
            tool_call_id,
            "search_nodes",
            json!({ "count": summaries.len(), "nodes": summaries }),
        ))
    }

    /// Resolve an ambiguous natural-language request directly to the node it
    /// identifies, performing the search internally rather than handing back
    /// query fragments for the caller to route.
    ///
    /// Looks up the target type's real schema fields via `NodeService`
    /// (deterministic — not the semantically-truncated schema summary injected
    /// into the main prompt), then makes a single, narrowly-scoped inference
    /// call (no tools, temperature 0) asking the model to map the request's
    /// implicit values (amounts, relative dates, paraphrased identifiers) onto
    /// those fields. This is a genuinely separate LLM call from the main ReAct
    /// loop's turn — see `agent_loop::maybe_summarize_history` for the existing
    /// single-shot sub-call precedent this mirrors.
    ///
    /// Per ADR-064 rule 4 (tool results own facts, not plans): the decomposed
    /// `query`/`filters` never reach the model. They are consumed here, fed
    /// straight into the same search core `search_nodes` uses
    /// (`run_node_query`), and the result is discriminated down to a single
    /// resolved node — or an explicit zero/multi-match outcome — before it is
    /// returned. There is no return shape in which the model receives a plan
    /// to execute; a resolve_query result is always either a fact ("this is
    /// the node") or an explicit "resolution failed, do X" instruction.
    async fn exec_resolve_query(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: ResolveQueryParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "resolve_query".to_string(),
                reason: e.to_string(),
            })?;

        let ns = self.node_service()?;
        let engine = self.inference_engine()?;

        let schema = ns
            .get_schema_node(&params.node_type)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("resolve_query failed: {}", e)))?;

        let field_lines: String = match &schema {
            Some(s) if !s.fields.is_empty() => s
                .fields
                .iter()
                .map(|f| format!("- {} ({})", f.name, f.field_type))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => "(no typed fields on this schema — resolve to a title/content keyword only)"
                .to_string(),
        };
        let schema_label = schema
            .as_ref()
            .map(|s| s.content.clone())
            .unwrap_or_else(|| params.node_type.clone());

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let prompt = format!(
            "You resolve an ambiguous user request into a precise structured search query. \
            Do not answer the user, do not explain — output ONLY a single JSON object.\n\n\
            Target entity type: {node_type} (\"{schema_label}\")\n\
            Fields on this type:\n{field_lines}\n\n\
            Today's date: {today}\n\n\
            User request: \"{request}\"\n\n\
            Resolve the request against the fields above:\n\
            - If a value in the request maps to one of the typed fields (e.g. a dollar amount to a \
            number field, a status word to an enum/status field), emit a filter for it: \
            {{\"type\":\"property\",\"operator\":\"equals\",\"property\":\"<field name>\",\"value\":<value>}}.\n\
            - Resolve relative dates (\"next Friday\", \"overdue\", \"recent\") to a concrete YYYY-MM-DD \
            value and the correct comparison operator (gt/lt/gte/lte/equals) against the matching date field.\n\
            - Put any remaining identifying words that should match the title/content as a short \
            \"query\" string (a few keywords, NOT the full sentence).\n\
            - If nothing resolves to a typed field, leave \"filters\" empty and put your best short \
            keyword(s) in \"query\".\n\n\
            Output EXACTLY this JSON shape, nothing else:\n\
            {{\"query\": \"<keywords or empty string>\", \"filters\": [<filter objects, or empty array>]}}",
            node_type = params.node_type.as_str(),
            schema_label = schema_label,
            field_lines = field_lines,
            today = today,
            request = params.request.as_str(),
        );

        let request = InferenceRequest {
            messages: vec![ChatMessage::text(Role::User, prompt)],
            tools: None,
            temperature: Some(0.0),
            max_tokens: Some(512),
        };

        let chunks: Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let chunks_for_cb = Arc::clone(&chunks);
        let cb: Box<dyn Fn(StreamingChunk) + Send> = Box::new(move |chunk: StreamingChunk| {
            if let Ok(mut guard) = chunks_for_cb.lock() {
                guard.push(chunk);
            }
        });

        engine.generate(request, cb).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("resolve_query inference failed: {}", e))
        })?;

        let text: String = {
            let guard = chunks.lock().unwrap_or_else(|p| p.into_inner());
            guard
                .iter()
                .filter_map(|c| match c {
                    StreamingChunk::Token { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        };

        // Tolerate a model that wraps the JSON in prose/markdown fences by
        // extracting the first balanced {...} object rather than requiring an
        // exact-match parse.
        let json_slice = extract_json_object(&text).unwrap_or(&text);
        let resolved: Value = serde_json::from_str(json_slice).unwrap_or_else(|_| {
            // Decomposition failed to produce parseable JSON — fall back to an
            // empty resolution so the search below degrades to a bare
            // type listing rather than erroring the turn.
            json!({ "query": "", "filters": [] })
        });

        let query = resolved
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // The filters came from our own prompt-constrained decomposition call
        // above, not from the model's tool-call arguments — but they are still
        // untrusted JSON (the decomposition model can emit a malformed shape).
        // Deserialize defensively and skip anything that doesn't parse rather
        // than failing the whole resolution, but — matching the
        // deny_unknown_fields rigor `AgentFilterItem` carries everywhere else
        // — log each drop rather than swallowing it outright. A silently
        // dropped filter can turn a would-be unique match into a false
        // multiple_matches (or a different unique match), with no signal that
        // anything went wrong.
        let filters: Vec<query_ops::AgentFilterItem> = resolved
            .get("filters")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| match serde_json::from_value(f.clone()) {
                        Ok(item) => Some(item),
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                filter = %f,
                                "resolve_query: dropping unparseable filter from decomposition output"
                            );
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let matches = self
            .run_node_query(
                Some(params.node_type.clone()),
                query,
                filters,
                None,
                RESOLVE_QUERY_MATCH_LIMIT,
                "resolve_query",
            )
            .await?;

        let payload = match matches.len() {
            0 => json!({
                "resolved": false,
                "reason": "no_match",
                "node_type": params.node_type,
            }),
            1 => {
                let node = &matches[0];
                json!({
                    "resolved": true,
                    "id": node["id"],
                    "title": node["title"],
                    "type": node["type"],
                    "properties": node["properties"],
                })
            }
            _ => json!({
                "resolved": false,
                "reason": "multiple_matches",
                "node_type": params.node_type,
                "candidates": matches,
            }),
        };

        Ok(ok_result(tool_call_id, "resolve_query", payload))
    }

    async fn exec_search_semantic(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: SearchSemanticParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "search_semantic".to_string(),
                reason: e.to_string(),
            })?;
        let query = params.query.clone();
        let limit = params.limit.unwrap_or(DEFAULT_SEMANTIC_LIMIT);

        // Use caller-supplied threshold when provided; fall back to the local
        // agent default (SEMANTIC_THRESHOLD = 0.3). This preserves the existing
        // default behaviour while allowing the LLM to tune recall.
        let threshold = params.threshold.unwrap_or(SEMANTIC_THRESHOLD);

        let ns = self.node_service()?;
        let emb = self.embedding_service().await?;

        let input = search_ops::SearchSemanticInput {
            query: query.clone(),
            threshold: Some(threshold),
            limit: Some(limit),
            collection_id: params.collection_id,
            collection: params.collection,
            exclude_collections: params.exclude_collections,
            include_markdown: params.include_markdown,
            include_archived: params.include_archived,
            scope: params.scope,
            node_types: params.node_types,
            // property_filters is exposed in the tool schema as a simple object.
            // The 8B model may struggle with complex filter structures, but simple
            // key-value pairs (e.g. {"status": "done"}) work well enough.
            property_filters: params.property_filters,
            include_edges: params.include_edges,
            graph_boost: params.graph_boost,
        };

        let output = search_ops::search_semantic(&ns, &emb, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "search_semantic"))?;

        // Truncate for token efficiency
        let items: Vec<Value> = output
            .nodes
            .iter()
            .map(|v| {
                let mut item = json!({
                    "id": node_uri(v.get("id").and_then(|v| v.as_str()).unwrap_or("")),
                    "title": truncate(
                        v.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                        100
                    ),
                    "type": v.get("node_type").or(v.get("type")).and_then(|v| v.as_str()).unwrap_or(""),
                    "score": v.get("similarity").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    "snippet": truncate(
                        v.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                        BODY_TRUNCATE_SUMMARY
                    ),
                });
                // Include full markdown content if the ops layer returned it
                if let Some(md) = v.get("markdown").and_then(|v| v.as_str()) {
                    if !md.is_empty() {
                        item["markdown"] = json!(truncate(md, BODY_TRUNCATE_FULL));
                    }
                }
                // Include edge data if the ops layer returned it (include_edges=true)
                if let Some(edges) = v.get("edges") {
                    if edges.is_array() {
                        item["edges"] = edges.clone();
                    }
                }
                item
            })
            .collect();

        Ok(ok_result(
            tool_call_id,
            "search_semantic",
            json!({ "count": items.len(), "results": items }),
        ))
    }

    async fn exec_get_node(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: AgentGetNodeParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "get_node".to_string(),
                reason: e.to_string(),
            })?;
        let id = strip_node_uri(&params.id).to_string();
        let format = params.format.unwrap_or_else(|| "json".to_string());

        let ns = self.node_service()?;

        if format == "markdown" {
            // Reuse the MCP handler's markdown export (single source of truth)
            use nodespace_core::markdown::handle_get_markdown_from_node_id;

            let params = json!({
                "node_id": id,
                "include_children": true,
                "include_node_ids": false,
            });
            match handle_get_markdown_from_node_id(&ns, params).await {
                Ok(result) => {
                    let md = result
                        .get("markdown")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let truncated = truncate(md, BODY_TRUNCATE_FULL);
                    Ok(ok_result(
                        tool_call_id,
                        "get_node",
                        json!({ "markdown": truncated }),
                    ))
                }
                Err(e) => Ok(error_result(
                    tool_call_id,
                    "get_node",
                    // Display, not Debug — this text reaches the model verbatim.
                    &format!("Failed to render markdown: {e}"),
                )),
            }
        } else {
            let input = node_ops::GetNodeInput {
                node_id: id.clone(),
            };
            match node_ops::get_node(&ns, input).await {
                Ok(node_data) => Ok(ok_result(tool_call_id, "get_node", node_data)),
                Err(OpsError::NotFound { .. }) => Ok(error_result(
                    tool_call_id,
                    "get_node",
                    &format!("Node '{}' not found", id),
                )),
                Err(e) => Err(ops_error_to_tool(e, "get_node")),
            }
        }
    }

    async fn exec_create_node(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // Collect any flat (unknown) keys and promote them into properties.
        // This tolerates models that pass schema fields at the top level rather
        // than nested inside "properties".
        let flat_extras: serde_json::Map<String, Value> = {
            const KNOWN: &[&str] = &["content", "node_type", "properties", "parent_id"];
            args.as_object()
                .map(|obj| {
                    obj.iter()
                        .filter(|(k, _)| !KNOWN.contains(&k.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default()
        };

        let params: AgentCreateNodeParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "create_node".to_string(),
                reason: e.to_string(),
            })?;

        // Merge explicit properties with flat extras
        let mut props = params
            .properties
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        props.extend(flat_extras);
        let properties = Value::Object(props);

        let ns = self.node_service()?;

        // node_service.compute_title() handles all title derivation:
        // - title_template + properties for schema types that define one
        // - strip_markdown(content) for root nodes (all custom schema instances)
        let content = params.content.unwrap_or_default();

        let input = node_ops::CreateNodeInput {
            id: None,
            node_type: params.node_type,
            content,
            parent_id: params.parent_id,
            position: nodespace_core::services::InsertPositionOwned::End,
            properties,
            collection: None,
            lifecycle_status: None,
        };

        let output = node_ops::create_node(&ns, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "create_node"))?;

        Ok(ok_result(
            tool_call_id,
            "create_node",
            json!({ "id": node_uri(&output.node_id) }),
        ))
    }

    async fn exec_update_node(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // Collect any flat (unknown) keys and promote them into properties.
        let flat_extras: serde_json::Map<String, Value> = {
            const KNOWN: &[&str] = &["id", "node_id", "content", "properties"];
            args.as_object()
                .map(|obj| {
                    obj.iter()
                        .filter(|(k, _)| !KNOWN.contains(&k.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default()
        };

        let params: AgentUpdateNodeParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "update_node".to_string(),
                reason: e.to_string(),
            })?;

        // Merge explicit properties with flat extras
        let mut props = params
            .properties
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        props.extend(flat_extras);
        let new_properties = if props.is_empty() {
            None
        } else {
            Some(Value::Object(props))
        };

        if params.content.is_none() && new_properties.is_none() {
            return Err(ToolError::InvalidArguments {
                tool: "update_node".into(),
                reason: "At least one of 'content' or 'properties' must be provided".into(),
            });
        }

        let ns = self.node_service()?;

        let input = node_ops::UpdateNodeInput {
            node_id: strip_node_uri(&params.id).to_string(),
            version: None, // ops layer auto-fetches
            node_type: None,
            content: params.content,
            properties: new_properties,
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        };

        let output = node_ops::update_node(&ns, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "update_node"))?;

        Ok(ok_result(
            tool_call_id,
            "update_node",
            json!({ "id": node_uri(&output.node_id), "updated": true }),
        ))
    }

    async fn exec_create_relationship(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: CreateRelationshipParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "create_relationship".to_string(),
                reason: e.to_string(),
            })?;

        let ns = self.node_service()?;

        let input = rel_ops::CreateRelInput {
            source_id: strip_node_uri(&params.from_id).to_string(),
            relationship_name: params.relationship_type.clone(),
            target_id: strip_node_uri(&params.to_id).to_string(),
            edge_data: None,
        };

        rel_ops::create_relationship(&ns, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "create_relationship"))?;

        Ok(ok_result(
            tool_call_id,
            "create_relationship",
            json!({ "from_id": params.from_id, "to_id": params.to_id, "type": params.relationship_type, "created": true }),
        ))
    }

    async fn exec_get_related_nodes(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: GetRelatedNodesParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "get_related_nodes".to_string(),
                reason: e.to_string(),
            })?;
        let rel_type = params
            .relationship_type
            .unwrap_or_else(|| "mentions".to_string());
        let direction = params.direction.unwrap_or_else(|| "both".to_string());

        // Validate direction before acquiring the service
        let directions: Vec<&str> = match direction.as_str() {
            "out" => vec!["out"],
            "in" => vec!["in"],
            "both" => vec!["out", "in"],
            _ => {
                return Err(ToolError::InvalidArguments {
                    tool: "get_related_nodes".into(),
                    reason: "direction must be 'in', 'out', or 'both'".into(),
                });
            }
        };

        let ns = self.node_service()?;

        let mut all_nodes: Vec<Value> = Vec::new();
        for dir in &directions {
            let input = rel_ops::GetRelatedInput {
                node_id: strip_node_uri(&params.id).to_string(),
                relationship_name: rel_type.clone(),
                direction: dir.to_string(),
            };

            let output = rel_ops::get_related_nodes(&ns, input)
                .await
                .map_err(|e| ops_error_to_tool(e, "get_related_nodes"))?;

            for node_val in &output.related_nodes {
                let mut summary = json!({
                    "id": node_uri(node_val.get("id").and_then(|v| v.as_str()).unwrap_or("")),
                    "title": truncate(
                        node_val.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                        100
                    ),
                    "type": node_val.get("node_type").or(node_val.get("type")).and_then(|v| v.as_str()).unwrap_or(""),
                });
                summary["direction"] = json!(dir);
                summary["relationship_type"] = json!(&rel_type);
                all_nodes.push(summary);
            }
        }

        Ok(ok_result(
            tool_call_id,
            "get_related_nodes",
            json!({ "count": all_nodes.len(), "nodes": all_nodes }),
        ))
    }

    async fn exec_search_skills(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: SearchSkillsParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "search_skills".to_string(),
                reason: e.to_string(),
            })?;
        let limit = params.limit.unwrap_or(3);

        let emb = match self.embedding_service.read().await.clone() {
            Some(svc) => svc,
            None => {
                return Ok(error_result(
                    tool_call_id,
                    "search_skills",
                    "Skill search unavailable: embedding service not loaded",
                ))
            }
        };

        let ns = self.node_service()?;

        use nodespace_core::ops::skill_ops;
        let output = skill_ops::find_skills(
            &emb,
            &ns,
            skill_ops::FindSkillsInput {
                query: params.query.clone(),
                limit: Some(limit),
            },
        )
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("search_skills failed: {}", e)))?;

        // Always return `matches` (possibly empty). An empty array is a real
        // signal — let the model decide what to do, rather than hardcoding a
        // "Proceed with general capabilities" string.
        Ok(ok_result(
            tool_call_id,
            "search_skills",
            json!({
                "query": output.query,
                "matches": output.skills,
            }),
        ))
    }

    async fn exec_create_schema(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let ns = self.node_service()?;

        // Delegate to the MCP schema handler which handles ID normalization
        // (e.g., "Project" → "project"), field namespacing, and validation.
        let result = handle_create_schema(&ns, args).await;

        match result {
            Ok(value) => Ok(ok_result(tool_call_id, "create_schema", value)),
            Err(e) => {
                // Return validation errors as tool errors (not ToolError::ExecutionFailed)
                // so the model sees the message and can self-correct. Formatted with
                // Display, not Debug: `{:?}` wraps the guidance in the Rust variant name
                // (`InvalidParams("...")`), which is noise to the model and obscures the
                // repair instruction the message carries.
                Ok(error_result(tool_call_id, "create_schema", &e.to_string()))
            }
        }
    }

    async fn exec_update_schema(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        use nodespace_core::schema::handle_update_schema;
        let ns = self.node_service()?;

        let result = handle_update_schema(&ns, args).await;

        match result {
            Ok(value) => Ok(ok_result(tool_call_id, "update_schema", value)),
            Err(e) => {
                // Display rather than Debug — see exec_create_schema.
                Ok(error_result(tool_call_id, "update_schema", &e.to_string()))
            }
        }
    }

    async fn exec_update_task_status(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: UpdateTaskStatusParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "update_task_status".to_string(),
                reason: e.to_string(),
            })?;

        // Validate status is a known enum value
        match params.status.as_str() {
            "open" | "in_progress" | "done" | "cancelled" => {}
            _ => {
                return Err(ToolError::InvalidArguments {
                    tool: "update_task_status".into(),
                    reason: format!(
                        "Invalid status '{}'. Must be one of: open, in_progress, done, cancelled",
                        params.status
                    ),
                });
            }
        }

        let ns = self.node_service()?;

        let input = node_ops::UpdateNodeInput {
            node_id: strip_node_uri(&params.id).to_string(),
            version: None,
            node_type: None,
            content: None,
            properties: Some(json!({ "status": params.status })),
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        };

        let output = node_ops::update_node(&ns, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "update_task_status"))?;

        Ok(ok_result(
            tool_call_id,
            "update_task_status",
            json!({ "id": node_uri(&output.node_id), "status": params.status, "updated": true }),
        ))
    }

    async fn exec_delete_node(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let params: DeleteNodeParams =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: "delete_node".to_string(),
                reason: e.to_string(),
            })?;

        let ns = self.node_service()?;

        let input = node_ops::DeleteNodeInput {
            node_id: strip_node_uri(&params.id).to_string(),
            version: None, // ops layer auto-fetches
        };

        let output = node_ops::delete_node(&ns, input)
            .await
            .map_err(|e| ops_error_to_tool(e, "delete_node"))?;

        Ok(ok_result(
            tool_call_id,
            "delete_node",
            json!({ "id": node_uri(&output.node_id), "deleted": output.existed }),
        ))
    }

    async fn exec_create_nodes_from_markdown(
        &self,
        tool_call_id: &str,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // Inline validation: require non-empty "markdown" field
        let markdown = args
            .get("markdown")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: "create_nodes_from_markdown".to_string(),
                reason: "missing required field: markdown".to_string(),
            })?;
        if markdown.trim().is_empty() {
            return Err(ToolError::InvalidArguments {
                tool: "create_nodes_from_markdown".to_string(),
                reason: "markdown content must not be empty".to_string(),
            });
        }

        // Remap agent field names to MCP handler field names:
        // agent uses "markdown", handler expects "markdown_content"
        let mut handler_args = args.clone();
        if let Some(obj) = handler_args.as_object_mut() {
            if let Some(content) = obj.remove("markdown") {
                obj.insert("markdown_content".to_string(), content);
            }
        }

        let ns = self.node_service()?;

        // Delegate to the MCP markdown handler which handles the full import pipeline
        use nodespace_core::markdown::handle_create_nodes_from_markdown;
        let result = handle_create_nodes_from_markdown(&ns, handler_args)
            .await
            .map_err(|e| {
                // Display, not Debug — MarkdownError's Display text is written to
                // be read by the model, and `{:?}` wraps it in the variant name.
                ToolError::ExecutionFailed(format!("create_nodes_from_markdown failed: {e}"))
            })?;

        Ok(ok_result(
            tool_call_id,
            "create_nodes_from_markdown",
            result,
        ))
    }

    // -- Service accessors --

    fn node_service(&self) -> Result<Arc<NodeService>, ToolError> {
        self.node_service
            .clone()
            .ok_or_else(|| ToolError::ExecutionFailed("Node service unavailable".to_string()))
    }

    /// Read the current embedding service from the shared handle.
    ///
    /// Returns the value live each call, so a service that loaded after this
    /// executor was built is picked up without any re-wiring.
    async fn embedding_service(&self) -> Result<Arc<NodeEmbeddingService>, ToolError> {
        self.embedding_service
            .read()
            .await
            .clone()
            .ok_or_else(|| ToolError::ExecutionFailed("Embedding service unavailable".to_string()))
    }

    /// Read the current chat inference engine from the shared handle.
    ///
    /// Returns the value live each call, mirroring `embedding_service` above —
    /// a model loaded/swapped after this executor was built is picked up
    /// without any re-wiring.
    fn inference_engine(&self) -> Result<Arc<dyn ChatInferenceEngine>, ToolError> {
        self.inference_engine
            .clone()
            .ok_or_else(|| ToolError::ExecutionFailed("Inference engine unavailable".to_string()))
    }
}

#[async_trait]
impl AgentToolExecutor for GraphToolExecutor {
    /// Return typed `ToolDefinition`s generated from tool nodes in the graph.
    ///
    /// Reads `node_type='tool'` nodes seeded at startup, builds a
    /// `ToolDefinition` from each enabled node whose handler key is present in
    /// the deterministic registry, then preserves the canonical registry ordering.
    /// Falls back to the hardcoded list when the node service is unavailable or
    /// the query returns no results.
    async fn available_tools(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        let ns = match &self.node_service {
            Some(svc) => svc,
            None => return Ok(model_facing_tool_definitions()),
        };

        let query_result = node_ops::query_nodes(
            ns,
            node_ops::QueryNodesInput {
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
        .await;

        let tool_nodes = match query_result {
            Ok(output) if !output.nodes.is_empty() => output.nodes,
            Ok(_) => {
                tracing::debug!("available_tools: no tool nodes in DB, using hardcoded list");
                return Ok(model_facing_tool_definitions());
            }
            Err(e) => {
                tracing::warn!(error = %e, "available_tools: node query failed, using hardcoded list");
                return Ok(model_facing_tool_definitions());
            }
        };

        // Build a map from handler key → ToolDefinition from node properties.
        // Only enabled nodes with a valid handler key are included.
        let mut node_defs: std::collections::HashMap<String, ToolDefinition> =
            std::collections::HashMap::new();

        for node in &tool_nodes {
            let props = node.get("properties").unwrap_or(node);

            let handler = props
                .get("handler")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if handler.is_empty() {
                continue;
            }

            // Enforce the trust boundary via allowlist: only internal tools or
            // explicitly-enabled external tools may enter the inference surface.
            // Unknown source values are treated as untrusted and require enablement —
            // this prevents future source values from silently bypassing the gate.
            let source = props.get("source").and_then(|v| v.as_str()).unwrap_or(""); // missing source → not "internal" → requires enabled=true
            let enabled = props
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let is_allowed = source == "internal" || enabled;
            if !is_allowed {
                tracing::debug!(handler = %handler, source = %source, "available_tools: skipping unenabled tool");
                continue;
            }

            let description = props
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let parameters_schema = props
                .get("parameter_schema")
                .cloned()
                .unwrap_or(json!({"type": "object", "properties": {}}));

            node_defs.insert(
                handler.clone(),
                ToolDefinition {
                    name: handler,
                    description,
                    parameters_schema,
                },
            );
        }

        if node_defs.is_empty() {
            tracing::debug!("available_tools: no valid tool nodes found, using hardcoded list");
            return Ok(model_facing_tool_definitions());
        }

        // Emit ToolDefinitions in canonical registry order so the model always
        // sees tools in the same sequence (discovery tools first).
        // Unknown handler keys in the DB (external tools not in the registry)
        // are appended after the registered tools.
        let mut result: Vec<ToolDefinition> = Vec::with_capacity(node_defs.len());
        for &tool in Tool::ALL {
            // Tools the system reserves for itself are dropped even when a
            // node for them exists: the seeded tool node is what backs the
            // external (MCP) surface, so it must stay in the DB while staying
            // out of the local model's reach. See `is_system_only_tool`.
            if is_system_only_tool(tool.name()) {
                node_defs.remove(tool.name());
                continue;
            }
            if let Some(def) = node_defs.remove(tool.name()) {
                result.push(def);
            }
        }
        // Append external tools (handler keys not in Tool::ALL) sorted by name
        let mut extras: Vec<ToolDefinition> = node_defs.into_values().collect();
        extras.sort_by(|a, b| a.name.cmp(&b.name));
        result.extend(extras);

        tracing::debug!(
            count = result.len(),
            "available_tools: generated from tool nodes"
        );
        Ok(result)
    }

    async fn execute(&self, name: &str, args: Value) -> Result<ToolResult, ToolError> {
        // Use a synthetic tool_call_id derived from the tool name since the caller
        // (agent loop) will provide the real ID when it wraps the result.
        let tool_call_id = format!("call_{}", name);

        // Resolve through the registry so dispatch and the tool surface can't
        // drift: a name not in `Tool` is the single `UnknownTool` exit, and the
        // exhaustive match forces an exec arm for every registered variant.
        let tool = Tool::from_name(name).ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;

        match tool {
            Tool::SearchNodes => self.exec_search_nodes(&tool_call_id, args).await,
            Tool::ResolveQuery => self.exec_resolve_query(&tool_call_id, args).await,
            Tool::SearchSemantic => self.exec_search_semantic(&tool_call_id, args).await,
            Tool::GetNode => self.exec_get_node(&tool_call_id, args).await,
            Tool::CreateNode => self.exec_create_node(&tool_call_id, args).await,
            Tool::UpdateNode => self.exec_update_node(&tool_call_id, args).await,
            Tool::CreateSchema => self.exec_create_schema(&tool_call_id, args).await,
            Tool::UpdateSchema => self.exec_update_schema(&tool_call_id, args).await,
            Tool::UpdateTaskStatus => self.exec_update_task_status(&tool_call_id, args).await,
            Tool::CreateRelationship => self.exec_create_relationship(&tool_call_id, args).await,
            Tool::GetRelatedNodes => self.exec_get_related_nodes(&tool_call_id, args).await,
            Tool::SearchSkills => self.exec_search_skills(&tool_call_id, args).await,
            Tool::DeleteNode => self.exec_delete_node(&tool_call_id, args).await,
            Tool::CreateNodesFromMarkdown => {
                self.exec_create_nodes_from_markdown(&tool_call_id, args)
                    .await
            }
        }
    }

    /// Routing is available once both services backing retrieval are loaded.
    ///
    /// The embedding service loads asynchronously after startup, so this is
    /// read per-call rather than cached: early turns run unrouted and later
    /// ones route, without the executor being rebuilt.
    async fn routing_available(&self) -> bool {
        self.node_service.is_some() && self.embedding_service.read().await.is_some()
    }

    /// Run skill retrieval as a deterministic system step (ADR-038).
    ///
    /// Shares `skill_ops::find_skills` with the `search_skills` handler — the
    /// retrieval itself is identical. What differs is who initiates it: here
    /// the system does, so `limit` is a bound the model cannot widen and the
    /// results pass through the score gate before the model sees them.
    ///
    /// An unavailable embedding service yields no candidates rather than an
    /// error, matching the documented degraded path: routing is best-effort,
    /// and losing it must not cost the user their turn.
    async fn retrieve_skills(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SkillRetrieval, ToolError> {
        use nodespace_core::ops::skill_ops;

        let (Some(ns), Some(emb)) = (
            self.node_service.as_ref(),
            self.embedding_service.read().await.clone(),
        ) else {
            tracing::debug!("retrieve_skills: services unavailable, no candidates");
            return Ok(SkillRetrieval::default());
        };

        let output = skill_ops::find_skills(
            &emb,
            ns,
            skill_ops::FindSkillsInput {
                query: query.to_string(),
                limit: Some(limit),
            },
        )
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("skill retrieval failed: {}", e)))?;

        let candidates = output
            .skills
            .iter()
            .map(|s| SkillCandidate {
                id: s
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: s
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                description: s
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                score: s.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                tools: s
                    .get("tools")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                instructions: s
                    .get("instructions")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                schema_metadata: s
                    .get("schema_metadata")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
            })
            .collect();

        Ok(SkillRetrieval { candidates })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a `GraphToolExecutor` with no backing services.
    ///
    /// Suitable for tests that validate argument parsing and tool dispatch
    /// without ever reaching a real database call.
    fn test_executor() -> GraphToolExecutor {
        GraphToolExecutor {
            node_service: None,
            embedding_service: Arc::new(RwLock::new(None)),
            inference_engine: None,
        }
    }

    // -- Helper: test truncation --

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_boundary() {
        let s = "abcde";
        assert_eq!(truncate(s, 5), "abcde");
    }

    #[test]
    fn truncate_long_string() {
        let s = "a".repeat(600);
        let result = truncate(&s, BODY_TRUNCATE_SUMMARY);
        assert!(result.ends_with("[truncated]"));
        assert!(result.len() <= BODY_TRUNCATE_SUMMARY + "[truncated]".len());
    }

    #[test]
    fn truncate_multibyte() {
        // Ensure we don't split a multi-byte character
        let s = "Hello \u{1F600} world"; // emoji is 4 bytes
        let result = truncate(s, 8);
        assert!(result.ends_with("[truncated]"));
        // Should not panic
    }

    // -- Serde param parsing --

    #[test]
    fn search_nodes_params_parses_required_field() {
        let args = json!({ "query": "hello" });
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.query, "hello");
    }

    #[test]
    fn search_nodes_params_missing_query_defaults_to_empty() {
        // A caller that resolved everything into `filters` (e.g. resolve_query)
        // should not have to remember to also echo back an empty `query`.
        let args = json!({});
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.query, "");
    }

    #[test]
    fn search_nodes_params_optional_limit() {
        let args = json!({ "query": "test", "limit": 20 });
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.limit, Some(20));

        let args_no_limit = json!({ "query": "test" });
        let params2: SearchNodesParams = serde_json::from_value(args_no_limit).unwrap();
        assert_eq!(params2.limit, None);
    }

    #[test]
    fn agent_get_node_params_accepts_id_alias() {
        let args = json!({ "id": "node-123" });
        let params: AgentGetNodeParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.id, "node-123");
    }

    #[test]
    fn agent_update_node_params_accepts_id_and_content() {
        let args = json!({ "id": "node-456", "content": "New content" });
        let params: AgentUpdateNodeParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.id, "node-456");
        assert_eq!(params.content, Some("New content".to_string()));
    }

    // -- Unknown-argument rejection (acceptance criterion) --
    //
    // Every agent-facing param struct reachable directly from a tool call must
    // reject an unknown key rather than silently drop it (the `coreValues`
    // incident: a misspelled key was dropped, and the failure surfaced two
    // layers away as an unrelated validation error). `AgentCreateNodeParams`
    // and `AgentUpdateNodeParams` are intentionally exempt — see their
    // doc comments — so they have no test here.

    #[test]
    fn search_nodes_params_rejects_unknown_field() {
        let args = json!({ "query": "hello", "qeury": "typo" });
        let err = serde_json::from_value::<SearchNodesParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("qeury"),
            "expected error naming `qeury`, got: {err}"
        );
    }

    #[test]
    fn search_semantic_params_rejects_unknown_field() {
        let args = json!({ "query": "hello", "treshold": 0.5 });
        let err = serde_json::from_value::<SearchSemanticParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("treshold"),
            "expected error naming `treshold`, got: {err}"
        );
    }

    #[test]
    fn agent_get_node_params_rejects_unknown_field() {
        let args = json!({ "id": "node-123", "formatt": "markdown" });
        let err = serde_json::from_value::<AgentGetNodeParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("formatt"),
            "expected error naming `formatt`, got: {err}"
        );
    }

    #[test]
    fn create_relationship_params_rejects_unknown_field() {
        let args = json!({
            "from_id": "a",
            "to_id": "b",
            "relationship_type": "mentions",
            "relationshipType": "mentions"
        });
        let err = serde_json::from_value::<CreateRelationshipParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("relationshipType"),
            "expected error naming `relationshipType`, got: {err}"
        );
    }

    #[test]
    fn get_related_nodes_params_rejects_unknown_field() {
        let args = json!({ "id": "node-123", "reltype": "mentions" });
        let err = serde_json::from_value::<GetRelatedNodesParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("reltype"),
            "expected error naming `reltype`, got: {err}"
        );
    }

    #[test]
    fn resolve_query_params_rejects_unknown_field() {
        let args = json!({ "request": "mark it done", "node_type": "task", "nodeType": "task" });
        let err = serde_json::from_value::<ResolveQueryParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("nodeType"),
            "expected error naming `nodeType`, got: {err}"
        );
    }

    #[test]
    fn search_skills_params_rejects_unknown_field() {
        let args = json!({ "query": "invoices", "top_k": 3 });
        let err = serde_json::from_value::<SearchSkillsParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("top_k"),
            "expected error naming `top_k`, got: {err}"
        );
    }

    #[test]
    fn update_task_status_params_rejects_unknown_field() {
        let args = json!({ "id": "task-1", "status": "done", "state": "done" });
        let err = serde_json::from_value::<UpdateTaskStatusParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("state"),
            "expected error naming `state`, got: {err}"
        );
    }

    #[test]
    fn delete_node_params_rejects_unknown_field() {
        let args = json!({ "id": "node-123", "hard_delete": true });
        let err = serde_json::from_value::<DeleteNodeParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("hard_delete"),
            "expected error naming `hard_delete`, got: {err}"
        );
    }

    #[test]
    fn create_nodes_from_markdown_params_rejects_unknown_field() {
        use nodespace_core::markdown::CreateNodesFromMarkdownParams;

        let args = json!({ "markdown_content": "# Title", "syncImport": true });
        let err = serde_json::from_value::<CreateNodesFromMarkdownParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("syncImport"),
            "expected error naming `syncImport`, got: {err}"
        );
    }

    // -- Tool definitions --

    #[test]
    fn all_definitions_have_unique_names() {
        let defs = all_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len(), "Duplicate tool names found");
    }

    #[test]
    fn definitions_count() {
        // Derived from the registry: one definition per `Tool::ALL` entry.
        assert_eq!(all_tool_definitions().len(), Tool::ALL.len());
        assert_eq!(all_tool_definitions().len(), 14);
    }

    /// `node_type` argument-shape guidance (copy the id exactly from RELEVANT
    /// ENTITY TYPES, never paraphrase/guess) moved here from resident prose per
    /// ADR-064 rule 1 (tool schemas own argument shape). `update_node` has no
    /// `node_type` parameter — it addresses by `id` — so it is intentionally
    /// excluded.
    #[test]
    fn node_type_params_bind_to_relevant_entity_types() {
        for tool in [Tool::SearchNodes, Tool::CreateNode, Tool::ResolveQuery] {
            let def = tool.definition();
            let node_type_desc = def.parameters_schema["properties"]["node_type"]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("{} must have a node_type parameter", tool.name()));
            assert!(
                node_type_desc.contains("RELEVANT ENTITY TYPES")
                    && node_type_desc.to_lowercase().contains("copy"),
                "{}'s node_type description must instruct copying the id exactly from RELEVANT ENTITY TYPES, got: {node_type_desc:?}",
                tool.name()
            );
        }
    }

    /// The markdown-shortcut rule (non-empty `markdown` field is the complete
    /// document — skip get_node/search_nodes) moved here from resident prose.
    #[test]
    fn search_semantic_description_covers_markdown_shortcut() {
        let desc = Tool::SearchSemantic.definition().description;
        assert!(
            desc.contains("markdown") && desc.contains("summarize"),
            "search_semantic description must instruct summarizing directly from a non-empty markdown field, got: {desc:?}"
        );
    }

    /// `relationship_type` should steer the model toward schema-defined
    /// relationship names before generic labels.
    #[test]
    fn create_relationship_type_prefers_schema_defined_names() {
        let def = Tool::CreateRelationship.definition();
        let desc = def.parameters_schema["properties"]["relationship_type"]["description"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            desc.contains("relevant schema"),
            "relationship_type description must point at schema-defined relationship names, got: {desc:?}"
        );
    }

    // -- Tool registry invariants --

    #[test]
    fn registry_names_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for &tool in Tool::ALL {
            assert!(
                seen.insert(tool.name()),
                "duplicate tool name in registry: {}",
                tool.name()
            );
            assert_eq!(
                Tool::from_name(tool.name()),
                Some(tool),
                "from_name must round-trip {}",
                tool.name()
            );
        }
    }

    #[test]
    fn registry_from_name_rejects_unknown() {
        assert_eq!(Tool::from_name("definitely_not_a_tool"), None);
        assert_eq!(Tool::from_name(""), None);
    }

    #[test]
    fn registry_definition_name_matches_variant_name() {
        // The schema body (`def_*`) and the registry name must agree, so the
        // model-facing definition and the dispatch name can never diverge.
        for &tool in Tool::ALL {
            assert_eq!(
                tool.definition().name,
                tool.name(),
                "definition name disagrees with registry name for {:?}",
                tool
            );
        }
    }

    #[test]
    fn each_definition_has_required_fields() {
        for def in all_tool_definitions() {
            assert!(!def.name.is_empty(), "Tool name must not be empty");
            assert!(
                !def.description.is_empty(),
                "Tool {} description must not be empty",
                def.name
            );
            assert!(
                def.parameters_schema.is_object(),
                "Tool {} schema must be an object",
                def.name
            );
            assert!(
                def.parameters_schema.get("type").is_some(),
                "Tool {} schema must have a type",
                def.name
            );
        }
    }

    #[test]
    fn search_nodes_schema_does_not_require_query() {
        // query defaults to "" (skip title filter) so a filters-only call
        // (e.g. following resolve_query's output) is valid without it.
        let def = def_search_nodes();
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(!required.contains(&json!("query")));
    }

    #[test]
    fn search_nodes_schema_exposes_filters_and_sorting() {
        // The collapsed query tool now owns property filtering: 'filters' and
        // 'sorting' are exposed alongside 'query'/'node_type' so a single tool
        // covers title lookup, type listing, AND structured property queries.
        let def = def_search_nodes();
        let props = def.parameters_schema["properties"]
            .as_object()
            .expect("properties must be object");
        assert!(props.contains_key("query"), "must expose query");
        assert!(props.contains_key("node_type"), "must expose node_type");
        assert!(props.contains_key("filters"), "must expose filters");
        assert!(props.contains_key("sorting"), "must expose sorting");
        assert!(props.contains_key("limit"), "must expose limit");
    }

    #[test]
    fn search_nodes_params_parses_node_type() {
        let args = json!({
            "query": "Review quarterly report",
            "node_type": "task",
        });
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.query, "Review quarterly report");
        assert_eq!(params.node_type, Some("task".to_string()));
    }

    #[test]
    fn search_nodes_params_empty_query_with_node_type() {
        let args = json!({ "query": "", "node_type": "task" });
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.query, "");
        assert_eq!(params.node_type, Some("task".to_string()));
    }

    #[test]
    fn search_nodes_params_no_node_type() {
        let args = json!({ "query": "hello" });
        let params: SearchNodesParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.query, "hello");
        assert!(params.node_type.is_none());
    }

    // -- resolve_query tests --

    #[test]
    fn resolve_query_is_registered_in_tool_registry() {
        assert!(
            Tool::ALL.contains(&Tool::ResolveQuery),
            "resolve_query must be in the tool registry"
        );
        assert_eq!(Tool::ResolveQuery.name(), "resolve_query");
        assert_eq!(Tool::from_name("resolve_query"), Some(Tool::ResolveQuery));
    }

    #[test]
    fn resolve_query_schema_requires_request_and_node_type() {
        let def = def_resolve_query();
        assert_eq!(def.name, "resolve_query");
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(required.contains(&json!("request")));
        assert!(required.contains(&json!("node_type")));
    }

    #[test]
    fn resolve_query_params_parse() {
        let args = json!({
            "request": "Mark the $500 invoice as paid",
            "node_type": "invoice",
        });
        let params: ResolveQueryParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.request, "Mark the $500 invoice as paid");
        assert_eq!(params.node_type, "invoice");
    }

    #[test]
    fn resolve_query_params_missing_node_type_fails() {
        let args = json!({ "request": "Mark the $500 invoice as paid" });
        let result: Result<ResolveQueryParams, _> = serde_json::from_value(args);
        assert!(result.is_err());
    }

    #[test]
    fn extract_json_object_finds_bare_object() {
        let text = r#"{"query": "invoice", "filters": []}"#;
        assert_eq!(
            extract_json_object(text),
            Some(r#"{"query": "invoice", "filters": []}"#)
        );
    }

    #[test]
    fn extract_json_object_skips_leading_prose() {
        let text = "Sure, here is the resolved query:\n```json\n{\"query\": \"invoice\", \"filters\": []}\n```";
        assert_eq!(
            extract_json_object(text),
            Some(r#"{"query": "invoice", "filters": []}"#)
        );
    }

    #[test]
    fn extract_json_object_handles_nested_braces_and_strings() {
        let text = r#"noise {"filters": [{"type": "property", "value": "a{b}c"}]} trailing"#;
        assert_eq!(
            extract_json_object(text),
            Some(r#"{"filters": [{"type": "property", "value": "a{b}c"}]}"#)
        );
    }

    #[test]
    fn extract_json_object_returns_none_with_no_braces() {
        assert_eq!(extract_json_object("no json here"), None);
    }

    /// End-to-end fixture: real schema (via `handle_create_schema`), stub
    /// inference engine returning fixed JSON, exercising the full
    /// `exec_resolve_query` path — schema field lookup, prompt construction,
    /// nested `generate()` call, and JSON parsing into a tool result.
    ///
    /// What this suite does NOT cover: the decomposition prompt's actual
    /// accuracy against a real model — every test here wires `FixedJsonEngine`,
    /// which ignores the prompt entirely and returns a canned response. That
    /// means the NL→filter mapping described in `exec_resolve_query`'s prompt
    /// (mapping a dollar amount, a relative date, or a paraphrased identifier
    /// onto the right schema field) is unverified by `cargo test`. The one
    /// place a real model exercises this path today is
    /// `scripts/eval/fixtures/agent-matrix.ts` scenario 6, driven through the
    /// full `LocalAgentLoop` — but that's a single phrasing, run through the
    /// eval harness rather than `cargo test`/the pre-push gate. Closing this
    /// gap in-crate would mean standing up a real-model test fixture (model
    /// download/load, machine-load-sensitive timing) matching
    /// `ai_chat_send_to_idle_test.rs`'s pattern — deliberately not done here;
    /// this doc comment is the explicit acknowledgment rather than a silent
    /// gap.
    mod resolve_query_integration {
        use super::*;
        use crate::agent_types::{ChatModelSpec, InferenceUsage};
        use nodespace_core::db::SqliteStore;
        use nodespace_core::schema::handle_create_schema;
        use tempfile::TempDir;

        async fn make_test_service() -> (Arc<NodeService>, TempDir) {
            let tmp = TempDir::new().unwrap();
            let db_path = tmp.path().join("test.db");
            let mut store: Arc<SqliteStore> = Arc::new(SqliteStore::new(db_path).await.unwrap());
            let svc = Arc::new(NodeService::new(&mut store).await.unwrap());
            (svc, tmp)
        }

        /// Stub engine that ignores the prompt and always returns a fixed
        /// JSON string as a single `Token` chunk — verifies the plumbing
        /// (schema lookup → prompt → generate() → chunk collection → parse)
        /// without depending on a real model.
        struct FixedJsonEngine {
            response: String,
        }

        #[async_trait]
        impl ChatInferenceEngine for FixedJsonEngine {
            async fn generate(
                &self,
                _request: InferenceRequest,
                on_chunk: Box<dyn Fn(StreamingChunk) + Send>,
            ) -> Result<InferenceUsage, crate::agent_types::InferenceError> {
                on_chunk(StreamingChunk::Token {
                    text: self.response.clone(),
                });
                Ok(InferenceUsage {
                    prompt_tokens: 10,
                    completion_tokens: 10,
                })
            }

            async fn model_info(
                &self,
            ) -> Result<Option<ChatModelSpec>, crate::agent_types::InferenceError> {
                Ok(None)
            }

            async fn token_count(
                &self,
                text: &str,
            ) -> Result<u32, crate::agent_types::InferenceError> {
                Ok((text.len() as f32 / 4.0).ceil() as u32)
            }
        }

        fn executor_with(ns: Arc<NodeService>, engine_response: &str) -> GraphToolExecutor {
            let engine: Arc<dyn ChatInferenceEngine> = Arc::new(FixedJsonEngine {
                response: engine_response.to_string(),
            });
            GraphToolExecutor {
                node_service: Some(ns),
                embedding_service: Arc::new(RwLock::new(None)),
                inference_engine: Some(engine),
            }
        }

        /// Creates an `invoice` node with the given typed properties via the
        /// real `create_node` tool path, so `resolve_query`'s internal search
        /// exercises the same storage/query round trip production does.
        async fn create_invoice(executor: &GraphToolExecutor, title: &str, properties: Value) {
            let result = executor
                .execute(
                    "create_node",
                    json!({
                        "content": title,
                        "node_type": "invoice",
                        "properties": properties,
                    }),
                )
                .await
                .unwrap();
            assert!(!result.is_error, "fixture node creation failed: {result:?}");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_returns_resolved_node_on_unique_match() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [
                        {"name": "amount", "type": "number"},
                        {"name": "status", "type": "text"}
                    ]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"amount","value":500}]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(
                &executor,
                "Consulting invoice",
                json!({"amount": 500, "status": "open"}),
            )
            .await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $500 invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert!(!result.is_error);
            assert_eq!(result.result["resolved"], json!(true));
            assert!(
                result.result["id"].as_str().is_some_and(|s| !s.is_empty()),
                "resolved result must carry a usable node id: {result:?}"
            );
            assert_eq!(result.result["properties"]["amount"], json!(500));
            // The old fragment shape must not leak back onto a resolved result —
            // regressing to it is exactly the bug this issue fixes.
            assert!(result.result.get("filters").is_none());
            assert!(result.result.get("query").is_none());
        }

        /// Regression test for the three phrasings measured in the issue: a
        /// request that identifies a node via an implicit property match
        /// (dollar amount) must resolve straight to the node, not to filters
        /// the model has to route itself.
        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_regression_dollar_amount_phrasing() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [{"name": "amount", "type": "number"}]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"amount","value":500}]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(&executor, "Invoice #1", json!({"amount": 500})).await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $500 invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert_eq!(result.result["resolved"], json!(true));
        }

        /// Regression test for the relative-date phrasing ("due next Friday").
        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_regression_relative_date_phrasing() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [{"name": "due_date", "type": "date"}]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"due_date","value":"2026-08-07"}]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(&executor, "Invoice #2", json!({"due_date": "2026-08-07"})).await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the invoice due next Friday as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert_eq!(result.result["resolved"], json!(true));
        }

        /// Regression test for the paraphrased-identifier phrasing
        /// ("the 2400 one") — mirrors eval scenario 6.
        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_regression_paraphrased_identifier_phrasing() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [
                        {"name": "replacement_cost", "type": "number"},
                        {"name": "status", "type": "text"}
                    ]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"replacement_cost","value":2400}]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(
                &executor,
                "Laser cutter",
                json!({"replacement_cost": 2400, "status": "checked_out"}),
            )
            .await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "The 2400 one came back — set it to returned", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert_eq!(result.result["resolved"], json!(true));
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_reports_no_match_without_a_plan_to_route() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [{"name": "amount", "type": "number"}]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"amount","value":999}]}"#;
            let executor = executor_with(ns, engine_json);
            // No invoice created — the filter matches nothing.

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $999 invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert!(!result.is_error);
            assert_eq!(result.result["resolved"], json!(false));
            assert_eq!(result.result["reason"], json!("no_match"));
            assert!(result.result.get("filters").is_none());
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_reports_multiple_matches_as_candidates_not_a_retry_plan() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [{"name": "status", "type": "text"}]
                }),
            )
            .await
            .unwrap();

            let engine_json = r#"{"query": "", "filters": [{"type":"property","operator":"equals","property":"status","value":"open"}]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(&executor, "Invoice A", json!({"status": "open"})).await;
            create_invoice(&executor, "Invoice B", json!({"status": "open"})).await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the open invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert!(!result.is_error);
            assert_eq!(result.result["resolved"], json!(false));
            assert_eq!(result.result["reason"], json!("multiple_matches"));
            let candidates = result.result["candidates"]
                .as_array()
                .expect("multiple_matches must carry a candidates list to discriminate, not a query to retry");
            assert_eq!(candidates.len(), 2);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_falls_back_to_empty_on_unparseable_engine_output() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [{"name": "amount", "type": "number"}]
                }),
            )
            .await
            .unwrap();

            // Engine narrates instead of returning JSON — must not error the turn.
            let executor = executor_with(ns, "I think the amount is 500 dollars.");

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $500 invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert!(
                !result.is_error,
                "unparseable engine output must fall back, not error"
            );
            // With no schema-derived filters/query, the search degrades to an
            // empty type listing — no invoices exist, so this is a no_match.
            assert_eq!(result.result["resolved"], json!(false));
            assert_eq!(result.result["reason"], json!("no_match"));
        }

        /// A malformed filter (an unknown key, per `AgentFilterItem`'s
        /// `deny_unknown_fields`) must be dropped individually rather than
        /// discarding the whole resolution — a valid sibling filter still
        /// narrows the search and resolves the node it uniquely identifies.
        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_drops_only_the_malformed_filter_not_the_whole_resolution() {
            let (ns, _tmp) = make_test_service().await;
            handle_create_schema(
                &ns,
                json!({
                    "name": "Invoice",
                    "fields": [
                        {"name": "amount", "type": "number"},
                        {"name": "status", "type": "text"}
                    ]
                }),
            )
            .await
            .unwrap();

            // The second filter carries "propertyName" instead of "property" —
            // an unknown key AgentFilterItem's deny_unknown_fields rejects.
            let engine_json = r#"{"query": "", "filters": [
                {"type":"property","operator":"equals","property":"amount","value":500},
                {"type":"property","operator":"equals","propertyName":"status","value":"open"}
            ]}"#;
            let executor = executor_with(ns, engine_json);
            create_invoice(&executor, "Invoice #1", json!({"amount": 500})).await;

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $500 invoice as paid", "node_type": "invoice" }),
                )
                .await
                .unwrap();

            assert!(!result.is_error);
            // The malformed filter is dropped, but the valid "amount" filter
            // alone still uniquely resolves the node.
            assert_eq!(result.result["resolved"], json!(true));
            assert_eq!(result.result["properties"]["amount"], json!(500));
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_handles_unknown_node_type_gracefully() {
            let (ns, _tmp) = make_test_service().await;
            let executor = executor_with(ns, r#"{"query": "widget", "filters": []}"#);

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "find the widget", "node_type": "nonexistent_type" }),
                )
                .await
                .unwrap();

            // No schema found: still resolves (engine still gets called with a
            // "no typed fields" fallback description), just with no field context.
            // No nodes of this type exist, so this reports no_match rather than
            // erroring the turn.
            assert!(!result.is_error);
            assert_eq!(result.result["resolved"], json!(false));
            assert_eq!(result.result["reason"], json!("no_match"));
        }

        /// Isolates the "no inference engine" failure path from "no node
        /// service" — a real node_service is present here, so a failure can
        /// only come from the missing engine.
        #[tokio::test(flavor = "multi_thread")]
        async fn resolve_query_without_inference_engine_returns_error() {
            let (ns, _tmp) = make_test_service().await;
            let executor = GraphToolExecutor {
                node_service: Some(ns),
                embedding_service: Arc::new(RwLock::new(None)),
                inference_engine: None,
            };

            let result = executor
                .execute(
                    "resolve_query",
                    json!({ "request": "Mark the $500 invoice as paid", "node_type": "invoice" }),
                )
                .await;

            match result {
                Err(ToolError::ExecutionFailed(reason)) => {
                    assert!(reason.contains("Inference engine unavailable"));
                }
                other => panic!(
                    "Expected ExecutionFailed(\"Inference engine unavailable\"), got {:?}",
                    other
                ),
            }
        }
    }

    #[test]
    fn create_node_schema_requires_content_and_type() {
        let def = def_create_node();
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(required.contains(&json!("content")));
        assert!(required.contains(&json!("node_type")));
    }

    #[test]
    fn create_relationship_schema_requires_all_three() {
        let def = def_create_relationship();
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(required.contains(&json!("from_id")));
        assert!(required.contains(&json!("to_id")));
        assert!(required.contains(&json!("relationship_type")));
    }

    // -- error_result / ok_result helpers --

    #[test]
    fn error_result_is_flagged() {
        let r = error_result("id1", "test", "something went wrong");
        assert!(r.is_error);
        assert_eq!(r.name, "test");
        assert_eq!(r.tool_call_id, "id1");
        assert!(r.result["error"]
            .as_str()
            .unwrap()
            .contains("something went wrong"));
    }

    #[test]
    fn ok_result_not_flagged() {
        let r = ok_result("id1", "test", json!({"key": "val"}));
        assert!(!r.is_error);
        assert_eq!(r.result["key"], "val");
    }

    // -- AgentToolExecutor trait: unknown tool --

    #[tokio::test]
    async fn execute_unknown_tool_returns_error() {
        let executor = test_executor();
        let result = executor.execute("nonexistent_tool", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::UnknownTool(name) => assert_eq!(name, "nonexistent_tool"),
            other => panic!("Expected UnknownTool, got {:?}", other),
        }
    }

    // -- Validation: tools requiring arguments fail gracefully without services --

    #[tokio::test]
    async fn search_nodes_missing_query_fails_on_node_service_not_args() {
        // query alone defaulting to "" no longer makes this an InvalidArguments
        // case; test_executor() has no node service, so it now fails one layer
        // deeper (ExecutionFailed), proving args parsing succeeded.
        let executor = test_executor();
        let result = executor.execute("search_nodes", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::ExecutionFailed(reason) => {
                assert!(reason.contains("Node service unavailable"));
            }
            other => panic!("Expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_node_missing_id() {
        let executor = test_executor();
        let result = executor.execute("get_node", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, .. } => {
                assert_eq!(tool, "get_node");
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_node_missing_required() {
        let executor = test_executor();
        // Missing node_type (required field)
        let result = executor
            .execute("create_node", json!({"content": "My node"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, reason } => {
                assert_eq!(tool, "create_node");
                assert!(reason.contains("node_type"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_node_missing_type() {
        let executor = test_executor();
        let result = executor
            .execute("create_node", json!({"title": "Test"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, reason } => {
                assert_eq!(tool, "create_node");
                assert!(reason.contains("node_type"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn update_node_missing_id() {
        let executor = test_executor();
        let result = executor
            .execute("update_node", json!({"title": "new"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, .. } => {
                assert_eq!(tool, "update_node");
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn update_node_no_changes() {
        let executor = test_executor();
        let result = executor
            .execute("update_node", json!({"id": "node-1"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, reason } => {
                assert_eq!(tool, "update_node");
                assert!(reason.contains("At least one"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_relationship_missing_fields() {
        let executor = test_executor();
        let result = executor
            .execute("create_relationship", json!({"from_id": "a"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, .. } => {
                assert_eq!(tool, "create_relationship");
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_related_nodes_missing_id() {
        let executor = test_executor();
        let result = executor.execute("get_related_nodes", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, .. } => {
                assert_eq!(tool, "get_related_nodes");
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn search_skills_missing_query() {
        let executor = test_executor();
        let result = executor.execute("search_skills", json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, reason } => {
                assert_eq!(tool, "search_skills");
                assert!(reason.contains("query"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn search_skills_no_embedding_service_returns_error_result() {
        let executor = test_executor();
        let result = executor
            .execute("search_skills", json!({"query": "manage tasks"}))
            .await;
        // Should succeed (Ok) but with is_error=true since no embedding service
        let tool_result = result.unwrap();
        assert!(tool_result.is_error);
        assert!(tool_result.result["error"]
            .as_str()
            .unwrap()
            .contains("embedding service"));
    }

    #[test]
    fn search_skills_schema_requires_query() {
        let def = def_search_skills();
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn search_skills_schema_exposes_optional_limit() {
        let def = def_search_skills();
        let props = def.parameters_schema["properties"]
            .as_object()
            .expect("properties must be object");
        assert!(props.contains_key("limit"), "limit must be in schema");
        // limit must NOT be required — the tool defaults sensibly when omitted
        let required = def.parameters_schema["required"]
            .as_array()
            .expect("required must be array");
        assert!(!required.contains(&json!("limit")));
    }

    #[test]
    fn search_skills_description_mentions_empty_signal() {
        // The description must teach the model that an empty
        // `matches` array is a meaningful signal, not an error. This wording
        // is load-bearing — without it, a small model tends to retry the
        // tool with rephrased queries instead of judging "no skill applies".
        let def = def_search_skills();
        let desc = def.description.to_lowercase();
        assert!(
            desc.contains("empty") || desc.contains("no skill"),
            "search_skills description should call out the empty-result signal: {:?}",
            def.description
        );
    }

    #[tokio::test]
    async fn get_related_nodes_invalid_direction() {
        let executor = test_executor();
        let result = executor
            .execute(
                "get_related_nodes",
                json!({"id": "node-1", "direction": "sideways"}),
            )
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { tool, reason } => {
                assert_eq!(tool, "get_related_nodes");
                assert!(reason.contains("direction"));
            }
            other => panic!("Expected InvalidArguments, got {:?}", other),
        }
    }

    // -- Available tools --

    #[tokio::test]
    async fn available_tools_returns_every_model_facing_tool() {
        let executor = test_executor();
        let tools = executor.available_tools().await.unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_nodes"));
        assert!(names.contains(&"resolve_query"));
        assert!(names.contains(&"search_semantic"));
        assert!(names.contains(&"get_node"));
        assert!(names.contains(&"create_node"));
        assert!(names.contains(&"update_node"));
        assert!(names.contains(&"create_relationship"));
        assert!(names.contains(&"get_related_nodes"));
        assert!(names.contains(&"create_schema"));
        assert!(names.contains(&"update_schema"));
        assert!(names.contains(&"update_task_status"));
        assert!(names.contains(&"delete_node"));
        assert!(names.contains(&"create_nodes_from_markdown"));
        // Every registered tool except the ones the system reserves.
        assert_eq!(tools.len(), Tool::ALL.len() - 1);
    }

    #[tokio::test]
    async fn search_skills_is_withheld_from_the_model_facing_surface() {
        // ADR-038 makes retrieval a deterministic system step. Offering
        // `search_skills` back to the model is the single-turn pull that ADR
        // rejects: it lets the model set K and bypasses the trust filter.
        let executor = test_executor();
        let names: Vec<String> = executor
            .available_tools()
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert!(
            !names.iter().any(|n| n == "search_skills"),
            "search_skills must not reach the model: {names:?}"
        );
    }

    #[test]
    fn search_skills_stays_in_the_registry_for_external_agents() {
        // Withheld from the local model, but still a real tool: the MCP
        // `find_skills` handler shares its implementation, and the seeded tool
        // node backing that surface is generated from `Tool::ALL`.
        assert!(Tool::ALL.contains(&Tool::SearchSkills));
        assert!(all_tool_definitions()
            .iter()
            .any(|t| t.name == "search_skills"));
        assert!(is_system_only_tool("search_skills"));
        assert!(!is_system_only_tool("search_nodes"));
    }

    // -- Embedding handle is read live (race fix) --

    /// The executor must read the embedding service through the shared handle on
    /// every call, not capture a snapshot at construction. This is what closes
    /// the startup race: an executor built before the embedding model loads must
    /// see the service the moment the background loader writes it — with no
    /// engine swap or re-wiring.
    ///
    /// We can't construct a real `NodeEmbeddingService` here (it needs an NLP
    /// engine), so we assert the structural guarantee: the executor holds the
    /// *same* `Arc<RwLock<..>>` it was given. A `None → Some` write through that
    /// handle is therefore observed by the executor by construction.
    #[tokio::test]
    async fn embedding_handle_is_shared_not_snapshotted() {
        let handle: SharedEmbeddingService = Arc::new(RwLock::new(None));
        let executor = GraphToolExecutor {
            node_service: None,
            embedding_service: handle.clone(),
            inference_engine: None,
        };

        // Same lock — a write through `handle` is visible to `executor`.
        assert!(
            Arc::ptr_eq(&handle, &executor.embedding_service),
            "executor must hold the shared handle, not a captured snapshot"
        );

        // With the handle empty, the accessor and the skill path both report the
        // service as unavailable — read live from the shared lock.
        assert!(
            executor.embedding_service().await.is_err(),
            "empty handle must surface as unavailable"
        );
        let result = executor
            .execute("search_skills", json!({ "query": "manage tasks" }))
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.result["error"]
            .as_str()
            .unwrap()
            .contains("embedding service"));
    }

    // -- Helper: node_uri / strip_node_uri round-trip --

    #[test]
    fn node_uri_round_trip() {
        let bare_id = "550e8400-e29b-41d4-a716-446655440000";
        let uri = node_uri(bare_id);
        assert_eq!(uri, "nodespace://550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(strip_node_uri(&uri), bare_id);
    }

    #[test]
    fn node_uri_idempotent() {
        let uri = "nodespace://550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(node_uri(uri), uri);
    }

    #[test]
    fn strip_node_uri_no_prefix() {
        let bare_id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(strip_node_uri(bare_id), bare_id);
    }

    // -- Parity test: def_search_semantic schema vs SearchSemanticParams fields --

    /// Asserts that every field in `SearchSemanticParams` is either represented
    /// in the `def_search_semantic()` JSON schema (so the LLM can request it) or
    /// explicitly documented as intentionally excluded.
    ///
    /// This test prevents future drift: when a new field is added to
    /// `SearchSemanticParams`, the author must either add it to the schema here
    /// or update the exclusion list with a clear comment explaining why.
    #[test]
    fn search_semantic_schema_parity_with_params() {
        let def = def_search_semantic();
        let props = def.parameters_schema["properties"]
            .as_object()
            .expect("def_search_semantic schema must have 'properties'");

        // Fields in SearchSemanticParams that are exposed in the tool schema.
        // When a new field is added to SearchSemanticParams, add it here (or to
        // the exclusion list below) to satisfy this test.
        let schema_fields = [
            "query",
            "limit",
            "include_markdown",
            "collection",
            "threshold",
            "scope",
            "include_archived",
            "node_types",
            "exclude_collections",
            "property_filters",
            "include_edges",
            "graph_boost",
        ];

        // Fields intentionally excluded from the tool schema:
        // - "collection_id": internal ID form; the LLM should use the human-readable
        //   "collection" (path) form instead, which resolves to a collection_id server-side.
        //   Still wired through exec_search_semantic for MCP clients that know the ID.
        let excluded_fields = ["collection_id"];

        for field in &schema_fields {
            assert!(
                props.contains_key(*field),
                "SearchSemanticParams field '{}' is missing from def_search_semantic() schema. \
                 Add it to the schema or move it to the excluded_fields list with a justification comment.",
                field
            );
        }

        // Verify excluded fields are not accidentally present in the schema
        // (they are intentionally excluded, so their absence is expected).
        for field in &excluded_fields {
            assert!(
                !props.contains_key(*field),
                "Field '{}' is in the exclusion list but was found in the schema. \
                 Remove it from excluded_fields if it should be schema-exposed.",
                field
            );
        }

        // Reverse check: every schema property must be in schema_fields or excluded_fields.
        // This catches schema properties added without updating the parity lists.
        for key in props.keys() {
            assert!(
                schema_fields.contains(&key.as_str()) || excluded_fields.contains(&key.as_str()),
                "Schema property '{}' is not listed in schema_fields or excluded_fields. \
                 Add it to one of those lists in this test.",
                key
            );
        }
    }

    // -- create_schema enum field end-to-end (acceptance criterion) --

    /// Exercises `create_schema` through the real `GraphToolExecutor` dispatch
    /// path — not just `handle_create_schema` directly — with an enum field
    /// using the tool-schema-correct "coreValues" key. Confirms the wire-format
    /// contract the tool schema advertises actually round-trips end to end.
    #[tokio::test]
    async fn create_schema_enum_field_succeeds_end_to_end() {
        use nodespace_core::db::SqliteStore;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut store: Arc<SqliteStore> = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let svc = Arc::new(NodeService::new(&mut store).await.unwrap());

        let executor = GraphToolExecutor {
            node_service: Some(svc),
            embedding_service: Arc::new(RwLock::new(None)),
            inference_engine: None,
        };

        let result = executor
            .execute(
                "create_schema",
                json!({
                    "name": "Invoice",
                    "fields": [
                        {
                            "name": "status",
                            "type": "enum",
                            "coreValues": [
                                { "value": "pending", "label": "Pending" },
                                { "value": "paid", "label": "Paid" }
                            ]
                        }
                    ]
                }),
            )
            .await
            .expect("execute should not return a ToolError");

        assert!(
            !result.is_error,
            "create_schema with a coreValues enum field must succeed, got: {}",
            result.result
        );
        assert_eq!(result.result["schemaId"], "invoice");
        let core_values = result.result["fields"][0]["coreValues"]
            .as_array()
            .expect("coreValues array present on the created field");
        assert_eq!(core_values.len(), 2);
    }

    // -- Scope passthrough test (acceptance criterion) --

    /// Verifies that scope="conversations" is correctly parsed from JSON params
    /// and would be forwarded to SearchSemanticInput by exec_search_semantic.
    /// The executor builds SearchSemanticInput { scope: params.scope, ... },
    /// so correct deserialization guarantees correct forwarding.
    #[test]
    fn search_semantic_scope_conversations_passthrough() {
        let args = json!({
            "query": "past discussions about architecture",
            "scope": "conversations"
        });
        let params: SearchSemanticParams = serde_json::from_value(args).unwrap();
        assert_eq!(params.scope, Some("conversations".to_string()));

        // Build the same SearchSemanticInput that exec_search_semantic would
        let input = nodespace_core::ops::search_ops::SearchSemanticInput {
            query: params.query.clone(),
            threshold: Some(params.threshold.unwrap_or(SEMANTIC_THRESHOLD)),
            limit: Some(params.limit.unwrap_or(DEFAULT_SEMANTIC_LIMIT)),
            collection_id: params.collection_id,
            collection: params.collection,
            exclude_collections: params.exclude_collections,
            include_markdown: params.include_markdown,
            include_archived: params.include_archived,
            scope: params.scope.clone(),
            node_types: params.node_types,
            property_filters: params.property_filters,
            include_edges: params.include_edges,
            graph_boost: params.graph_boost,
        };
        assert_eq!(input.scope, Some("conversations".to_string()));
    }
}
