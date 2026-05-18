//! Skill discovery operations.
//!
//! Shared logic for skill search used by the local agent's `search_skills`
//! tool and the MCP `find_skills` handler exposed to external agents.
//!
//! Uses `semantic_search_nodes_of_type` so skill lookup runs a linear cosine
//! scan against the small skill embedding set instead of going through HNSW
//! + post-filter — faster *and* exact when the candidate set is small.
//!
//! Issues #1051, #1130.

use crate::services::NodeEmbeddingService;
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

/// Search for skill nodes via semantic search and return flat results.
///
/// Returns up to `limit` matches (default 3) with `id`, `name`, `description`,
/// `confidence`, and `tools`. No filtering or bucketing — the caller (model
/// or MCP client) inspects the raw confidence score and decides how to act.
/// An empty `skills` array is a meaningful signal: "no skill is even loosely
/// related to this query."
pub async fn find_skills(
    embedding_service: &Arc<NodeEmbeddingService>,
    input: FindSkillsInput,
) -> Result<FindSkillsOutput, OpsError> {
    let limit = input.limit.unwrap_or(3).min(MAX_SKILL_LIMIT);

    let skill_results = embedding_service
        .semantic_search_nodes_of_type(&input.query, "skill", limit, SKILL_SEARCH_THRESHOLD)
        .await
        .map_err(|e| OpsError::Internal(format!("Skill search failed: {}", e)))?;

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

        skills.push(json!({
            "id": node.id,
            "name": node.content,
            "description": description,
            "confidence": confidence,
            "tools": tool_whitelist,
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
