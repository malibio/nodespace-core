//! Node CRUD operation commands for Text, Task, and Date nodes
//!
//! As of Issue #1113, all commands proxy through the in-process gRPC server
//! (nodespace-daemon) instead of calling `packages/core` directly.

use crate::types::{
    node_to_typed_value as types_node_to_typed_value,
    nodes_to_typed_values as types_nodes_to_typed_values, DeleteResult, Node, NodeQuery,
    NodeReference, NodeUpdate, TaskNodeUpdate,
};
use chrono::{DateTime, Utc};
use nodespace_proto::nodespace::{
    CreateMentionRequest, CreateNodeRequest, DeleteMentionRequest, DeleteNodeRequest,
    GetChildrenRequest, GetChildrenTreeRequest, GetNodeRequest, GetSchemaDefinitionRequest,
    MentionAutocompleteRequest, MentionTargetRequest, MoveNodeRequest, NodeData, NodeResponse,
    OptionalStringClear, OptionalTimestampClear, QueryNodesSimpleRequest, ReorderNodeRequest,
    UpdateNodeRequest, UpdateTaskNodeRequest, UpsertNodeWithParentRequest,
};
use nodespace_proto::NodeServiceClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use tonic::transport::Channel;
use tonic::Request;

use crate::services::GrpcClient;

/// Input for creating a node - timestamps generated server-side
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeInput {
    pub id: String,
    pub node_type: String,
    pub content: String,
    pub parent_id: Option<String>,
    // root_id removed - backend auto-derives root from parent chain (Issue #533)
    /// Sibling node ID to insert after (None = insert at beginning of siblings)
    /// Used for correct ordering when creating child nodes via Enter key
    #[serde(default)]
    pub insert_after_node_id: Option<String>,
    pub properties: serde_json::Value,
    // embedding_vector dropped - not in proto (Issue #1113)
}

/// Structured error type for Tauri commands
///
/// Provides better observability and debugging by including error codes
/// and optional details alongside user-facing messages.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// User-facing error message
    pub message: String,
    /// Machine-readable error code
    pub code: String,
    /// Optional detailed error information for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

fn status_to_command_error(status: tonic::Status) -> CommandError {
    let code = match status.code() {
        tonic::Code::NotFound => "NODE_NOT_FOUND",
        tonic::Code::Aborted => "VERSION_CONFLICT",
        tonic::Code::AlreadyExists => "COLLECTION_EXISTS",
        tonic::Code::InvalidArgument => "INVALID_ARGUMENT",
        _ => "GRPC_ERROR",
    }
    .to_string();
    CommandError {
        message: status.message().to_string(),
        code,
        details: Some(format!("{:?}", status.code())),
    }
}

/// Convert proto NodeData → core Node
pub(crate) fn proto_node_data_to_node(nd: NodeData) -> Result<Node, CommandError> {
    let properties = serde_json::from_str::<serde_json::Value>(&nd.properties)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let created_at = DateTime::parse_from_rfc3339(&nd.created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CommandError {
            message: format!("Invalid created_at timestamp: {}", e),
            code: "PARSE_ERROR".to_string(),
            details: Some(nd.created_at.clone()),
        })?;
    let modified_at = DateTime::parse_from_rfc3339(&nd.modified_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CommandError {
            message: format!("Invalid modified_at timestamp: {}", e),
            code: "PARSE_ERROR".to_string(),
            details: Some(nd.modified_at.clone()),
        })?;

    Ok(Node {
        id: nd.id,
        node_type: nd.node_type,
        content: nd.content,
        version: nd.version,
        created_at,
        modified_at,
        properties,
        lifecycle_status: nd.lifecycle_status,
        mentions: vec![],
        mentioned_in: vec![],
        title: None,
    })
}

/// Convert proto NodeResponse → core Node
fn proto_node_response_to_node(resp: NodeResponse) -> Result<Node, CommandError> {
    let nd = resp.node_data.ok_or_else(|| CommandError {
        message: "gRPC response missing node_data".to_string(),
        code: "GRPC_ERROR".to_string(),
        details: None,
    })?;
    proto_node_data_to_node(nd)
}

