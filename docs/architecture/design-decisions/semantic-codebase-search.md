# Semantic Codebase Search Architecture

## Overview

This document describes the architecture for semantic codebase search in NodeSpace, a desktop-first knowledge management application. The system extracts code symbols, generates embeddings, and enables natural language search across codebases.

## Decision Context

NodeSpace needs intelligent code understanding for:

- **Contextual AI Assistance**: Providing relevant code context to the AI chat system
- **Semantic Search**: Finding code by meaning, not just text matching
- **Knowledge Graph Integration**: Connecting code symbols to notes and documentation
- **Cross-Repository Understanding**: Linking related code across projects

As a desktop application, the solution must:

- Work offline without external services
- Run efficiently on consumer hardware (8-16GB RAM)
- Support GPU acceleration where available (Metal/CUDA/Vulkan)
- Share resources with other AI features (text generation)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     NodeSpace Desktop App                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐  │
│  │  tree-sitter │───>│ InferenceStack│───>│    SurrealDB     │  │
│  │   (Parser)   │    │  (llama.cpp)  │    │ (Vector Storage) │  │
│  └──────────────┘    └──────────────┘    └──────────────────┘  │
│         │                   │                      │            │
│         v                   v                      v            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                   Unified Memory Pool                     │  │
│  │  - Embedding model: always loaded (~140MB)               │  │
│  │  - Generation model: lazy loaded (~3GB when needed)      │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Component Selection

### Code Parsing: tree-sitter

**Why tree-sitter:**
- Incremental parsing (fast re-indexing on file changes)
- Language-agnostic AST representation
- Battle-tested in VS Code, Neovim, GitHub
- Rust bindings with excellent performance

**Supported Languages:**
- TypeScript/JavaScript (including TSX/JSX)
- Rust
- (Extensible to Python, Go, etc.)

**Extracted Information:**
- Functions, methods, classes, interfaces, types
- Documentation strings and comments
- Visibility modifiers (public/private/export)
- Signatures and type annotations
- Line number ranges for navigation

### Embeddings: llama.cpp with nomic-embed-text-v1.5

**Why llama.cpp over candle-onnx:**

| Aspect | candle-onnx | llama.cpp |
|--------|-------------|-----------|
| GPU Support | Limited (Metal issues) | Excellent (Metal/CUDA/Vulkan) |
| Model Ecosystem | ONNX only | GGUF (large ecosystem) |
| Batching | Striding errors on Metal | Native support |
| Build Complexity | Simpler | More dependencies |
| Performance | Good | Excellent |

**Why nomic-embed-text-v1.5:**
- **Long Context**: 8192 tokens (vs 512 for many alternatives)
- **Task Prefixes**: `search_document:` and `search_query:` for asymmetric retrieval
- **Size**: Q8_0 quantization is ~140MB (similar to bge-small ONNX)
- **Quality**: Competitive with larger models on MTEB benchmarks
- **Vision Alignment**: Future path to multimodal code understanding

**Alternative Models Considered:**

| Model | Dimensions | Max Tokens | Size (Q8) | Notes |
|-------|------------|------------|-----------|-------|
| bge-small-en-v1.5 | 384 | 512 | ~130MB | Previous choice, limited context |
| nomic-embed-text-v1.5 | 768 | 8192 | ~140MB | **Selected** |
| mxbai-embed-large | 1024 | 512 | ~670MB | High quality but large |
| e5-mistral-7b | 4096 | 32K | ~14GB | Too large for desktop |

### Vector Storage: SurrealDB with RocksDB

**Why SurrealDB:**
- Embedded mode (no separate database process)
- Native vector search with cosine similarity
- Schema flexibility for symbol metadata
- RocksDB backend for persistence
- Graph queries for relationship traversal

