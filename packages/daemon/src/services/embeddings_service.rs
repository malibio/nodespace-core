//! tonic `EmbeddingsService` implementation backed by `nodespace-core`.
//!
//! Wraps `NodeEmbeddingService` and `EmbeddingProcessor`. The GPU drain
//! protocol (`release_gpu_context`) is handled on daemon shutdown, not by
//! any individual RPC caller.

use std::sync::Arc;

use nodespace_core::models::EmbeddingConfig;
use nodespace_core::services::{EmbeddingProcessor, NodeEmbeddingService, NodeService};
use tonic::{Request, Response, Status};

use crate::nodespace::{
    embeddings_service_server::EmbeddingsService as GrpcEmbeddingsService, BatchEmbeddingFailure,
    BatchQueueEmbeddingsRequest, BatchQueueEmbeddingsResponse, EmbeddingStatusResponse,
    GetEmbeddingStatusRequest, GetStaleCountRequest, GetStaleCountResponse, QueueEmbeddingRequest,
    QueueEmbeddingResponse, RegenerateEmbeddingRequest, RegenerateEmbeddingResponse,
    SearchSemanticRequest, SearchSemanticResponse, TriggerBatchEmbedRequest,
    TriggerBatchEmbedResponse,
};
use crate::services::node_service::node_to_proto;

pub struct EmbeddingsServiceImpl {
    node_service: Arc<NodeService>,
    embedding_service: Arc<NodeEmbeddingService>,
    processor: Arc<EmbeddingProcessor>,
}

impl EmbeddingsServiceImpl {
    pub fn new(
        node_service: Arc<NodeService>,
        embedding_service: Arc<NodeEmbeddingService>,
        processor: Arc<EmbeddingProcessor>,
    ) -> Self {
        Self {
            node_service,
            embedding_service,
            processor,
        }
    }

    /// Shared implementation for stale-count queries used by both
    /// `get_embedding_status` and `get_stale_count` to avoid duplication.
    async fn stale_count_inner(&self) -> Result<i32, Status> {
        let ids = self
            .node_service
            .store()
            .get_stale_embedding_root_ids(None, 0, EmbeddingConfig::default().max_retries)
            .await
            .map_err(|e| Status::internal(format!("Failed to get stale count: {}", e)))?;
        Ok(i32::try_from(ids.len()).unwrap_or(i32::MAX))
    }
}

#[tonic::async_trait]
impl GrpcEmbeddingsService for EmbeddingsServiceImpl {
    async fn get_embedding_status(
        &self,
        _request: Request<GetEmbeddingStatusRequest>,
    ) -> Result<Response<EmbeddingStatusResponse>, Status> {
        let stale_count = self.stale_count_inner().await?;
        Ok(Response::new(EmbeddingStatusResponse {
            available: true,
            stale_count,
        }))
    }

    async fn search_semantic(
        &self,
        request: Request<SearchSemanticRequest>,
    ) -> Result<Response<SearchSemanticResponse>, Status> {
        let req = request.into_inner();

        if req.query.trim().is_empty() {
            return Err(Status::invalid_argument("query cannot be empty"));
        }

        let threshold = if req.threshold == 0.0 {
            None
        } else {
            Some(req.threshold as f64)
        };
        let limit = if req.limit == 0 {
            20i64
        } else {
            req.limit as i64
        };

        let query_embedding = self
            .embedding_service
            .nlp_engine()
            .generate_embedding(&req.query)
            .map_err(|e| Status::internal(format!("Failed to generate query embedding: {}", e)))?;

        let store = self.node_service.store();
        let search_results = store
            .search_embeddings(&query_embedding, limit, threshold)
            .await
            .map_err(|e| Status::internal(format!("Vector search failed: {}", e)))?;

        let mut nodes = Vec::with_capacity(search_results.len());
        for result in search_results {
            if let Ok(Some(node)) = store.get_node(&result.node_id).await {
                nodes.push(node_to_proto(node, None, None));
            }
        }

        Ok(Response::new(SearchSemanticResponse { nodes }))
    }

    async fn regenerate_embedding(
        &self,
        request: Request<RegenerateEmbeddingRequest>,
    ) -> Result<Response<RegenerateEmbeddingResponse>, Status> {
        let req = request.into_inner();

        let node = self
            .node_service
            .get_node(&req.node_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get node: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Node not found: {}", req.node_id)))?;

        self.embedding_service
            .queue_for_embedding(&node.id)
            .await
            .map_err(|e| Status::internal(format!("Failed to queue embedding: {}", e)))?;

        Ok(Response::new(RegenerateEmbeddingResponse {}))
    }

    async fn queue_embedding(
        &self,
        request: Request<QueueEmbeddingRequest>,
    ) -> Result<Response<QueueEmbeddingResponse>, Status> {
        let req = request.into_inner();

        let node = self
            .node_service
            .get_node(&req.node_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get node: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Node not found: {}", req.node_id)))?;

        self.embedding_service
            .queue_for_embedding(&node.id)
            .await
            .map_err(|e| Status::internal(format!("Failed to queue embedding: {}", e)))?;

        Ok(Response::new(QueueEmbeddingResponse {}))
    }

    async fn trigger_batch_embed(
        &self,
        _request: Request<TriggerBatchEmbedRequest>,
    ) -> Result<Response<TriggerBatchEmbedResponse>, Status> {
        self.processor
            .trigger_batch_embed()
            .map_err(|e| Status::internal(format!("Failed to trigger batch embed: {}", e)))?;

        Ok(Response::new(TriggerBatchEmbedResponse {}))
    }

    async fn get_stale_count(
        &self,
        _request: Request<GetStaleCountRequest>,
    ) -> Result<Response<GetStaleCountResponse>, Status> {
        let count = self.stale_count_inner().await?;
        Ok(Response::new(GetStaleCountResponse { count }))
    }

    async fn batch_queue_embeddings(
        &self,
        request: Request<BatchQueueEmbeddingsRequest>,
    ) -> Result<Response<BatchQueueEmbeddingsResponse>, Status> {
        let req = request.into_inner();
        let mut success_count = 0i32;
        let mut failures = Vec::new();

        for node_id in req.node_ids {
            match self.node_service.get_node(&node_id).await {
                Ok(Some(node)) => {
                    match self.embedding_service.queue_for_embedding(&node.id).await {
                        Ok(_) => success_count += 1,
                        Err(e) => failures.push(BatchEmbeddingFailure {
                            node_id: node_id.clone(),
                            error: format!("Failed to queue embedding: {}", e),
                        }),
                    }
                }
                Ok(None) => failures.push(BatchEmbeddingFailure {
                    node_id: node_id.clone(),
                    error: "Node not found".to_string(),
                }),
                Err(e) => failures.push(BatchEmbeddingFailure {
                    node_id: node_id.clone(),
                    error: format!("Failed to get node: {}", e),
                }),
            }
        }

        Ok(Response::new(BatchQueueEmbeddingsResponse {
            success_count,
            failures,
        }))
    }
}
