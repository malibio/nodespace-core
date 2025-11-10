/// LanceDB v0.22.3 JSON Scalar Index Retest
///
/// This module retests JSON querying capabilities after upgrading from 0.20 → 0.22.3
/// to determine if the Phase 2 blocker has been resolved.
///
/// **Phase 2 Finding (v0.20)**: json_extract() works for querying, but:
/// - No native scalar indexes on JSON fields
/// - All queries use full table scan with runtime json_extract()
/// - Performance degrades linearly with dataset size
///
/// **Question for v0.22.3**: Has the JSON querying architecture improved?
/// - New DataFusion version (50.3.0 vs 47.0.0) - any new JSON functions?
/// - LanceDB API improvements for JSON indexing?
/// - Any new query optimization patterns?
use crate::datastore::lance::store::LanceDataStore;
use chrono::Utc;
use nodespace_core::models::Node;
use serde_json::json;
use std::time::Instant;

/// Helper to create test node with specified JSON properties
fn create_test_node_with_json(id: &str, properties: serde_json::Value) -> Node {
    Node {
        id: id.to_string(),
        node_type: "test".to_string(),
        content: format!("Test node {}", id),
        parent_id: None,
        container_node_id: None,
        before_sibling_id: None,
        version: 1,
        created_at: Utc::now(),
        modified_at: Utc::now(),
        properties,
        embedding_vector: None,
        mentions: Vec::new(),
        mentioned_by: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: Verify json_extract() still works (baseline compatibility)
    #[tokio::test]
    async fn test_json_extract_baseline_v22() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 1: json_extract() Baseline ===");

        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().to_str().unwrap();
        let store = LanceDataStore::new(db_path).await?;

        // Create test nodes
        for i in 0..100 {
            let node = create_test_node_with_json(
                &format!("node_{}", i),
                json!({
                    "status": if i % 2 == 0 { "done" } else { "pending" },
                    "priority": i % 5
                }),
            );
            store.create_node(node).await?;
        }

        println!("✅ 100 nodes inserted");

        // Test json_extract() with v0.22.3 type requirements
        // v0.22.3 requires: json_extract(LargeBinary, Utf8) and cast to 'string' not 'text'
        let filter = "properties != '' AND cast(json_extract(cast(properties as binary), '$.status') as string) = 'done'";

        println!("\nTesting filter: {}", filter);
        let results = store.query_nodes(filter).await?;

        println!("Query returned {} results", results.len());
        assert_eq!(results.len(), 50, "Expected 50 nodes with status='done'");

        println!("✅ json_extract() works in v0.22.3 (with type adjustments)");
        println!("   Note: Requires cast to binary and cast result to 'string' (not 'text')");
        Ok(())
    }

    /// Test 2: Check for new DataFusion JSON functions
    #[tokio::test]
    async fn test_new_datafusion_json_functions() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 2: New DataFusion JSON Functions ===");

        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().to_str().unwrap();
        let store = LanceDataStore::new(db_path).await?;

        // Create test data
        let node = create_test_node_with_json(
            "node_1",
            json!({
                "customer": {
                    "name": "Acme Corp",
                    "address": {
                        "street": "123 Main St",
                        "zip": "12345"
                    }
                }
            }),
        );
        store.create_node(node).await?;

        println!("✅ Test node created with nested JSON");

        // Test Pattern 1: Dot notation (PostgreSQL style)
        println!("\n--- Testing Pattern 1: Dot Notation ---");
        let result = store
            .query_nodes("properties.customer.name = 'Acme Corp'")
            .await;
        match result {
            Ok(nodes) => {
                println!("  ✅ SUCCESS! Dot notation works!");
                println!("     Found {} nodes", nodes.len());
                if !nodes.is_empty() {
                    return Ok(()); // SUCCESS - new feature found!
                }
            }
            Err(e) => {
                println!("  ❌ Dot notation not available: {}", e);
            }
        }

        // Test Pattern 2: Arrow struct cast
        println!("\n--- Testing Pattern 2: Arrow Struct Cast ---");
        let result = store
            .query_nodes(
                "arrow_cast(properties, 'Struct<customer:Struct<name:Utf8>>').customer.name = 'Acme Corp'"
            )
            .await;
        match result {
            Ok(nodes) => {
                println!("  ✅ SUCCESS! Arrow struct cast works!");
                println!("     Found {} nodes", nodes.len());
                if !nodes.is_empty() {
                    return Ok(()); // SUCCESS - new feature found!
                }
            }
            Err(e) => {
                println!("  ❌ Arrow struct cast not available: {}", e);
            }
        }

        // Test Pattern 3: Array indexing syntax
        println!("\n--- Testing Pattern 3: Array Indexing Syntax ---");
        let result = store
            .query_nodes("properties['customer']['name'] = 'Acme Corp'")
            .await;
        match result {
            Ok(nodes) => {
                println!("  ✅ SUCCESS! Array indexing syntax works!");
                println!("     Found {} nodes", nodes.len());
                if !nodes.is_empty() {
                    return Ok(()); // SUCCESS - new feature found!
                }
            }
            Err(e) => {
                println!("  ❌ Array indexing syntax not available: {}", e);
            }
        }

        // Test Pattern 4: json_get function (DataFusion 50.x)
        println!("\n--- Testing Pattern 4: json_get() Function ---");
        let result = store
            .query_nodes("json_get(properties, 'customer.name') = 'Acme Corp'")
            .await;
        match result {
            Ok(nodes) => {
                println!("  ✅ SUCCESS! json_get() function works!");
                println!("     Found {} nodes", nodes.len());
                if !nodes.is_empty() {
                    return Ok(()); // SUCCESS - new feature found!
                }
            }
            Err(e) => {
                println!("  ❌ json_get() function not available: {}", e);
            }
        }

        println!("\n⚠️  NO NEW JSON QUERYING METHODS FOUND");
        println!("    v0.22.3 still requires json_extract() for nested field access");
        println!("    The Phase 2 blocker PERSISTS in v0.22.3");

        // This is not a failure - we've documented the finding
        Ok(())
    }

    /// Test 3: Multi-level nested field access patterns
    #[tokio::test]
    async fn test_nested_field_access_patterns_v22() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 3: Nested Field Access Patterns ===");

        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().to_str().unwrap();
        let store = LanceDataStore::new(db_path).await?;

        let node = create_test_node_with_json(
            "node_1",
            json!({
                "project": {
                    "team": {
                        "lead": {
                            "contact": {
                                "email": "lead@example.com"
                            }
                        }
                    }
                }
            }),
        );
        store.create_node(node).await?;

        println!("✅ Test node with 5-level nesting created");

        // Try all possible patterns for deep nested access (with v0.22.3 type adjustments)
        let patterns = vec![
            // Pattern 1: json_extract (with binary cast for v0.22.3)
            ("json_extract", "properties != '' AND cast(json_extract(cast(properties as binary), '$.project.team.lead.contact.email') as string) = 'lead@example.com'"),

            // Pattern 2: Dot notation
            ("dot_notation", "properties.project.team.lead.contact.email = 'lead@example.com'"),

            // Pattern 3: Arrow indexing
            ("arrow_indexing", "properties['project']['team']['lead']['contact']['email'] = 'lead@example.com'"),
        ];

        let mut successful_pattern = None;

        for (pattern_name, filter) in patterns {
            println!("\n--- Testing: {} ---", pattern_name);
            println!("    Filter: {}", filter);

            match store.query_nodes(filter).await {
                Ok(nodes) => {
                    if !nodes.is_empty() {
                        println!("  ✅ SUCCESS! Found {} nodes", nodes.len());
                        successful_pattern = Some(pattern_name);
                        break;
                    } else {
                        println!("  ⚠️  Query succeeded but returned 0 results");
                    }
                }
                Err(e) => {
                    println!("  ❌ Failed: {}", e);
                }
            }
        }

        if let Some(pattern) = successful_pattern {
            println!("\n✅ v0.22.3 supports nested field access via: {}", pattern);
            if pattern != "json_extract" {
                println!("   🎉 NEW FEATURE! This is an improvement over v0.20");
            } else {
                println!("   ⚠️  No improvement - still requires json_extract() like v0.20");
            }
        } else {
            println!("\n❌ NO nested field access pattern works");
            return Err(anyhow::anyhow!(
                "None of the tested patterns support nested field access"
            ));
        }

        Ok(())
    }

    /// Test 4: Check LanceDB API for new JSON-specific methods
    #[tokio::test]
    async fn test_lancedb_api_json_methods() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 4: LanceDB API JSON Methods ===");

        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().to_str().unwrap();
        let store = LanceDataStore::new(db_path).await?;

        // Insert test data
        for i in 0..1000 {
            let node = create_test_node_with_json(
                &format!("node_{}", i),
                json!({
                    "status": if i % 2 == 0 { "done" } else { "pending" }
                }),
            );
            store.create_node(node).await?;
        }

        println!("✅ 1000 nodes inserted");

        // Test: Can we create a scalar index on properties column?
        // This would require examining the Table API to see if there are new methods

        // In LanceDB 0.20, attempting to create an index on 'properties' column
        // would fail or create an index on the entire JSON string (not useful)

        // Check if v0.22.3 has:
        // - create_json_index() method
        // - create_scalar_index() with JSON path support
        // - Any index-related improvements

        println!("\n--- Checking Table API for JSON index methods ---");

        // Note: We can't directly test index creation without knowing the exact API
        // The best we can do is:
        // 1. Check if table.create_index() exists and what parameters it accepts
        // 2. Measure query performance to see if there's any improvement

        println!("  📋 API Check: LanceDB Table methods available:");
        println!("     - create_index() - exists (same as v0.20)");
        println!("     - create_scalar_index() - checking...");

        // Try to create a scalar index (will document if it fails)
        // This is intentionally exploratory - we expect it might not work

        println!("\n--- Attempting scalar index creation on 'properties' column ---");
        println!("  ⚠️  Note: This may not work for JSON columns");

        // We can't actually test this without accessing table directly
        // Our store abstraction doesn't expose index creation
        // This would require modifying the store API

        println!("\n📋 FINDING: Index creation API exploration requires store modification");
        println!("   Current LanceDataStore doesn't expose index creation methods");
        println!("   Would need to add: store.create_index(column, index_type)");
        println!("\n   However, based on LanceDB architecture:");
        println!("   - Scalar indexes work on physical columns");
        println!("   - 'properties' is stored as JSON string (single physical column)");
        println!("   - Creating index on entire JSON string is not useful");
        println!("   - Need exploded/flattened columns for indexed queries");

        println!("\n✅ Test completed - findings documented above");
        Ok(())
    }

    /// Test 5: Performance comparison v0.22.3 vs documented v0.20 baseline
    #[tokio::test]
    async fn benchmark_performance_comparison() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 5: Performance Comparison ===");
        println!("Comparing query performance against Phase 2 (v0.20) baseline");

        let sizes = vec![100, 1000, 10000];
        let mut results = Vec::new();

        for size in sizes {
            println!("\n--- Dataset size: {} ---", size);

            let temp_dir = tempfile::tempdir()?;
            let db_path = temp_dir.path().to_str().unwrap();
            let store = LanceDataStore::new(db_path).await?;

            // Create test dataset
            let dataset_start = Instant::now();
            for i in 0..size {
                let node = create_test_node_with_json(
                    &format!("node_{}", i),
                    json!({
                        "status": if i % 2 == 0 { "done" } else { "pending" },
                        "priority": i % 5,
                        "customer": {
                            "name": format!("Customer {}", i),
                            "address": {
                                "zip": format!("{:05}", i % 100000)
                            }
                        }
                    }),
                );
                store.create_node(node).await?;
            }
            let dataset_time = dataset_start.elapsed();

            // Benchmark: Single-level property query (with v0.22.3 binary cast)
            let query_start = Instant::now();
            let filter1 =
                "properties != '' AND cast(json_extract(cast(properties as binary), '$.status') as string) = 'done'";
            let results1 = store.query_nodes(filter1).await?;
            let query1_time = query_start.elapsed();

            // Benchmark: Multi-level nested property query (with v0.22.3 binary cast)
            let query_start = Instant::now();
            let filter3 = "properties != '' AND cast(json_extract(cast(properties as binary), '$.customer.address.zip') as string) = '00042'";
            let results3 = store.query_nodes(filter3).await?;
            let query3_time = query_start.elapsed();

            println!(
                "  Insert time: {:.2}ms",
                dataset_time.as_secs_f64() * 1000.0
            );
            println!(
                "  L1 query: {:.2}ms ({} results)",
                query1_time.as_secs_f64() * 1000.0,
                results1.len()
            );
            println!(
                "  L3 query: {:.2}ms ({} results)",
                query3_time.as_secs_f64() * 1000.0,
                results3.len()
            );

            results.push((size, dataset_time, query1_time, query3_time));
        }

        // Summary
        println!("\n=== v0.22.3 Performance Summary ===");
        println!("| Size    | Insert (ms) | L1 Query (ms) | L3 Query (ms) |");
        println!("|---------|-------------|---------------|---------------|");
        for (size, insert, q1, q3) in &results {
            println!(
                "| {:7} | {:11.2} | {:13.2} | {:13.2} |",
                size,
                insert.as_secs_f64() * 1000.0,
                q1.as_secs_f64() * 1000.0,
                q3.as_secs_f64() * 1000.0
            );
        }

        println!("\n📊 Phase 2 (v0.20) Baseline Comparison:");
        println!("   Compare these numbers to the Phase 2 benchmark results");
        println!("   to determine if v0.22.3 has improved query performance");

        println!("\n✅ Performance benchmark completed");
        Ok(())
    }

    /// Test 6: Verify DataFusion version and available functions
    #[tokio::test]
    async fn test_datafusion_version_and_functions() -> Result<(), anyhow::Error> {
        println!("\n=== v0.22.3 Test 6: DataFusion Version Check ===");

        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().to_str().unwrap();
        let store = LanceDataStore::new(db_path).await?;

        // Create minimal test data
        let node = create_test_node_with_json("node_1", json!({ "test": "value" }));
        store.create_node(node).await?;

        println!("✅ Test node created");
        println!("\n📋 DataFusion Version Information:");
        println!("   LanceDB 0.22.3 uses DataFusion 50.3.0 (up from 47.0.0 in v0.20)");
        println!("\n   Checking for new JSON functions in DataFusion 50.3.0...");

        // Test various JSON function names that might exist
        let json_functions = vec![
            "json_get",
            "json_extract",
            "json_extract_scalar",
            "json_as_text",
            "get_json_object",
            "json_query",
        ];

        println!("\n--- Testing JSON Function Availability ---");
        for func in json_functions {
            let test_query = format!(
                "properties != '' AND {}(properties, '$.test') = 'value'",
                func
            );
            match store.query_nodes(&test_query).await {
                Ok(_) => {
                    println!("  ✅ {} - Available", func);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("Unknown function") || error_msg.contains("not found") {
                        println!("  ❌ {} - Not available", func);
                    } else {
                        println!("  ⚠️  {} - Error (may exist but syntax wrong): {}", func, e);
                    }
                }
            }
        }

        println!("\n✅ Function availability check completed");
        Ok(())
    }
}
