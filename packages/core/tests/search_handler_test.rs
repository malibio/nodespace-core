//! Integration tests for MCP Search Handlers
//!
//! Tests cover:
//! - handle_search_semantic function
//! - Collection filtering
//! - Error handling
//! - Parameter validation

use anyhow::Result;
use nodespace_core::{
    db::SurrealStore,
    mcp::handlers::search::{handle_search_semantic, SearchSemanticParams},
    services::{embedding_service::NodeEmbeddingService, NodeService},
};
use nodespace_nlp_engine::{EmbeddingConfig as NlpConfig, EmbeddingService};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Test helper: Create a test NLP engine (uninitialized for testing)
fn create_test_nlp_engine() -> Arc<EmbeddingService> {
    let config = NlpConfig::default();
    Arc::new(EmbeddingService::new(config).unwrap())
}

/// Test helper: Create a unified test environment with shared database
async fn create_test_services() -> Result<(
    Arc<NodeEmbeddingService>,
    Arc<NodeService>,
    Arc<SurrealStore>,
    TempDir,
)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let mut store = Arc::new(SurrealStore::new(db_path).await?);

    let node_service = Arc::new(NodeService::new(&mut store).await?);
    let nlp_engine = create_test_nlp_engine();
    let node_accessor: Arc<dyn nodespace_core::services::NodeAccessor> = node_service.clone();
    let behaviors = node_service.behaviors().clone();
    let embedding_service = Arc::new(NodeEmbeddingService::new(
        nlp_engine,
        store.clone(),
        node_accessor,
        behaviors,
    ));

    Ok((embedding_service, node_service, store, temp_dir))
}

// =========================================================================
// Parameter Parsing Integration Tests
// =========================================================================

#[test]
fn test_search_params_parse_minimal() {
    let params = json!({
        "query": "test query"
    });

    let parsed: SearchSemanticParams = serde_json::from_value(params).unwrap();
    assert_eq!(parsed.query, "test query");
    assert_eq!(parsed.threshold, None);
    assert_eq!(parsed.limit, None);
    assert_eq!(parsed.collection_id, None);
    assert_eq!(parsed.collection, None);
}

#[test]
fn test_search_params_parse_full() {
    let params = json!({
        "query": "machine learning",
        "threshold": 0.8,
        "limit": 50,
        "collection_id": "coll-123",
        "collection": "ai:research"
    });

    let parsed: SearchSemanticParams = serde_json::from_value(params).unwrap();
    assert_eq!(parsed.query, "machine learning");
    assert_eq!(parsed.threshold, Some(0.8));
    assert_eq!(parsed.limit, Some(50));
    assert_eq!(parsed.collection_id, Some("coll-123".to_string()));
    assert_eq!(parsed.collection, Some("ai:research".to_string()));
}

#[test]
fn test_search_params_missing_query_fails() {
    let params = json!({
        "threshold": 0.8
    });

    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err());
}

// =========================================================================
// Handler Validation Tests
// =========================================================================

#[tokio::test]
async fn test_search_rejects_empty_query() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": ""
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("empty"));
    Ok(())
}

#[tokio::test]
async fn test_search_rejects_whitespace_query() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "   "
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("empty") || err.message.contains("whitespace"));
    Ok(())
}

#[tokio::test]
async fn test_search_rejects_threshold_below_zero() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "threshold": -0.1
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("threshold"));
    Ok(())
}

#[tokio::test]
async fn test_search_rejects_threshold_above_one() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "threshold": 1.5
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("threshold"));
    Ok(())
}

#[tokio::test]
async fn test_search_rejects_limit_exceeds_max() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "limit": 5000
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("limit") || err.message.contains("1000"));
    Ok(())
}

#[tokio::test]
async fn test_search_accepts_boundary_threshold_zero() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "threshold": 0.0
    });

    // Should not fail on validation, may fail on actual search (no embeddings)
    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    // Either succeeds with empty results or fails with embedding-related error, not validation
    if let Err(e) = result {
        assert!(!e.message.contains("threshold"));
    }
    Ok(())
}

