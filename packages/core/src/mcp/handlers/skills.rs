//! MCP handler for skill discovery.
//!
//! Exposes `find_skills` as an MCP tool for external agents (via ACP) to
//! discover available skills in the NodeSpace knowledge graph.
//!
//! Issue #1051 (original); issue #1130 flattened the response shape so the
//! caller (model or external agent) sees raw confidence scores and judges
//! relevance itself. An empty `matches` array is a meaningful signal —
//! never wrap it in canned guidance.
//!
//! ADR-030 Phase 4.

use crate::mcp::MCPError;
use crate::ops::skill_ops;
use crate::services::{NodeEmbeddingService, NodeService};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct FindSkillsParams {
    query: String,
    limit: Option<usize>,
}

/// Handle find_skills tool call via shared skill_ops layer.
///
/// Returns `{query, matches}` — same shape as the local agent's
/// `search_skills` tool — so both consumers see identical payloads from the
/// shared `skill_ops::find_skills` backend.
pub async fn handle_find_skills(
    _node_service: &Arc<NodeService>,
    embedding_service: &Arc<NodeEmbeddingService>,
    arguments: Value,
) -> Result<Value, MCPError> {
    let params: FindSkillsParams = serde_json::from_value(arguments)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    let output = skill_ops::find_skills(
        embedding_service,
        skill_ops::FindSkillsInput {
            query: params.query,
            limit: params.limit,
        },
    )
    .await
    .map_err(|e| MCPError::internal_error(e.to_string()))?;

    Ok(json!({
        "query": output.query,
        "matches": output.skills,
    }))
}
