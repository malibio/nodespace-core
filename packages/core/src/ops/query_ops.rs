//! Execute-query ops wrapper.
//!
//! Bridges agent tool calls to `QueryService`. The agent passes a
//! flat-property filter shape; this module maps it to `QueryDefinition`
//! and delegates to `QueryService::execute`.

use crate::models::Node;
use crate::ops::OpsError;
use crate::services::node_service::NodeService;
use crate::services::query_service::{
    FilterOperator, FilterType, QueryDefinition, QueryFilter, QueryService, SortConfig,
    SortDirection,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

// ============================================================================
// Agent-facing filter shape
// ============================================================================

/// A single filter item as passed by the agent tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFilterItem {
    /// Filter category: "property", "content", "relationship", "metadata".
    #[serde(rename = "type")]
    pub filter_type: String,
    /// Comparison operator: "equals", "contains", "gt", "lt", "gte", "lte",
    /// "in", "exists".
    pub operator: String,
    /// Property key (for property and metadata filters).
    #[serde(default)]
    pub property: Option<String>,
    /// Value to compare against.
    #[serde(default)]
    pub value: Option<Value>,
    /// Case sensitivity for text comparisons (default: true).
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// Relationship type for relationship filters.
    #[serde(default)]
    pub relationship_type: Option<String>,
    /// Target node ID for relationship filters.
    #[serde(default)]
    pub node_id: Option<String>,
}

/// A single sort config item as passed by the agent tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSortItem {
    pub field: String,
    #[serde(default)]
    pub direction: Option<String>,
}

// ============================================================================
// Input / Output
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ExecuteQueryInput {
    /// Target node type ("task", "text", etc.) or "*" for all types.
    pub target_type: String,
    /// List of filter conditions.
    #[serde(default)]
    pub filters: Vec<AgentFilterItem>,
    /// Optional sorting.
    #[serde(default)]
    pub sorting: Option<Vec<AgentSortItem>>,
    /// Max results to return (default: 50).
    #[serde(default)]
    pub limit: Option<usize>,
}

pub type ExecuteQueryOutput = crate::ops::node_ops::QueryNodesOutput;

// ============================================================================
// Conversion helpers
// ============================================================================

fn parse_filter_type(s: &str) -> Result<FilterType, OpsError> {
    match s {
        "property" => Ok(FilterType::Property),
        "content" => Ok(FilterType::Content),
        "relationship" => Ok(FilterType::Relationship),
        "metadata" => Ok(FilterType::Metadata),
        other => Err(OpsError::InvalidParams(format!(
            "Unknown filter type '{}'. Supported: property, content, relationship, metadata",
            other
        ))),
    }
}

fn parse_filter_operator(s: &str) -> Result<FilterOperator, OpsError> {
    match s {
        "equals" => Ok(FilterOperator::Equals),
        "contains" => Ok(FilterOperator::Contains),
        "gt" => Ok(FilterOperator::GreaterThan),
        "lt" => Ok(FilterOperator::LessThan),
        "gte" => Ok(FilterOperator::GreaterThanOrEqual),
        "lte" => Ok(FilterOperator::LessThanOrEqual),
        "in" => Ok(FilterOperator::In),
        "exists" => Ok(FilterOperator::Exists),
        other => Err(OpsError::InvalidParams(format!(
            "Unknown operator '{}'. Supported: equals, contains, gt, lt, gte, lte, in, exists",
            other
        ))),
    }
}

fn parse_sort_direction(s: &str) -> SortDirection {
    match s {
        "desc" => SortDirection::Descending,
        _ => SortDirection::Ascending,
    }
}

fn parse_relationship_type(
    s: &str,
) -> Result<crate::services::query_service::RelationshipType, OpsError> {
    use crate::services::query_service::RelationshipType;
    match s {
        "parent" => Ok(RelationshipType::Parent),
        "children" => Ok(RelationshipType::Children),
        "mentions" => Ok(RelationshipType::Mentions),
        "mentioned_by" => Ok(RelationshipType::MentionedBy),
        other => Err(OpsError::InvalidParams(format!(
            "Unknown relationship type '{}'. Supported: parent, children, mentions, mentioned_by",
            other
        ))),
    }
}

fn to_query_filter(item: AgentFilterItem) -> Result<QueryFilter, OpsError> {
    let filter_type = parse_filter_type(&item.filter_type)?;
    let operator = parse_filter_operator(&item.operator)?;

    let relationship_type = match &item.relationship_type {
        Some(rt) => Some(parse_relationship_type(rt)?),
        None => None,
    };

    Ok(QueryFilter {
        filter_type,
        operator,
        property: item.property,
        value: item.value,
        case_sensitive: item.case_sensitive,
        relationship_type,
        node_id: item.node_id,
    })
}