#[tokio::test]
async fn test_search_accepts_boundary_threshold_one() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "threshold": 1.0
    });

    // Should not fail on validation
    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    if let Err(e) = result {
        assert!(!e.message.contains("threshold"));
    }
    Ok(())
}

#[tokio::test]
async fn test_search_accepts_max_limit() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "valid query",
        "limit": 1000
    });

    // Should not fail on validation
    let result = handle_search_semantic(&node_service, &embedding_service, params).await;
    if let Err(e) = result {
        assert!(!e.message.contains("limit") && !e.message.contains("1000"));
    }
    Ok(())
}

// =========================================================================
// Collection Path Resolution Tests
// =========================================================================

#[tokio::test]
async fn test_search_with_collection_id_filter() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    // Using collection_id instead of collection path
    let params = json!({
        "query": "valid query",
        "collection_id": "some-collection-id"
    });

    let result = handle_search_semantic(&node_service, &embedding_service, params).await;

    // The handler will attempt collection filtering
    // Either succeeds with empty results or fails gracefully
    match result {
        Ok(response) => {
            // If successful, results may be empty (no matching nodes in collection)
            assert!(response.get("nodes").is_some());
            assert!(response.get("count").is_some());
        }
        Err(e) => {
            // If error, it's related to search/embeddings, not validation
            // Collection ID filtering just filters results, doesn't fail
            let msg = e.message.to_lowercase();
            assert!(
                !msg.contains("invalid parameters"),
                "Should not be validation error: {}",
                e.message
            );
        }
    }
    Ok(())
}

// =========================================================================
// Response Structure Tests
// =========================================================================

#[tokio::test]
async fn test_search_response_has_required_fields() -> Result<()> {
    let (embedding_service, node_service, _store, _temp_dir) = create_test_services().await?;

    let params = json!({
        "query": "test query"
    });

    // This may fail with "no embeddings" but let's test if it at least tries
    let result = handle_search_semantic(&node_service, &embedding_service, params).await;

    // If the service isn't initialized, it may return an error, but the error
    // should be about embeddings, not structure
    if let Ok(response) = result {
        assert!(response.get("nodes").is_some());
        assert!(response.get("count").is_some());
        assert!(response.get("query").is_some());
        assert!(response.get("threshold").is_some());
    }
    Ok(())
}

// =========================================================================
// Default Value Tests
// =========================================================================

#[test]
fn test_search_defaults_applied_correctly() {
    let params = json!({"query": "test"});
    let parsed: SearchSemanticParams = serde_json::from_value(params).unwrap();

    // Apply defaults as the handler does
    let threshold = parsed.threshold.unwrap_or(0.7);
    let limit = parsed.limit.unwrap_or(20);

    assert_eq!(threshold, 0.7, "Default threshold should be 0.7");
    assert_eq!(limit, 20, "Default limit should be 20");
}

// =========================================================================
// HNSW Vector Search Integration Tests
// =========================================================================

/// Test that the HNSW vector search query executes without error against a real database.
/// This catches SurrealQL syntax regressions (e.g., wrong KNN operator for HNSW vs MTREE).
#[tokio::test]
async fn test_search_embeddings_hnsw_query_executes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    // Create a node so we have something to embed
    use nodespace_core::models::Node;
    let node = store
        .create_node(
            Node::new(
                "text".to_string(),
                "The quick brown fox jumps over the lazy dog".to_string(),
                serde_json::json!({}),
            ),
            None,
            None,
        )
        .await?;

    // Insert a synthetic 768-dim embedding directly (bypasses NLP engine)
    use nodespace_core::models::NewEmbedding;
    let vector: Vec<f32> = (0..768).map(|i| (i as f32) / 768.0).collect();
    store
        .upsert_embeddings(
            &node.id,
            vec![NewEmbedding {
                node_id: node.id.clone(),
                vector: vector.clone(),
                model_name: Some("test-model".to_string()),
                chunk_index: 0,
                chunk_start: 0,
                chunk_end: 42,
                total_chunks: 1,
                content_hash: "abc123".to_string(),
                token_count: 10,
            }],
        )
        .await?;

    // Query with the same vector — must succeed (no error) and return the node
    let results = store
        .search_embeddings(&vector, 5, Some(0.0))
        .await
        .expect("HNSW vector search query must not fail");

    assert_eq!(results.len(), 1, "Should find the embedded node");
    assert_eq!(results[0].node_id, node.id);

    Ok(())
}

