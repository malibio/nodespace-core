/// CORRECT JSON Scalar Index Tests for LanceDB v0.22.3
///
/// This test suite validates the "Dynamic JSON with Opt-In Indexing" pattern:
/// 1. Store properties as plain JSON strings (DataType::Utf8)
/// 2. Create scalar indexes on specific paths when needed
/// 3. Query efficiently with LanceDB parsing JSON and using indexes
///
/// This is the CORRECT approach for v0.22.3's JSON scalar index feature.
use crate::datastore::lance::store::LanceDataStore;
use chrono::Utc;
use nodespace_core::models::Node;
use serde_json::json;
use std::time::Instant;
use tempfile::tempdir;

/// Helper function to create a test node with dynamic properties
fn create_test_node(id: &str, node_type: &str, properties: serde_json::Value) -> Node {
    Node {
        id: id.to_string(),
        node_type: node_type.to_string(),
        content: format!("Test node {}", id),
        parent_id: None,
        container_node_id: None,
        before_sibling_id: None,
        version: 1,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        properties,
        embedding_vector: None,
        mentions: vec![],
        mentioned_by: vec![],
    }
}

#[tokio::test]
async fn test_dynamic_json_with_opt_in_indexing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_dynamic_json.lance");
    let store = LanceDataStore::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create store");

    println!("\n=== Test 1: Dynamic JSON with Opt-In Indexing ===\n");

    // Step 1: Ingest data with dynamic JSON (no schema required)
    println!("Step 1: Ingesting 1000 nodes with dynamic properties...");
    for i in 0..1000 {
        let node = create_test_node(
            &format!("node_{}", i),
            "task",
            json!({
                "status": if i % 2 == 0 { "done" } else { "pending" },
                "priority": i % 5,
                "user_rank": i % 100,
                "tags": ["urgent", "backend"],
            }),
        );
        store
            .create_node(node)
            .await
            .expect("Failed to create node");
    }
    println!("✅ Ingestion complete\n");

    // Step 2: Query WITHOUT index (should work but be slower)
    println!("Step 2: Query without index (full table scan)...");
    let start = Instant::now();
    let results = store
        .query_nodes("properties.status = 'done'")
        .await
        .expect("Query failed");
    let duration_no_index = start.elapsed();
    println!(
        "✅ Found {} results in {:?} (no index)",
        results.len(),
        duration_no_index
    );
    assert_eq!(results.len(), 500, "Expected 500 nodes with status='done'");

    // Step 3: Create scalar index on specific path
    println!("\nStep 3: Creating scalar index on 'properties.status'...");
    let start = Instant::now();
    store
        .create_json_path_index("properties.status")
        .await
        .expect("Failed to create index");
    let index_creation_time = start.elapsed();
    println!("✅ Index created in {:?}\n", index_creation_time);

    // Step 4: Query WITH index (should be faster)
    println!("Step 4: Query with index (using scalar index)...");
    let start = Instant::now();
    let results = store
        .query_nodes("properties.status = 'done'")
        .await
        .expect("Query failed");
    let duration_with_index = start.elapsed();
    println!(
        "✅ Found {} results in {:?} (with index)",
        results.len(),
        duration_with_index
    );
    assert_eq!(results.len(), 500);

    // Step 5: Performance comparison
    println!("\n=== Performance Comparison ===");
    println!("  Without index: {:?}", duration_no_index);
    println!("  With index:    {:?}", duration_with_index);
    println!("  Index creation: {:?}", index_creation_time);

    if duration_with_index < duration_no_index {
        let speedup = duration_no_index.as_secs_f64() / duration_with_index.as_secs_f64();
        println!("  ✅ Speedup: {:.2}x faster with index\n", speedup);
    } else {
        println!("  ⚠️  No speedup observed (may need more data or different query pattern)\n");
    }
}

