//! Skill discovery operations.
//!
//! Shared logic for skill search used by the local agent's `search_skills`
//! tool and the MCP `find_skills` handler exposed to external agents.

use crate::services::{
    flatten_subtree_content, NodeEmbeddingService, NodeService, SearchNodeFilters,
};
use serde_json::{json, Value};
use std::sync::Arc;

use super::OpsError;

/// Similarity threshold for skill search.
///
/// Set to zero so the model sees every match with strictly positive cosine
/// similarity, including weak ones, and decides for itself which (if any)
/// skill is relevant. The underlying store filter is `composite_score >
/// $threshold`, so a zero match (orthogonal vector) is still excluded — but
/// that's the cosine noise floor, not a confidence judgment call.
/// Server-side bucketing was explicitly removed in favour of letting the
/// LLM judge confidence from the raw score; a non-zero floor here would
/// partially undo that by silently hiding the long tail.
const SKILL_SEARCH_THRESHOLD: f32 = 0.0;

/// Maximum schemas to include in `schema_metadata` when no `node_types` scope
/// is set on the matched skill. Bounds token cost for general-purpose skills.
/// Skills that declare an explicit `node_types` list are not capped.
const MAX_UNSCOPED_SCHEMA_METADATA: usize = 5;

/// Upper bound on `limit` requested by the caller.
///
/// Skill libraries are small in practice (~8-20 seeded skills plus a handful
/// of user-defined ones). A cap of 10 is large enough to expose every skill
/// in a typical workspace yet keeps the response token-cheap for small local
/// models. Revisit if user-defined skill libraries grow past ~30 skills.
const MAX_SKILL_LIMIT: usize = 10;

/// Input for find_skills operation.
#[derive(Debug)]
pub struct FindSkillsInput {
    pub query: String,
    pub limit: Option<usize>,
}

/// Output for find_skills operation.
#[derive(Debug)]
pub struct FindSkillsOutput {
    pub skills: Vec<Value>,
    pub query: String,
    pub total_results: usize,
}

/// Render a skill node's child subtree as flat markdown.
///
/// Fetches the full subtree in a single DB query, then walks it depth-first
/// (root children first, their children next) and joins each node's content
/// with a blank line separator. The skill root itself is excluded — callers
/// already have its `name`/`description`.
///
/// Empty/childless skills return an empty string without error.
async fn render_skill_instructions(node_service: &NodeService, skill_id: &str) -> String {
    // root_node unused — callers already have the skill's name/description
    let (_, node_map, adjacency_list) = match node_service.get_subtree_data(skill_id).await {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!(
                error = %e,
                skill_id = %skill_id,
                "render_skill_instructions: failed to fetch subtree"
            );
            return String::new();
        }
    };

    // One get_subtree_data query per skill. Acceptable under MAX_SKILL_LIMIT = 10;
    // a batch API would eliminate serial round trips if the limit grows.
    flatten_subtree_content(skill_id, &node_map, &adjacency_list).join("\n\n")
}

/// A skill node's discovery-relevant properties, decoded from whichever shape
/// `node.properties` is actually in.
#[derive(Debug, PartialEq)]
struct SkillProperties {
    description: String,
    tool_whitelist: Value,
    scoped_type_ids: Vec<String>,
}

impl SkillProperties {
    /// Decode from a skill node's `properties`.
    ///
    /// `skill` has a registered core schema (ADR-030), so `NodeService` hoists
    /// its schema-defined fields under `properties.skill.*` on write (same as
    /// `task` under `properties.task.*` — see `behaviors/mod.rs`'s
    /// task-property validation). Reading `node.properties` flat found
    /// nothing on any seeded skill node, so every skill's tools and
    /// entity-type guidance silently vanished at the routing gate. Fall back
    /// to the flat top level for a node that predates hoisting or was
    /// constructed directly, as every test in this module does.
    fn from_node_properties(properties: &Value) -> Self {
        let skill_props = properties
            .get("skill")
            .filter(|v| v.is_object())
            .unwrap_or(properties);

        let description = skill_props
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_whitelist = skill_props
            .get("tool_whitelist")
            .cloned()
            .unwrap_or(json!([]));
        let scoped_type_ids = skill_props
            .get("node_types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            description,
            tool_whitelist,
            scoped_type_ids,
        }
    }
}

