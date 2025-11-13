//! Node Embedding Service
//!
//! **STATUS: Database tracking complete, NLP integration pending**
//!
//! Database-level embedding staleness tracking implemented in Issue #481.
//! Full embedding generation will be re-enabled in a future issue after
//! NLP engine integration work is complete.
//!
//! **Future Work (separate issue):**
//! - Update constructor to take Arc<SurrealStore>
//! - Migrate all database operations to SurrealStore methods
//! - Implement SurrealDB-native vector search
//! - Test embedding generation and search functionality

use crate::models::Node;
use crate::services::error::NodeServiceError;
use nodespace_nlp_engine::EmbeddingService;
use std::sync::Arc;

/// Embedding vector dimension for BAAI/bge-small-en-v1.5 model
pub const EMBEDDING_DIMENSION: usize = 384;

/// Node embedding service (temporarily disabled)
pub struct NodeEmbeddingService {
    /// NLP engine for generating embeddings
    #[allow(dead_code)]
    nlp_engine: Arc<EmbeddingService>,
}

impl NodeEmbeddingService {
    /// Create a new NodeEmbeddingService (temporarily returns stub)
    ///
    /// Database-level embedding staleness tracking is now implemented.
    /// Full embedding generation will be re-enabled in a future issue.
    pub fn new(nlp_engine: Arc<EmbeddingService>) -> Self {
        tracing::info!(
            "NodeEmbeddingService stubbed - embedding generation pending future NLP integration"
        );
        Self { nlp_engine }
    }

    /// Stub: Embed container
    ///
    /// Database tracking ready - full implementation pending future issue
    pub fn embed_container(&self, _node: &Node) -> Result<(), NodeServiceError> {
        tracing::debug!("Embedding generation stub - NLP integration pending");
        Ok(())
    }

    /// Stub: Batch embed containers
    ///
    /// Database tracking ready - full implementation pending future issue
    pub fn batch_embed_containers(&self, _limit: Option<usize>) -> Result<usize, NodeServiceError> {
        tracing::debug!("Batch embedding stub - NLP integration pending");
        Ok(0)
    }
}