/// Test that search_embeddings returns empty results (not an error) when no embeddings exist.
#[tokio::test]
async fn test_search_embeddings_empty_db_returns_empty() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    let vector: Vec<f32> = vec![0.0f32; 768];
    let results = store
        .search_embeddings(&vector, 5, Some(0.0))
        .await
        .expect("Search on empty database must not fail");

    assert!(
        results.is_empty(),
        "Empty database should return no results"
    );
    Ok(())
}

// =========================================================================
// search_embeddings_by_node_type (Issue #1130 — typed linear-scan search)
// =========================================================================
//
// Test helpers shared across the typed-scan tests. Each test seeds nodes of
// different types with synthetic embeddings, then asserts that the typed scan
// filters correctly via the `node.node_type = $node_type` record-link
// traversal — the SurrealQL pattern is new in this PR and these tests are
// what prevents a silent regression where an empty result is indistinguishable
// from a correct "no matches" outcome.

/// Insert a node with a synthetic 768-dim embedding directly (bypasses NLP engine).
async fn seed_node_with_embedding(
    store: &Arc<SurrealStore>,
    node_type: &str,
    content: &str,
    vector: Vec<f32>,
) -> Result<String> {
    use nodespace_core::models::{NewEmbedding, Node};

    let node = store
        .create_node(
            Node::new(node_type.to_string(), content.to_string(), json!({})),
            None,
            None,
        )
        .await?;

    store
        .upsert_embeddings(
            &node.id,
            vec![NewEmbedding {
                node_id: node.id.clone(),
                vector,
                model_name: Some("test-model".to_string()),
                chunk_index: 0,
                chunk_start: 0,
                chunk_end: content.len() as i32,
                total_chunks: 1,
                content_hash: format!("hash-{}", node.id),
                token_count: 10,
            }],
        )
        .await?;

    Ok(node.id)
}

/// Confident match: a skill node and a text node both share the query vector
/// exactly, but the typed scan must only return the skill node — proving the
/// `node.node_type = $node_type` filter actually works against the
/// record-link traversal in SurrealQL.
#[tokio::test]
async fn test_search_embeddings_by_node_type_filters_to_requested_type() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    // Same vector for both nodes so similarity is identical — only the type
    // filter can distinguish them.
    let vector: Vec<f32> = (0..768).map(|i| (i as f32) / 768.0).collect();

    let skill_id =
        seed_node_with_embedding(&store, "skill", "Node Creation", vector.clone()).await?;
    let _text_id =
        seed_node_with_embedding(&store, "text", "Random note content", vector.clone()).await?;

    let results = store
        .search_embeddings_by_node_type(&vector, "skill", 5, Some(0.0))
        .await
        .expect("typed scan must not fail");

    assert_eq!(
        results.len(),
        1,
        "expected exactly the skill node, got {} results",
        results.len()
    );
    assert_eq!(results[0].node_id, skill_id);
    assert_eq!(
        results[0].node.as_ref().map(|n| n.node_type.as_str()),
        Some("skill"),
        "returned node must be of type 'skill'"
    );
    Ok(())
}

/// Empty result when no embeddings exist for the requested type.
/// Critical because empty results are the "no skill applies" signal exposed
/// to the model — a query that silently errored out would be much worse than
/// one that returns the correct empty set.
#[tokio::test]
async fn test_search_embeddings_by_node_type_returns_empty_for_unknown_type() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    // Seed a text node, but query for skills — expect a clean empty result.
    let vector: Vec<f32> = (0..768).map(|i| (i as f32) / 768.0).collect();
    let _text_id = seed_node_with_embedding(&store, "text", "Just a note", vector.clone()).await?;

    let results = store
        .search_embeddings_by_node_type(&vector, "skill", 5, Some(0.0))
        .await
        .expect("typed scan must not fail on empty result set");

    assert!(
        results.is_empty(),
        "querying for a node_type with no embeddings should return empty, got {:?}",
        results.iter().map(|r| &r.node_id).collect::<Vec<_>>()
    );
    Ok(())
}

