//! tonic `NodeService` implementation backed by `nodespace-core`.
//!
//! Each RPC handler:
//!   1. Parses the proto request into the corresponding `ops` input type.
//!   2. Calls the matching `nodespace_core::ops` function.
//!   3. Converts the result back into proto messages.
//!   4. Maps `OpsError` → `tonic::Status`.
//!
//! `Chat` returns `Unimplemented` — covered by a separate streaming issue.

use std::pin::Pin;
use std::sync::Arc;

use nodespace_core::db::events::DomainEvent;
use nodespace_core::models::Node;
use nodespace_core::ops::{
    node_ops::{self, CreateNodeInput, DeleteNodeInput, UpdateNodeInput},
    search_ops::{self, SearchSemanticInput},
    OpsError,
};
use nodespace_core::services::{NodeEmbeddingService, NodeService as CoreNodeService};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    node_event::Event as NodeEventKind, node_service_server::NodeService as GrpcNodeService,
    ChatRequest, ChatResponse, CreateNodeRequest, DeleteNodeRequest, DeleteNodeResponse,
    GetChildrenRequest, GetNodeRequest, NodeData, NodeDeleted, NodeEvent, NodeListResponse,
    NodeResponse, SearchRequest, UpdateNodeRequest, WatchRequest,
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

        let input = CreateNodeInput {
            node_type: req.node_type,
            content: req.content,
            parent_id: empty_to_none(req.parent_id),
            properties: parse_properties(&req.properties).map_err(properties_error)?,
            collection: empty_to_none(req.collection),
            lifecycle_status: empty_to_none(req.lifecycle_status),
        };

        let output = node_ops::create_node(&self.node_service, input)
            .await
            .map_err(ops_error_to_status)?;

        let node = fetch_node(&self.node_service, &output.node_id).await?;

        Ok(Response::new(NodeResponse {
            node_id: output.node_id,
            node_type: output.node_type,
            parent_id: output.parent_id.clone().unwrap_or_default(),
            collection_id: output.collection_id.clone().unwrap_or_default(),
            node_data: Some(node_to_proto(node, output.parent_id, output.collection_id)),
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

        let input = UpdateNodeInput {
            node_id: req.node_id.clone(),
            version: req.version,
            node_type: empty_to_none(req.node_type),
            content: req.content,
            properties,
            add_to_collection: empty_to_none(req.add_to_collection),
            remove_from_collection: empty_to_none(req.remove_from_collection),
            lifecycle_status: empty_to_none(req.lifecycle_status),
        };

        let output = node_ops::update_node(&self.node_service, input)
            .await
            .map_err(ops_error_to_status)?;

        let node = fetch_node(&self.node_service, &output.node_id).await?;
        let node_type = node.node_type.clone();
        let collection_id = output.collection_added.clone();

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

        let output = node_ops::delete_node(
            &self.node_service,
            DeleteNodeInput {
                node_id: req.node_id,
                version: req.version,
            },
        )
        .await
        .map_err(ops_error_to_status)?;

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
            .map_err(|e| ops_error_to_status(OpsError::from(e)))?;

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

    async fn search_nodes(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<NodeListResponse>, Status> {
        let req = request.into_inner();

        let embedding_service = self.embedding_service.as_ref().ok_or_else(|| {
            Status::unavailable("Embedding service not initialized — semantic search disabled")
        })?;

        // `semantic` field reserved for a future structured-query mode. Until
        // that lands the handler always performs semantic search. Log so
        // clients can observe the discrepancy via tracing.
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

        // Re-fetch raw nodes so we can populate every NodeData field directly
        // from the canonical Node struct rather than re-parsing the typed JSON.
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

fn node_to_proto(node: Node, parent_id: Option<String>, collection_id: Option<String>) -> NodeData {
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
        DomainEvent::RelationshipCreated { .. }
        | DomainEvent::RelationshipUpdated { .. }
        | DomainEvent::RelationshipDeleted { .. } => None,
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
