//! Shared parameter types for agent tool execution.
//!
//! These structs deserialize the JSON arguments the local agent passes to its
//! search tools. They previously lived under `mcp::params` (re-exported from the
//! MCP search handler); they moved here when the MCP transport was deleted, since
//! the agent tool executor is now their only consumer.
//!
//! Every struct here (and its counterparts across the agent/core crates) carries
//! `#[serde(deny_unknown_fields)]`: a misspelled tool-call argument key must be
//! rejected immediately, not silently dropped and surfaced two layers downstream
//! as an unrelated error (see the `coreValues` incident this generalizes from).
//! Serde's stock rejection message already names both the offending key and the
//! accepted field names (`unknown field \`x\`, expected one of \`a\`, \`b\`, ...`)
//! — no custom error formatting is needed to satisfy that requirement.

use crate::ops::query_ops::{AgentFilterItem, AgentSortItem};
use serde::{Deserialize, Deserializer};

/// Deserialize `null` as the type's default rather than as a type error.
///
/// Paired with `#[serde(default)]`, this makes an explicitly-null field behave
/// exactly like an omitted one. Used only for fields where the two genuinely
/// mean the same thing, so that a caller stating "nothing here" explicitly is
/// not rejected for choosing the more verbose spelling.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Parameters for the `search_nodes` tool — the single query tool for finding,
/// listing, and filtering nodes by title, type, and/or typed properties.
///
/// Plain title/type listing (no `filters`) runs through the title-index path;
/// any `filters`/`sorting` route through `QueryService` for SQL property queries.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchNodesParams {
    /// Keyword or phrase to search for in node titles. Pass an empty string or
    /// `"*"` to skip the title filter (useful when filtering only by
    /// node_type) — both are recognized as "enumerate everything", not a
    /// literal search term (see `search_ops::normalize_enumerate_query`).
    /// Defaults to empty when omitted entirely — a caller that resolved
    /// everything into `filters` (e.g. via `resolve_query`) should not have
    /// to remember to also echo back an empty `query`.
    ///
    /// An explicit `null` is read as that same empty string rather than
    /// rejected. A model that has resolved its intent into `filters` states "no
    /// title filter" as `"query": null` at least as readily as by omitting the
    /// key — measured 3 of 3 on the locked model — and the two plainly mean the
    /// same thing here, so rejecting one of them fails a correct call over its
    /// spelling. `deny_unknown_fields` still rejects a *misspelled* key, which
    /// is the case that guard exists for; this only accepts a null where an
    /// absent value is already valid.
    #[serde(default, deserialize_with = "null_as_default")]
    pub query: String,

    /// Filter by node type (e.g., "task", "text").
    #[serde(default)]
    pub node_type: Option<String>,

    /// Optional typed-property filters (status, due_date, amount, custom fields)
    /// with operators. When present, the query routes through `QueryService`.
    #[serde(default)]
    pub filters: Vec<AgentFilterItem>,

    /// Optional sort configuration, applied in order.
    #[serde(default)]
    pub sorting: Option<Vec<AgentSortItem>>,

    /// Maximum number of results. Default: 50.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Parameters for the `search_semantic` tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSemanticParams {
    /// Natural language search query.
    pub query: String,

    /// Minimum similarity threshold (0.0-1.0, higher = stricter filter).
    /// Results must have similarity > threshold to be included. Default: 0.7.
    #[serde(default)]
    pub threshold: Option<f32>,

    /// Maximum number of results. Default: 20.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Filter by collection ID - returns only results from this collection.
    #[serde(default)]
    pub collection_id: Option<String>,

    /// Filter by collection path (e.g., "hr:policy") - resolves path to collection ID.
    #[serde(default)]
    pub collection: Option<String>,

    /// Exclude results from these collections (by path, e.g., ["archived", "drafts"]).
    /// Results in any of these collections will be filtered out.
    #[serde(default)]
    pub exclude_collections: Option<Vec<String>>,

    /// Number of top results to include full markdown content for (0-5).
    /// This saves AI agents from needing to call get_markdown_from_node_id separately.
    /// Default: 1 (include markdown for top result only). Set to 0 to disable, max 5.
    #[serde(default)]
    pub include_markdown: Option<usize>,

    /// Include archived nodes in search results (default: false).
    /// By default, search only returns active nodes.
    #[serde(default)]
    pub include_archived: Option<bool>,

    /// Search scope - controls which node types are included.
    /// Values: "knowledge" (default), "conversations", "everything".
    #[serde(default)]
    pub scope: Option<String>,

    /// Filter by specific node types (e.g., ["task", "text"]).
    /// If set, only nodes whose node_type is in this list will be included.
    #[serde(default)]
    pub node_types: Option<Vec<String>>,

    /// Filter by node properties (key-value pairs).
    /// If set, only nodes whose properties contain all specified key-value pairs
    /// will be included. Multiple filters are combined with AND logic.
    #[serde(default)]
    pub property_filters: Option<serde_json::Value>,

    /// When true, attach outgoing relationships of each result node as an "edges" array.
    /// Each edge entry has: {"relationship": "...", "target_id": "...", "target_title": "..."}.
    /// Default: false (no edge data included).
    #[serde(default)]
    pub include_edges: Option<bool>,

    /// When true, re-rank results by blending vector similarity with graph connectivity degree.
    /// Blending formula: combined_score = 0.7 * similarity + 0.3 * normalized_degree.
    /// Surfaces well-connected, central knowledge nodes over isolated but textually similar ones.
    /// Default: false (pure similarity ranking).
    #[serde(default)]
    pub graph_boost: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `AgentFilterItem` has its own direct rejection test in `query_ops.rs`;
    /// this confirms the same guard fires when it arrives nested inside a real
    /// `search_nodes` payload, not just when deserialized standalone.
    #[test]
    fn search_nodes_params_rejects_unknown_field_in_nested_filter() {
        let args = json!({
            "query": "",
            "filters": [
                { "type": "property", "operator": "equals", "property": "status", "caseSensitive": false }
            ]
        });
        let err = serde_json::from_value::<SearchNodesParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("caseSensitive"),
            "expected error naming `caseSensitive`, got: {err}"
        );
    }

    /// A model that resolved its intent into `filters` states "no title filter"
    /// as an explicit `null` as readily as by omitting the key. Both mean the
    /// same thing, so both must deserialize — rejecting the null spelling fails
    /// an otherwise correct call.
    #[test]
    fn search_nodes_params_reads_null_query_as_empty() {
        let explicit_null = json!({
            "query": null,
            "node_type": "task",
            "filters": [{"type": "property", "operator": "equals", "property": "status", "value": "open"}]
        });
        let params = serde_json::from_value::<SearchNodesParams>(explicit_null)
            .expect("an explicit null query must deserialize");
        assert_eq!(params.query, "");

        // And the two spellings must agree.
        let omitted = json!({"node_type": "task"});
        let from_omitted = serde_json::from_value::<SearchNodesParams>(omitted).unwrap();
        let from_null = serde_json::from_value::<SearchNodesParams>(
            json!({"node_type": "task", "query": null}),
        )
        .unwrap();
        assert_eq!(from_omitted.query, from_null.query);
    }

    /// The null allowance must not weaken the misspelled-key guard it sits
    /// beside — that is the case `deny_unknown_fields` exists for.
    #[test]
    fn null_query_allowance_does_not_admit_unknown_keys() {
        let args = json!({"query": null, "nodeType": "task"});
        let err = serde_json::from_value::<SearchNodesParams>(args).unwrap_err();
        assert!(
            err.to_string().contains("nodeType"),
            "expected error naming `nodeType`, got: {err}"
        );
    }
}