/// Empty database — no nodes of any type, no embeddings. Should be a clean
/// empty result (not an error), since `find_skills` may be called on a fresh
/// workspace before any skills are seeded.
#[tokio::test]
async fn test_search_embeddings_by_node_type_empty_db_returns_empty() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    let vector: Vec<f32> = vec![0.0f32; 768];
    let results = store
        .search_embeddings_by_node_type(&vector, "skill", 5, Some(0.0))
        .await
        .expect("typed scan on empty database must not fail");

    assert!(results.is_empty(), "empty DB should return no results");
    Ok(())
}

/// Ranking + limit: two skill nodes with different distances from the query —
/// the higher-similarity one must come first, and `limit=1` truncates to just
/// the top match. Exercises the ORDER BY composite_score DESC + LIMIT clause
/// that the local agent depends on for the default `limit=3` behaviour.
#[tokio::test]
async fn test_search_embeddings_by_node_type_orders_by_similarity_and_respects_limit() -> Result<()>
{
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let store = Arc::new(SurrealStore::new(db_path).await?);

    // Build two distinct vectors: `near` is the query, `far` is orthogonal-ish.
    let near: Vec<f32> = (0..768).map(|i| (i as f32) / 768.0).collect();
    let far: Vec<f32> = (0..768).map(|i| ((767 - i) as f32) / 768.0).collect();

    let near_skill_id =
        seed_node_with_embedding(&store, "skill", "Near skill", near.clone()).await?;
    let _far_skill_id = seed_node_with_embedding(&store, "skill", "Far skill", far.clone()).await?;

    // limit=1 → only the top match comes back
    let top = store
        .search_embeddings_by_node_type(&near, "skill", 1, Some(0.0))
        .await
        .expect("typed scan must not fail");
    assert_eq!(top.len(), 1, "limit=1 must return exactly 1 result");
    assert_eq!(
        top[0].node_id, near_skill_id,
        "top result must be the nearest skill"
    );

    // limit=5 → both come back, with the near one ranked first
    let all = store
        .search_embeddings_by_node_type(&near, "skill", 5, Some(0.0))
        .await?;
    assert_eq!(all.len(), 2);
    assert_eq!(
        all[0].node_id,
        near_skill_id,
        "near skill must rank above far skill (got {:?})",
        all.iter()
            .map(|r| (&r.node_id, r.score))
            .collect::<Vec<_>>()
    );
    assert!(
        all[0].score >= all[1].score,
        "results must be sorted by descending composite score"
    );
    Ok(())
}

// =========================================================================
// Malformed Input Tests
// =========================================================================

#[test]
fn test_search_params_rejects_array_instead_of_object() {
    // Pass an array instead of an object
    let params = json!(["query", "test"]);
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject array input");
}

#[test]
fn test_search_params_rejects_string_instead_of_object() {
    // Pass a string instead of an object
    let params = json!("just a string");
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject string input");
}

#[test]
fn test_search_params_rejects_null() {
    // Pass null
    let params = json!(null);
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject null input");
}

#[test]
fn test_search_params_rejects_number() {
    // Pass a number instead of an object
    let params = json!(42);
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject number input");
}

#[test]
fn test_search_params_rejects_wrong_type_for_query() {
    // Query should be a string, not a number
    let params = json!({
        "query": 12345
    });
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject non-string query");
}

#[test]
fn test_search_params_rejects_wrong_type_for_threshold() {
    // Threshold should be a number, not a string
    let params = json!({
        "query": "valid query",
        "threshold": "not a number"
    });
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject non-numeric threshold");
}

#[test]
fn test_search_params_rejects_wrong_type_for_limit() {
    // Limit should be a number, not a string
    let params = json!({
        "query": "valid query",
        "limit": "not a number"
    });
    let result: Result<SearchSemanticParams, _> = serde_json::from_value(params);
    assert!(result.is_err(), "Should reject non-numeric limit");
}