**Schema Design:**
```sql
DEFINE TABLE symbols SCHEMAFULL;
DEFINE FIELD repository ON symbols TYPE string;
DEFINE FIELD file_path ON symbols TYPE string;
DEFINE FIELD name ON symbols TYPE string;
DEFINE FIELD kind ON symbols TYPE string;  -- function, class, interface, etc.
DEFINE FIELD content ON symbols TYPE string;
DEFINE FIELD signature ON symbols TYPE option<string>;
DEFINE FIELD docstring ON symbols TYPE option<string>;
DEFINE FIELD embedding ON symbols TYPE array<float>;
DEFINE FIELD start_line ON symbols TYPE int;
DEFINE FIELD end_line ON symbols TYPE int;

DEFINE INDEX idx_embedding ON symbols FIELDS embedding MTREE DIMENSION 768;
```

## Unified Inference Stack

### Design Rationale

Desktop applications face memory constraints. Rather than loading separate models for embeddings and text generation, we use a unified `InferenceStack` that:

1. **Always loads** the embedding model (small, ~140MB, frequent use)
2. **Lazy loads** the generation model (large, ~3GB, occasional use)
3. **Shares** the llama.cpp backend and GPU resources
4. **Allows unloading** the generation model when memory is needed

### Implementation

```rust
pub struct InferenceStack {
    backend: LlamaBackend,
    config: InferenceConfig,

    // Always loaded - embedding model (small, ~140MB)
    embedding_model: LlamaModel,
    embedding_dimension: usize,

    // Lazy loaded - generation model (large, ~3GB)
    generation_model: Option<LlamaModel>,

    // Embedding cache (LRU, default 10,000 entries)
    cache: Arc<Mutex<LruCache<String, Vec<f32>>>>,
}
```

### Key APIs

```rust
impl InferenceStack {
    // Embedding (uses search_document: prefix)
    pub fn embed_document(&self, text: &str) -> Result<Vec<f32>>;

    // Query embedding (uses search_query: prefix)
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>>;

    // Text generation (lazy loads model)
    pub fn generate(&mut self, prompt: &str, params: &GenerationParams) -> Result<String>;

    // Memory management
    pub fn unload_generation_model(&mut self);
    pub fn is_generation_ready(&self) -> bool;
}
```

### Memory Profile

| Component | Memory | Loaded |
|-----------|--------|--------|
| Embedding model (nomic Q8) | ~140MB | Always |
| Embedding cache (10K entries) | ~30MB | Always |
| Generation model (Ministral 3B Q4) | ~2-3GB | On demand |
| **Typical usage** | **~200MB** | Embeddings only |
| **Peak usage** | **~3.5GB** | With generation |

## Performance Characteristics

### Indexing Performance

Tested on nodespace-core (12,144 symbols):

| Metric | Value |
|--------|-------|
| Total indexing time | ~130 seconds |
| Embedding throughput | ~93 symbols/second |
| Parse time | ~15% of total |
| Embedding time | ~85% of total |

### Search Performance

| Operation | Latency |
|-----------|---------|
| Query embedding | ~10-15ms |
| Vector search (10K symbols) | ~5-10ms |
| Total search latency | ~20-30ms |

### Incremental Updates

- File hash tracking prevents re-processing unchanged files
- Typical re-index of changed files: <1 second
- Background indexing doesn't block UI

## Desktop-Specific Considerations

### Why This Architecture is Desktop-Ready

1. **Single-Process Design**: No external services to manage (unlike Ollama or separate embedding servers)

2. **Resource Sharing**: Embedding and generation share GPU memory efficiently through unified backend

3. **Graceful Degradation**:
   - Works without GPU (CPU fallback)
   - Works without generation model (embeddings-only mode)
   - LRU cache reduces repeated embedding costs

4. **Memory Awareness**:
   - Lazy loading prevents unnecessary memory consumption
   - `unload_generation_model()` frees memory when needed
   - Configurable cache size for different hardware

5. **Cross-Platform GPU**:
   - macOS: Metal acceleration (default)
   - Windows: CUDA for NVIDIA GPUs
   - Linux: CUDA or Vulkan
   - Fallback: CPU (slower but functional)

### Configuration

