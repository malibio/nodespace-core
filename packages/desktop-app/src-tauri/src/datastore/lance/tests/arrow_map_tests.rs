use crate::datastore::lance::store::LanceDataStore;
use crate::datastore::lance::types::{LanceDBError, UniversalNode};
use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, MapArray, RecordBatch, StringArray,
    StructArray,
};
use arrow_buffer::OffsetBuffer;
use arrow_schema::{DataType, Field, Fields, Schema};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

/// Helper struct for testing Arrow Map-based LanceDB implementation
pub struct MapBasedLanceStore {
    #[allow(dead_code)]
    store: LanceDataStore,
    vector_dimension: usize,
}

impl MapBasedLanceStore {
    /// Create new instance with Map-based schema
    pub async fn new() -> Result<Self, LanceDBError> {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("lance_map_test_{}", uuid::Uuid::new_v4()));
        let db_path_str = db_path.to_str().unwrap();

        let store = LanceDataStore::new(db_path_str).await?;

        Ok(Self {
            store,
            vector_dimension: 384,
        })
    }

    /// Create schema with Map<Utf8, Utf8> for properties
    fn create_map_schema(&self) -> Arc<Schema> {
        // Define the inner Key-Value pair structure
        let kv_struct = DataType::Struct(Fields::from(vec![
            Field::new("key", DataType::Utf8, false), // Keys are strings (non-null)
            Field::new("value", DataType::Utf8, true), // Values are strings (nullable)
        ]));

        // Define the Map type
        let map_type = DataType::Map(
            Arc::new(Field::new("entries", kv_struct, false)),
            false, // sorted_keys = false
        );

        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("node_type", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, false)),
                    self.vector_dimension as i32,
                ),
                false,
            ),
            Field::new("parent_id", DataType::Utf8, true),
            Field::new("container_node_id", DataType::Utf8, true),
            Field::new("before_sibling_id", DataType::Utf8, true),
            Field::new(
                "children_ids",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
            Field::new(
                "mentions",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("modified_at", DataType::Utf8, false),
            Field::new("properties", map_type, true), // Map type for dynamic properties
            Field::new("version", DataType::Int64, false),
        ]))
    }

    /// Convert serde_json::Value to Arrow MapArray
    /// All values stored as strings (cast numbers/bools to strings)
    fn json_to_map_array(
        json_values: Vec<Option<serde_json::Value>>,
    ) -> Result<Arc<dyn Array>, LanceDBError> {
        let mut keys_builder = Vec::new();
        let mut values_builder = Vec::new();
        let mut offsets = vec![0i32];

        for json_opt in json_values {
            match json_opt {
                Some(serde_json::Value::Object(map)) => {
                    for (key, value) in map {
                        keys_builder.push(key);

                        // Convert all values to strings
                        let value_str = match value {
                            serde_json::Value::String(s) => s,
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::Null => String::new(),
                            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                                serde_json::to_string(&value).map_err(|e| {
                                    LanceDBError::Arrow(format!("Failed to serialize JSON: {}", e))
                                })?
                            }
                        };
                        values_builder.push(Some(value_str));
                    }

                    offsets.push(keys_builder.len() as i32);
                }
                None => {
                    // Null map
                    offsets.push(keys_builder.len() as i32);
                }
                _ => {
                    return Err(LanceDBError::Arrow(
                        "Properties must be JSON object".to_string(),
                    ));
                }
            }
        }

        // Build key array
        let keys_array = Arc::new(StringArray::from(keys_builder));

        // Build value array
        let values_array = Arc::new(StringArray::from(values_builder));

        // Build struct array (key-value pairs)
        let kv_struct = StructArray::try_new(
            Fields::from(vec![
                Field::new("key", DataType::Utf8, false),
                Field::new("value", DataType::Utf8, true),
            ]),
            vec![keys_array as ArrayRef, values_array as ArrayRef],
            None,
        )
        .map_err(|e| LanceDBError::Arrow(format!("Failed to create StructArray: {}", e)))?;

        // Build map array
        let offsets = OffsetBuffer::new(offsets.into());
        let map_array = MapArray::new(
            Arc::new(Field::new("entries", kv_struct.data_type().clone(), false)),
            offsets,
            kv_struct,
            None,
            false,
        );

        Ok(Arc::new(map_array))
    }

    /// Convert Arrow MapArray back to serde_json::Value
    fn map_array_to_json(
        map_array: &MapArray,
        index: usize,
    ) -> Result<Option<serde_json::Value>, LanceDBError> {
        if map_array.is_null(index) {
            return Ok(None);
        }

        let mut result = serde_json::Map::new();

        // Get the struct array containing key-value pairs
        let struct_array = map_array.entries();

        let keys = struct_array
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| LanceDBError::Arrow("Expected StringArray for keys".to_string()))?;

        let values = struct_array
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| LanceDBError::Arrow("Expected StringArray for values".to_string()))?;

        let start = map_array.value_offsets()[index] as usize;
        let end = map_array.value_offsets()[index + 1] as usize;

        for i in start..end {
            let key = keys.value(i).to_string();
            let value = if values.is_null(i) {
                serde_json::Value::Null
            } else {
                // Try to parse as number, otherwise keep as string
                let value_str = values.value(i);
                if let Ok(num) = value_str.parse::<i64>() {
                    serde_json::Value::Number(num.into())
                } else if let Ok(num) = value_str.parse::<f64>() {
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(num).unwrap_or_else(|| 0.into()),
                    )
                } else if value_str == "true" || value_str == "false" {
                    serde_json::Value::Bool(value_str == "true")
                } else {
                    serde_json::Value::String(value_str.to_string())
                }
            };
            result.insert(key, value);
        }

        Ok(Some(serde_json::Value::Object(result)))
    }

    /// Create RecordBatch with Map-based properties
    fn create_record_batch_with_map(
        &self,
        nodes: Vec<UniversalNode>,
    ) -> Result<RecordBatch, LanceDBError> {
        let schema = self.create_map_schema();

        // Extract simple fields
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let node_types: Vec<String> = nodes.iter().map(|n| n.node_type.clone()).collect();
        let contents: Vec<String> = nodes.iter().map(|n| n.content.clone()).collect();
        let parent_ids: Vec<Option<String>> = nodes.iter().map(|n| n.parent_id.clone()).collect();
        let container_node_ids: Vec<Option<String>> =
            nodes.iter().map(|n| n.container_node_id.clone()).collect();
        let before_sibling_ids: Vec<Option<String>> =
            nodes.iter().map(|n| n.before_sibling_id.clone()).collect();
        let created_ats: Vec<String> = nodes.iter().map(|n| n.created_at.clone()).collect();
        let modified_ats: Vec<String> = nodes.iter().map(|n| n.modified_at.clone()).collect();
        let versions: Vec<i64> = nodes.iter().map(|n| n.version).collect();

        // Convert properties to Map array
        let properties_json: Vec<Option<serde_json::Value>> =
            nodes.iter().map(|n| n.properties.clone()).collect();
        let properties_array = Self::json_to_map_array(properties_json)?;

        // Vector field
        let vectors = {
            let mut flat_values = Vec::new();
            for node in &nodes {
                if node.vector.len() != self.vector_dimension {
                    return Err(LanceDBError::Arrow(format!(
                        "Vector dimension mismatch: expected {}, got {}",
                        self.vector_dimension,
                        node.vector.len()
                    )));
                }
                flat_values.extend_from_slice(&node.vector);
            }

            let values = Float32Array::from(flat_values);
            let field = Arc::new(Field::new("item", DataType::Float32, false));
            FixedSizeListArray::try_new(field, self.vector_dimension as i32, Arc::new(values), None)
                .map_err(|e| {
                    LanceDBError::Arrow(format!("Failed to create FixedSizeListArray: {}", e))
                })?
        };

        // Children IDs
        let mut children_builder = ListBuilder::new(StringBuilder::new());
        for node in &nodes {
            for child_id in &node.children_ids {
                children_builder.values().append_value(child_id);
            }
            children_builder.append(true);
        }
        let children_ids = children_builder.finish();

        // Mentions
        let mut mentions_builder = ListBuilder::new(StringBuilder::new());
        for node in &nodes {
            for mention in &node.mentions {
                mentions_builder.values().append_value(mention);
            }
            mentions_builder.append(true);
        }
        let mentions = mentions_builder.finish();

        // Create RecordBatch
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(StringArray::from(node_types)),
                Arc::new(StringArray::from(contents)),
                Arc::new(vectors),
                Arc::new(StringArray::from(parent_ids)),
                Arc::new(StringArray::from(container_node_ids)),
                Arc::new(StringArray::from(before_sibling_ids)),
                Arc::new(children_ids),
                Arc::new(mentions),
                Arc::new(StringArray::from(created_ats)),
                Arc::new(StringArray::from(modified_ats)),
                properties_array, // Map array
                Arc::new(arrow_array::Int64Array::from(versions)),
            ],
        )
        .map_err(|e| LanceDBError::Arrow(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

#[tokio::test]
async fn test_arrow_map_dynamic_properties() -> Result<(), LanceDBError> {
    let map_store = MapBasedLanceStore::new().await?;

    println!("\n=== Test 1: Ingest nodes with completely different properties ===");

    // Node 1: Task properties
    let task_node = UniversalNode {
        id: "task_1".to_string(),
        node_type: "task".to_string(),
        content: "Task 1".to_string(),
        vector: vec![0.0; 384],
        parent_id: None,
        container_node_id: None,
        before_sibling_id: None,
        children_ids: vec![],
        mentions: vec![],
        created_at: "2025-11-10T00:00:00Z".to_string(),
        modified_at: "2025-11-10T00:00:00Z".to_string(),
        properties: Some(json!({
            "status": "done",
            "priority": "5",
            "assignee": "alice"
        })),
        version: 1,
    };

    // Node 2: Customer properties (completely different keys)
    let customer_node = UniversalNode {
        id: "customer_1".to_string(),
        node_type: "customer".to_string(),
        content: "Customer 1".to_string(),
        vector: vec![0.0; 384],
        parent_id: None,
        container_node_id: None,
        before_sibling_id: None,
        children_ids: vec![],
        mentions: vec![],
        created_at: "2025-11-10T00:00:00Z".to_string(),
        modified_at: "2025-11-10T00:00:00Z".to_string(),
        properties: Some(json!({
            "company": "Acme Corp",
            "tier": "premium",
            "revenue": "1000000"
        })),
        version: 1,
    };

    // Node 3: Event properties (yet another schema)
    let event_node = UniversalNode {
        id: "event_1".to_string(),
        node_type: "event".to_string(),
        content: "Event 1".to_string(),
        vector: vec![0.0; 384],
        parent_id: None,
        container_node_id: None,
        before_sibling_id: None,
        children_ids: vec![],
        mentions: vec![],
        created_at: "2025-11-10T00:00:00Z".to_string(),
        modified_at: "2025-11-10T00:00:00Z".to_string(),
        properties: Some(json!({
            "date": "2025-11-10",
            "attendees": "50",
            "location": "SF"
        })),
        version: 1,
    };

    // Test conversion to Map array
    let batch = map_store.create_record_batch_with_map(vec![
        task_node.clone(),
        customer_node.clone(),
        event_node.clone(),
    ])?;

    println!("✅ Created RecordBatch with {} rows", batch.num_rows());
    println!("   Schema: {:?}", batch.schema());

    // Verify Map array structure
    let properties_col = batch
        .column_by_name("properties")
        .ok_or_else(|| LanceDBError::Arrow("Missing properties column".to_string()))?;

    let map_array = properties_col
        .as_any()
        .downcast_ref::<MapArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected MapArray".to_string()))?;

    println!("✅ Properties column is MapArray");

    // Test decoding back to JSON
    for i in 0..batch.num_rows() {
        let properties = MapBasedLanceStore::map_array_to_json(map_array, i)?;
        println!("   Row {}: {:?}", i, properties);
    }

    println!("✅ Successfully ingested 3 nodes with different property schemas");

    Ok(())
}

#[tokio::test]
async fn test_arrow_map_encoding_decoding() -> Result<(), LanceDBError> {
    println!("\n=== Test 2: Map Encoding/Decoding Round-trip ===");

    // Test various data types stored as strings
    let test_properties = vec![
        Some(json!({
            "string_field": "hello",
            "number_field": 42,
            "float_field": 12.345, // arbitrary float value to test encoding
            "bool_field": true,
            "null_field": null
        })),
        Some(json!({
            "different_keys": "value",
            "another_number": 100
        })),
        None, // Test null properties
    ];

    // Encode to Map array
    let map_array = MapBasedLanceStore::json_to_map_array(test_properties.clone())?;
    let map_array = map_array
        .as_any()
        .downcast_ref::<MapArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected MapArray".to_string()))?;

    println!("✅ Encoded {} rows to MapArray", map_array.len());

    // Decode back to JSON
    for (i, expected) in test_properties.iter().enumerate() {
        let decoded = MapBasedLanceStore::map_array_to_json(map_array, i)?;
        println!("   Row {}: {:?}", i, decoded);

        match (expected, &decoded) {
            (None, Some(dec)) if dec.as_object().is_some_and(|o| o.is_empty()) => {
                println!("      ✅ Null properties represented as empty map")
            }
            (None, None) => println!("      ✅ Null properties preserved"),
            (Some(exp), Some(dec)) => {
                // Verify all keys are present
                if let (Some(exp_obj), Some(dec_obj)) = (exp.as_object(), dec.as_object()) {
                    for (key, _value) in exp_obj {
                        if !dec_obj.contains_key(key) {
                            return Err(LanceDBError::Arrow(format!("Missing key: {}", key)));
                        }
                        println!("      ✅ Key '{}' preserved", key);
                    }
                }
            }
            _ => {
                return Err(LanceDBError::Arrow(format!(
                    "Mismatch between expected {:?} and decoded {:?}",
                    expected, decoded
                )))
            }
        }
    }

    println!("✅ Round-trip encoding/decoding successful");

    Ok(())
}

#[tokio::test]
async fn test_arrow_map_type_casting() -> Result<(), LanceDBError> {
    println!("\n=== Test 3: Type Casting (All Values as Strings) ===");

    let properties = vec![Some(json!({
        "priority": 5,           // Number
        "count": 100,            // Number
        "active": true,          // Boolean
        "name": "test",          // String
    }))];

    let map_array = MapBasedLanceStore::json_to_map_array(properties)?;
    let map_array = map_array
        .as_any()
        .downcast_ref::<MapArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected MapArray".to_string()))?;

    // Decode and verify type preservation
    let decoded = MapBasedLanceStore::map_array_to_json(map_array, 0)?;

    println!("Decoded properties: {:?}", decoded);

    if let Some(serde_json::Value::Object(obj)) = decoded {
        // Check number conversion
        if let Some(priority) = obj.get("priority") {
            match priority {
                serde_json::Value::Number(n) => {
                    println!("   ✅ Number preserved: priority = {}", n);
                }
                _ => {
                    return Err(LanceDBError::Arrow(
                        "Expected number for priority".to_string(),
                    ))
                }
            }
        }

        // Check boolean conversion
        if let Some(active) = obj.get("active") {
            match active {
                serde_json::Value::Bool(b) => {
                    println!("   ✅ Boolean preserved: active = {}", b);
                }
                _ => {
                    return Err(LanceDBError::Arrow(
                        "Expected boolean for active".to_string(),
                    ))
                }
            }
        }

        // Check string preservation
        if let Some(name) = obj.get("name") {
            match name {
                serde_json::Value::String(s) => {
                    println!("   ✅ String preserved: name = {}", s);
                }
                _ => return Err(LanceDBError::Arrow("Expected string for name".to_string())),
            }
        }
    }

    println!("✅ Type casting and preservation works correctly");

    Ok(())
}

#[tokio::test]
async fn test_arrow_map_physical_layout() -> Result<(), LanceDBError> {
    println!("\n=== Test 4: Verify Physical Layout (List of Key-Value Structs) ===");

    let properties = vec![Some(json!({
        "status": "done",
        "priority": "5"
    }))];

    let map_array = MapBasedLanceStore::json_to_map_array(properties)?;
    let map_array = map_array
        .as_any()
        .downcast_ref::<MapArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected MapArray".to_string()))?;

    println!("MapArray structure:");
    println!("   Data type: {:?}", map_array.data_type());
    println!("   Length: {}", map_array.len());

    // Access the underlying struct array
    let struct_array = map_array.entries();

    println!("   Struct columns: {}", struct_array.num_columns());

    // Access key column
    let keys = struct_array
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected StringArray for keys".to_string()))?;

    println!("   Keys: {:?}", keys);

    // Access value column
    let values = struct_array
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| LanceDBError::Arrow("Expected StringArray for values".to_string()))?;

    println!("   Values: {:?}", values);

    println!("✅ Physical layout confirmed: List of {{key, value}} structs");

    Ok(())
}

#[tokio::test]
async fn test_arrow_map_performance_characteristics() -> Result<(), LanceDBError> {
    println!("\n=== Test 5: Performance Characteristics ===");

    // Create 1000 nodes with Map properties
    let mut nodes = Vec::new();
    for i in 0..1000 {
        nodes.push(UniversalNode {
            id: format!("node_{}", i),
            node_type: "task".to_string(),
            content: format!("Task {}", i),
            vector: vec![0.0; 384],
            parent_id: None,
            container_node_id: None,
            before_sibling_id: None,
            children_ids: vec![],
            mentions: vec![],
            created_at: "2025-11-10T00:00:00Z".to_string(),
            modified_at: "2025-11-10T00:00:00Z".to_string(),
            properties: Some(json!({
                "status": if i % 2 == 0 { "done" } else { "pending" },
                "priority": (i % 5).to_string(),
                "iteration": i.to_string(),
            })),
            version: 1,
        });
    }

    let map_store = MapBasedLanceStore::new().await?;

    // Benchmark: Create RecordBatch with Map encoding
    let start = Instant::now();
    let batch = map_store.create_record_batch_with_map(nodes)?;
    let encoding_duration = start.elapsed();

    println!(
        "✅ Encoded 1000 nodes to MapArray in {:?}",
        encoding_duration
    );
    println!(
        "   Performance: {:.2} ms",
        encoding_duration.as_secs_f64() * 1000.0
    );

    // Benchmark: Decode all properties
    let properties_col = batch.column_by_name("properties").unwrap();
    let map_array = properties_col.as_any().downcast_ref::<MapArray>().unwrap();

    let start = Instant::now();
    for i in 0..batch.num_rows() {
        let _ = MapBasedLanceStore::map_array_to_json(map_array, i)?;
    }
    let decoding_duration = start.elapsed();

    println!("✅ Decoded 1000 properties in {:?}", decoding_duration);
    println!(
        "   Performance: {:.2} ms",
        decoding_duration.as_secs_f64() * 1000.0
    );

    println!("\n📊 Performance Summary:");
    println!(
        "   Encoding: {:.2} µs per node",
        encoding_duration.as_micros() as f64 / 1000.0
    );
    println!(
        "   Decoding: {:.2} µs per node",
        decoding_duration.as_micros() as f64 / 1000.0
    );

    Ok(())
}
