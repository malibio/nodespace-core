# NodeSpace NLP Engine

Unified vector embedding service using Candle + ONNX for semantic search across NodeSpace knowledge graphs and codebases.

## Features

- **Local Model Bundling**: Models bundled with application, no network required
- **Metal GPU Acceleration**: Optimized performance on macOS with automatic CPU fallback
- **Efficient Caching**: LRU cache with automatic eviction for <5ms cache hits
- **Batch Operations**: Efficient batch embedding generation
- **Turso Integration**: F32_BLOB format for direct database storage

## Architecture

- **Model**: BAAI/bge-small-en-v1.5 (384 dimensions)
- **Runtime**: Candle + ONNX
- **GPU Support**: Metal (macOS) with CPU fallback
- **Cache**: LruCache with automatic LRU eviction

## Performance Targets

- **Model Loading**: <30 seconds (ONNX format)
- **Single Embedding**: <10ms (Metal GPU)
- **Batch 100 Embeddings**: <500ms
- **Cache Hit Latency**: <5ms
- **Memory Footprint**: <500MB (model + cache)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
nodespace-nlp-engine = { workspace = true }
```

## Usage

### Basic Example

```rust
use nodespace_nlp_engine::{EmbeddingService, EmbeddingConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create service with default configuration
    let config = EmbeddingConfig::default();
    let mut service = EmbeddingService::new(config)?;

    // Initialize (loads model from bundled path)
    service.initialize()?;

    // Generate single embedding
    let embedding = service.generate_embedding("Hello, world!")?;
    println!("Generated embedding with {} dimensions", embedding.len());

    // Generate batch embeddings
    let texts = vec!["First text", "Second text", "Third text"];
    let embeddings = service.generate_batch(texts)?;
    println!("Generated {} embeddings", embeddings.len());

    Ok(())
}
```

### Storage Integration (Turso)

```rust
use nodespace_nlp_engine::EmbeddingService;

// Generate embedding
let embedding = service.generate_embedding("search query")?;

// Convert to F32_BLOB for Turso storage
let blob = EmbeddingService::to_blob(&embedding);

// Store in database
// INSERT INTO embeddings (node_id, embedding_vector) VALUES (?, ?)

// Retrieve from database and convert back
let recovered = EmbeddingService::from_blob(&blob);
```

### Custom Configuration

```rust
use nodespace_nlp_engine::EmbeddingConfig;
use std::path::PathBuf;

let config = EmbeddingConfig {
    model_name: "BAAI/bge-small-en-v1.5".to_string(),
    model_path: Some(PathBuf::from("/custom/path/to/model")),
    max_sequence_length: 512,
    use_instruction_prefix: false,
    cache_capacity: 10000,
};

let mut service = EmbeddingService::new(config)?;
service.initialize()?;
```

## Model Bundling

### Directory Structure

Models are stored in the centralized NodeSpace data directory (same pattern as database):

```
~/.nodespace/
├── database/
│   └── nodespace.db          # Database location
└── models/
    └── BAAI-bge-small-en-v1.5/   # Model location
        ├── model.onnx
        └── tokenizer.json
```

### Downloading the Model

1. **Install huggingface-hub CLI** (developers only):
   ```bash
   pip install huggingface-hub
   ```

2. **Download model files to ~/.nodespace/models/**:
   ```bash
   # Create directory
   mkdir -p ~/.nodespace/models

   # Download model
   huggingface-cli download BAAI/bge-small-en-v1.5 --local-dir ~/.nodespace/models/BAAI-bge-small-en-v1.5
   ```

**Note for end users**: The model will be bundled with the application. Manual download is only needed for development.

3. **Convert to ONNX** (if not already in ONNX format):
   ```python
   from optimum.onnxruntime import ORTModelForFeatureExtraction
   from transformers import AutoTokenizer

   model = ORTModelForFeatureExtraction.from_pretrained("BAAI/bge-small-en-v1.5", export=True)
   tokenizer = AutoTokenizer.from_pretrained("BAAI/bge-small-en-v1.5")

   model.save_pretrained("packages/nlp-engine/models/bge-small-en-v1.5")
   tokenizer.save_pretrained("packages/nlp-engine/models/bge-small-en-v1.5")
   ```

## Testing

Run tests with the embedding service feature enabled:

```bash
cargo test --features embedding-service
```

Run tests without model (stub mode):

```bash
cargo test
```

## Feature Flags

- `embedding-service` (default): Enable full embedding functionality with Candle + ONNX
  - When disabled, provides stub implementation returning zero vectors

## Device Support

The service automatically detects and uses the best available device:

1. **Metal GPU** (macOS) - Best performance
2. **CPU Fallback** - Automatic fallback if GPU unavailable

Check device being used:

```rust
println!("Using device: {}", service.device_info());
```

## Cache Management

```rust
// Get cache statistics
let (size, capacity) = service.cache_stats();
println!("Cache: {}/{} entries", size, capacity);