```rust
pub struct InferenceConfig {
    pub embedding_model_path: PathBuf,     // Required
    pub generation_model_path: Option<PathBuf>,  // Optional
    pub n_gpu_layers: u32,                 // 99 = all layers
    pub embedding_ctx_size: u32,           // 8192 for nomic
    pub generation_ctx_size: u32,          // 4096 typical
    pub n_threads: i32,                    // CPU threads
    pub cache_capacity: usize,             // LRU cache size
}
```

### Build Configuration

```toml
[features]
default = []
cuda = ["llama-cpp-2/cuda"]
metal = ["llama-cpp-2/metal"]
vulkan = ["llama-cpp-2/vulkan"]
```

## Integration with NodeSpace

### AI Chat Context

The semantic search provides relevant code context for AI conversations:

```rust
// When user asks about code in AI chat
let results = indexer.search(
    "authentication middleware",
    limit: 5,
    threshold: 0.6,
    repository: Some("nodespace-core"),
    language: None,
    kind: Some("function"),
).await?;

// Results feed into AI context
let context = results.iter()
    .map(|r| format!("{}:\n{}", r.file_path, r.content))
    .collect::<Vec<_>>()
    .join("\n\n");
```

### Knowledge Graph Integration

Symbols can be linked to notes and documentation:

```
[Code Symbol] --references--> [Documentation Node]
[Code Symbol] --implements--> [Architecture Decision]
[Code Symbol] --relates_to--> [Code Symbol]
```

## Deployment

### Model Distribution

The embedding model is **bundled with the application installer**:

| Model | Size | Location |
|-------|------|----------|
| nomic-embed-text-v1.5.Q8_0.gguf | ~140MB | `resources/models/` |

**Rationale:**
- Guarantees offline-first functionality from first launch
- No internet required after installation
- Acceptable installer size increase (~150MB)
- Consistent experience across all users

### Tauri Resource Configuration

```json
// tauri.conf.json
{
  "bundle": {
    "resources": [
      "resources/models/nomic-embed-text-v1.5.Q8_0.gguf"
    ]
  }
}
```

### Runtime Model Path Resolution

```rust
use tauri::api::path::resource_dir;

fn get_embedding_model_path(app: &tauri::AppHandle) -> PathBuf {
    let resource_path = app.path_resolver()
        .resolve_resource("models/nomic-embed-text-v1.5.Q8_0.gguf")
        .expect("Failed to resolve embedding model path");
    resource_path
}
```

### Platform-Specific Locations

| Platform | Bundled Model Location |
|----------|------------------------|
| macOS | `NodeSpace.app/Contents/Resources/models/` |
| Windows | `C:\Program Files\NodeSpace\resources\models\` |
| Linux | `/usr/share/nodespace/resources/models/` |

### Generation Model (In-App Download)

The text generation model (~3GB) is **downloaded on first use**:

| Model | Size | Source |
|-------|------|--------|
| Ministral-3B-Instruct (Q4_K_M) | ~2.5GB | HuggingFace |

**Download Flow:**

```
┌─────────────────────────────────────────────────────┐
│  User triggers generation feature                    │
│  (e.g., "Explain this code", "Generate docstring")  │
└─────────────────────────────────────────────────────┘
                         │
                         v
┌─────────────────────────────────────────────────────┐
│  Check: Model exists in app data directory?         │
│  Location: ~/Library/Application Support/NodeSpace/ │
│            models/ministral-3b-instruct-q4_k_m.gguf │
└─────────────────────────────────────────────────────┘
                         │
              ┌──────────┴──────────┐
              │ No                  │ Yes
              v                     v
┌─────────────────────────┐  ┌─────────────────────────┐
│  Show download dialog:  │  │  Load model and proceed │
│  "Download AI Model"    │  └─────────────────────────┘
│  ~2.5GB, ~5 min on fast │
│  connection             │
│  [Download] [Cancel]    │
└─────────────────────────┘
              │
              v
┌─────────────────────────────────────────────────────┐
│  Download with progress:                            │
│  ████████████░░░░░░░░ 58% (1.4GB / 2.5GB)          │
│  Estimated: 2 min remaining                         │
│  [Cancel]                                           │
└─────────────────────────────────────────────────────┘
              │
              v
