//! Skill discovery operations.
//!
//! Shared logic for skill search used by the local agent's `search_skills`
//! tool and the MCP `find_skills` handler exposed to external agents.
//!
//! Uses `semantic_search_nodes_of_type` so skill lookup runs a linear cosine
//! scan against the small skill embedding set instead of going through HNSW
//! + post-filter — faster *and* exact when the candidate set is small.
//!
//! Issues #1051, #1130, #1283.

use crate::services::{NodeEmbeddingService, NodeService};
use serde_json::{json, Value};
use std::sync::Arc;

use super::OpsError;

/// Similarity threshold for skill search.
///
/// Set to zero so the model sees every match with strictly positive cosine
/// similarity, including weak ones, and decides for itself which (if any)
/// skill is relevant. The underlying store filter is `composite_score >
/// $threshold`, so a zero match (orthogonal vector) is still excluded — but
/// that's the cosine noise floor, not a confidence judgment call. Issue
/// #1130 explicitly removed server-side bucketing in favour of letting the
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

/// Search for skill nodes via semantic search and return flat results with
/// schema metadata for the matched skill's scoped types.
///
/// Returns up to `limit` matches (default 3) with `id`, `name`, `description`,
/// `confidence`, `tools`, and `schema_metadata`. The `schema_metadata` field
/// contains type IDs, field names, and enum values for entity types associated
/// with the skill's `tool_whitelist` scope — giving the model exactly the schema
/// context it needs for the current intent without injecting the full type list
/// every turn (#1283).
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

    let skill_results = embedding_service
        .semantic_search_nodes_of_type(&input.query, "skill", limit, SKILL_SEARCH_THRESHOLD)
        .await
        .map_err(|e| OpsError::Internal(format!("Skill search failed: {}", e)))?;

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

        skills.push(json!({
            "id": node.id,
            "name": node.content,
            "description": description,
            "confidence": confidence,
            "tools": tool_whitelist,
            "schema_metadata": schema_metadata,
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
