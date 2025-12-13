# NodeSpace Embedding Architecture

Vector embedding service for semantic search across NodeSpace knowledge graphs.

## Overview

NodeSpace uses a **root-aggregate embedding model** where only root nodes (nodes without parents) of embeddable types get embedded. The embedding represents the semantic content of the entire subtree (root + all descendants).

## Core Concepts

### Root-Aggregate Model

| Concept | Description |
|---------|-------------|
| **Root Node** | A node with no parent edge (top of a subtree) |
| **Embeddable Types** | `text`, `header`, `code-block`, `schema` |
| **NOT Embedded** | `task`, `date`, `person`, `ai-chat`, any child node |
| **Embedding Content** | Root content + ALL descendant content aggregated |

**Why root-aggregate?**
- Child nodes often lack semantic meaning alone (a bullet point under a header)
- Tasks are queryable via structured filters, not semantic search
- Date nodes are containers for daily journal entries, not semantic content
- The "document" is the root + its children as a unit

### Chunking Strategy

Large content is split into overlapping chunks (model limit is 512 tokens):

| Content Size | Strategy |
|--------------|----------|
| < 512 tokens | Single embedding |
| > 512 tokens | Multiple chunks with ~100 token overlap |

Each chunk becomes a separate embedding record. Search returns the best-matching chunk per node.

## Database Schema

Embeddings are stored in a dedicated `embedding` table (not on the `node` table):

```sql
DEFINE TABLE embedding SCHEMAFULL;

-- Link to root node (consistent with spoke pattern)
DEFINE FIELD node ON embedding TYPE record<node>;

-- Vector data
DEFINE FIELD vector ON embedding TYPE array<float>;
DEFINE FIELD dimension ON embedding TYPE int DEFAULT 384;

-- Model info (future multi-model support)
DEFINE FIELD model_name ON embedding TYPE string DEFAULT 'bge-small-en-v1.5';

-- Chunking
DEFINE FIELD chunk_index ON embedding TYPE int DEFAULT 0;
DEFINE FIELD chunk_start ON embedding TYPE int DEFAULT 0;
DEFINE FIELD chunk_end ON embedding TYPE int;
DEFINE FIELD total_chunks ON embedding TYPE int DEFAULT 1;

-- Content tracking
DEFINE FIELD content_hash ON embedding TYPE string;
DEFINE FIELD token_count ON embedding TYPE int;

-- Staleness
DEFINE FIELD stale ON embedding TYPE bool DEFAULT true;

-- Error tracking
DEFINE FIELD error_count ON embedding TYPE int DEFAULT 0;
DEFINE FIELD last_error ON embedding TYPE string;

-- Timestamps
DEFINE FIELD created_at ON embedding TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON embedding TYPE datetime DEFAULT time::now();

-- Indexes
DEFINE INDEX idx_embedding_node ON embedding COLUMNS node;
DEFINE INDEX idx_embedding_stale ON embedding COLUMNS stale;
DEFINE INDEX idx_embedding_unique ON embedding COLUMNS node, model_name, chunk_index UNIQUE;
```

## Architecture

### NLP Engine

- **Model**: BAAI/bge-small-en-v1.5 (384 dimensions, 512 token limit)
- **Runtime**: Candle + ONNX
- **GPU Support**: Metal (macOS) with CPU fallback
- **Cache**: LRU cache for repeated queries

### Embedding Queue (Backend-Managed)

All embedding logic lives in `NodeService` - no frontend involvement.

```rust
struct EmbeddingQueue {
    pending: HashMap<String, Instant>,  // root_id -> last_change_time
    debounce_duration: Duration,        // 30 seconds default
}
```

### Change Flow

| Event | Action |
|-------|--------|
| New root node created (embeddable type) | Add root_id to queue, start debounce |
| Root node edited | Add root_id to queue, reset debounce |
| Child node created/edited/deleted | Find root via graph query, add root_id to queue, reset debounce |
| Child node moved | Add old root + new root to queue |