#[tokio::test]
async fn test_nested_json_path_indexing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_nested_paths.lance");
    let store = LanceDataStore::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create store");

    println!("\n=== Test 2: Nested JSON Path Indexing ===\n");

    // Ingest with nested properties
    println!("Ingesting 500 nodes with nested JSON...");
    for i in 0..500 {
        let node = create_test_node(
            &format!("customer_{}", i),
            "customer",
            json!({
                "customer": {
                    "name": format!("Customer {}", i),
                    "address": {
                        "street": format!("{} Main St", i),
                        "zip": format!("{:05}", i % 100),
                        "country": "US"
                    },
                    "tier": if i % 2 == 0 { "premium" } else { "standard" }
                }
            }),
        );
        store
            .create_node(node)
            .await
            .expect("Failed to create node");
    }
    println!("✅ Ingestion complete\n");

    // Create indexes on nested paths
    println!("Creating indexes on nested paths...");
    store
        .create_json_path_indexes(&[
            "properties.customer.address.zip",
            "properties.customer.tier",
        ])
        .await
        .expect("Failed to create indexes");
    println!("✅ Indexes created\n");

    // Query with nested path filter
    println!("Querying with nested path (properties.customer.address.zip = '00042')...");
    let results = store
        .query_nodes("properties.customer.address.zip = '00042'")
        .await
        .expect("Query failed");
    println!("✅ Found {} customers with zip 00042\n", results.len());
    assert!(
        !results.is_empty(),
        "Should find at least one customer with zip 00042"
    );

    // Query with multiple filters
    println!("Querying with multiple nested filters...");
    let results = store
        .query_nodes(
            "properties.customer.tier = 'premium' AND properties.customer.address.country = 'US'",
        )
        .await
        .expect("Query failed");
    println!("✅ Found {} premium US customers\n", results.len());
    assert_eq!(
        results.len(),
        250,
        "Half should be premium (250 out of 500)"
    );
}

#[tokio::test]
async fn test_multiple_json_path_indexes() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_multiple_indexes.lance");
    let store = LanceDataStore::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create store");

    println!("\n=== Test 3: Multiple JSON Path Indexes ===\n");

    // Ingest diverse data
    println!("Ingesting 1000 project nodes...");
    for i in 0..1000 {
        let status = match i % 3 {
            0 => "planning",
            1 => "active",
            _ => "completed",
        };
        let node = create_test_node(
            &format!("project_{}", i),
            "project",
            json!({
                "status": status,
                "priority": i % 5,
                "budget": (i * 1000) as f64,
                "team_size": i % 20,
            }),
        );
        store
            .create_node(node)
            .await
            .expect("Failed to create node");
    }
    println!("✅ Ingestion complete\n");

    // Create multiple indexes
    println!("Creating multiple indexes...");
    store
        .create_json_path_indexes(&[
            "properties.status",
            "properties.priority",
            "properties.budget",
            "properties.team_size",
        ])
        .await
        .expect("Failed to create indexes");
    println!("✅ Indexes created\n");

    // Complex query using multiple indexed fields
    println!("Running complex query with multiple indexed fields...");
    let results = store
        .query_nodes("properties.status = 'active' AND properties.priority >= 3 AND properties.team_size > 10")
        .await
        .expect("Query failed");
    println!("✅ Complex query returned {} results\n", results.len());

    // Verify all indexes exist
    println!("Verifying indexes exist...");
    assert!(
        store
            .has_json_path_index("properties.status")
            .await
            .expect("Failed to check index"),
        "Status index should exist"
    );
    assert!(
        store
            .has_json_path_index("properties.priority")
            .await
            .expect("Failed to check index"),
        "Priority index should exist"
    );
    println!("✅ All indexes verified\n");
}

#[tokio::test]
async fn test_sparse_json_property_indexing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_sparse_properties.lance");
    let store = LanceDataStore::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create store");

    println!("\n=== Test 4: Sparse JSON Property Indexing ===\n");

    // Ingest with sparse properties (only 10% have estimated_hours)
    println!("Ingesting 1000 nodes with sparse properties (10% coverage)...");
    for i in 0..1000 {
        let properties = if i % 10 == 0 {
            json!({
                "status": "pending",
                "estimated_hours": i / 10,
            })
        } else {
            json!({
                "status": "pending",
            })
        };

        let node = create_test_node(&format!("sparse_task_{}", i), "task", properties);
        store
            .create_node(node)
            .await
            .expect("Failed to create node");
    }
    println!("✅ Ingestion complete\n");

    // Create index on sparse property
    println!("Creating index on sparse property (10% coverage)...");
    store
        .create_json_path_index("properties.estimated_hours")
        .await
        .expect("Failed to create index");
    println!("✅ Index created\n");

    // Query for nodes WITH the property
    println!("Querying for nodes with estimated_hours (IS NOT NULL)...");
    let results = store
        .query_nodes("properties.estimated_hours IS NOT NULL")
        .await
        .expect("Query failed");
    println!("✅ Found {} nodes with estimated_hours\n", results.len());
    assert_eq!(results.len(), 100, "Should find 100 nodes (10% of 1000)");

    // Query for specific values
    println!("Querying for nodes with estimated_hours > 50...");
    let results = store
        .query_nodes("properties.estimated_hours > 50")
        .await
        .expect("Query failed");
    println!(
        "✅ Found {} nodes with estimated_hours > 50\n",
        results.len()
    );
    assert!(!results.is_empty(), "Should find nodes with high estimates");
}

