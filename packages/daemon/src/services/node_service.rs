//! tonic `NodeService` implementation backed by `nodespace-core`.
//!
//! Each RPC handler:
//!   1. Parses the proto request into the corresponding core input type.
//!   2. Calls the matching `nodespace_core::services` method (or `ops` function).
//!   3. Converts the result back into proto messages.
//!   4. Maps `NodeServiceError`/`OpsError` → `tonic::Status`.
//!
//! `Chat` returns `Unimplemented` — covered by a separate streaming issue.

use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use nodespace_core::db::events::DomainEvent;
use nodespace_core::models::{
    Node, NodeQuery, NodeUpdate, TaskNodeUpdate, TaskPriority, TaskStatus,
};
use nodespace_core::ops::{
    collection_ops::{
        self, AddNodeToCollectionByPathInput, AddNodeToCollectionInput, CreateCollectionInput,
        FindCollectionByPathInput, GetAllCollectionsInput, GetCollectionByNameInput,
        GetCollectionMembersInput, GetCollectionMembersRecursiveInput, GetNodeCollectionsInput,
        RemoveNodeFromCollectionInput, RenameCollectionInput,
    },
    node_ops,
    search_ops::{self, SearchSemanticInput},
    OpsError,
};
use nodespace_core::services::{
    InsertPosition, InsertPositionOwned, NodeAccessor, NodeEmbeddingService,
    NodeService as CoreNodeService, NodeServiceError,
};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    node_event::Event as NodeEventKind, node_service_server::NodeService as GrpcNodeService,
    AddNodeToCollectionByPathRequest, AddNodeToCollectionRequest, BatchUpdateFailure, ChatRequest,
    ChatResponse, CollectionIdResponse, CollectionIdsResponse, CollectionInfo,
    CollectionListResponse, CollectionMembersRequest, CreateCollectionRequest,
    CreateMentionRequest, CreateNodeRequest, DeleteCollectionRequest, DeleteMentionRequest,
    DeleteNodeRequest, DeleteNodeResponse, Empty, ExportMarkdownRequest, ExportMarkdownResponse,
    FindCollectionByPathRequest, GetAllCollectionsRequest, GetAllSchemasRequest,
    GetChildrenRequest, GetChildrenTreeRequest, GetCollectionByNameRequest, GetNodeRequest,
    GetNodesBatchRequest, GetNodesBatchResponse, GetRootsRequest, GetSchemaDefinitionRequest,
    MentionAutocompleteRequest, MentionIdsResponse, MentionResponse, MentionTargetRequest,
    MoveNodeRequest, NodeCollectionsRequest, NodeData, NodeDeleted, NodeEvent, NodeListResponse,
    NodeReference, NodeReferenceListResponse, NodeResponse, NodeTreeResponse, OptionalNodeResponse,
    OptionalStringClear, OptionalTimestampClear, QueryNodesSimpleRequest,
    RelationshipDeletedPayload, RelationshipPayload, RemoveNodeFromCollectionRequest,
    RenameCollectionRequest, ReorderNodeRequest, ReorderNodeResponse, SearchRequest,
    UpdateNodeRequest, UpdateNodesBatchRequest, UpdateNodesBatchResponse, UpdateTaskNodeRequest,
    UpsertNodeWithParentRequest, WatchRequest,
};

/// gRPC adapter that owns shared handles to the core services.
///
/// `NodeEmbeddingService` is optional because semantic search is gracefully
/// disabled when the NLP engine fails to start (matches the tiered-init
/// pattern in the Tauri shell).
pub struct NodeServiceImpl {
    node_service: Arc<CoreNodeService>,
    embedding_service: Option<Arc<NodeEmbeddingService>>,
}

impl NodeServiceImpl {
    pub fn new(
        node_service: Arc<CoreNodeService>,
        embedding_service: Option<Arc<NodeEmbeddingService>>,
    ) -> Self {
        Self {
            node_service,
            embedding_service,
        }
    }
}

#[tonic::async_trait]
impl GrpcNodeService for NodeServiceImpl {
    async fn create_node(
        &self,
        request: Request<CreateNodeRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        let properties = parse_properties(&req.properties).map_err(properties_error)?;
        let parent_id_opt = req.parent_id;

        use crate::nodespace::create_node_request::Position as CreatePos;
        let position = match req.position {
            Some(CreatePos::Beginning(_)) => InsertPositionOwned::Beginning,
            Some(CreatePos::End(_)) => InsertPositionOwned::End,
            Some(CreatePos::After(id)) => InsertPositionOwned::After(id),
            None => InsertPositionOwned::End,
        };

        let input = node_ops::CreateNodeInput {
            id: req.id,
            node_type: req.node_type,
            content: req.content,
            parent_id: parent_id_opt.clone(),
            position,
            properties,
            collection: req.collection,
            lifecycle_status: req.lifecycle_status,
        };

        let output = node_ops::create_node(&self.node_service, input)
            .await
            .map_err(ops_error_to_status)?;

        let node = fetch_node(&self.node_service, &output.node_id).await?;
        let node_type = node.node_type.clone();
        let parent_id = parent_id_opt.unwrap_or_default();
        let collection_id = output.collection_id;

        Ok(Response::new(NodeResponse {
            node_id: output.node_id,
            node_type,
            parent_id,
            collection_id: collection_id.clone().unwrap_or_default(),
            node_data: Some(node_to_proto(node, None, collection_id)),
        }))
    }

    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        let node = fetch_node(&self.node_service, &req.node_id).await?;
        let node_type = node.node_type.clone();

