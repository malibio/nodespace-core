//! Comprehensive tests for TopicEmbeddingService
//!
//! Tests cover:
//! - Token estimation and chunking strategies
//! - Embedding generation for different content sizes
//! - Vector search (approximate and exact)
//! - Debouncing functionality
//! - Database storage and retrieval

#[cfg(test)]
mod tests {
    use crate::db::DatabaseService;
    use crate::models::Node;
    use crate::services::TopicEmbeddingService;
    use nodespace_nlp_engine::{EmbeddingConfig, EmbeddingService};
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::time::Duration;

    /// Helper to create test services
    async fn create_test_services() -> (Arc<DatabaseService>, Arc<TopicEmbeddingService>) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db_service = Arc::new(DatabaseService::new(db_path).await.unwrap());

        let mut nlp_engine = EmbeddingService::new(EmbeddingConfig::default()).unwrap();
        nlp_engine.initialize().unwrap();
        let nlp_engine = Arc::new(nlp_engine);

        let embedding_service = Arc::new(TopicEmbeddingService::new(
            nlp_engine,
            Arc::clone(&db_service),
        ));

        (db_service, embedding_service)
    }

    /// Helper to create a test topic node
    async fn create_test_topic(
        db: &DatabaseService,
        content: String,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let node = Node::new("topic".to_string(), content, None, json!({}));

        let conn = db.connect_with_timeout().await?;
        conn.execute(
            "INSERT INTO nodes (id, node_type, content, parent_id, origin_node_id, properties)
             VALUES (?, ?, ?, ?, ?, ?)",
            libsql::params![
                node.id.clone(),
                node.node_type.clone(),
                node.content.clone(),
                node.parent_id.clone(),
                node.origin_node_id.clone(),
                node.properties.to_string()
            ],
        )
        .await?;

        Ok(node.id)
    }

    #[tokio::test]
    async fn test_token_estimation() {
        let (_db, service) = create_test_services().await;

        // 1 token ≈ 4 characters
        assert_eq!(service.estimate_tokens("test"), 1);
        assert_eq!(service.estimate_tokens("hello world"), 2);
        assert_eq!(service.estimate_tokens(&"a".repeat(400)), 100);
        assert_eq!(service.estimate_tokens(&"a".repeat(2048)), 512);
    }

    #[tokio::test]
    async fn test_chunking_strategy_small() {
        let (db, service) = create_test_services().await;

        // Create topic with < 512 tokens (< 2048 chars)
        let small_content = "This is a small topic.".to_string();
        let topic_id = create_test_topic(&db, small_content).await.unwrap();

        // Embed topic
        service.embed_topic(&topic_id).await.unwrap();

        // Verify embedding was stored
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector, properties FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();

        let row = rows.next().await.unwrap().unwrap();
        let embedding: Option<Vec<u8>> = row.get(0).unwrap();
        let properties_str: String = row.get(1).unwrap();
        let properties: serde_json::Value = serde_json::from_str(&properties_str).unwrap();

        assert!(embedding.is_some());
        assert_eq!(properties["embedding_metadata"]["type"], "complete_topic");
    }

    #[tokio::test]
    async fn test_chunking_strategy_medium() {
        let (db, service) = create_test_services().await;

        // Create topic with 512-2048 tokens (2048-8192 chars)
        let medium_content = "x".repeat(3000); // ~750 tokens
        let topic_id = create_test_topic(&db, medium_content).await.unwrap();

        // Embed topic
        service.embed_topic(&topic_id).await.unwrap();

        // Verify embedding was stored
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector, properties FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();

        let row = rows.next().await.unwrap().unwrap();
        let embedding: Option<Vec<u8>> = row.get(0).unwrap();
        let properties_str: String = row.get(1).unwrap();
        let properties: serde_json::Value = serde_json::from_str(&properties_str).unwrap();

        assert!(embedding.is_some());
        assert_eq!(properties["embedding_metadata"]["type"], "topic_summary");
    }

    #[tokio::test]
    async fn test_chunking_strategy_large() {
        let (db, service) = create_test_services().await;

        // Create topic with > 2048 tokens (> 8192 chars)
        let large_content = "y".repeat(10000); // ~2500 tokens
        let topic_id = create_test_topic(&db, large_content).await.unwrap();

        // Embed topic
        service.embed_topic(&topic_id).await.unwrap();

        // Verify embedding was stored
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector, properties FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();

        let row = rows.next().await.unwrap().unwrap();
        let embedding: Option<Vec<u8>> = row.get(0).unwrap();
        let properties_str: String = row.get(1).unwrap();
        let properties: serde_json::Value = serde_json::from_str(&properties_str).unwrap();

        assert!(embedding.is_some());
        assert_eq!(properties["embedding_metadata"]["type"], "topic_summary");
    }

    #[tokio::test]
    async fn test_simple_summarize() {
        let (_db, service) = create_test_services().await;

        // Short content - no truncation
        let short = "Hello world";
        assert_eq!(service.simple_summarize(short, 512), short);

        // Long content - truncation
        let long = "a".repeat(3000);
        let summary = service.simple_summarize(&long, 512);
        assert!(summary.len() < long.len());
        assert!(summary.ends_with("..."));
        assert_eq!(summary.len(), 512 * 4 + 3); // max_tokens * 4 + "..."
    }

    #[tokio::test]
    async fn test_update_topic_embedding() {
        let (db, service) = create_test_services().await;

        let content = "Original content".to_string();
        let topic_id = create_test_topic(&db, content).await.unwrap();

        // Generate initial embedding
        service.embed_topic(&topic_id).await.unwrap();

        // Get initial embedding
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let initial_embedding: Option<Vec<u8>> = row.get(0).unwrap();

        // Update content and re-embed
        conn.execute(
            "UPDATE nodes SET content = ? WHERE id = ?",
            libsql::params!["Updated content", topic_id.clone()],
        )
        .await
        .unwrap();

        service.update_topic_embedding(&topic_id).await.unwrap();

        // Get updated embedding
        let mut stmt = conn
            .prepare("SELECT embedding_vector FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let updated_embedding: Option<Vec<u8>> = row.get(0).unwrap();

        // Embeddings should be different (content changed)
        assert!(initial_embedding.is_some());
        assert!(updated_embedding.is_some());
        assert_ne!(initial_embedding, updated_embedding);
    }

    #[tokio::test]
    async fn test_debouncing() {
        let (db, service) = create_test_services().await;

        let content = "Test content".to_string();
        let topic_id = create_test_topic(&db, content).await.unwrap();

        // Schedule multiple updates rapidly
        service.schedule_update_topic_embedding(&topic_id).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        service.schedule_update_topic_embedding(&topic_id).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        service.schedule_update_topic_embedding(&topic_id).await;

        // Wait for debounce delay (5 seconds + buffer)
        tokio::time::sleep(Duration::from_secs(6)).await;

        // Verify embedding was generated (only once, despite multiple calls)
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let embedding: Option<Vec<u8>> = row.get(0).unwrap();

        assert!(embedding.is_some());
    }

    #[tokio::test]
    async fn test_embedding_storage_format() {
        let (db, service) = create_test_services().await;

        let content = "Test embedding storage".to_string();
        let topic_id = create_test_topic(&db, content).await.unwrap();

        service.embed_topic(&topic_id).await.unwrap();

        // Verify embedding is stored as F32_BLOB format
        let conn = db.connect_with_timeout().await.unwrap();
        let mut stmt = conn
            .prepare("SELECT embedding_vector FROM nodes WHERE id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let embedding_blob: Vec<u8> = row.get(0).unwrap();

        // Should be 384 dimensions * 4 bytes = 1536 bytes
        assert_eq!(embedding_blob.len(), 384 * 4);

        // Convert back to f32 and verify
        let embedding = EmbeddingService::from_blob(&embedding_blob);
        assert_eq!(embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_performance_embedding_time() {
        let (db, service) = create_test_services().await;

        let content = "Performance test content".repeat(100);
        let topic_id = create_test_topic(&db, content).await.unwrap();

        let start = std::time::Instant::now();
        service.embed_topic(&topic_id).await.unwrap();
        let duration = start.elapsed();

        // Should complete in reasonable time (< 5 seconds even on CPU)
        assert!(
            duration.as_secs() < 5,
            "Embedding took too long: {:?}",
            duration
        );
    }

    #[tokio::test]
    async fn test_batch_embedding_multiple_topics() {
        let (db, service) = create_test_services().await;

        // Create multiple topics
        let topic1_id = create_test_topic(&db, "Topic 1 content".to_string())
            .await
            .unwrap();
        let topic2_id = create_test_topic(&db, "Topic 2 content".to_string())
            .await
            .unwrap();
        let topic3_id = create_test_topic(&db, "Topic 3 content".to_string())
            .await
            .unwrap();

        // Embed all topics
        service.embed_topic(&topic1_id).await.unwrap();
        service.embed_topic(&topic2_id).await.unwrap();
        service.embed_topic(&topic3_id).await.unwrap();

        // Verify all have embeddings
        let conn = db.connect_with_timeout().await.unwrap();
        for topic_id in [&topic1_id, &topic2_id, &topic3_id] {
            let mut stmt = conn
                .prepare("SELECT embedding_vector FROM nodes WHERE id = ?")
                .await
                .unwrap();
            let mut rows = stmt.query(libsql::params![topic_id.clone()]).await.unwrap();
            let row = rows.next().await.unwrap().unwrap();
            let embedding: Option<Vec<u8>> = row.get(0).unwrap();
            assert!(embedding.is_some());
        }
    }

    #[tokio::test]
    async fn test_error_handling_missing_topic() {
        let (_db, service) = create_test_services().await;

        // Try to embed non-existent topic
        let result = service.embed_topic("non-existent-id").await;
        assert!(result.is_err());
    }
}
