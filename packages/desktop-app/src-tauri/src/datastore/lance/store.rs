use super::types::{LanceDBError, UniversalNode};
use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, LargeBinaryArray, ListArray, RecordBatch,
    RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, Utc};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection, Table};
use nodespace_core::models::Node;
use std::sync::Arc;
use tokio::sync::RwLock;

/// LanceDB DataStore implementation with native Arrow columnar storage
pub struct LanceDataStore {
    connection: Connection,
    table: Arc<RwLock<Option<Table>>>,
    table_name: String,
    _db_path: String,
    vector_dimension: usize,
}

impl LanceDataStore {
    /// Initialize new LanceDB connection with Arrow-based storage
    pub async fn new(db_path: &str) -> Result<Self, LanceDBError> {
        Self::with_vector_dimension(db_path, 384).await
    }

    /// Initialize new LanceDB connection with custom vector dimension
    pub async fn with_vector_dimension(
        db_path: &str,
        vector_dimension: usize,
    ) -> Result<Self, LanceDBError> {
        let connection = connect(db_path)
            .execute()
            .await
            .map_err(|e| LanceDBError::Connection(format!("LanceDB connection failed: {}", e)))?;

        let instance = Self {
            connection,
            table: Arc::new(RwLock::new(None)),
            table_name: "universal_nodes".to_string(),
            _db_path: db_path.to_string(),
            vector_dimension,
        };

        // Initialize Arrow-based table
        instance.initialize_table().await?;

        Ok(instance)
    }

    /// Initialize the Arrow-based table with Universal Document Schema
    async fn initialize_table(&self) -> Result<(), LanceDBError> {
        let schema = self.create_universal_schema();

        // Check if table already exists
        let table_names =
            self.connection.table_names().execute().await.map_err(|e| {
                LanceDBError::Operation(format!("Failed to get table names: {}", e))
            })?;

        let table = if table_names.contains(&self.table_name) {
            // Open existing table
            self.connection
                .open_table(&self.table_name)
                .execute()
                .await
                .map_err(|e| LanceDBError::Operation(format!("Failed to open table: {}", e)))?
        } else {
            // Create new table with empty data
            let empty_batch = self.create_empty_record_batch(schema.clone())?;
            let batches =
                RecordBatchIterator::new(vec![empty_batch].into_iter().map(Ok), schema.clone());

            self.connection
                .create_table(&self.table_name, Box::new(batches))
                .execute()
                .await
                .map_err(|e| LanceDBError::Operation(format!("Failed to create table: {}", e)))?
        };

        // Store table reference
        *self.table.write().await = Some(table);

        // Create vector index for similarity search
        self.create_vector_index().await?;

        Ok(())
    }