/// Validate that node type has a schema via gRPC GetSchemaDefinition RPC
async fn validate_node_type(
    node_type: &str,
    client: &mut NodeServiceClient<Channel>,
) -> Result<(), CommandError> {
    match client
        .get_schema_definition(Request::new(GetSchemaDefinitionRequest {
            schema_id: node_type.to_string(),
        }))
        .await
    {
        Ok(_) => Ok(()),
        Err(s)
            if s.code() == tonic::Code::NotFound || s.code() == tonic::Code::FailedPrecondition =>
        {
            Err(CommandError {
                message: format!("No schema found for node type: {}", node_type),
                code: "SCHEMA_NOT_FOUND".to_string(),
                details: None,
            })
        }
        Err(s) => Err(status_to_command_error(s)),
    }
}

/// Convert a Node to its strongly-typed JSON representation (Issue #673)
pub fn node_to_typed_value(node: Node) -> Result<Value, CommandError> {
    types_node_to_typed_value(node).map_err(|e| CommandError {
        message: e.clone(),
        code: "CONVERSION_ERROR".to_string(),
        details: Some(e),
    })
}

/// Convert a list of Nodes to their strongly-typed JSON representations (Issue #673)
pub fn nodes_to_typed_values(nodes: Vec<Node>) -> Result<Vec<Value>, CommandError> {
    types_nodes_to_typed_values(nodes).map_err(|e| CommandError {
        message: e.clone(),
        code: "CONVERSION_ERROR".to_string(),
        details: Some(e),
    })
}

/// Input for creating a root node (top-level container)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRootNodeInput {
    pub content: String,
    pub node_type: String,
    #[serde(default)]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub mentioned_by: Option<String>,
}

/// Input for saving a node with automatic parent creation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveNodeWithParentInput {
    pub node_id: String,
    pub content: String,
    pub node_type: String,
    pub parent_id: String,
    pub root_id: String,
    // before_sibling_id removed - backend uses fractional ordering on has_child edges (Issue #616)
}

/// Create a new node of any type with a registered schema
#[tauri::command]
pub async fn create_node(
    client: State<'_, GrpcClient>,
    node: CreateNodeInput,
) -> Result<String, CommandError> {
    let mut c = client.client().await;
    validate_node_type(&node.node_type, &mut c).await?;

    let properties_str = node.properties.to_string();
    let resp = c
        .create_node(Request::new(CreateNodeRequest {
            id: node.id,
            node_type: node.node_type,
            content: node.content,
            parent_id: node.parent_id.unwrap_or_default(),
            insert_after_node_id: node.insert_after_node_id.unwrap_or_default(),
            properties: properties_str,
            collection: String::new(),
            lifecycle_status: String::new(),
        }))
        .await
        .map_err(status_to_command_error)?;

    Ok(resp.into_inner().node_id)
}

/// Create a new root node (top-level node that can contain other nodes)
#[tauri::command]
pub async fn create_root_node(
    client: State<'_, GrpcClient>,
    input: CreateRootNodeInput,
) -> Result<String, CommandError> {
    let mut c = client.client().await;
    validate_node_type(&input.node_type, &mut c).await?;

    let properties_str = input.properties.to_string();
    let resp = c
        .create_node(Request::new(CreateNodeRequest {
            id: String::new(), // server generates ID for root nodes
            node_type: input.node_type,
            content: input.content,
            parent_id: String::new(), // no parent = root
            insert_after_node_id: String::new(),
            properties: properties_str,
            collection: String::new(),
            lifecycle_status: String::new(),
        }))
        .await
        .map_err(status_to_command_error)?;

    let node_id = resp.into_inner().node_id;

    // If mentioned_by is provided, create mention relationship
    if let Some(mentioning_node_id) = input.mentioned_by {
        c.create_mention(Request::new(CreateMentionRequest {
            mentioning_node_id,
            mentioned_node_id: node_id.clone(),
        }))
        .await
        .map_err(status_to_command_error)?;
    }

    Ok(node_id)
}

