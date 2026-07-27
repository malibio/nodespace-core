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

    for (node, confidence) in &skill_results {
        let description = node
            .properties
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tool_whitelist = node
            .properties
            .get("tool_whitelist")
            .cloned()
            .unwrap_or(json!([]));

        // Attach schema metadata for entity types relevant to this skill.
        // The skill's `node_types` property lists the type IDs in scope.
        // When absent, fall back to all custom (non-core) schemas, capped at
        // MAX_UNSCOPED_SCHEMA_METADATA to bound token cost for general-purpose
        // skills that haven't declared an explicit scope. Skills should set
        // `node_types` to avoid this fallback as workspaces grow.
        let scoped_type_ids: Vec<String> = node
            .properties
            .get("node_types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let schema_metadata: Vec<Value> = all_schemas
            .iter()
            .filter(|s| {
                if scoped_type_ids.is_empty() {
                    !s.is_core
                } else {
                    scoped_type_ids.contains(&s.id)
                }
            })
            .take(if scoped_type_ids.is_empty() {
                // Unscoped: cap to avoid returning all schemas for general-purpose skills.
                MAX_UNSCOPED_SCHEMA_METADATA
            } else {
                // Scoped: return all explicitly requested type IDs.
                scoped_type_ids.len()
            })
            .map(|s| {
                let fields: Vec<Value> = s
                    .fields
                    .iter()
                    .map(|f| {
                        let mut field = json!({
                            "name": f.name,
                            "type": f.field_type,
                        });
                        // Include enum values so the model can use exact values in tool calls.
                        if f.field_type == "enum" {
                            let mut vals: Vec<String> = Vec::new();
                            if let Some(core_vals) = &f.core_values {
                                vals.extend(core_vals.iter().map(|v| v.value.clone()));
                            }
                            if let Some(user_vals) = &f.user_values {
                                vals.extend(user_vals.iter().map(|v| v.value.clone()));
                            }
                            if !vals.is_empty() {
                                field["enum_values"] = json!(vals);
                            }
                        }
                        field
                    })
                    .collect();

                let mut entry = json!({
                    "type_id": s.id,
                    "name": s.content,
                    "fields": fields,
                });
                if let Some(tmpl) = &s.title_template {
                    entry["title_template"] = json!(tmpl);
                }
                entry
            })
            .collect();

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
    use crate::models::Node;
    use serde_json::json;
    use std::collections::HashMap;

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