    /// Create the Universal Document Schema
    fn create_universal_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("node_type", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            // Vector field - FixedSizeList of Float32 for LanceDB vector indexing
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, false)),
                    self.vector_dimension as i32,
                ),
                false,
            ),
            Field::new("parent_id", DataType::Utf8, true), // Nullable
            Field::new("container_node_id", DataType::Utf8, true), // Nullable
            Field::new("before_sibling_id", DataType::Utf8, true), // Nullable
            // Children IDs - List of String (nullable for empty lists)
            Field::new(
                "children_ids",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
            // Mentions - List of String (nullable for empty lists)
            Field::new(
                "mentions",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
            Field::new("created_at", DataType::Utf8, false),
            Field::new("modified_at", DataType::Utf8, false),
            Field::new("properties", DataType::LargeBinary, true), // Nullable JSON as binary for json_extract()
            Field::new("version", DataType::Int64, false),
        ]))
    }

    /// Create an empty RecordBatch for table initialization
    fn create_empty_record_batch(&self, schema: Arc<Schema>) -> Result<RecordBatch, LanceDBError> {
        // Create empty FixedSizeListArray for vectors with configurable dimension
        let empty_values = Float32Array::from(Vec::<f32>::new());
        let field = Arc::new(Field::new("item", DataType::Float32, false));
        let empty_vectors = FixedSizeListArray::try_new(
            field,
            self.vector_dimension as i32,
            Arc::new(empty_values),
            None,
        )
        .map_err(|e| {
            LanceDBError::Arrow(format!("Failed to create empty FixedSizeListArray: {}", e))
        })?;

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(Vec::<String>::new())), // id
                Arc::new(StringArray::from(Vec::<String>::new())), // node_type
                Arc::new(StringArray::from(Vec::<String>::new())), // content
                Arc::new(empty_vectors),                           // vector
                Arc::new(StringArray::from(Vec::<Option<String>>::new())), // parent_id
                Arc::new(StringArray::from(Vec::<Option<String>>::new())), // container_node_id
                Arc::new(StringArray::from(Vec::<Option<String>>::new())), // before_sibling_id
                Arc::new(ListBuilder::new(StringBuilder::new()).finish()), // children_ids
                Arc::new(ListBuilder::new(StringBuilder::new()).finish()), // mentions
                Arc::new(StringArray::from(Vec::<String>::new())), // created_at
                Arc::new(StringArray::from(Vec::<String>::new())), // modified_at
                Arc::new(LargeBinaryArray::from(Vec::<Option<&[u8]>>::new())), // properties (binary for v0.22.3)
                Arc::new(arrow_array::Int64Array::from(Vec::<i64>::new())),    // version
            ],
        )
        .map_err(|e| LanceDBError::Arrow(format!("Failed to create empty batch: {}", e)))?;

        Ok(batch)
    }

    /// Create vector index for efficient similarity search
    async fn create_vector_index(&self) -> Result<(), LanceDBError> {
        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            // Only create index if table has data
            let stats = table
                .count_rows(None)
                .await
                .map_err(|e| LanceDBError::Operation(format!("Failed to get row count: {}", e)))?;

            if stats > 0 {
                // Create IVF (Inverted File) index for vector similarity search
                match table
                    .create_index(
                        &["vector"],
                        lancedb::index::Index::IvfPq(Default::default()),
                    )
                    .replace(true) // Replace existing index if present
                    .execute()
                    .await
                {
                    Ok(_) => {}
                    Err(_) => {
                        // This is not a fatal error - index can be created later when data exists
                    }
                }
            }
        }
        Ok(())
    }

    /// Convert nodespace-core Node to UniversalNode
    fn node_to_universal(&self, node: Node, embedding: Option<Vec<f32>>) -> UniversalNode {
        let default_vector = vec![0.0; self.vector_dimension];
        let vector = embedding.unwrap_or(default_vector);

        // Extract children IDs from metadata if present (for compatibility)
        let children_ids = if let Some(children) = node.properties.get("children_ids") {
            if let Some(arr) = children.as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        UniversalNode {
            id: node.id.clone(),
            node_type: node.node_type.clone(),
            content: node.content.clone(),
            vector,
            parent_id: node.parent_id.clone(),
            container_node_id: node.container_node_id.clone(),
            before_sibling_id: node.before_sibling_id.clone(),
            children_ids,
            mentions: node.mentions.clone(),
            created_at: node.created_at.to_rfc3339(),
            modified_at: node.modified_at.to_rfc3339(),
            properties: Some(node.properties.clone()),
            version: node.version,
        }
    }

    /// Convert UniversalNode back to nodespace-core Node
    fn universal_to_node(&self, universal: UniversalNode) -> Result<Node, LanceDBError> {
        let created_at = DateTime::parse_from_rfc3339(&universal.created_at)
            .map_err(|e| LanceDBError::InvalidNode(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let modified_at = DateTime::parse_from_rfc3339(&universal.modified_at)
            .map_err(|e| {
                LanceDBError::InvalidNode(format!("Invalid modified_at timestamp: {}", e))
            })?
            .with_timezone(&Utc);

        Ok(Node {
            id: universal.id,
            node_type: universal.node_type,
            content: universal.content,
            parent_id: universal.parent_id,
            container_node_id: universal.container_node_id,
            before_sibling_id: universal.before_sibling_id,
            version: universal.version,
            created_at,
            modified_at,
            properties: universal
                .properties
                .unwrap_or_else(|| serde_json::json!({})),
            embedding_vector: None, // TODO: Convert from Vec<f32> to Vec<u8> if needed
            mentions: universal.mentions,
            mentioned_by: Vec::new(), // Not stored in LanceDB, computed on query
        })
    }

    /// Create RecordBatch from nodes using proper ListArray construction
    fn create_record_batch_from_nodes(
        &self,
        nodes: Vec<UniversalNode>,
        schema: Arc<Schema>,
    ) -> Result<RecordBatch, LanceDBError> {
        if nodes.is_empty() {
            return self.create_empty_record_batch(schema);
        }

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
        // Convert properties to binary format for LanceDB v0.22.3 json_extract() compatibility
        let properties_bytes: Vec<Vec<u8>> = nodes
            .iter()
            .map(|n| {
                n.properties
                    .as_ref()
                    .map(|v| v.to_string().into_bytes())
                    .unwrap_or_default()
            })
            .collect();
        let properties_refs: Vec<Option<&[u8]>> = properties_bytes
            .iter()
            .map(|bytes| {
                if bytes.is_empty() {
                    None
                } else {
                    Some(bytes.as_slice())
                }
            })
            .collect();
        let versions: Vec<i64> = nodes.iter().map(|n| n.version).collect();

        // Vector field: Vec<f32> -> FixedSizeListArray for LanceDB vector indexing
        let vectors = {
            // Collect all vector values into a flat array
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

        // Children IDs: Vec<String> -> ListArray for string lists
        let mut children_builder = ListBuilder::new(StringBuilder::new());
        for node in &nodes {
            for child_id in &node.children_ids {
                children_builder.values().append_value(child_id);
            }
            children_builder.append(true);
        }
        let children_ids = children_builder.finish();

        // Mentions: Vec<String> -> ListArray for string lists
        let mut mentions_builder = ListBuilder::new(StringBuilder::new());
        for node in &nodes {
            for mention in &node.mentions {
                mentions_builder.values().append_value(mention);
            }
            mentions_builder.append(true);
        }
        let mentions = mentions_builder.finish();

        // Create RecordBatch with all columns
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(StringArray::from(node_types)),
                Arc::new(StringArray::from(contents)),
                Arc::new(vectors), // vector field for LanceDB indexing
                Arc::new(StringArray::from(parent_ids)),
                Arc::new(StringArray::from(container_node_ids)),
                Arc::new(StringArray::from(before_sibling_ids)),
                Arc::new(children_ids),
                Arc::new(mentions),
                Arc::new(StringArray::from(created_ats)),
                Arc::new(StringArray::from(modified_ats)),
                Arc::new(LargeBinaryArray::from(properties_refs)), // Binary for v0.22.3 json_extract()
                Arc::new(arrow_array::Int64Array::from(versions)),
            ],
        )
        .map_err(|e| LanceDBError::Arrow(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }

    /// Extract UniversalNode objects from Arrow RecordBatch
    fn extract_nodes_from_batch(
        &self,
        batch: &RecordBatch,
    ) -> Result<Vec<UniversalNode>, LanceDBError> {
        let mut nodes = Vec::new();
        let num_rows = batch.num_rows();

        if num_rows == 0 {
            return Ok(nodes);
        }

        // Extract column arrays with proper error handling
        let ids = batch
            .column_by_name("id")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LanceDBError::Arrow("Missing or invalid id column".to_string()))?;

        let node_types = batch
            .column_by_name("node_type")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                LanceDBError::Arrow("Missing or invalid node_type column".to_string())
            })?;

        let contents = batch
            .column_by_name("content")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| LanceDBError::Arrow("Missing or invalid content column".to_string()))?;

        let created_ats = batch
            .column_by_name("created_at")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                LanceDBError::Arrow("Missing or invalid created_at column".to_string())
            })?;

        let modified_ats = batch
            .column_by_name("modified_at")
            .and_then(|col| col.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                LanceDBError::Arrow("Missing or invalid modified_at column".to_string())
            })?;

        let versions = batch
            .column_by_name("version")
            .and_then(|col| col.as_any().downcast_ref::<arrow_array::Int64Array>())
            .ok_or_else(|| LanceDBError::Arrow("Missing or invalid version column".to_string()))?;

        // Extract vector FixedSizeListArray
        let vector_list_array = batch
            .column_by_name("vector")
            .and_then(|col| col.as_any().downcast_ref::<FixedSizeListArray>())
            .ok_or_else(|| LanceDBError::Arrow("Missing or invalid vector column".to_string()))?;

        // Extract children_ids ListArray
        let children_list_array = batch
            .column_by_name("children_ids")
            .and_then(|col| col.as_any().downcast_ref::<ListArray>());

        // Extract mentions ListArray
        let mentions_list_array = batch
            .column_by_name("mentions")
            .and_then(|col| col.as_any().downcast_ref::<ListArray>());

        for i in 0..num_rows {
            let id = ids.value(i).to_string();
            let node_type = node_types.value(i).to_string();
            let content = contents.value(i).to_string();
            let created_at = created_ats.value(i).to_string();
            let modified_at = modified_ats.value(i).to_string();
            let version = versions.value(i);

            // Extract optional fields
            let parent_id = batch
                .column_by_name("parent_id")
                .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                .and_then(|arr| {
                    if arr.is_null(i) {
                        None
                    } else {
                        Some(arr.value(i).to_string())
                    }
                });

            let container_node_id = batch
                .column_by_name("container_node_id")
                .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                .and_then(|arr| {
                    if arr.is_null(i) {
                        None
                    } else {
                        Some(arr.value(i).to_string())
                    }
                });

            let before_sibling_id = batch
                .column_by_name("before_sibling_id")
                .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                .and_then(|arr| {
                    if arr.is_null(i) {
                        None
                    } else {
                        Some(arr.value(i).to_string())
                    }
                });

            let properties_bytes = batch
                .column_by_name("properties")
                .and_then(|col| col.as_any().downcast_ref::<LargeBinaryArray>())
                .and_then(|arr| {
                    if arr.is_null(i) {
                        None
                    } else {
                        Some(arr.value(i).to_vec())
                    }
                });

            // Convert properties binary back to JSON Value
            let properties = properties_bytes.and_then(|bytes| {
                std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|s| serde_json::from_str(s).ok())
            });

            // Extract vector embedding from FixedSizeListArray
            let vector = if !vector_list_array.is_null(i) {
                let vector_list = vector_list_array.value(i);
                if let Some(float_array) = vector_list.as_any().downcast_ref::<Float32Array>() {
                    (0..float_array.len())
                        .map(|j| float_array.value(j))
                        .collect()
                } else {
                    vec![0.0; self.vector_dimension] // Fallback if vector extraction fails
                }
            } else {
                vec![0.0; self.vector_dimension] // Fallback for null vector
            };

            // Extract children_ids from ListArray
            let children_ids = if let Some(children_list_array) = children_list_array {
                if !children_list_array.is_null(i) {
                    let children_list = children_list_array.value(i);
                    if let Some(string_array) = children_list.as_any().downcast_ref::<StringArray>()
                    {
                        (0..string_array.len())
                            .map(|j| string_array.value(j).to_string())
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            // Extract mentions from ListArray
            let mentions = if let Some(mentions_list_array) = mentions_list_array {
                if !mentions_list_array.is_null(i) {
                    let mentions_list = mentions_list_array.value(i);
                    if let Some(string_array) = mentions_list.as_any().downcast_ref::<StringArray>()
                    {
                        (0..string_array.len())
                            .map(|j| string_array.value(j).to_string())
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            let node = UniversalNode {
                id,
                node_type,
                content,
                vector,
                parent_id,
                container_node_id,
                before_sibling_id,
                children_ids,
                mentions,
                created_at,
                modified_at,
                properties,
                version,
            };

            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Store a single node using Arrow persistence
    pub async fn store_node_arrow(
        &self,
        universal_node: UniversalNode,
    ) -> Result<(), LanceDBError> {
        let schema = self.create_universal_schema();
        let batch = self.create_record_batch_from_nodes(vec![universal_node], schema.clone())?;

        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            let batches = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema);

            table.add(Box::new(batches)).execute().await.map_err(|e| {
                LanceDBError::Operation(format!("Failed to add data to table: {}", e))
            })?;

            // Force filesystem sync for persistence
            let _ = table.count_rows(None).await;

            // Give LanceDB time to complete disk writes
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        } else {
            return Err(LanceDBError::Database("Table not initialized".to_string()));
        }

        Ok(())
    }

    /// Create a new node
    pub async fn create_node(&self, node: Node) -> Result<String, LanceDBError> {
        let universal = self.node_to_universal(node.clone(), None);
        self.store_node_arrow(universal).await?;
        Ok(node.id)
    }

    /// Read a node by ID
    pub async fn read_node(&self, id: &str) -> Result<Option<Node>, LanceDBError> {
        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            let results_stream = table
                .query()
                .only_if(format!("id = '{}'", id))
                .limit(1)
                .execute()
                .await
                .map_err(|e| LanceDBError::Operation(format!("Query by ID failed: {}", e)))?;

            let batches: Vec<RecordBatch> = futures::TryStreamExt::try_collect(results_stream)
                .await
                .map_err(|e| {
                    LanceDBError::Operation(format!("Failed to collect query results: {}", e))
                })?;

            for batch in batches.iter() {
                if batch.num_rows() > 0 {
                    let universal_nodes = self.extract_nodes_from_batch(batch)?;
                    for universal_node in universal_nodes {
                        if universal_node.id == id {
                            return Ok(Some(self.universal_to_node(universal_node)?));
                        }
                    }
                }
            }

            Ok(None)
        } else {
            Err(LanceDBError::Database("Table not initialized".to_string()))
        }
    }

    /// Query nodes with optional filter
    pub async fn query_nodes(&self, filter: &str) -> Result<Vec<Node>, LanceDBError> {
        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            let results = if filter.is_empty() {
                table
                    .query()
                    .limit(1000)
                    .execute()
                    .await
                    .map_err(|e| LanceDBError::Operation(format!("Query failed: {}", e)))?
            } else {
                table
                    .query()
                    .only_if(filter)
                    .limit(1000)
                    .execute()
                    .await
                    .map_err(|e| LanceDBError::Operation(format!("Query failed: {}", e)))?
            };

            let batches: Vec<RecordBatch> = futures::TryStreamExt::try_collect(results)
                .await
                .map_err(|e| {
                    LanceDBError::Operation(format!("Failed to collect results: {}", e))
                })?;

            let mut nodes = Vec::new();
            for batch in batches {
                let batch_nodes = self.extract_nodes_from_batch(&batch)?;
                for universal_node in batch_nodes {
                    nodes.push(self.universal_to_node(universal_node)?);
                }
            }

            Ok(nodes)
        } else {
            Err(LanceDBError::Database("Table not initialized".to_string()))
        }
    }

    /// Update an existing node
    pub async fn update_node(&self, node: Node) -> Result<(), LanceDBError> {
        // Delete the old version
        self.delete_node(&node.id).await?;

        // Insert the new version
        let universal = self.node_to_universal(node, None);
        self.store_node_arrow(universal).await?;

        Ok(())
    }

    /// Delete a node by ID
    pub async fn delete_node(&self, id: &str) -> Result<(), LanceDBError> {
        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            table
                .delete(&format!("id = '{}'", id.replace("'", "''")))
                .await
                .map_err(|e| LanceDBError::Operation(format!("Delete operation failed: {}", e)))?;

            Ok(())
        } else {
            Err(LanceDBError::Database("Table not initialized".to_string()))
        }
    }

    /// Add a batch of nodes atomically (single LanceDB version)
    pub async fn add_batch(&self, batch: RecordBatch) -> Result<(), LanceDBError> {
        let table_guard = self.table.read().await;
        if let Some(table) = table_guard.as_ref() {
            let schema = self.create_universal_schema();
            let batches = RecordBatchIterator::new(vec![batch].into_iter().map(Ok), schema);

            table
                .add(Box::new(batches))
                .execute()
                .await
                .map_err(|e| LanceDBError::Operation(format!("Failed to add batch: {}", e)))?;

            Ok(())
        } else {
            Err(LanceDBError::Database("Table not initialized".to_string()))
        }
    }
}