/// Create a mention relationship between two nodes
#[tauri::command]
pub async fn create_node_mention(
    client: State<'_, GrpcClient>,
    mentioning_node_id: String,
    mentioned_node_id: String,
) -> Result<(), CommandError> {
    let mut c = client.client().await;
    c.create_mention(Request::new(CreateMentionRequest {
        mentioning_node_id,
        mentioned_node_id,
    }))
    .await
    .map_err(status_to_command_error)?;
    Ok(())
}

/// Get a node by ID
#[tauri::command]
pub async fn get_node(
    client: State<'_, GrpcClient>,
    id: String,
) -> Result<Option<Value>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_node(Request::new(GetNodeRequest { node_id: id }))
        .await;

    match resp {
        Ok(r) => {
            let node = proto_node_response_to_node(r.into_inner())?;
            Ok(Some(node_to_typed_value(node)?))
        }
        Err(s) if s.code() == tonic::Code::NotFound => Ok(None),
        Err(s) => Err(status_to_command_error(s)),
    }
}

/// Update an existing node
#[tauri::command]
pub async fn update_node(
    client: State<'_, GrpcClient>,
    id: String,
    version: i64,
    update: NodeUpdate,
) -> Result<Value, CommandError> {
    let mut c = client.client().await;

    let content_preview = update.content.as_ref().map(|c| {
        if c.len() > 50 {
            format!("{}...", &c[..50])
        } else {
            c.clone()
        }
    });
    tracing::debug!(
        "update_node: id={}, version={}, content={:?}, node_type={:?}",
        id,
        version,
        content_preview,
        update.node_type
    );

    let req = UpdateNodeRequest {
        node_id: id.clone(),
        version: Some(version),
        node_type: update.node_type.unwrap_or_default(),
        content: update.content,
        properties: update.properties.map(|p| p.to_string()),
        add_to_collection: String::new(),
        remove_from_collection: String::new(),
        lifecycle_status: update.lifecycle_status.unwrap_or_default(),
    };

    let resp = c
        .update_node(Request::new(req))
        .await
        .map_err(status_to_command_error)?;

    let node = proto_node_response_to_node(resp.into_inner())?;

    tracing::debug!(
        "update_node: SUCCESS id={}, new_version={}",
        id,
        node.version
    );

    node_to_typed_value(node)
}

/// Delete a node by ID with cascade deletion
#[tauri::command]
pub async fn delete_node(
    client: State<'_, GrpcClient>,
    id: String,
    version: i64,
) -> Result<DeleteResult, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .delete_node(Request::new(DeleteNodeRequest {
            node_id: id,
            version: Some(version),
        }))
        .await
        .map_err(status_to_command_error)?;

    let dr = resp.into_inner();
    Ok(DeleteResult {
        existed: dr.existed,
    })
}

/// Atomically move a node to a new parent with new sibling position (with OCC)
#[tauri::command]
pub async fn move_node(
    client: State<'_, GrpcClient>,
    node_id: String,
    version: i64,
    new_parent_id: Option<String>,
    insert_after_node_id: Option<String>,
) -> Result<Value, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .move_node(Request::new(MoveNodeRequest {
            node_id,
            version,
            new_parent_id: new_parent_id.unwrap_or_default(),
            insert_after_node_id: insert_after_node_id.unwrap_or_default(),
        }))
        .await
        .map_err(status_to_command_error)?;

    let node = proto_node_response_to_node(resp.into_inner())?;
    node_to_typed_value(node)
}

/// Reorder a node by changing its sibling position
#[tauri::command]
pub async fn reorder_node(
    client: State<'_, GrpcClient>,
    node_id: String,
    version: i64,
    insert_after_node_id: Option<String>,
) -> Result<(), CommandError> {
    let mut c = client.client().await;
    c.reorder_node(Request::new(ReorderNodeRequest {
        node_id,
        version,
        insert_after_node_id: insert_after_node_id.unwrap_or_default(),
    }))
    .await
    .map_err(status_to_command_error)?;
    Ok(())
}

/// Get child nodes of a parent node
#[tauri::command]
pub async fn get_children(
    client: State<'_, GrpcClient>,
    parent_id: String,
) -> Result<Vec<Value>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_children(Request::new(GetChildrenRequest { node_id: parent_id }))
        .await
        .map_err(status_to_command_error)?;

    let nodes: Result<Vec<Node>, CommandError> = resp
        .into_inner()
        .nodes
        .into_iter()
        .map(proto_node_data_to_node)
        .collect();

    nodes_to_typed_values(nodes?)
}