// Clear cache
service.clear_cache();
```

## Error Handling

```rust
use nodespace_nlp_engine::{EmbeddingService, EmbeddingError};

match service.generate_embedding("text").await {
    Ok(embedding) => println!("Success: {} dims", embedding.len()),
    Err(EmbeddingError::NotInitialized) => println!("Service not initialized"),
    Err(EmbeddingError::ModelNotFound(path)) => println!("Model not found at: {}", path),
    Err(e) => println!("Error: {}", e),
}
```

## Dependencies

- `candle-core`: Core tensor operations
- `candle-nn`: Neural network primitives
- `candle-onnx`: ONNX model loading
- `tokenizers`: Text tokenization
- `lru`: LRU cache with automatic eviction
- `tokio`: Async runtime

---

# Topic Embedding Service

The `TopicEmbeddingService` provides adaptive chunking and embedding generation for topic nodes, built on top of the NLP engine.

## Features

- **Adaptive Chunking**: Automatically adjusts embedding strategy based on content size
- **Turso Native Vector Search**: Uses DiskANN algorithm for fast approximate nearest neighbor search
- **Debounced Re-embedding**: Batches rapid content changes to avoid excessive embedding operations
- **Metadata Tracking**: Stores embedding type, generation time, and token counts

## Architecture

### Chunking Strategies

The service uses three strategies based on estimated token count:

| Token Range | Strategy | Implementation |
|------------|----------|----------------|
| < 512 | **Complete Topic** | Single embedding for entire content |
| 512-2048 | **Summary + Sections** | Summary embedding + top-level section embeddings |
| > 2048 | **Hierarchical** | Summary embedding + recursive section embeddings |

### Token Estimation

Conservative approach to prevent underestimation:

```rust
fn estimate_tokens(content: &str) -> usize {
    // 1 token ≈ 3.5 chars + 20% safety margin
    ((content.len() as f32 / 3.5) * 1.2).ceil() as usize
}
```

**Why conservative?**
- Better to overestimate than truncate important content
- Handles non-English and technical text more safely
- Prevents choosing wrong chunking strategy

## Usage

### Basic Setup

```rust
use nodespace_core::services::TopicEmbeddingService;
use nodespace_core::db::DatabaseService;
use std::sync::Arc;

// Initialize database and NLP engine
let db = Arc::new(DatabaseService::new(db_path).await?);

// Create embedding service with defaults
let service = TopicEmbeddingService::new_with_defaults(db)?;
```

### Generate Embeddings

```rust
// Embed a single topic
service.embed_topic("topic-node-id").await?;

// The service automatically:
// 1. Fetches topic and children from database
// 2. Estimates total tokens
// 3. Chooses appropriate chunking strategy
// 4. Generates embeddings
// 5. Stores in database with metadata
```

### Semantic Search

```rust
// Fast approximate search (uses DiskANN index)
let results = service.search_topics(
    "machine learning algorithms",
    0.7,  // similarity threshold (lower = more similar)
    20    // max results
).await?;

// Exact search (slower but more accurate)
let exact_results = service.exact_search_topics(
    "machine learning algorithms",
    0.7,
    20
).await?;
```

### Content Change Handling

```rust
// Debounced re-embedding (waits 5 seconds of inactivity)
service.schedule_update_topic_embedding("topic-id").await;

// Immediate re-embedding (no debouncing)
service.update_topic_embedding("topic-id").await?;
```

## Database Schema

### Vector Storage

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    -- ... other fields ...
    embedding_vector F32_BLOB(384),  -- Native Turso vector type
    properties JSON
);

-- Vector index for fast ANN search
CREATE INDEX idx_nodes_embedding_vector
ON nodes(libsql_vector_idx(embedding_vector));
```