┌─────────────────────────────────────────────────────┐
│  Verify checksum → Move to final location → Load   │
└─────────────────────────────────────────────────────┘
```

**Implementation Notes:**

```rust
const GENERATION_MODEL_URL: &str =
    "https://huggingface.co/bartowski/Ministral-3b-instruct-GGUF/resolve/main/Ministral-3b-instruct-Q4_K_M.gguf";
const GENERATION_MODEL_SHA256: &str = "..."; // Verify integrity

fn get_generation_model_path() -> PathBuf {
    let app_data = dirs::data_dir()
        .expect("No app data directory")
        .join("NodeSpace")
        .join("models");
    app_data.join("ministral-3b-instruct-q4_k_m.gguf")
}

async fn ensure_generation_model(
    on_progress: impl Fn(u64, u64),  // (downloaded, total)
) -> Result<PathBuf> {
    let path = get_generation_model_path();
    if path.exists() {
        return Ok(path);
    }

    // Create directory
    std::fs::create_dir_all(path.parent().unwrap())?;

    // Download with progress callback
    download_with_progress(GENERATION_MODEL_URL, &path, on_progress).await?;

    // Verify checksum
    verify_sha256(&path, GENERATION_MODEL_SHA256)?;

    Ok(path)
}
```

**User Data Locations:**

| Platform | Model Storage |
|----------|---------------|
| macOS | `~/Library/Application Support/NodeSpace/models/` |
| Windows | `%APPDATA%\NodeSpace\models\` |
| Linux | `~/.local/share/nodespace/models/` |

**UX Considerations:**

- **No account required**: HuggingFace allows anonymous downloads for public, ungated models
- Download is **opt-in**: Only triggered when user needs generation features
- **Resumable**: Support partial downloads if connection drops
- **Cancellable**: User can cancel and use embedding-only features
- **Offline after download**: Model persists across app restarts
- **Clear storage info**: Show model size in settings for users to manage disk space

## Future Enhancements

### Planned

1. **Streaming Generation**: Real-time token output for chat
2. **Temperature/Top-p Sampling**: Better generation quality
3. **Multi-Repository Search**: Unified search across projects

### Considered

1. **Incremental Embedding Updates**: Update only changed functions
2. **Code-Aware Chunking**: Split large functions intelligently
3. **Cross-Language Linking**: Connect TypeScript interfaces to Rust implementations

## Technical Notes

### Encoder vs Decoder Models

nomic-embed-text is a BERT-based encoder model. Key differences from decoder models:

```rust
// Encoder models (embeddings): use encode()
ctx.encode(&mut batch)?;
let embedding = ctx.embeddings_seq_ith(0)?;

// Decoder models (generation): use decode()
ctx.decode(&mut batch)?;
let token = sample_next_token(&ctx)?;
```

**Important**: For encoder models, `n_ubatch` must be >= number of tokens in the batch.

### Task Prefixes for Asymmetric Retrieval

nomic-embed uses different prefixes for documents vs queries:

```rust
const SEARCH_DOCUMENT_PREFIX: &str = "search_document: ";
const SEARCH_QUERY_PREFIX: &str = "search_query: ";

// Indexing: prefix with search_document:
let doc_embedding = embed(&format!("{}{}", SEARCH_DOCUMENT_PREFIX, code));

// Searching: prefix with search_query:
let query_embedding = embed(&format!("{}{}", SEARCH_QUERY_PREFIX, query));
```

This asymmetric approach improves retrieval quality by ~5-10% on benchmarks.

## Conclusion

The semantic codebase search architecture provides:

- **Offline-First**: No external services or internet required
- **Desktop-Optimized**: Reasonable memory usage with lazy loading
- **GPU-Accelerated**: Metal/CUDA/Vulkan support for fast inference
- **Unified Stack**: Single backend for embeddings and generation
- **Production-Ready**: Tested on real codebase with good performance

This architecture integrates cleanly with NodeSpace's existing AI infrastructure while maintaining the desktop-first, privacy-respecting principles of the application.