/// Get a node with its entire subtree as a nested tree structure
#[tauri::command]
pub async fn get_children_tree(
    client: State<'_, GrpcClient>,
    parent_id: String,
) -> Result<serde_json::Value, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_children_tree(Request::new(GetChildrenTreeRequest { node_id: parent_id }))
        .await
        .map_err(status_to_command_error)?;

    let tree_json = resp.into_inner().tree_json;
    serde_json::from_str(&tree_json).map_err(|e| CommandError {
        message: format!("Failed to parse tree JSON: {}", e),
        code: "PARSE_ERROR".to_string(),
        details: Some(tree_json),
    })
}

/// Bulk fetch all nodes belonging to a root node (viewer/page)
#[tauri::command]
pub async fn get_nodes_by_root_id(
    client: State<'_, GrpcClient>,
    root_id: String,
) -> Result<Vec<Value>, CommandError> {
    let mut c = client.client().await;
    // Phase 5 (Issue #511): Redirect to get_children (graph-native)
    let resp = c
        .get_children(Request::new(GetChildrenRequest { node_id: root_id }))
        .await
        .map_err(status_to_command_error)?;

    let nodes: Result<Vec<Node>, CommandError> = resp
        .into_inner()
        .nodes
        .into_iter()
        .map(proto_node_data_to_node)
        .collect();

    nodes_to_typed_values(nodes?)
}

/// Query nodes with flexible filtering
#[tauri::command]
pub async fn query_nodes_simple(
    client: State<'_, GrpcClient>,
    query: NodeQuery,
) -> Result<Vec<Value>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .query_nodes_simple(Request::new(QueryNodesSimpleRequest {
            id: query.id,
            mentioned_by: query.mentioned_by,
            content_contains: query.content_contains,
            title_contains: query.title_contains,
            node_type: query.node_type,
            limit: query.limit.unwrap_or(0) as u32,
            offset: query.offset.unwrap_or(0) as u32,
        }))
        .await
        .map_err(status_to_command_error)?;

    let nodes: Result<Vec<Node>, CommandError> = resp
        .into_inner()
        .nodes
        .into_iter()
        .map(proto_node_data_to_node)
        .collect();

    nodes_to_typed_values(nodes?)
}

/// Mention autocomplete query - specialized endpoint for @mention feature
#[tauri::command]
pub async fn mention_autocomplete(
    client: State<'_, GrpcClient>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Value>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .mention_autocomplete(Request::new(MentionAutocompleteRequest {
            query,
            limit: limit.unwrap_or(0) as u32,
        }))
        .await
        .map_err(status_to_command_error)?;

    let nodes: Result<Vec<Node>, CommandError> = resp
        .into_inner()
        .nodes
        .into_iter()
        .map(proto_node_data_to_node)
        .collect();

    nodes_to_typed_values(nodes?)
}

/// Save a node with automatic parent creation - unified upsert operation
#[tauri::command]
pub async fn save_node_with_parent(
    client: State<'_, GrpcClient>,
    input: SaveNodeWithParentInput,
) -> Result<(), CommandError> {
    let mut c = client.client().await;
    validate_node_type(&input.node_type, &mut c).await?;

    c.upsert_node_with_parent(Request::new(UpsertNodeWithParentRequest {
        node_id: input.node_id,
        content: input.content,
        node_type: input.node_type,
        parent_id: input.parent_id,
        root_id: input.root_id,
    }))
    .await
    .map_err(status_to_command_error)?;

    Ok(())
}

/// Get outgoing mentions (nodes that this node mentions)
#[tauri::command]
pub async fn get_outgoing_mentions(
    client: State<'_, GrpcClient>,
    node_id: String,
) -> Result<Vec<String>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_outgoing_mentions(Request::new(MentionTargetRequest { node_id }))
        .await
        .map_err(status_to_command_error)?;

    Ok(resp.into_inner().node_ids)
}

