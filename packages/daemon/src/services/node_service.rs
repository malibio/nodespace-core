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
    search_ops::{self, SearchSemanticInput},
    OpsError,
};
use nodespace_core::services::{
    CollectionService, CreateNodeParams, NodeEmbeddingService, NodeService as CoreNodeService,
    NodeServiceError,
};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    node_event::Event as NodeEventKind, node_service_server::NodeService as GrpcNodeService,
    AddNodeToCollectionByPathRequest, AddNodeToCollectionRequest, ChatRequest, ChatResponse,
    CollectionIdResponse, CollectionIdsResponse, CollectionInfo, CollectionListResponse,
    CollectionMembersRequest, CreateCollectionRequest, CreateMentionRequest, CreateNodeRequest,
    DeleteCollectionRequest, DeleteMentionRequest, DeleteNodeRequest, DeleteNodeResponse, Empty,
    FindCollectionByPathRequest, GetAllCollectionsRequest, GetAllSchemasRequest,
    GetChildrenRequest, GetChildrenTreeRequest, GetCollectionByNameRequest, GetNodeRequest,
    GetRootsRequest, GetSchemaDefinitionRequest, MentionAutocompleteRequest, MentionIdsResponse,
    MentionResponse, MentionTargetRequest, MoveNodeRequest, NodeCollectionsRequest, NodeData,
    NodeDeleted, NodeEvent, NodeListResponse, NodeReference, NodeReferenceListResponse,
    NodeResponse, NodeTreeResponse, OptionalNodeResponse, OptionalStringClear,
    OptionalTimestampClear, QueryNodesSimpleRequest, RelationshipDeletedPayload,
    RelationshipPayload, RemoveNodeFromCollectionRequest, RenameCollectionRequest,
    ReorderNodeRequest, ReorderNodeResponse, SearchRequest, UpdateNodeRequest,
    UpdateTaskNodeRequest, UpsertNodeWithParentRequest, WatchRequest,
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
        let collection_path = empty_to_none(req.collection);
        let parent_id_opt = empty_to_none(req.parent_id);

        let node_id = self
            .node_service
            .create_node_with_parent(CreateNodeParams {
                id: empty_to_none(req.id),
                node_type: req.node_type,
                content: req.content,
                parent_id: parent_id_opt.clone(),
                insert_after_node_id: empty_to_none(req.insert_after_node_id),
                properties,
            })
            .await
            .map_err(service_error_to_status)?;

        // Add to collection if a path was supplied (mirrors node_ops::create_node).
        let collection_id = if let Some(path) = &collection_path {
            let store = self.node_service.store();
            let collection_service = CollectionService::new(store, &self.node_service);
            let resolved = collection_service
                .add_to_collection_by_path(&node_id, path)
                .await
                .map_err(service_error_to_status)?;
            Some(resolved.leaf_id().to_string())
        } else {
            None
        };

        // Optionally update lifecycle status to non-default value.
        if let Some(status) = empty_to_none(req.lifecycle_status) {
            if status != "active" {
                let update = NodeUpdate {
                    lifecycle_status: Some(status),
                    ..Default::default()
                };
                // Auto-fetch version: use -1 sentinel? Use update_node with current node version.
                let current = fetch_node(&self.node_service, &node_id).await?;
                self.node_service
                    .update_node(&node_id, current.version, update)
                    .await
                    .map_err(service_error_to_status)?;
            }
        }

        let node = fetch_node(&self.node_service, &node_id).await?;
        let node_type = node.node_type.clone();
        let parent_id = parent_id_opt.unwrap_or_default();

        Ok(Response::new(NodeResponse {
            node_id: node_id.clone(),
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

        let update = NodeUpdate {
            node_type: empty_to_none(req.node_type),
            content: req.content,
            properties,
            title: None,
            lifecycle_status: empty_to_none(req.lifecycle_status),
        };

        // Auto-fetch version if not supplied (matches the prior ops-layer behaviour).
        let version = match req.version {
            Some(v) => v,
            None => fetch_node(&self.node_service, &req.node_id).await?.version,
        };

        let node = self
            .node_service
            .update_node(&req.node_id, version, update)
            .await
            .map_err(service_error_to_status)?;

        // add_to_collection / remove_from_collection are handled out-of-band so we
        // preserve the original ops-layer behaviour without forcing the update
        // path through node_ops (which would silently drop several fields).
        let mut collection_id: Option<String> = None;
        if let Some(path) = empty_to_none(req.add_to_collection) {
            let store = self.node_service.store();
            let collection_service = CollectionService::new(store, &self.node_service);
            let resolved = collection_service
                .add_to_collection_by_path(&req.node_id, &path)
                .await
                .map_err(service_error_to_status)?;
            collection_id = Some(resolved.leaf_id().to_string());
        }
        if let Some(cid) = empty_to_none(req.remove_from_collection) {
            let store = self.node_service.store();
            let collection_service = CollectionService::new(store, &self.node_service);
            collection_service
                .remove_from_collection(&req.node_id, &cid)
                .await
                .map_err(service_error_to_status)?;
        }

        let node_type = node.node_type.clone();
        Ok(Response::new(NodeResponse {
            node_id: node.id.clone(),
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

        let version = match req.version {
            Some(v) => v,
            None => {
                // Auto-fetch behaviour: if node missing, return existed=false idempotently.
                match self
                    .node_service
                    .get_node(&req.node_id)
                    .await
                    .map_err(service_error_to_status)?
                {
                    Some(n) => n.version,
                    None => {
                        return Ok(Response::new(DeleteNodeResponse {
                            node_id: req.node_id,
                            existed: false,
                        }))
                    }
                }
            }
        };

        let result = self
            .node_service
            .delete_node(&req.node_id, version)
            .await
            .map_err(service_error_to_status)?;

        Ok(Response::new(DeleteNodeResponse {
            node_id: req.node_id,
            existed: result.existed,
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
            collection_id: empty_to_none(req.collection_id.clone()),
            collection: empty_to_none(req.collection),
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

        let new_parent = empty_to_none(req.new_parent_id);
        let insert_after = empty_to_none(req.insert_after_node_id);

        let node = self
            .node_service
            .move_node(
                &req.node_id,
                req.version,
                new_parent.as_deref(),
                insert_after.as_deref(),
            )
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
        let insert_after = empty_to_none(req.insert_after_node_id);

        self.node_service
            .reorder_node(&req.node_id, req.version, insert_after.as_deref())
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
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let entries = collection_service
            .get_all_collections_with_counts()
            .await
            .map_err(service_error_to_status)?;

        let collections = entries
            .into_iter()
            .map(
                |(node, member_count, parent_collection_ids)| CollectionInfo {
                    node: Some(node_to_proto(node, None, None)),
                    member_count: member_count as u32,
                    parent_collection_ids,
                },
            )
            .collect();

        Ok(Response::new(CollectionListResponse { collections }))
    }

    async fn get_collection_members(
        &self,
        request: Request<CollectionMembersRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let members = collection_service
            .get_collection_members(&req.collection_id)
            .await
            .map_err(service_error_to_status)?;

        let collection_id = req.collection_id.clone();
        let nodes: Vec<NodeData> = members
            .into_iter()
            .map(|n| node_to_proto(n, None, Some(collection_id.clone())))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: req.collection_id,
        }))
    }

    async fn get_collection_members_recursive(
        &self,
        request: Request<CollectionMembersRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);

        let member_ids = collection_service
            .get_collection_members_recursive(&req.collection_id)
            .await
            .map_err(service_error_to_status)?;

        let nodes_map = store
            .get_nodes_by_ids(&member_ids)
            .await
            .map_err(|e| Status::internal(format!("Failed to batch fetch nodes: {}", e)))?;

        // Preserve ordering from member_ids; filter out missing entries.
        let collection_id = req.collection_id.clone();
        let nodes: Vec<NodeData> = member_ids
            .into_iter()
            .filter_map(|id| nodes_map.get(&id).cloned())
            .map(|n| node_to_proto(n, None, Some(collection_id.clone())))
            .collect();
        let count = nodes.len() as i32;

        Ok(Response::new(NodeListResponse {
            nodes,
            count,
            collection_id: req.collection_id,
        }))
    }

    async fn get_node_collections(
        &self,
        request: Request<NodeCollectionsRequest>,
    ) -> Result<Response<CollectionIdsResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let ids = collection_service
            .get_node_collections(&req.node_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(CollectionIdsResponse {
            collection_ids: ids,
        }))
    }

    async fn add_node_to_collection(
        &self,
        request: Request<AddNodeToCollectionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        collection_service
            .add_to_collection(&req.node_id, &req.collection_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(Empty {}))
    }

    async fn add_node_to_collection_by_path(
        &self,
        request: Request<AddNodeToCollectionByPathRequest>,
    ) -> Result<Response<CollectionIdResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let resolved = collection_service
            .add_to_collection_by_path(&req.node_id, &req.collection_path)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(CollectionIdResponse {
            collection_id: resolved.leaf_id().to_string(),
        }))
    }

    async fn remove_node_from_collection(
        &self,
        request: Request<RemoveNodeFromCollectionRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        collection_service
            .remove_from_collection(&req.node_id, &req.collection_id)
            .await
            .map_err(service_error_to_status)?;
        Ok(Response::new(Empty {}))
    }

    async fn find_collection_by_path(
        &self,
        request: Request<FindCollectionByPathRequest>,
    ) -> Result<Response<OptionalNodeResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let result = collection_service
            .find_collection_by_path(&req.collection_path)
            .await
            .map_err(service_error_to_status)?;

        let node_response = result.map(|n| {
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
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);
        let result = collection_service
            .get_collection_by_name(&req.name)
            .await
            .map_err(service_error_to_status)?;

        let node_response = result.map(|n| {
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
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);

        // Reject duplicate names (matches Tauri command pre-check).
        if collection_service
            .get_collection_by_name(&req.name)
            .await
            .map_err(service_error_to_status)?
            .is_some()
        {
            return Err(Status::already_exists(format!(
                "Collection '{}' already exists",
                req.name
            )));
        }

        let properties = if req.description.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "description": req.description })
        };

        let collection_id = self
            .node_service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "collection".to_string(),
                content: req.name,
                parent_id: None,
                insert_after_node_id: None,
                properties,
            })
            .await
            .map_err(service_error_to_status)?;

        Ok(Response::new(CollectionIdResponse { collection_id }))
    }

    async fn rename_collection(
        &self,
        request: Request<RenameCollectionRequest>,
    ) -> Result<Response<NodeResponse>, Status> {
        let req = request.into_inner();
        let store = self.node_service.store();
        let collection_service = CollectionService::new(store, &self.node_service);

        // Reject if another collection already uses this name.
        if let Some(existing) = collection_service
            .get_collection_by_name(&req.new_name)
            .await
            .map_err(service_error_to_status)?
        {
            if existing.id != req.collection_id {
                return Err(Status::already_exists(format!(
                    "Collection '{}' already exists",
                    req.new_name
                )));
            }
        }

        let update = NodeUpdate {
            content: Some(req.new_name),
            ..Default::default()
        };

        let node = self
            .node_service
            .update_node(&req.collection_id, req.version, update)
            .await
            .map_err(service_error_to_status)?;

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
                        // `Lagged`. Acceptable because lookups are RocksDB
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

fn empty_to_none(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

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
        .map_err(|e| Status::internal(format!("get_node failed: {}", e)))?
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
        OpsError::VersionConflict {
            node_id,
            expected,
            actual,
            ..
        } => Status::aborted(format!(
            "Version conflict on {}: expected {}, got {}",
            node_id, expected, actual
        )),
        OpsError::ValidationFailed(msg) => {
            Status::failed_precondition(format!("Validation failed: {}", msg))
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