        Ok(Response::new(NodeResponse {
            node_id: req.node_id,
            node_type,
            parent_id: String::new(),
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    async fn update_node(
        &self,
        request: Request<UpdateNodeRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        let properties = match req.properties.as_deref() {
            Some(s) => Some(parse_properties(s).map_err(properties_error)?),
            None => None,
        };

        let input = node_ops::UpdateNodeInput {
            node_id: req.node_id,
            version: req.version,
            node_type: req.node_type,
            content: req.content,
            properties,
            add_to_collection: req.add_to_collection,
            remove_from_collection: req.remove_from_collection,
            lifecycle_status: req.lifecycle_status,
        };

        let output = node_ops::update_node(&self.node_service, input)
            .await
            .map_err(ops_error_to_status)?;

        let node = fetch_node(&self.node_service, &output.node_id).await?;
        let node_type = node.node_type.clone();
        let collection_id = output.collection_added;

        Ok(Response::new(NodeResponse {
            node_id: output.node_id,
            node_type,
            parent_id: String::new(),
            collection_id: collection_id.clone().unwrap_or_default(),
            node_data: Some(node_to_proto(node, None, collection_id)),
        }))
    }

    async fn delete_node(
        &self,
        request: Request<DeleteNodeRequest>,
    ) -> Result<Response<DeleteNodeResponse>, Status> {
        let req = request.into_inner();

        let input = node_ops::DeleteNodeInput {
            node_id: req.node_id,
            version: req.version,
        };

        // node_ops::delete_node handles auto-fetch; map NotFound to existed=false.
        let output = match node_ops::delete_node(&self.node_service, input).await {
            Ok(o) => o,
            Err(OpsError::NotFound { id }) => {
                return Ok(Response::new(DeleteNodeResponse {
                    node_id: id,
                    existed: false,
                }));
            }
            Err(e) => return Err(ops_error_to_status(e)),
        };

        Ok(Response::new(DeleteNodeResponse {
            node_id: output.node_id,
            existed: output.existed,
        }))
    }

    async fn get_children(
        &self,
        request: Request<GetChildrenRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();

        let children = self
            .node_service
            .get_children(&req.node_id)
            .await
            .map_err(service_error_to_status)?;

        let parent_id = req.node_id.clone();
        let nodes: Vec<NodeData> = children
            .into_iter()
            .map(|n| node_to_proto(n, Some(parent_id.clone()), None))
            .collect();

        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: String::new(),
        }))
    }

    async fn get_children_tree(
        &self,
        request: Request<GetChildrenTreeRequest>,
    ) -> Result<Response<NodeTreeResponse>, Status> {
        let req = request.into_inner();
        let tree = self
            .node_service
            .get_children_tree(&req.node_id)
            .await
            .map_err(service_error_to_status)?;

        Ok(Response::new(NodeTreeResponse {
            tree_json: tree.to_string(),
        }))
    }

