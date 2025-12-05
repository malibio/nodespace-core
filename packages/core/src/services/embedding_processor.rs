//! Background Embedding Processor (Issue #729)
//!
//! Provides event-driven background processing of stale root-aggregate embeddings:
//! - Sleeps until woken by a trigger (no polling when idle)
//! - Processes all stale embeddings until queue is empty
//! - Returns to sleep waiting for next trigger
//! - Graceful shutdown support
//!
//! ## Event-Driven Model
//!
//! Instead of polling every N seconds, the processor:
//! 1. Sleeps waiting for a wake signal
//! 2. When woken, processes ALL stale embeddings until none remain
//! 3. Returns to sleep
//!
//! This is more efficient than polling - no CPU/DB overhead when idle.
//!
//! ## Root-Aggregate Model
//!
//! This processor works with the root-aggregate embedding model where:
//! - Only root nodes (no parent) of embeddable types get embedded
//! - Embeddings are stored in the `embedding` table, not on nodes
//! - The `stale` flag tracks which embeddings need regeneration

use crate::services::error::NodeServiceError;
use crate::services::NodeEmbeddingService;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Handle to wake the embedding processor
///
/// This is a lightweight, cloneable handle that can be passed to other services
/// (like `NodeService`) to trigger embedding processing when nodes change.
///
/// Multiple wakes are coalesced - the processor will process all stale embeddings
/// in a single run regardless of how many wake signals were sent.
#[derive(Clone)]
pub struct EmbeddingWaker {
    trigger_tx: mpsc::Sender<()>,
}

impl EmbeddingWaker {
    /// Wake the embedding processor to start processing
    ///
    /// Non-blocking. If the processor is already awake or has pending work,
    /// this is a no-op (signals are coalesced).
    pub fn wake(&self) {
        // Use try_send to avoid blocking - if channel is full, processor is already awake
        match self.trigger_tx.try_send(()) {
            Ok(_) => {
                tracing::debug!("EmbeddingProcessor wake signal sent");
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel full means processor will wake up anyway
                tracing::debug!("EmbeddingProcessor already has pending wake");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("EmbeddingProcessor has shut down, wake ignored");
            }
        }
    }
}

/// Embedding processor for background tasks
///
/// Processes stale embeddings in the background using the root-aggregate model.
/// Event-driven: sleeps until triggered, then processes until queue is empty.
pub struct EmbeddingProcessor {
    #[allow(dead_code)]
    embedding_service: Arc<NodeEmbeddingService>,
    waker: EmbeddingWaker,
    _shutdown_tx: mpsc::Sender<()>,
}

impl EmbeddingProcessor {
    /// Create and start embedding processor with background task
    ///
    /// Spawns an event-driven background task that:
    /// 1. Sleeps until triggered via `wake()` or `trigger_batch_embed()`
    /// 2. Processes ALL stale embeddings until queue is empty
    /// 3. Returns to sleep waiting for next trigger
    ///
    /// ## Event-Driven Model
    ///
    /// Unlike polling, this approach has zero overhead when idle. The processor
    /// only runs when there's actual work to do.
    ///
    /// ## Root-Aggregate Model (Issue #729)
    ///
    /// Uses `process_stale_embeddings()` which:
    /// 1. Queries the `embedding` table for stale entries
    /// 2. For each stale root node, aggregates its subtree content
    /// 3. Generates new embeddings and updates the table
    ///
    /// # Arguments
    /// * `embedding_service` - The embedding service for processing nodes
    ///
    /// # Returns
    /// A new EmbeddingProcessor instance with active background task
    pub fn new(embedding_service: Arc<NodeEmbeddingService>) -> Result<Self, NodeServiceError> {
        tracing::info!("EmbeddingProcessor initializing (event-driven model)");

        let (trigger_tx, mut trigger_rx) = mpsc::channel::<()>(10);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Spawn event-driven background task
        let service_clone = embedding_service.clone();
        tokio::spawn(async move {
            loop {
                // Sleep until triggered or shutdown
                tokio::select! {
                    biased; // Check shutdown first

                    _ = shutdown_rx.recv() => {
                        tracing::info!("EmbeddingProcessor shutting down");
                        break;
                    }

                    Some(_) = trigger_rx.recv() => {
                        tracing::debug!("EmbeddingProcessor woken up");
                        // Drain any additional pending triggers (coalesce rapid triggers)
                        while trigger_rx.try_recv().is_ok() {}

                        // Process until no more stale embeddings
                        Self::process_until_empty(&service_clone).await;
                    }
                }
            }
        });

        let waker = EmbeddingWaker { trigger_tx };

        Ok(Self {
            embedding_service,
            waker,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// Get a cloneable waker handle
    ///
    /// Use this to pass to other services (like `NodeService`) so they can
    /// wake the processor when embedding work is queued.
    pub fn waker(&self) -> EmbeddingWaker {
        self.waker.clone()
    }

    /// Process all stale embeddings until none remain
    ///
    /// Keeps processing batches until `process_stale_embeddings` returns 0.
    /// This ensures the queue is fully drained before returning to sleep.
    async fn process_until_empty(service: &Arc<NodeEmbeddingService>) {
        let mut total_processed = 0;

        loop {
            match service.process_stale_embeddings(Some(10)).await {
                Ok(0) => {
                    // No more stale embeddings - done
                    if total_processed > 0 {
                        tracing::info!(
                            "EmbeddingProcessor finished - processed {} total embeddings",
                            total_processed
                        );
                    } else {
                        tracing::debug!("EmbeddingProcessor woke but no stale embeddings found");
                    }
                    break;
                }
                Ok(count) => {
                    total_processed += count;
                    tracing::debug!(
                        "Processed {} embeddings (total: {})",
                        count,
                        total_processed
                    );
                    // Continue processing next batch
                }
                Err(e) => {
                    tracing::error!(
                        "Embedding processing failed after {} embeddings: {}",
                        total_processed,
                        e
                    );
                    // Stop processing on error - will retry on next wake
                    break;
                }
            }
        }
    }

    /// Wake the processor to start processing stale embeddings
    ///
    /// This is the primary way to trigger embedding processing. Call this
    /// after creating stale markers (e.g., after node create/update/delete).
    ///
    /// The wake signal is coalesced - multiple rapid wakes result in a single
    /// processing run that drains all stale embeddings.
    pub fn wake(&self) {
        self.waker.wake();
    }

    /// Trigger batch embedding immediately (alias for wake)
    ///
    /// Useful for explicit user actions like "Sync All" button or app startup.
    #[inline]
    pub fn trigger_batch_embed(&self) -> Result<(), NodeServiceError> {
        self.waker.wake();
        Ok(())
    }

    /// Shutdown processor gracefully
    ///
    /// Sends shutdown signal to background task. The task will complete
    /// any in-progress operations and exit cleanly.
    pub fn shutdown(self) -> Result<(), NodeServiceError> {
        tracing::info!("Shutting down EmbeddingProcessor");
        // Channels will be dropped, signaling shutdown
        Ok(())
    }
}