/// Whether `phrase` (already lowercased) appears in `haystack` (already
/// lowercased) at word boundaries — not as a substring of a longer word.
///
/// Single-word phrases (the common case: a type id like `ticket`) are
/// checked by exact token match. Multi-word phrases (a display name like
/// `Pull Request`) are checked as a contiguous run of exact tokens, so
/// `pull` alone does not count as a match.
fn mentions_phrase(haystack: &str, phrase: &str) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }
    let tokens: Vec<&str> = haystack
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if words.len() == 1 {
        tokens.iter().any(|t| *t == words[0])
    } else {
        tokens
            .windows(words.len())
            .any(|window| window.iter().zip(&words).all(|(t, w)| t == w))
    }
}

/// The single non-core schema `query` names by id or display name, when
/// exactly one is nameable this way.
///
/// A purely mechanical, string-level signal — not a new relevance model or a
/// confidence judgment. Two or more named types (the query mentions several
/// non-core types by name) or none (it names none) both return `None`: real
/// ambiguity is left to the caller's existing broader fallback rather than
/// guessed at here.
///
/// This is the narrower, per-query retrieval pass core#2148 calls for: a
/// request that plainly says "create a ticket" scopes `schema_metadata` to
/// just `ticket` instead of `find_skills`' unscoped top-N fallback, which
/// would otherwise sweep in every other non-core type in the same fallback
/// window (e.g. `adr`) — the exact imprecision that caused
/// `declare_write_tool_fields` to union unrelated types' fields onto a write
/// tool's declaration. Per-skill `node_types` (the other mechanism
/// `find_skills` already supports) cannot fix this case: the seeded skills
/// whose whitelist includes `create_node`/`update_node` (Node Creation,
/// Graph Editing) are deliberately generic across every non-core type, so
/// there is no single static type list to give them without contradicting
/// their purpose. This mechanism narrows per query instead, so a generic
/// skill still contributes a scoped `schema_metadata` when the query itself
/// determines the type.
fn schema_named_in_query<'a>(
    query: &str,
    all_schemas: &'a [crate::models::SchemaNode],
) -> Option<&'a crate::models::SchemaNode> {
    let query_lower = query.to_lowercase();
    let mut named = all_schemas.iter().filter(|s| {
        !s.is_core
            && (mentions_phrase(&query_lower, &s.id.to_lowercase())
                || mentions_phrase(&query_lower, &s.content.to_lowercase()))
    });

    let first = named.next()?;
    if named.next().is_some() {
        // Two or more non-core types named in the same query — genuinely
        // ambiguous by this signal. Not this function's job to break the tie.
        None
    } else {
        Some(first)
    }
}