/// Get incoming mentions (nodes that mention this node - BACKLINKS)
#[tauri::command]
pub async fn get_incoming_mentions(
    client: State<'_, GrpcClient>,
    node_id: String,
) -> Result<Vec<String>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_incoming_mentions(Request::new(MentionTargetRequest { node_id }))
        .await
        .map_err(status_to_command_error)?;

    Ok(resp.into_inner().node_ids)
}

/// Get root nodes of nodes that mention the target node (backlinks at root level)
#[tauri::command]
pub async fn get_mentioning_roots(
    client: State<'_, GrpcClient>,
    node_id: String,
) -> Result<Vec<NodeReference>, CommandError> {
    let mut c = client.client().await;
    let resp = c
        .get_mentioning_roots(Request::new(MentionTargetRequest { node_id }))
        .await
        .map_err(status_to_command_error)?;

    let references = resp
        .into_inner()
        .references
        .into_iter()
        .map(|r| NodeReference {
            id: r.id,
            title: r.title,
            node_type: r.node_type,
        })
        .collect();

    Ok(references)
}

/// Build UpdateTaskNodeRequest from TaskNodeUpdate
fn task_update_to_proto(id: &str, version: i64, update: TaskNodeUpdate) -> UpdateTaskNodeRequest {
    UpdateTaskNodeRequest {
        node_id: id.to_string(),
        version,
        status: update.status.map(|s| s.as_str().to_string()),
        priority: update.priority.map(|opt| match opt {
            None => OptionalStringClear {
                clear: true,
                value: String::new(),
            },
            Some(p) => OptionalStringClear {
                clear: false,
                value: p.as_str().to_string(),
            },
        }),
        due_date: update.due_date.map(|opt| match opt {
            None => OptionalTimestampClear {
                clear: true,
                value: String::new(),
            },
            Some(dt) => OptionalTimestampClear {
                clear: false,
                value: dt.to_rfc3339(),
            },
        }),
        assignee: update.assignee.map(|opt| match opt {
            None => OptionalStringClear {
                clear: true,
                value: String::new(),
            },
            Some(a) => OptionalStringClear {
                clear: false,
                value: a,
            },
        }),
        started_at: update.started_at.map(|opt| match opt {
            None => OptionalTimestampClear {
                clear: true,
                value: String::new(),
            },
            Some(dt) => OptionalTimestampClear {
                clear: false,
                value: dt.to_rfc3339(),
            },
        }),
        completed_at: update.completed_at.map(|opt| match opt {
            None => OptionalTimestampClear {
                clear: true,
                value: String::new(),
            },
            Some(dt) => OptionalTimestampClear {
                clear: false,
                value: dt.to_rfc3339(),
            },
        }),
        content: update.content,
        properties: None,
    }
}

/// Update a task node with type-safe property updates
#[tauri::command]
pub async fn update_task_node(
    client: State<'_, GrpcClient>,
    id: String,
    version: i64,
    update: TaskNodeUpdate,
) -> Result<Value, CommandError> {
    let mut c = client.client().await;
    let req = task_update_to_proto(&id, version, update);
    let resp = c
        .update_task_node(Request::new(req))
        .await
        .map_err(status_to_command_error)?;

    let node = proto_node_response_to_node(resp.into_inner())?;
    node_to_typed_value(node)
}

/// Delete a mention relationship between two nodes
#[tauri::command]
pub async fn delete_node_mention(
    client: State<'_, GrpcClient>,
    mentioning_node_id: String,
    mentioned_node_id: String,
) -> Result<(), CommandError> {
    let mut c = client.client().await;
    c.delete_mention(Request::new(DeleteMentionRequest {
        mentioning_node_id,
        mentioned_node_id,
    }))
    .await
    .map_err(status_to_command_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_error_serialization() {
        let err = CommandError {
            message: "Test error".to_string(),
            code: "TEST_ERROR".to_string(),
            details: Some("Debug info".to_string()),
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Test error"));
        assert!(json.contains("TEST_ERROR"));
        assert!(json.contains("Debug info"));
    }

    #[test]
    fn test_command_error_without_details() {
        let err = CommandError {
            message: "Simple error".to_string(),
            code: "SIMPLE".to_string(),
            details: None,
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Simple error"));
        // Details field should be omitted when None
        assert!(!json.contains("details"));
    }
}