#[tokio::test]
async fn test_index_performance_benchmark() {
    println!("\n=== Test 5: Index Performance Benchmark ===\n");

    let sizes = vec![100, 1000, 5000];

    for size in sizes {
        println!("--- Testing with {} nodes ---", size);

        let temp_dir = tempdir().expect("Failed to create temp dir");
        let db_path = temp_dir
            .path()
            .join(format!("test_benchmark_{}.lance", size));
        let store = LanceDataStore::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create store");

        // Ingest data
        println!("  Ingesting {} nodes...", size);
        for i in 0..size {
            let category = match i % 5 {
                0 => "A",
                1 => "B",
                2 => "C",
                3 => "D",
                _ => "E",
            };
            let node = create_test_node(
                &format!("bench_{}_{}", size, i),
                "test",
                json!({
                    "category": category,
                    "value": i,
                }),
            );
            store
                .create_node(node)
                .await
                .expect("Failed to create node");
        }

        // Benchmark: Query without index
        println!("  Benchmarking query without index...");
        let start = Instant::now();
        let results = store
            .query_nodes("properties.category = 'A'")
            .await
            .expect("Query failed");
        let time_no_index = start.elapsed();
        let result_count = results.len();

        // Create index
        println!("  Creating index...");
        let start = Instant::now();
        store
            .create_json_path_index("properties.category")
            .await
            .expect("Failed to create index");
        let index_creation_time = start.elapsed();

        // Benchmark: Query with index
        println!("  Benchmarking query with index...");
        let start = Instant::now();
        let results = store
            .query_nodes("properties.category = 'A'")
            .await
            .expect("Query failed");
        let time_with_index = start.elapsed();

        println!("\n  Results for {} nodes:", size);
        println!("    Query (no index):   {:?}", time_no_index);
        println!("    Index creation:     {:?}", index_creation_time);
        println!("    Query (with index): {:?}", time_with_index);
        println!("    Results returned:   {}", result_count);

        if time_with_index < time_no_index {
            let speedup = time_no_index.as_secs_f64() / time_with_index.as_secs_f64();
            println!("    ✅ Speedup: {:.2}x faster with index", speedup);
        } else {
            println!("    ⚠️  No speedup observed");
        }
        println!();

        assert_eq!(results.len(), result_count, "Results should be consistent");
    }
}

#[tokio::test]
async fn test_json_scalar_index_correctness() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_correctness.lance");
    let store = LanceDataStore::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create store");

    println!("\n=== Test 6: JSON Scalar Index Correctness ===\n");

    // Create test data with known values
    println!("Creating test data with known values...");
    let test_cases = vec![
        ("task_1", "done"),
        ("task_2", "pending"),
        ("task_3", "done"),
        ("task_4", "in_progress"),
        ("task_5", "done"),
    ];

    for (id, status) in &test_cases {
        let node = create_test_node(
            id,
            "task",
            json!({
                "status": status,
            }),
        );
        store
            .create_node(node)
            .await
            .expect("Failed to create node");
    }
    println!("✅ Test data created\n");

    // Create index
    println!("Creating index on properties.status...");
    store
        .create_json_path_index("properties.status")
        .await
        .expect("Failed to create index");
    println!("✅ Index created\n");

    // Test exact match query
    println!("Testing exact match: properties.status = 'done'");
    let results = store
        .query_nodes("properties.status = 'done'")
        .await
        .expect("Query failed");
    println!("  Found {} results", results.len());
    assert_eq!(results.len(), 3, "Should find exactly 3 'done' tasks");

    // Verify result IDs
    let result_ids: Vec<String> = results.iter().map(|n| n.id.clone()).collect();
    assert!(result_ids.contains(&"task_1".to_string()));
    assert!(result_ids.contains(&"task_3".to_string()));
    assert!(result_ids.contains(&"task_5".to_string()));
    println!("  ✅ Correct results returned\n");

    // Test another value
    println!("Testing exact match: properties.status = 'pending'");
    let results = store
        .query_nodes("properties.status = 'pending'")
        .await
        .expect("Query failed");
    println!("  Found {} results", results.len());
    assert_eq!(results.len(), 1, "Should find exactly 1 'pending' task");
    assert_eq!(results[0].id, "task_2");
    println!("  ✅ Correct result returned\n");
}