/// Search for skill nodes via semantic search and return flat results with
/// schema metadata for the matched skill's scoped types.
///
/// Returns up to `limit` matches (default 3) with `id`, `name`, `description`,
/// `confidence`, `tools`, `schema_metadata`, and `instructions`. The `instructions`
/// field is the skill's child subtree rendered to markdown — the actual procedure
/// the model must follow. The `schema_metadata` field contains type IDs,
/// field names, and enum values for entity types associated with the skill's
/// `tool_whitelist` scope.
///
/// No filtering or bucketing — the caller (model or MCP client) inspects the
/// raw confidence score and decides how to act. An empty `skills` array is a
/// meaningful signal: "no skill is even loosely related to this query."
pub async fn find_skills(
    embedding_service: &Arc<NodeEmbeddingService>,
    node_service: &Arc<NodeService>,
    input: FindSkillsInput,
) -> Result<FindSkillsOutput, OpsError> {
    let limit = input.limit.unwrap_or(3).min(MAX_SKILL_LIMIT);

    // Skills are a small fraction of the total corpus (~8-20 nodes in a workspace
    // of potentially thousands). `semantic_search_nodes` applies a 3× over-fetch
    // when filters are active, but that still yields only `limit * 3` KNN candidates
    // from the global embedding space. At low skill density, most of those slots will
    // be occupied by non-skill nodes and discarded. Pre-inflating here ensures the
    // inner KNN window is large enough to contain the requested number of skill nodes
    // before post-filtering. `semantic_search_nodes` already truncates to the limit
    // it receives, so we truncate the final result ourselves.
    let search_limit = (limit * 5).max(limit + 15);
    let skill_filter = SearchNodeFilters {
        node_types: Some(vec!["skill".to_string()]),
        property_filters: None,
    };
    let mut skill_results = embedding_service
        .semantic_search_nodes(
            &input.query,
            search_limit,
            SKILL_SEARCH_THRESHOLD,
            Some(&skill_filter),
        )
        .await
        .map_err(|e| OpsError::Internal(format!("Skill search failed: {}", e)))?;
    skill_results.truncate(limit);

    // Fetch all schemas once; used to attach metadata to each matched skill.
    let all_schemas = node_service
        .get_all_schemas()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "find_skills: failed to fetch schemas for metadata attachment");
        })
        .unwrap_or_default();

    let total_results = skill_results.len();
    let mut skills = Vec::with_capacity(total_results);

    // Computed once per call, not per matched skill: it depends only on the
    // query text and the schema list, neither of which varies across
    // `skill_results`. See `schema_named_in_query` — `None` when the query
    // doesn't determine a single non-core type, in which case every
    // unscoped-branch candidate below keeps today's fallback unchanged.
    let query_named_schema = schema_named_in_query(&input.query, &all_schemas);

    for (node, confidence) in &skill_results {
        let SkillProperties {
            description,
            tool_whitelist,
            scoped_type_ids,
        } = SkillProperties::from_node_properties(&node.properties);

        // Attach schema metadata for entity types relevant to this skill.
        // The skill's `node_types` property lists the type IDs in scope. When
        // absent: if the query itself names exactly one non-core type, scope
        // to that type (see `schema_named_in_query`); otherwise fall back to
        // all custom (non-core) schemas, capped at MAX_UNSCOPED_SCHEMA_METADATA
        // to bound token cost for general-purpose skills whose query didn't
        // resolve to one type.
        let schema_metadata: Vec<Value> = if scoped_type_ids.is_empty() {
            match query_named_schema {
                Some(named) => vec![
                    super::entity_types_block::EntityTypeDescriptor::from_schema(named).to_json(),
                ],
                None => all_schemas
                    .iter()
                    .filter(|s| !s.is_core)
                    .take(MAX_UNSCOPED_SCHEMA_METADATA)
                    .map(|s| {
                        // Encoded from the same descriptor the prompt block
                        // renders from, so this JSON cannot describe a schema
                        // differently than the model is told about it.
                        super::entity_types_block::EntityTypeDescriptor::from_schema(s).to_json()
                    })
                    .collect(),
            }
        } else {
            all_schemas
                .iter()
                .filter(|s| scoped_type_ids.contains(&s.id))
                .take(scoped_type_ids.len())
                .map(|s| super::entity_types_block::EntityTypeDescriptor::from_schema(s).to_json())
                .collect()
        };

        let instructions = render_skill_instructions(node_service.as_ref(), &node.id).await;

        skills.push(json!({
            "id": node.id,
            "name": node.content,
            "description": description,
            "confidence": confidence,
            "tools": tool_whitelist,
            "schema_metadata": schema_metadata,
            "instructions": instructions,
        }));
    }

    tracing::info!(
        query = %input.query,
        results_found = total_results,
        top_score = skill_results.first().map(|(_, s)| *s).unwrap_or(0.0),
        "find_skills executed"
    );

    Ok(FindSkillsOutput {
        skills,
        query: input.query,
        total_results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, SchemaNode};
    use serde_json::json;
    use std::collections::HashMap;

    fn make_schema(id: &str, content: &str, is_core: bool) -> SchemaNode {
        SchemaNode::from_node(Node::new_with_id(
            id.to_string(),
            "schema".to_string(),
            content.to_string(),
            json!({ "isCore": is_core, "fields": [] }),
        ))
        .expect("schema node")
    }

    #[test]
    fn mentions_phrase_matches_a_single_word_type_id_at_word_boundaries() {
        assert!(mentions_phrase("create a ticket for the bug", "ticket"));
        assert!(!mentions_phrase("update the address field", "adr"));
        assert!(!mentions_phrase("no match here", "ticket"));
    }

    #[test]
    fn mentions_phrase_matches_a_multi_word_display_name_contiguously() {
        assert!(mentions_phrase(
            "open a pull request for this",
            "pull request"
        ));
        assert!(!mentions_phrase("pull the request later", "pull request"));
    }

    #[test]
    fn schema_named_in_query_scopes_to_the_single_named_non_core_type() {
        let schemas = vec![
            make_schema("ticket", "Ticket", false),
            make_schema("adr", "ADR", false),
        ];
        let found = schema_named_in_query("create a ticket for the login bug", &schemas);
        assert_eq!(found.map(|s| s.id.as_str()), Some("ticket"));
    }

    #[test]
    fn schema_named_in_query_matches_by_display_name_too() {
        let schemas = vec![
            make_schema("ticket", "Ticket", false),
            make_schema("adr", "Architecture Decision Record", false),
        ];
        let found = schema_named_in_query(
            "draft an architecture decision record for the new store",
            &schemas,
        );
        assert_eq!(found.map(|s| s.id.as_str()), Some("adr"));
    }

    #[test]
    fn schema_named_in_query_returns_none_when_two_types_are_named() {
        let schemas = vec![
            make_schema("ticket", "Ticket", false),
            make_schema("adr", "ADR", false),
        ];
        let found = schema_named_in_query("link this ticket to the adr", &schemas);
        assert!(found.is_none());
    }

    #[test]
    fn schema_named_in_query_returns_none_when_no_type_is_named() {
        let schemas = vec![
            make_schema("ticket", "Ticket", false),
            make_schema("adr", "ADR", false),
        ];
        let found = schema_named_in_query("what did we work on yesterday", &schemas);
        assert!(found.is_none());
    }

    #[test]
    fn schema_named_in_query_ignores_core_schemas() {
        // A query naming a core type ("task") alongside a real non-core match
        // must not let the core mention count toward ambiguity — core types
        // are never in the unscoped fallback's candidate pool to begin with.
        let schemas = vec![
            make_schema("task", "Task", true),
            make_schema("ticket", "Ticket", false),
        ];
        let found = schema_named_in_query("create a ticket, not a task", &schemas);
        assert_eq!(found.map(|s| s.id.as_str()), Some("ticket"));
    }

    fn make_node(id: &str, content: &str) -> Node {
        Node {
            id: id.to_string(),
            node_type: "text".to_string(),
            content: content.to_string(),
            version: 1,
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            properties: json!({}),
            mentions: vec![],
            mentioned_in: vec![],
            title: None,
            lifecycle_status: "active".to_string(),
        }
    }

    #[test]
    fn skill_properties_reads_the_hoisted_shape_node_service_actually_writes() {
        // The shape every seeded skill node has in the live database: `skill`
        // is a schema-typed node type (ADR-030), so NodeService hoists its
        // schema-defined fields under `properties.skill.*` on write, the same
        // as `task` under `properties.task.*`. A reader expecting flat
        // `properties.description` finds nothing on a real node.
        let properties = json!({
            "skill": {
                "description": "Modify existing nodes",
                "tool_whitelist": ["update_node", "resolve_query"],
                "node_types": ["invoice"],
            }
        });

        assert_eq!(
            SkillProperties::from_node_properties(&properties),
            SkillProperties {
                description: "Modify existing nodes".to_string(),
                tool_whitelist: json!(["update_node", "resolve_query"]),
                scoped_type_ids: vec!["invoice".to_string()],
            }
        );
    }

    #[test]
    fn skill_properties_falls_back_to_flat_shape() {
        // A node with no `skill` namespace key at all (predates hoisting, or
        // constructed directly the way every other test in this module does)
        // must still read correctly rather than silently returning defaults.
        let properties = json!({
            "description": "Modify existing nodes",
            "tool_whitelist": ["update_node"],
        });

        assert_eq!(
            SkillProperties::from_node_properties(&properties),
            SkillProperties {
                description: "Modify existing nodes".to_string(),
                tool_whitelist: json!(["update_node"]),
                scoped_type_ids: vec![],
            }
        );
    }

    #[test]
    fn skill_properties_falls_back_to_flat_shape_when_skill_key_is_null() {
        // `properties.get("skill")` returning `Some(&Value::Null)` must not
        // be treated as "the namespace is present" — a bare `.unwrap_or`
        // would substitute `Null` instead of falling back, silently
        // discarding any flat data sitting alongside it. Not reachable
        // through NodeService's real write path (hoisting always leaves an
        // object, never `null`), but a node built by hand or through a raw
        // store write could have this shape.
        let properties = json!({
            "skill": null,
            "description": "Modify existing nodes",
            "tool_whitelist": ["update_node"],
        });

        assert_eq!(
            SkillProperties::from_node_properties(&properties),
            SkillProperties {
                description: "Modify existing nodes".to_string(),
                tool_whitelist: json!(["update_node"]),
                scoped_type_ids: vec![],
            }
        );
    }

    #[test]
    fn skill_properties_missing_entirely_defaults_safely() {
        assert_eq!(
            SkillProperties::from_node_properties(&json!({})),
            SkillProperties {
                description: String::new(),
                tool_whitelist: json!([]),
                scoped_type_ids: vec![],
            }
        );
    }

    #[test]
    fn flatten_subtree_content_empty_skill() {
        let node_map: HashMap<String, Node> = HashMap::new();
        let adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        let parts = flatten_subtree_content("skill-root", &node_map, &adjacency_list);
        assert!(parts.is_empty());
    }

    #[test]
    fn flatten_subtree_content_flat_children() {
        let mut node_map = HashMap::new();
        node_map.insert("c1".to_string(), make_node("c1", "Step one"));
        node_map.insert("c2".to_string(), make_node("c2", "Step two"));
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        adjacency_list.insert(
            "skill-root".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        let parts = flatten_subtree_content("skill-root", &node_map, &adjacency_list);
        assert_eq!(parts, vec!["Step one", "Step two"]);
    }

    #[test]
    fn flatten_subtree_content_nested_children() {
        let mut node_map = HashMap::new();
        node_map.insert("c1".to_string(), make_node("c1", "Section header"));
        node_map.insert("c1a".to_string(), make_node("c1a", "Sub-step A"));
        node_map.insert("c2".to_string(), make_node("c2", "Another section"));
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        adjacency_list.insert(
            "skill-root".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        adjacency_list.insert("c1".to_string(), vec!["c1a".to_string()]);
        let parts = flatten_subtree_content("skill-root", &node_map, &adjacency_list);
        assert_eq!(
            parts,
            vec!["Section header", "Sub-step A", "Another section"]
        );
    }

    #[test]
    fn flatten_subtree_content_skips_empty_content() {
        let mut node_map = HashMap::new();
        node_map.insert("c1".to_string(), make_node("c1", ""));
        node_map.insert("c2".to_string(), make_node("c2", "Has content"));
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        adjacency_list.insert(
            "skill-root".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        let parts = flatten_subtree_content("skill-root", &node_map, &adjacency_list);
        assert_eq!(parts, vec!["Has content"]);
    }

    #[test]
    fn skill_filter_matches_only_skill_nodes() {
        // Validates the post-filter predicate used in find_skills. The hybrid
        // BM25+KNN routing (semantic_search_nodes) requires a live DB and is
        // not unit-testable here; integration coverage lives in embedding_service
        // tests. This test only validates that the SearchNodeFilters predicate
        // correctly restricts results to skill-typed nodes.
        use crate::services::SearchNodeFilters;
        let filter = SearchNodeFilters {
            node_types: Some(vec!["skill".to_string()]),
            property_filters: None,
        };
        let empty_props = serde_json::json!({});
        assert!(filter.matches("skill", &empty_props));
        assert!(!filter.matches("text", &empty_props));
        assert!(!filter.matches("schema", &empty_props));
        assert!(!filter.matches("ai-chat", &empty_props));
    }

    #[test]
    fn flatten_subtree_content_join_produces_instructions_string() {
        // Verify the full flatten_subtree_content → join pipeline that produces the
        // `instructions` value delivered to the model. A regression in the flatten
        // logic (e.g. wrong separator, missing DFS step) breaks this test.
        let mut node_map = HashMap::new();
        node_map.insert("c1".to_string(), make_node("c1", "Step one"));
        node_map.insert("c2".to_string(), make_node("c2", "Step two"));
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        adjacency_list.insert(
            "skill-root".to_string(),
            vec!["c1".to_string(), "c2".to_string()],
        );
        let instructions =
            flatten_subtree_content("skill-root", &node_map, &adjacency_list).join("\n\n");
        assert_eq!(instructions, "Step one\n\nStep two");
    }
}