### Processing Flow

When debounce timer expires (30s no changes):

1. **Fetch fresh content** - Root node + all descendants via `get_descendants()`
2. **Aggregate content** - Combine root + children content
3. **Chunk if needed** - Split into 512-token chunks with overlap
4. **Generate embeddings** - One per chunk via NLP engine
5. **Store in database** - Insert/update embedding records
6. **Clear stale flag** - Mark `stale = false`

### Finding Root from Child

Single graph query to traverse up to root:

```sql
-- Find ancestor with no incoming has_child edge
SELECT VALUE in FROM has_child
WHERE out = $child_id
CONNECT BY out = in
-- Filter to node with no parent
```

## Usage

### Semantic Search

```rust
// Search returns best-matching chunk per node
let results = embedding_service.search("authentication flow", 0.5, 20).await?;

for (node, similarity) in results {
    println!("{}: {:.2}", node.id, similarity);
}
```

### Search Query (SurrealDB)

```sql
SELECT
    node,
    math::max(vector::similarity::cosine(vector, $query_vector)) AS best_similarity
FROM embedding
WHERE stale = false
GROUP BY node
HAVING best_similarity > $threshold
ORDER BY best_similarity DESC
LIMIT $limit;
```

### Tauri Commands

```typescript
// Search
const results = await invoke('search_roots', {
    params: {
        query: 'authentication',
        threshold: 0.5,
        limit: 20
    }
});

// Manual re-index (development/debugging)
await invoke('reindex_root', { rootId: 'node-id' });
```

## NLP Engine Details

### Model Bundling

Models are stored in the centralized NodeSpace data directory:

```
~/.nodespace/
├── database/
│   └── nodespace.db
└── models/
    └── BAAI-bge-small-en-v1.5/
        ├── model.onnx
        └── tokenizer.json
```

### Downloading the Model (Development)

```bash
pip install huggingface-hub
mkdir -p ~/.nodespace/models
huggingface-cli download BAAI/bge-small-en-v1.5 --local-dir ~/.nodespace/models/BAAI-bge-small-en-v1.5
```

### Performance Targets

| Operation | Target |
|-----------|--------|
| Model Loading | < 30 seconds |
| Single Embedding | < 10ms (Metal GPU) |
| Batch 100 Embeddings | < 500ms |
| Cache Hit | < 5ms |
| Memory Footprint | < 500MB |

## Configuration

```rust
pub struct EmbeddingConfig {
    pub debounce_duration: Duration,    // Default: 30 seconds
    pub max_tokens_per_chunk: usize,    // Default: 512
    pub overlap_tokens: usize,          // Default: 100
    pub max_descendants: usize,         // Default: 1000
    pub max_content_size: usize,        // Default: 10MB
}
```

## Error Handling

| Error | Behavior |
|-------|----------|
| Content too large | Truncate with warning, mark `truncated` flag |
| NLP generation failed | Retry up to 3 times, then log error |
| Circular reference | Detect via visited set, skip with warning |
| Node not found | Skip (may have been deleted) |

Errors are tracked in the embedding record:
- `error_count` - Number of failed attempts
- `last_error` - Most recent error message

## Testing

```bash
# Unit tests (no model required)
cargo test --lib

# Integration tests (requires model)
cargo test --lib -- --ignored

# With embedding service feature
cargo test --features embedding-service
```

## Future Enhancements

- **Semantic Code Search** - Index local code repositories via tree-sitter, embed humanized representations. See [Semantic Code Search Architecture](../features/semantic-code-search.md)
- **Multi-model support** - Schema already supports `model_name` field
- **Node type plugin embeddability** - Let node types declare if/how they embed
- **Vector index optimization** - HNSW/DiskANN for sub-linear search
- **Adaptive debounce** - Shorter for small docs, longer for large

## References

- [BAAI/bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5)
- [Candle ML Framework](https://github.com/huggingface/candle)
- [SurrealDB Vector Functions](https://surrealdb.com/docs/surrealql/functions/vector)