### Embedding Metadata

Stored in `properties.embedding_metadata`:

```json
{
  "embedding_metadata": {
    "type": "complete_topic" | "topic_summary" | "topic_section",
    "parent_topic": "topic-id",
    "generated_at": "2025-10-10T12:00:00Z",
    "token_count": 345,
    "depth": 0
  }
}
```

## Vector Search

### Turso Native Functions

```sql
-- Fast approximate search (uses DiskANN index)
SELECT * FROM vector_top_k(
    'idx_nodes_embedding_vector',
    vector(?),  -- query embedding blob
    20          -- limit
) vt
JOIN nodes n ON n.rowid = vt.rowid
WHERE n.node_type = 'topic';

-- Exact cosine distance search
SELECT *,
    vector_distance_cosine(embedding_vector, vector(?)) as distance
FROM nodes
WHERE node_type = 'topic'
  AND embedding_vector IS NOT NULL
  AND distance < 0.7
ORDER BY distance ASC
LIMIT 20;
```

## Performance

### Targets

- **Embedding Generation**: < 3 seconds (CPU), < 1 second (GPU)
- **Vector Search**: < 100ms for 10k+ nodes (with DiskANN index)
- **Cache Hit**: < 5ms (NLP engine cache)

### Optimization Tips

1. **Use approximate search** for interactive queries (faster)
2. **Use exact search** for critical operations (more accurate)
3. **Batch operations** when embedding multiple topics
4. **Debounce content changes** to avoid excessive re-embedding

## Tauri Commands

Frontend integration via Tauri commands:

```typescript
// Generate embedding
await invoke('generate_topic_embedding', { topicId: 'id' });

// Search topics
const results = await invoke('search_topics', {
    params: {
        query: 'machine learning',
        threshold: 0.7,
        limit: 20
    }
});

// Batch operations
const result = await invoke('batch_generate_embeddings', {
    topicIds: ['id1', 'id2', 'id3']
});

console.log(`Success: ${result.successCount}`);
console.log(`Errors:`, result.failedEmbeddings);
```

## Testing

### Unit Tests

Run without NLP model:

```bash
cargo test --lib
```

### Integration Tests

Requires NLP model files:

```bash
cargo test --lib -- --ignored
```

### Performance Tests

Creates 10k+ nodes:

```bash
cargo test --lib test_vector_search_performance -- --ignored --nocapture
```

## Troubleshooting

### Model Not Found Error

```
ModelNotFound("Model file not found: ~/.nodespace/models/BAAI-bge-small-en-v1.5/model.onnx")
```

**Solution**: Download model files (see "Model Bundling" section above)

### Slow Embedding Generation

**Checklist**:
1. Check if GPU acceleration is active: `service.device_info()`
2. Verify cache is enabled and warm
3. Check content size (> 2048 tokens will be slower)

### Vector Search Returns No Results

**Checklist**:
1. Verify embeddings exist: `SELECT COUNT(*) FROM nodes WHERE embedding_vector IS NOT NULL`
2. Check threshold value (lower = more strict, higher = more lenient)
3. Verify vector index exists: `PRAGMA index_list('nodes')`

## Migration Guide

### From In-Memory Vector Search

If migrating from an in-memory vector search implementation:

1. **Remove in-memory logic** - Turso handles vector search natively
2. **Update queries** - Use `vector_top_k()` instead of in-memory distance calculations
3. **Add vector index** - Create `libsql_vector_idx` index for performance
4. **Test performance** - Should be faster with DiskANN algorithm

### Changing Embedding Model

If upgrading to a different embedding model:

1. **Update `EMBEDDING_DIMENSION` constant** in `embedding_service.rs`
2. **Migrate database schema**: `F32_BLOB(384)` → `F32_BLOB(new_dimension)`
3. **Re-generate all embeddings** with new model
4. **Update documentation** with new model details

## License

MIT

## References

- [BAAI/bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5)
- [Candle ML Framework](https://github.com/huggingface/candle)
- [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard)
- [Turso Vector Search](https://turso.tech/vector)
- [Issue #183: Implementation Details](https://github.com/malibio/nodespace-core/issues/183)