fn nodes_to_typed_values(nodes: Vec<Node>) -> Result<Vec<Value>, OpsError> {
    crate::models::nodes_to_typed_values(nodes).map_err(OpsError::Internal)
}

// ============================================================================
// Identifier validation
// ============================================================================

/// Validate that an identifier (node type, property name, sort field) only
/// contains characters that are safe to interpolate into a SQL identifier
/// position. The allowlist is `[A-Za-z0-9_:-]` which covers all real node
/// types, property keys, and metadata fields while blocking injection vectors.
fn validate_identifier(value: &str, label: &str) -> Result<(), OpsError> {
    if value == "*" {
        return Ok(());
    }
    if value.is_empty() {
        return Err(OpsError::InvalidParams(format!(
            "{} must not be empty",
            label
        )));
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
    {
        Ok(())
    } else {
        Err(OpsError::InvalidParams(format!(
            "{} '{}' contains invalid characters; only [A-Za-z0-9_:-] are allowed",
            label, value
        )))
    }
}

// ============================================================================
// Operation
// ============================================================================

/// Execute a structured property query via `QueryService`, returning raw
/// domain `Node`s.
///
/// Converts the agent's flat filter shape into a `QueryDefinition` and
/// delegates to `QueryService::execute`, which generates proper SQL
/// `json_extract` conditions against SQLite. Shared by `execute_query`
/// (agent tool call, typed-JSON output) and the gRPC `ExecuteQuery` handler
/// (proto `NodeData` output) so validation/mapping isn't duplicated.
pub async fn execute_query_nodes(
    node_service: &Arc<NodeService>,
    input: ExecuteQueryInput,
) -> Result<Vec<Node>, OpsError> {
    validate_identifier(&input.target_type, "target_type")?;

    for item in &input.filters {
        if let Some(prop) = &item.property {
            validate_identifier(prop, "filter property")?;
        }
    }

    if let Some(sorting) = &input.sorting {
        for sort in sorting {
            validate_identifier(&sort.field, "sort field")?;
        }
    }

    let limit = input.limit.unwrap_or(50);

    let filters: Vec<QueryFilter> = input
        .filters
        .into_iter()
        .map(to_query_filter)
        .collect::<Result<_, _>>()?;

    let sorting: Option<Vec<SortConfig>> = input.sorting.map(|items| {
        items
            .into_iter()
            .map(|s| SortConfig {
                field: s.field,
                direction: parse_sort_direction(s.direction.as_deref().unwrap_or("asc")),
            })
            .collect()
    });

    let query = QueryDefinition {
        target_type: input.target_type,
        filters,
        sorting,
        limit: Some(limit),
    };

    let query_service = QueryService::new(node_service.store().clone());
    query_service
        .execute(&query)
        .await
        .map_err(|e| OpsError::Internal(format!("execute_query failed: {}", e)))
}

/// Execute a structured property query, returning typed JSON values.
///
/// Thin wrapper over [`execute_query_nodes`] for callers (agent tool call)
/// that want the typed-value shape rather than raw `Node`s.
pub async fn execute_query(
    node_service: &Arc<NodeService>,
    input: ExecuteQueryInput,
) -> Result<ExecuteQueryOutput, OpsError> {
    let nodes = execute_query_nodes(node_service, input).await?;
    let count = nodes.len();
    let typed_nodes = nodes_to_typed_values(nodes)?;

    Ok(ExecuteQueryOutput {
        nodes: typed_nodes,
        count,
        collection_id: None,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_all_operators() {
        for (s, expected) in [
            ("equals", FilterOperator::Equals),
            ("contains", FilterOperator::Contains),
            ("gt", FilterOperator::GreaterThan),
            ("lt", FilterOperator::LessThan),
            ("gte", FilterOperator::GreaterThanOrEqual),
            ("lte", FilterOperator::LessThanOrEqual),
            ("in", FilterOperator::In),
            ("exists", FilterOperator::Exists),
        ] {
            let parsed = parse_filter_operator(s).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn parse_unknown_operator_is_error() {
        let err = parse_filter_operator("like").unwrap_err();
        assert!(matches!(err, OpsError::InvalidParams(_)));
    }

    #[test]
    fn parse_all_filter_types() {
        for s in ["property", "content", "relationship", "metadata"] {
            assert!(parse_filter_type(s).is_ok());
        }
        assert!(parse_filter_type("unknown").is_err());
    }

    #[test]
    fn to_query_filter_property() {
        let item = AgentFilterItem {
            filter_type: "property".to_string(),
            operator: "equals".to_string(),
            property: Some("status".to_string()),
            value: Some(json!("open")),
            case_sensitive: None,
            relationship_type: None,
            node_id: None,
        };
        let qf = to_query_filter(item).unwrap();
        assert_eq!(qf.filter_type, FilterType::Property);
        assert_eq!(qf.operator, FilterOperator::Equals);
        assert_eq!(qf.property.as_deref(), Some("status"));
        assert_eq!(qf.value, Some(json!("open")));
    }

    #[test]
    fn execute_query_input_deserializes_minimal() {
        let v = json!({"target_type": "task"});
        let input: ExecuteQueryInput = serde_json::from_value(v).unwrap();
        assert_eq!(input.target_type, "task");
        assert!(input.filters.is_empty());
        assert!(input.sorting.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn execute_query_input_deserializes_full() {
        let v = json!({
            "target_type": "task",
            "filters": [
                {"type": "property", "operator": "equals", "property": "status", "value": "open"}
            ],
            "sorting": [{"field": "due_date", "direction": "asc"}],
            "limit": 25
        });
        let input: ExecuteQueryInput = serde_json::from_value(v).unwrap();
        assert_eq!(input.filters.len(), 1);
        assert_eq!(input.sorting.as_ref().unwrap().len(), 1);
        assert_eq!(input.limit, Some(25));
    }

    // -- Unknown-field rejection (acceptance criterion, #1816) --

    #[test]
    fn agent_filter_item_rejects_unknown_field() {
        let args = json!({
            "type": "property",
            "operator": "equals",
            "property": "status",
            "caseSensitive": false
        });
        let err = serde_json::from_value::<AgentFilterItem>(args).unwrap_err();
        assert!(
            err.to_string().contains("caseSensitive"),
            "expected error naming `caseSensitive`, got: {err}"
        );
    }

    #[test]
    fn agent_sort_item_rejects_unknown_field() {
        let args = json!({ "field": "due_date", "direction": "asc", "order": "asc" });
        let err = serde_json::from_value::<AgentSortItem>(args).unwrap_err();
        assert!(
            err.to_string().contains("order"),
            "expected error naming `order`, got: {err}"
        );
    }

    #[test]
    fn validate_identifier_accepts_valid() {
        assert!(validate_identifier("task", "t").is_ok());
        assert!(validate_identifier("due_date", "t").is_ok());
        assert!(validate_identifier("custom:field", "t").is_ok());
        assert!(validate_identifier("my-type", "t").is_ok());
        assert!(validate_identifier("*", "t").is_ok());
    }

    #[test]
    fn validate_identifier_rejects_injection() {
        assert!(validate_identifier("'; DROP TABLE node; --", "t").is_err());
        assert!(validate_identifier("a b", "t").is_err());
        assert!(validate_identifier("a.b", "t").is_err());
        assert!(validate_identifier("", "t").is_err());
    }

    mod integration {
        use super::*;
        use crate::db::SqliteStore;
        use crate::models::Node;
        use crate::services::NodeService;
        use tempfile::TempDir;

        async fn make_test_service() -> (Arc<NodeService>, TempDir) {
            let tmp = TempDir::new().unwrap();
            let db_path = tmp.path().join("test.db");
            let mut store: Arc<SqliteStore> = Arc::new(SqliteStore::new(db_path).await.unwrap());
            let svc = Arc::new(NodeService::new(&mut store).await.unwrap());
            (svc, tmp)
        }

        fn task_node(id: &str, status: &str, due_date: Option<&str>) -> Node {
            let mut props = json!({"status": status});
            if let Some(d) = due_date {
                props["due_date"] = json!(d);
            }
            Node {
                id: id.to_string(),
                node_type: "task".to_string(),
                content: format!("Task {}", id),
                version: 1,
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                properties: props,
                mentions: vec![],
                mentioned_in: vec![],
                title: Some(format!("Task {}", id)),
                lifecycle_status: "active".to_string(),
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn execute_query_filters_by_status() {
            let (svc, _tmp) = make_test_service().await;

            svc.create_node(task_node("t1", "open", None))
                .await
                .unwrap();
            svc.create_node(task_node("t2", "done", None))
                .await
                .unwrap();
            svc.create_node(task_node("t3", "open", None))
                .await
                .unwrap();

            let input: ExecuteQueryInput = serde_json::from_value(json!({
                "target_type": "task",
                "filters": [
                    {"type": "property", "operator": "equals", "property": "status", "value": "open"}
                ]
            }))
            .unwrap();

            let output = execute_query(&svc, input).await.unwrap();
            assert_eq!(
                output.count, 2,
                "expected 2 open tasks, got {}",
                output.count
            );
            for node in &output.nodes {
                assert_eq!(node.get("status").and_then(|v| v.as_str()), Some("open"));
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn execute_query_rejects_invalid_identifier() {
            let (svc, _tmp) = make_test_service().await;

            let input: ExecuteQueryInput = serde_json::from_value(json!({
                "target_type": "task'; DROP TABLE node; --",
                "filters": []
            }))
            .unwrap();

            let err = execute_query(&svc, input).await.unwrap_err();
            assert!(matches!(err, OpsError::InvalidParams(_)));
        }
    }
}
