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
    let embedding = service.generate_embedding("Hello, world!").await?;
    println!("Generated embedding with {} dimensions", embedding.len());

    // Generate batch embeddings
    let texts = vec!["First text", "Second text", "Third text"];
    let embeddings = service.generate_batch(texts).await?;
    println!("Generated {} embeddings", embeddings.len());

    Ok(())
}
```

### Storage Integration (Turso)

```rust
use nodespace_nlp_engine::EmbeddingService;

// Generate embedding
let embedding = service.generate_embedding("search query").await?;

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

## License

MIT

## References

- [BAAI/bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5)
- [Candle ML Framework](https://github.com/huggingface/candle)
- [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard)