    async fn get_roots(
        &self,
        request: Request<GetRootsRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 {
            None
        } else {
            Some(req.limit as usize)
        };
        let offset = if req.offset == 0 {
            None
        } else {
            Some(req.offset as usize)
        };

        let roots = self
            .node_service
            .get_roots(limit, offset)
            .await
            .map_err(service_error_to_status)?;

        let nodes: Vec<NodeData> = roots
            .into_iter()
            .map(|n| node_to_proto(n, None, None))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: String::new(),
        }))
    }

    async fn search_nodes(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();

        let embedding_service = self.embedding_service.as_ref().ok_or_else(|| {
            Status::unavailable("Embedding service not initialized — semantic search disabled")
        })?;

        if !req.semantic {
            tracing::debug!(
                "SearchRequest.semantic=false ignored; structured query mode not yet implemented"
            );
        }

        let threshold = if req.threshold == 0.0 {
            None
        } else {
            Some(req.threshold)
        };
        let limit = if req.limit == 0 {
            None
        } else {
            Some(req.limit as usize)
        };

        let node_types = if req.node_types.is_empty() {
            None
        } else {
            Some(req.node_types)
        };

        let property_filters = if req.filters.is_empty() {
            None
        } else {
            Some(
                serde_json::from_str::<serde_json::Value>(&req.filters).map_err(|e| {
                    Status::invalid_argument(format!("Invalid filters JSON: {}", e))
                })?,
            )
        };

        let input = SearchSemanticInput {
            query: req.query,
            threshold,
            limit,
            collection_id: req.collection_id,
            collection: req.collection,
            exclude_collections: None,
            include_markdown: Some(0),
            include_archived: None,
            scope: None,
            node_types,
            property_filters,
            include_edges: None,
            graph_boost: None,
        };

        let output = search_ops::search_semantic(&self.node_service, embedding_service, input)
            .await
            .map_err(ops_error_to_status)?;

        let mut nodes = Vec::with_capacity(output.nodes.len());
        for value in output.nodes {
            let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            match self.node_service.get_node(id).await {
                Ok(Some(node)) => nodes.push(node_to_proto(node, None, None)),
                Ok(None) => tracing::warn!(node_id = %id, "search result missing on re-fetch"),
                Err(e) => {
                    tracing::warn!(node_id = %id, error = %e, "failed to re-fetch search result")
                }
            }
        }

        let count = nodes.len() as i32;
        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: output.collection_id.unwrap_or_default(),
        }))
    }

    async fn query_nodes_simple(
        &self,
        request: Request<QueryNodesSimpleRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();

        let query = NodeQuery {
            id: req.id,
            mentioned_by: req.mentioned_by,
            content_contains: req.content_contains,
            title_contains: req.title_contains,
            node_type: req.node_type,
            limit: if req.limit == 0 {
                None
            } else {
                Some(req.limit as usize)
            },
            offset: if req.offset == 0 {
                None
            } else {
                Some(req.offset as usize)
            },
        };

        let nodes = self
            .node_service
            .query_nodes_simple(query)
            .await
            .map_err(service_error_to_status)?;

        let proto_nodes: Vec<NodeData> = nodes
            .into_iter()
            .map(|n| node_to_proto(n, None, None))
            .collect();
        let count = proto_nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes: proto_nodes,
            count,
            collection_id: String::new(),
        }))
    }

    async fn mention_autocomplete(
        &self,
        request: Request<MentionAutocompleteRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();

        let limit = if req.limit == 0 {
            None
        } else {
            Some(req.limit as usize)
        };

        let nodes = self
            .node_service
            .mention_autocomplete(&req.query, limit)
            .await
            .map_err(service_error_to_status)?;

        let proto_nodes: Vec<NodeData> = nodes
            .into_iter()
            .map(|n| node_to_proto(n, None, None))
            .collect();
        let count = proto_nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes: proto_nodes,
            count,
            collection_id: String::new(),
        }))
    }

    async fn upsert_node_with_parent(
        &self,
        request: Request<UpsertNodeWithParentRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        self.node_service
            .upsert_node_with_parent(
                &req.node_id,
                &req.content,
                &req.node_type,
                &req.parent_id,
                &req.root_id,
                None, // before_sibling_id intentionally None per #616 fractional ordering
            )
            .await
            .map_err(service_error_to_status)?;

        let node = fetch_node(&self.node_service, &req.node_id).await?;
        let node_type = node.node_type.clone();
        Ok(Response::new(NodeResponse {
            node_id: req.node_id,
            node_type,
            parent_id: req.parent_id,
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    async fn move_node(
        &self,
        request: Request<MoveNodeRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        // Normalize "" to None: both unset and empty-string mean "move to root".
        let new_parent = req.new_parent_id.filter(|s| !s.is_empty());

        use crate::nodespace::move_node_request::Position as MovePos;
        let position = match req.position {
            Some(MovePos::Beginning(_)) => InsertPosition::Beginning,
            Some(MovePos::End(_)) => InsertPosition::End,
            Some(MovePos::After(ref id)) => InsertPosition::After(id.as_str()),
            None => InsertPosition::End,
        };

        let node = self
            .node_service
            .move_node(&req.node_id, req.version, new_parent.as_deref(), position)
            .await
            .map_err(service_error_to_status)?;

        let node_type = node.node_type.clone();
        Ok(Response::new(NodeResponse {
            node_id: node.id.clone(),
            node_type,
            parent_id: new_parent.unwrap_or_default(),
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    async fn reorder_node(
        &self,
        request: Request<ReorderNodeRequest>,
    ) -> Result<Response<ReorderNodeResponse>, Status> {
        let req = request.into_inner();

        use crate::nodespace::reorder_node_request::Position as ReorderPos;
        let position = match req.position {
            Some(ReorderPos::Beginning(_)) => InsertPosition::Beginning,
            Some(ReorderPos::End(_)) => InsertPosition::End,
            Some(ReorderPos::After(ref id)) => InsertPosition::After(id.as_str()),
            None => InsertPosition::End,
        };

        self.node_service
            .reorder_node(&req.node_id, req.version, position)
            .await
            .map_err(service_error_to_status)?;

        Ok(Response::new(ReorderNodeResponse {}))
    }

    async fn create_mention(
        &self,
        request: Request<CreateMentionRequest>,
    ) -> Result<Response<MentionResponse>, Status> {
        let req = request.into_inner();
        self.node_service
            .create_mention(&req.mentioning_node_id, &req.mentioned_node_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(MentionResponse {
            mentioning_node_id: req.mentioning_node_id,
            mentioned_node_id: req.mentioned_node_id,
        }))
    }

    async fn delete_mention(
        &self,
        request: Request<DeleteMentionRequest>,
    ) -> Result<Response<MentionResponse>, Status> {
        let req = request.into_inner();
        self.node_service
            .remove_mention(&req.mentioning_node_id, &req.mentioned_node_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(MentionResponse {
            mentioning_node_id: req.mentioning_node_id,
            mentioned_node_id: req.mentioned_node_id,
        }))
    }

    async fn get_outgoing_mentions(
        &self,
        request: Request<MentionTargetRequest>,
    ) -> Result<Response<MentionIdsResponse>, Status> {
        let req = request.into_inner();
        let ids = self
            .node_service
            .get_mentions(&req.node_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(MentionIdsResponse { node_ids: ids }))
    }

    async fn get_incoming_mentions(
        &self,
        request: Request<MentionTargetRequest>,
    ) -> Result<Response<MentionIdsResponse>, Status> {
        let req = request.into_inner();
        let ids = self
            .node_service
            .get_mentioned_by(&req.node_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(MentionIdsResponse { node_ids: ids }))
    }

    async fn get_mentioning_roots(
        &self,
        request: Request<MentionTargetRequest>,
    ) -> Result<Response<NodeReferenceListResponse>, Status> {
        let req = request.into_inner();
        let refs = self
            .node_service
            .get_mentioning_containers(&req.node_id)
            .await
            .map_err(service_error_to_status)?;

        let references = refs
            .into_iter()
            .map(|r| NodeReference {
                id: r.id,
                title: r.title,
                node_type: r.node_type,
            })
            .collect();

        Ok(Response::new(NodeReferenceListResponse { references }))
    }

    async fn update_task_node(
        &self,
        request: Request<UpdateTaskNodeRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();

        let update = build_task_node_update(
            req.status,
            req.priority,
            req.due_date,
            req.assignee,
            req.started_at,
            req.completed_at,
            req.content,
        )
        .map_err(Status::invalid_argument)?;

        let task = self
            .node_service
            .update_task_node(&req.node_id, req.version, update)
            .await
            .map_err(service_error_to_status)?;

        // Convert TaskNode back to Node for proto wire shape. Frontend reconstructs
        // the typed view via task_node_to_typed_value on the Tauri side.
        let node: Node = task.into_node();
        let node_type = node.node_type.clone();
        let node_id = node.id.clone();

        Ok(Response::new(NodeResponse {
            node_id,
            node_type,
            parent_id: String::new(),
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    // -- Markdown export -----------------------------------------------------

    async fn export_markdown(
        &self,
        request: Request<ExportMarkdownRequest>,
    ) -> Result<Response<ExportMarkdownResponse>, Status> {
        let req = request.into_inner();

        use serde_json::json;
        let params = json!({
            "node_id": req.node_id,
            "include_children": req.include_children.unwrap_or(true),
            "max_depth": if req.max_depth == 0 { 20u32 } else { req.max_depth },
            "include_node_ids": req.include_node_ids.unwrap_or(true),
        });

        let result =
            nodespace_core::markdown::handle_get_markdown_from_node_id(&self.node_service, params)
                .await
                .map_err(|e| match e {
                    nodespace_core::markdown::MarkdownError::NotFound(m) => Status::not_found(m),
                    nodespace_core::markdown::MarkdownError::InvalidParams(m) => {
                        Status::invalid_argument(m)
                    }
                    nodespace_core::markdown::MarkdownError::CreationFailed(m) => {
                        Status::failed_precondition(format!("Node creation failed: {m}"))
                    }
                    nodespace_core::markdown::MarkdownError::Internal(m) => Status::internal(m),
                })?;

        let markdown = result["markdown"]
            .as_str()
            .ok_or_else(|| Status::internal("ExportMarkdown: missing 'markdown' in response"))?
            .to_string();
        let node_count = result["node_count"]
            .as_u64()
            .ok_or_else(|| Status::internal("ExportMarkdown: missing 'node_count' in response"))?
            as u32;

        Ok(Response::new(ExportMarkdownResponse {
            markdown,
            node_count,
        }))
    }

    // -- Batch operations ----------------------------------------------------

    async fn get_nodes_batch(
        &self,
        request: Request<GetNodesBatchRequest>,
    ) -> Result<Response<GetNodesBatchResponse>, Status> {
        let req = request.into_inner();

        if req.node_ids.is_empty() {
            return Err(Status::invalid_argument("node_ids cannot be empty"));
        }
        if req.node_ids.len() > 100 {
            return Err(Status::invalid_argument(format!(
                "Batch size exceeds maximum of 100 (got {})",
                req.node_ids.len()
            )));
        }

        let id_refs: Vec<&str> = req.node_ids.iter().map(String::as_str).collect();
        let fetched = self
            .node_service
            .get_nodes(&id_refs)
            .await
            .map_err(service_error_to_status)?;

        let fetched_ids: std::collections::HashSet<String> =
            fetched.iter().map(|n| n.id.clone()).collect();
        let not_found: Vec<String> = req
            .node_ids
            .iter()
            .filter(|id| !fetched_ids.contains(*id))
            .cloned()
            .collect();

        let nodes: Vec<NodeData> = fetched
            .into_iter()
            .map(|n| node_to_proto(n, None, None))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(GetNodesBatchResponse {
            nodes,
            not_found,
            count,
        }))
    }

    async fn update_nodes_batch(
        &self,
        request: Request<UpdateNodesBatchRequest>,
    ) -> Result<Response<UpdateNodesBatchResponse>, Status> {
        let req = request.into_inner();

        if req.updates.is_empty() {
            return Err(Status::invalid_argument("updates cannot be empty"));
        }
        if req.updates.len() > 100 {
            return Err(Status::invalid_argument(format!(
                "Batch size exceeds maximum of 100 (got {})",
                req.updates.len()
            )));
        }

        let mut updated = Vec::new();
        let mut failed = Vec::new();

        for item in req.updates {
            let version = match item.version {
                Some(v) => v,
                None => {
                    tracing::warn!(
                        node_id = %item.node_id,
                        "OCC bypassed: version not provided for batch update (race condition possible)"
                    );
                    match self.node_service.get_node(&item.node_id).await {
                        Ok(Some(node)) => node.version,
                        Ok(None) => {
                            failed.push(BatchUpdateFailure {
                                node_id: item.node_id.clone(),
                                error: format!("Node '{}' not found", item.node_id),
                            });
                            continue;
                        }
                        Err(e) => {
                            failed.push(BatchUpdateFailure {
                                node_id: item.node_id.clone(),
                                error: e.to_string(),
                            });
                            continue;
                        }
                    }
                }
            };

            let props = match item.properties {
                Some(ref s) => match parse_properties(s) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        failed.push(BatchUpdateFailure {
                            node_id: item.node_id.clone(),
                            error: format!("invalid properties JSON: {e}"),
                        });
                        continue;
                    }
                },
                None => None,
            };

            let node_update = NodeUpdate {
                content: item.content,
                node_type: item.node_type,
                properties: props,
                title: None,
                lifecycle_status: None,
            };

            match self
                .node_service
                .update_node(&item.node_id, version, node_update)
                .await
            {
                Ok(_) => updated.push(item.node_id),
                Err(e) => {
                    failed.push(BatchUpdateFailure {
                        node_id: item.node_id,
                        error: e.to_string(),
                    });
                }
            }
        }

        let count = updated.len() as i32;
        Ok(Response::new(UpdateNodesBatchResponse {
            updated,
            failed,
            count,
        }))
    }

    // -- Schemas (read-only) -------------------------------------------------

    async fn get_all_schemas(
        &self,
        _request: Request<GetAllSchemasRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let query = NodeQuery {
            node_type: Some("schema".to_string()),
            ..Default::default()
        };
        let nodes = self
            .node_service
            .query_nodes_simple(query)
            .await
            .map_err(service_error_to_status)?;

        let proto_nodes: Vec<NodeData> = nodes
            .into_iter()
            .map(|n| node_to_proto(n, None, None))
            .collect();
        let count = proto_nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes: proto_nodes,
            count,
            collection_id: String::new(),
        }))
    }

    async fn get_schema_definition(
        &self,
        request: Request<GetSchemaDefinitionRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();
        let node = fetch_node(&self.node_service, &req.schema_id).await?;
        if node.node_type != "schema" {
            return Err(Status::failed_precondition(format!(
                "Node '{}' is not a schema (type={})",
                req.schema_id, node.node_type
            )));
        }
        let node_type = node.node_type.clone();
        Ok(Response::new(NodeResponse {
            node_id: req.schema_id,
            node_type,
            parent_id: String::new(),
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    // -- Collections ---------------------------------------------------------

    async fn get_all_collections(
        &self,
        _request: Request<GetAllCollectionsRequest>,
    ) -> Result<Response<CollectionListResponse>, Status> {
        let output =
            collection_ops::get_all_collections(&self.node_service, GetAllCollectionsInput)
                .await
                .map_err(ops_error_to_status)?;

        let collections = output
            .collections
            .into_iter()
            .map(|e| CollectionInfo {
                node: Some(node_to_proto(e.node, None, None)),
                member_count: e.member_count as u32,
                parent_collection_ids: e.parent_collection_ids,
            })
            .collect();

        Ok(Response::new(CollectionListResponse { collections }))
    }

    async fn get_collection_members(
        &self,
        request: Request<CollectionMembersRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::get_collection_members(
            &self.node_service,
            GetCollectionMembersInput {
                collection_id: req.collection_id,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

        let nodes: Vec<NodeData> = output
            .members
            .into_iter()
            .map(|n| node_to_proto(n, None, Some(output.collection_id.clone())))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: output.collection_id,
        }))
    }

    async fn get_collection_members_recursive(
        &self,
        request: Request<CollectionMembersRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::get_collection_members_recursive(
            &self.node_service,
            GetCollectionMembersRecursiveInput {
                collection_id: req.collection_id,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

        let nodes: Vec<NodeData> = output
            .members
            .into_iter()
            .map(|n| node_to_proto(n, None, Some(output.collection_id.clone())))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: output.collection_id,
        }))
    }

    async fn get_node_collections(
        &self,
        request: Request<NodeCollectionsRequest>,
    ) -> Result<Response<CollectionIdsResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::get_node_collections(
            &self.node_service,
            GetNodeCollectionsInput {
                node_id: req.node_id,
            },
        )
        .await
        .map_err(ops_error_to_status)?;
        Ok(Response::new(CollectionIdsResponse {
            collection_ids: output.collection_ids,
        }))
    }

    async fn add_node_to_collection(
        &self,
        request: Request<AddNodeToCollectionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        collection_ops::add_node_to_collection(
            &self.node_service,
            AddNodeToCollectionInput {
                node_id: req.node_id,
                collection_id: req.collection_id,
            },
        )
        .await
        .map_err(ops_error_to_status)?;
        Ok(Response::new(Empty {}))
    }

    async fn add_node_to_collection_by_path(
        &self,
        request: Request<AddNodeToCollectionByPathRequest>,
    ) -> Result<Response<CollectionIdResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::add_node_to_collection_by_path(
            &self.node_service,
            AddNodeToCollectionByPathInput {
                node_id: req.node_id,
                collection_path: req.collection_path,
            },
        )
        .await
        .map_err(ops_error_to_status)?;
        Ok(Response::new(CollectionIdResponse {
            collection_id: output.collection_id,
        }))
    }

    async fn remove_node_from_collection(
        &self,
        request: Request<RemoveNodeFromCollectionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        collection_ops::remove_node_from_collection(
            &self.node_service,
            RemoveNodeFromCollectionInput {
                node_id: req.node_id,
                collection_id: req.collection_id,
            },
        )
        .await
        .map_err(ops_error_to_status)?;
        Ok(Response::new(Empty {}))
    }

    async fn find_collection_by_path(
        &self,
        request: Request<FindCollectionByPathRequest>,
    ) -> Result<Response<OptionalNodeResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::find_collection_by_path(
            &self.node_service,
            FindCollectionByPathInput {
                collection_path: req.collection_path,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

        let node_response = output.collection.map(|n| {
            let node_type = n.node_type.clone();
            let node_id = n.id.clone();
            NodeResponse {
                node_id,
                node_type,
                parent_id: String::new(),
                collection_id: String::new(),
                node_data: Some(node_to_proto(n, None, None)),
            }
        });
        Ok(Response::new(OptionalNodeResponse {
            node: node_response,
        }))
    }

    async fn get_collection_by_name(
        &self,
        request: Request<GetCollectionByNameRequest>,
    ) -> Result<Response<OptionalNodeResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::get_collection_by_name(
            &self.node_service,
            GetCollectionByNameInput { name: req.name },
        )
        .await
        .map_err(ops_error_to_status)?;

        let node_response = output.collection.map(|n| {
            let node_type = n.node_type.clone();
            let node_id = n.id.clone();
            NodeResponse {
                node_id,
                node_type,
                parent_id: String::new(),
                collection_id: String::new(),
                node_data: Some(node_to_proto(n, None, None)),
            }
        });
        Ok(Response::new(OptionalNodeResponse {
            node: node_response,
        }))
    }

    async fn create_collection(
        &self,
        request: Request<CreateCollectionRequest>,
    ) -> Result<Response<CollectionIdResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::create_collection(
            &self.node_service,
            CreateCollectionInput {
                name: req.name,
                description: req.description,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

        Ok(Response::new(CollectionIdResponse {
            collection_id: output.collection_id,
        }))
    }

    async fn rename_collection(
        &self,
        request: Request<RenameCollectionRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();
        let output = collection_ops::rename_collection(
            &self.node_service,
            RenameCollectionInput {
                collection_id: req.collection_id,
                new_name: req.new_name,
                version: req.version,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

        let node = output.node;
        let node_type = node.node_type.clone();
        let node_id = node.id.clone();
        Ok(Response::new(NodeResponse {
            node_id,
            node_type,
            parent_id: String::new(),
            collection_id: String::new(),
            node_data: Some(node_to_proto(node, None, None)),
        }))
    }

    async fn delete_collection(
        &self,
        request: Request<DeleteCollectionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        self.node_service
            .delete_node(&req.collection_id, req.version)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(Empty {}))
    }

    // -- Streaming (unimplemented; tracked separately) -----------------------

    type WatchNodesStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<NodeEvent, Status>> + Send + 'static>>;

    async fn watch_nodes(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchNodesStream>, Status> {
        let req = request.into_inner();
        if !req.node_type.is_empty() || !req.root_id.is_empty() {
            // Filtering is intentionally out of scope for the initial implementation
            // (issue #1114 lists it as a Non-Goal). Log so clients can see the
            // request was accepted but the filter is being ignored.
            tracing::debug!(
                node_type = %req.node_type,
                root_id = %req.root_id,
                "WatchNodes filter fields are not yet implemented; streaming all events"
            );
        }

        let mut rx = self.node_service.subscribe_to_events();
        // Clone the Arc so the stream owns its own handle — the stream future
        // outlives `&self` (it is returned to tonic and polled independently),
        // so it cannot borrow from the handler scope.
        let node_service = self.node_service.clone();

        let stream = async_stream::stream! {
            loop {
                match rx.recv().await {
                    Ok(envelope) => {
                        // Translation is serial: a slow `get_node` lookup will
                        // delay the next `rx.recv()` and increase the risk of
                        // `Lagged`. Acceptable because lookups are SQLite
                        // point-reads and lag is observable downstream. If a
                        // future workload makes this hot, parallelize by
                        // dispatching translations to a bounded mpsc.
                        if let Some(event) = convert_domain_event(&envelope.event, &node_service).await {
                            yield Ok(event);
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        // The broadcast channel ring buffer overflowed. A slow
                        // client missed `skipped` events. Log and continue —
                        // dropping the stream on lag would be worse than the
                        // client briefly being out of sync, and `Lagged` is
                        // observable from the broadcast layer (not a bug).
                        tracing::warn!(skipped, "WatchNodes subscriber lagged; some events dropped");
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    type ChatStream = ReceiverStream<Result<ChatResponse, Status>>;

    async fn chat(
        &self,
        _request: Request<tonic::Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        Err(Status::unimplemented(
            "Chat streaming is not yet implemented — tracked separately",
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_properties(s: &str) -> Result<serde_json::Value, serde_json::Error> {
    if s.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(s)
}

fn properties_error(e: serde_json::Error) -> Status {
    Status::invalid_argument(format!("Invalid properties JSON: {}", e))
}

async fn fetch_node(service: &Arc<CoreNodeService>, node_id: &str) -> Result<Node, Status> {
    service
        .get_node(node_id)
        .await
        .map_err(service_error_to_status)?
        .ok_or_else(|| Status::not_found(format!("Node not found: {}", node_id)))
}

pub(crate) fn node_to_proto(
    node: Node,
    parent_id: Option<String>,
    collection_id: Option<String>,
) -> NodeData {
    NodeData {
        id: node.id,
        node_type: node.node_type,
        content: node.content,
        parent_id,
        properties: node.properties.to_string(),
        version: node.version,
        lifecycle_status: node.lifecycle_status,
        created_at: node.created_at.to_rfc3339(),
        modified_at: node.modified_at.to_rfc3339(),
        collection_id: collection_id.unwrap_or_default(),
    }
}

/// Translate a core `DomainEvent` into a proto `NodeEvent`.
///
/// Returns `None` for non-node events (relationships) — those are out of scope
/// for `WatchNodes` (per issue #1114 Non-Goals: relationship streaming is a
/// separate concern).
///
/// For `NodeCreated` and `NodeUpdated`, fetches the current node payload so
/// clients receive full node data inline and don't need a follow-up `GetNode`.
/// If the node has already been deleted by the time we look it up (a race
/// possible under concurrent mutations), the event is dropped — the next event
/// in the stream will be the corresponding `NodeDeleted`.
async fn convert_domain_event(
    event: &DomainEvent,
    node_service: &Arc<CoreNodeService>,
) -> Option<NodeEvent> {
    match event {
        DomainEvent::NodeCreated { node_id, .. } => match node_service.get_node(node_id).await {
            Ok(Some(node)) => Some(NodeEvent {
                event: Some(NodeEventKind::Created(node_to_proto(node, None, None))),
            }),
            Ok(None) => {
                tracing::debug!(node_id = %node_id, "NodeCreated event skipped: node already gone");
                None
            }
            Err(e) => {
                tracing::warn!(node_id = %node_id, error = %e, "failed to fetch node for NodeCreated event");
                None
            }
        },
        DomainEvent::NodeUpdated { node_id, .. } => match node_service.get_node(node_id).await {
            Ok(Some(node)) => Some(NodeEvent {
                event: Some(NodeEventKind::Updated(node_to_proto(node, None, None))),
            }),
            Ok(None) => {
                tracing::debug!(node_id = %node_id, "NodeUpdated event skipped: node already gone");
                None
            }
            Err(e) => {
                tracing::warn!(node_id = %node_id, error = %e, "failed to fetch node for NodeUpdated event");
                None
            }
        },
        DomainEvent::NodeDeleted { id, node_type } => Some(NodeEvent {
            event: Some(NodeEventKind::Deleted(NodeDeleted {
                node_id: id.clone(),
                node_type: node_type.clone(),
            })),
        }),
        DomainEvent::RelationshipCreated { relationship } => Some(NodeEvent {
            event: Some(NodeEventKind::RelationshipCreated(relationship_to_proto(
                relationship,
            ))),
        }),
        DomainEvent::RelationshipUpdated { relationship } => Some(NodeEvent {
            event: Some(NodeEventKind::RelationshipUpdated(relationship_to_proto(
                relationship,
            ))),
        }),
        DomainEvent::RelationshipDeleted {
            id,
            from_id,
            to_id,
            relationship_type,
        } => Some(NodeEvent {
            event: Some(NodeEventKind::RelationshipDeleted(
                RelationshipDeletedPayload {
                    id: id.clone(),
                    from_id: from_id.clone(),
                    to_id: to_id.clone(),
                    relationship_type: relationship_type.clone(),
                },
            )),
        }),
    }
}

/// Translate a `RelationshipEvent` from the in-process domain channel
/// into the proto wire form. `properties` is JSON-encoded as a string
/// so the proto schema stays stable across additions to the
/// underlying `serde_json::Value` payload — the desktop watcher
/// re-parses it back to JSON before emitting the Tauri event.
fn relationship_to_proto(
    rel: &nodespace_core::db::events::RelationshipEvent,
) -> RelationshipPayload {
    RelationshipPayload {
        id: rel.id.clone(),
        from_id: rel.from_id.clone(),
        to_id: rel.to_id.clone(),
        relationship_type: rel.relationship_type.clone(),
        properties: rel.properties.to_string(),
    }
}

fn ops_error_to_status(err: OpsError) -> Status {
    match err {
        OpsError::NotFound { id } => Status::not_found(format!("Not found: {}", id)),
        OpsError::AlreadyExists { id } => Status::already_exists(format!("Already exists: {}", id)),
        OpsError::VersionConflict {
            node_id,
            expected,
            actual,
            current_node,
        } => {
            let message = format!(
                "Version conflict on {}: expected {}, got {}",
                node_id, expected, actual
            );
            let payload = serde_json::json!({
                "node_id": node_id,
                "expected": expected,
                "actual": actual,
                "current_node": current_node,
            });
            let mut status = Status::new(tonic::Code::Aborted, message);
            if let Ok(json) = serde_json::to_string(&payload) {
                if let Ok(val) =
                    json.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
                {
                    status.metadata_mut().insert("x-version-conflict", val);
                }
            }
            status
        }
        OpsError::ValidationFailed(msg) => {
            Status::invalid_argument(format!("Validation failed: {}", msg))
        }
        OpsError::InvalidParams(msg) => Status::invalid_argument(msg),
        OpsError::Internal(msg) => Status::internal(msg),
    }
}

fn service_error_to_status(err: NodeServiceError) -> Status {
    ops_error_to_status(OpsError::from(err))
}

/// Build a `TaskNodeUpdate` from the proto's tri-state wrappers.
///
/// `OptionalStringClear`/`OptionalTimestampClear` encode the
/// Option<Option<T>> pattern: outer `None` ⇒ field unset on the wire, which we
/// surface as "no change". When the wrapper is present, `clear=true` writes
/// `Some(None)` (clear value) and `clear=false` writes `Some(Some(parsed))`.
fn build_task_node_update(
    status: Option<String>,
    priority: Option<OptionalStringClear>,
    due_date: Option<OptionalTimestampClear>,
    assignee: Option<OptionalStringClear>,
    started_at: Option<OptionalTimestampClear>,
    completed_at: Option<OptionalTimestampClear>,
    content: Option<String>,
) -> Result<TaskNodeUpdate, String> {
    let status = match status {
        None => None,
        Some(s) => Some(
            serde_json::from_value::<TaskStatus>(serde_json::Value::String(s.clone()))
                .map_err(|e| format!("Invalid task status '{}': {}", s, e))?,
        ),
    };

    let priority = match priority {
        None => None,
        Some(w) if w.clear => Some(None),
        Some(w) => Some(Some(parse_task_priority(&w.value)?)),
    };

    let assignee = match assignee {
        None => None,
        Some(w) if w.clear => Some(None),
        Some(w) => Some(Some(w.value)),
    };

    let due_date = parse_optional_timestamp(due_date, "due_date")?;
    let started_at = parse_optional_timestamp(started_at, "started_at")?;
    let completed_at = parse_optional_timestamp(completed_at, "completed_at")?;

    Ok(TaskNodeUpdate {
        status,
        priority,
        due_date,
        assignee,
        started_at,
        completed_at,
        content,
    })
}

fn parse_task_priority(value: &str) -> Result<TaskPriority, String> {
    serde_json::from_value::<TaskPriority>(serde_json::Value::String(value.to_string()))
        .map_err(|e| format!("Invalid task priority '{}': {}", value, e))
}

fn parse_optional_timestamp(
    wrapper: Option<OptionalTimestampClear>,
    field_name: &str,
) -> Result<Option<Option<DateTime<Utc>>>, String> {
    match wrapper {
        None => Ok(None),
        Some(w) if w.clear => Ok(Some(None)),
        Some(w) => {
            let parsed = DateTime::parse_from_rfc3339(&w.value)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| format!("Invalid RFC3339 timestamp for {}: {}", field_name, e))?;
            Ok(Some(Some(parsed)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::ops::node_ops;
    use nodespace_core::services::{CollectionService, NodeService as CoreNodeService};
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn make_service() -> (Arc<NodeServiceImpl>, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut store = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let core_svc = Arc::new(CoreNodeService::new(&mut store).await.unwrap());
        let svc = Arc::new(NodeServiceImpl::new(core_svc, None));
        (svc, tmp)
    }

    /// Creating a node via the gRPC handler (with collection + lifecycle_status) must
    /// produce the same persisted state as calling node_ops::create_node directly.
    #[tokio::test]
    async fn create_node_rpc_parity_with_node_ops() {
        let (svc, _tmp) = make_service().await;

        // --- via RPC handler ---
        let rpc_req = Request::new(crate::nodespace::CreateNodeRequest {
            id: None,
            node_type: "text".to_string(),
            content: "parity-test".to_string(),
            parent_id: None,
            collection: Some("test-collection".to_string()),
            lifecycle_status: Some("archived".to_string()),
            properties: "{}".to_string(),
            position: None,
        });
        let rpc_resp = svc.create_node(rpc_req).await.unwrap().into_inner();
        let rpc_node_id = rpc_resp.node_id.clone();

        // Assert collection membership was set
        assert!(
            !rpc_resp.collection_id.is_empty(),
            "RPC handler must populate collection_id"
        );
        // Assert lifecycle_status is reflected in returned NodeData
        let rpc_node_data = rpc_resp.node_data.unwrap();
        assert_eq!(
            rpc_node_data.lifecycle_status, "archived",
            "RPC handler must apply lifecycle_status"
        );

        // --- via node_ops directly (same NodeService, fresh second node) ---
        let ops_input = node_ops::CreateNodeInput {
            id: None,
            node_type: "text".to_string(),
            content: "parity-test".to_string(),
            parent_id: None,
            position: nodespace_core::services::InsertPositionOwned::End,
            properties: serde_json::json!({}),
            collection: Some("test-collection".to_string()),
            lifecycle_status: Some("archived".to_string()),
        };
        let ops_output = node_ops::create_node(&svc.node_service, ops_input)
            .await
            .unwrap();

        // Both nodes should be in a collection and have lifecycle_status=archived
        assert!(
            ops_output.collection_id.is_some(),
            "ops must populate collection_id"
        );
        let ops_node = svc
            .node_service
            .get_node(&ops_output.node_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ops_node.lifecycle_status, "archived");

        // Both nodes must be distinct (idempotency: each call creates a new node)
        assert_ne!(rpc_node_id, ops_output.node_id);
    }

    /// update_node via RPC with no version supplied must auto-fetch and not return VersionConflict.
    #[tokio::test]
    async fn update_node_rpc_auto_fetch_version() {
        let (svc, _tmp) = make_service().await;

        // Create a node first
        let create_req = Request::new(crate::nodespace::CreateNodeRequest {
            id: None,
            node_type: "text".to_string(),
            content: "original".to_string(),
            parent_id: None,
            collection: None,
            lifecycle_status: None,
            properties: "{}".to_string(),
            position: None,
        });
        let created = svc.create_node(create_req).await.unwrap().into_inner();
        let node_id = created.node_id;

        // Update without sending version — handler must auto-fetch
        let update_req = Request::new(crate::nodespace::UpdateNodeRequest {
            node_id: node_id.clone(),
            content: Some("updated".to_string()),
            node_type: None,
            properties: None,
            version: None, // omit version — triggers auto-fetch path
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        });
        let updated = svc.update_node(update_req).await.unwrap().into_inner();
        assert_eq!(updated.node_data.unwrap().content, "updated");
    }

    /// update_node with add_to_collection then remove_from_collection must transition membership.
    #[tokio::test]
    async fn update_node_rpc_collection_add_then_remove() {
        let (svc, _tmp) = make_service().await;

        // Create a node
        let create_req = Request::new(crate::nodespace::CreateNodeRequest {
            id: None,
            node_type: "text".to_string(),
            content: "membership-test".to_string(),
            parent_id: None,
            collection: None,
            lifecycle_status: None,
            properties: "{}".to_string(),
            position: None,
        });
        let created = svc.create_node(create_req).await.unwrap().into_inner();
        let node_id = created.node_id;

        // Add to collection via update
        let add_req = Request::new(crate::nodespace::UpdateNodeRequest {
            node_id: node_id.clone(),
            content: None,
            node_type: None,
            properties: None,
            version: None,
            add_to_collection: Some("membership-coll".to_string()),
            remove_from_collection: None,
            lifecycle_status: None,
        });
        let added = svc.update_node(add_req).await.unwrap().into_inner();
        let collection_id = added.collection_id.clone();
        assert!(
            !collection_id.is_empty(),
            "add_to_collection must return collection_id"
        );

        // Verify membership via core
        let store = svc.node_service.store();
        let coll_svc = CollectionService::new(store, &svc.node_service);
        let members_before: Vec<String> = coll_svc.get_node_collections(&node_id).await.unwrap();
        assert!(
            members_before.contains(&collection_id),
            "node must be in collection after add"
        );

        // Remove from collection via update
        let remove_req = Request::new(crate::nodespace::UpdateNodeRequest {
            node_id: node_id.clone(),
            content: None,
            node_type: None,
            properties: None,
            version: None,
            add_to_collection: None,
            remove_from_collection: Some(collection_id.clone()),
            lifecycle_status: None,
        });
        svc.update_node(remove_req).await.unwrap();

        let members_after: Vec<String> = coll_svc.get_node_collections(&node_id).await.unwrap();
        assert!(
            !members_after.contains(&collection_id),
            "node must not be in collection after remove"
        );
    }

    /// Deleting a missing node via RPC returns existed=false (idempotent).
    #[tokio::test]
    async fn delete_node_rpc_missing_node_returns_existed_false() {
        let (svc, _tmp) = make_service().await;

        let req = Request::new(crate::nodespace::DeleteNodeRequest {
            node_id: "nonexistent-node-id".to_string(),
            version: None,
        });
        let resp = svc.delete_node(req).await.unwrap().into_inner();
        assert!(!resp.existed);
    }

    // -- Error mapping parity tests ------------------------------------------
    //
    // For each NodeServiceError variant: assert the resulting tonic::Status code.
    // These exercise the full NodeServiceError → OpsError → Status chain.

    fn to_status(err: NodeServiceError) -> tonic::Status {
        service_error_to_status(err)
    }

    #[test]
    fn error_mapping_node_not_found_returns_not_found() {
        let s = to_status(NodeServiceError::node_not_found("abc"));
        assert_eq!(s.code(), tonic::Code::NotFound);
    }

    #[test]
    fn error_mapping_version_conflict_returns_aborted() {
        let s = to_status(NodeServiceError::version_conflict("n1", 3, 5));
        assert_eq!(s.code(), tonic::Code::Aborted);
        // The payload must be present so the Tauri command can surface it.
        let header = s
            .metadata()
            .get("x-version-conflict")
            .expect("x-version-conflict header missing");
        let json: serde_json::Value = serde_json::from_str(header.to_str().unwrap()).unwrap();
        assert_eq!(json["node_id"], "n1");
        assert_eq!(json["expected"], 3);
        assert_eq!(json["actual"], 5);
    }

    #[test]
    fn error_mapping_validation_failed_returns_invalid_argument() {
        use nodespace_core::models::ValidationError;
        let s = to_status(NodeServiceError::ValidationFailed(
            ValidationError::MissingField("x".into()),
        ));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_invalid_parent_returns_invalid_argument() {
        let s = to_status(NodeServiceError::invalid_parent("p1"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_invalid_root_returns_invalid_argument() {
        let s = to_status(NodeServiceError::invalid_root("r1"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_circular_reference_returns_invalid_argument() {
        let s = to_status(NodeServiceError::circular_reference("A→B→A"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_hierarchy_violation_returns_invalid_argument() {
        let s = to_status(NodeServiceError::hierarchy_violation("root immutable"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_invalid_update_returns_invalid_argument() {
        let s = to_status(NodeServiceError::invalid_update("cannot change type"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_invalid_collection_path_returns_invalid_argument() {
        let s = to_status(NodeServiceError::invalid_collection_path("empty segment"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_collection_cycle_returns_invalid_argument() {
        let s = to_status(NodeServiceError::collection_cycle("A→B→A"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_collection_depth_exceeded_returns_invalid_argument() {
        let s = to_status(NodeServiceError::collection_depth_exceeded(
            "a:b:c:d:e:f",
            5,
        ));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_playbook_validation_failed_returns_invalid_argument() {
        let s = to_status(NodeServiceError::PlaybookValidationFailed {
            errors: "bad rule".into(),
        });
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn error_mapping_collection_not_found_returns_not_found() {
        let s = to_status(NodeServiceError::collection_not_found("inbox"));
        assert_eq!(s.code(), tonic::Code::NotFound);
    }

    #[test]
    fn error_mapping_database_error_returns_internal() {
        use nodespace_core::db::DatabaseError;
        let s = to_status(NodeServiceError::DatabaseError(
            DatabaseError::initialization_failed("conn lost"),
        ));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_transaction_failed_returns_internal() {
        let s = to_status(NodeServiceError::transaction_failed("commit failed"));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_serialization_error_returns_internal() {
        let s = to_status(NodeServiceError::serialization_error("bad json"));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_query_failed_returns_internal() {
        let s = to_status(NodeServiceError::query_failed("syntax error"));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_bulk_operation_failed_returns_internal() {
        let s = to_status(NodeServiceError::bulk_operation_failed("3 of 5 failed"));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_initialization_error_returns_internal() {
        let s = to_status(NodeServiceError::initialization_error("db unavailable"));
        assert_eq!(s.code(), tonic::Code::Internal);
    }

    #[test]
    fn error_mapping_not_a_container_returns_invalid_argument() {
        let s = to_status(NodeServiceError::not_a_container("parent-id", "query"));
        assert_eq!(s.code(), tonic::Code::InvalidArgument);
    }
}
