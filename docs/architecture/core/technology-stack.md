# Technology Stack

## Overview

NodeSpace's technology stack is carefully chosen to provide optimal performance, developer experience, and maintainability for an AI-native desktop application. The architecture centers on a **Tauri desktop application** with **embedded SurrealDB** and **Rust/TypeScript services**, creating a unified approach for sophisticated knowledge management capabilities with direct database access and zero abstraction overhead.

## Core Architecture

### Desktop Framework: Tauri 2.0

**Why Tauri:**
- **Native Performance**: Compiled to native binaries with minimal overhead
- **Security**: Sandboxed environment with granular permission system
- **Cross-Platform**: Single codebase for macOS, Windows, and Linux
- **Small Bundle Size**: ~10-20MB vs 100MB+ Electron applications
- **Web Technology Integration**: Modern web APIs with native capabilities

**Key Features Used:**
- **Tray Icon Support**: System tray integration for always-available access
- **Native File System**: Direct file access without web security limitations
- **IPC Commands**: Type-safe communication between frontend and backend
- **Auto-Updater**: Built-in application update mechanism
- **Native Notifications**: System-level notification integration
- **Native Menus**: Context menus for multi-node selection operations

### Rust Business Logic Layer

**Why Rust for Business Logic:**
- **MCP Server Integration**: Expose business logic to AI agents via Model Context Protocol
- **AI Integration**: Direct llama.cpp-rs integration for local inference
- **Type Safety**: Compile-time guarantees for critical business logic
- **Performance**: Native performance for database operations and AI processing
- **Future Vision**: Eventually move everything to Rust (including frontend via Dioxus/Leptos)

**Rust Business Logic Stack:**
```toml
[dependencies]
# Database layer
surrealdb = "2.1"               # SurrealDB embedded (RocksDB storage)
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# AI Integration
llama-cpp-rs = "0.5"            # Local LLM inference (future)
fastembed = "0.7"               # Embedding generation (future)

# MCP Server
mcp-sdk = "0.1"                 # Model Context Protocol

# Async runtime
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
thiserror = "1.0"

# Type safety and validation
validator = "0.18"
```

**Rust Architecture Patterns:**
- **Direct SurrealDB Access**: Zero abstraction overhead, single database backend
- **Pure JSON Schema**: All entity data in JSON properties field (SCHEMALESS tables)
- **Schema-as-Node**: Schemas stored as nodes with `node_type = "schema"`
- **Graph Relations**: Native SurrealDB RELATE for mentions and backlinks
- **Optimistic Concurrency**: Version-based conflict detection
- **MCP Exposure**: Business logic accessible to AI agents via stdio protocol

### Frontend: Svelte

**Why Svelte:**
- **Compile-Time Optimizations**: No virtual DOM overhead
- **Reactive by Default**: Automatic state management and updates
- **Small Bundle Size**: Minimal runtime footprint
- **Developer Experience**: Intuitive syntax and excellent tooling
- **Performance**: Faster than React/Vue for complex UIs

**Key Features:**
- **SvelteKit**: Full-stack framework with file-based routing
- **Reactive Stores**: Advanced state management for multi-node selection
- **Component Composition**: Reusable UI components with hierarchical display
- **CSS-in-JS**: Scoped styling with theme-aware selection highlighting
- **TypeScript Integration**: Type-safe frontend development
- **Advanced Interactions**: Multi-modal selection (mouse, keyboard, touch)
- **Accessibility**: WCAG 2.1 compliant hierarchical navigation

**UI Libraries & Design System:**
```json
{
  "dependencies": {
    "svelte": "^5.0.0",
    "@sveltejs/kit": "^2.9.0", 
    "typescript": "~5.6.2",
    "@types/node": "^20.14.0",
    "vite": "^6.0.3",
    "tailwindcss": "^4.1.11",
    "clsx": "^2.1.1",
    "tailwind-merge": "^3.3.1",
    "tailwind-variants": "^1.0.0"
  }
}
```

**Design System: shadcn-svelte Integration**
- **shadcn-svelte**: Professional UI component library with copy-paste approach
- **Unified Color System**: Single source of truth using shadcn-svelte variables
- **Hybrid Architecture**: Industry-standard components + custom NodeSpace components
- **Full Customization**: Components copied into codebase for complete control

**Color System Architecture:**
```css
/* NodeSpace Professional Theme (mapped to shadcn-svelte variables) */
:root {
  --primary: 210 100% 40%;              /* NodeSpace Blue #007acc */
  --background: 0 0% 100%;              /* Clean white backgrounds */
  --foreground: 220 13% 10%;            /* Dark text #1a1a1a */
  --muted: 210 40% 98%;                 /* Light gray panels */
  --border: 214 32% 91%;                /* Subtle borders #e1e5e9 */
  --radius: 0.5rem;                     /* Consistent border radius */
}

.dark {
  --primary: 210 100% 50%;              /* Brighter blue for dark mode */
  --background: 224 71% 4%;             /* Dark blue-gray background */
  --foreground: 213 31% 91%;            /* Light text for readability */
  /* ... additional dark mode mappings */
}
```

**Component Hierarchy:**
```
Tier 1: shadcn-svelte Foundation (Button, Input, Card, Dialog, etc.)
  ↓ Professional, accessible, industry-standard patterns
  
Tier 2: NodeSpace Domain Components (BaseNode, TextNode, etc.)
  ↓ Built on Tier 1 foundation, adds AI and knowledge management features
```

## AI Integration Stack

### Primary: mistral.rs

**Why mistral.rs:**
- **Native Rust Integration**: Direct library integration, no HTTP overhead
- **UQFF Support**: Ultra-Quick File Format for 3-5x faster model loading
- **Metal GPU Acceleration**: Optimized performance on macOS with Metal
- **Memory Efficiency**: Granular control over GPU/CPU memory allocation
- **Model Flexibility**: Support for various quantization levels and model sizes

**Configuration:**
```rust
use mistralrs::{UqffVisionModelBuilder, TextMessageRole, RequestBuilder};

pub struct AIConfig {
    pub model_path: String,           // "/path/to/gemma-3n-8b-it-UQFF"
    pub max_context_length: usize,    // 8192 tokens
    pub temperature: f32,             // 0.7 for balanced creativity
    pub max_tokens: usize,            // 1024 for responses
    pub use_gpu: bool,               // Metal GPU acceleration
}
```

### Alternative Backends

**Configurable AI Backend System:**
```rust
#[derive(Debug, Clone)]
pub enum AIBackend {
    MistralRS {
        model_path: String,
        quantization: QuantizationLevel,
    },
    Ollama {
        endpoint: String,
        model_name: String,
    },
    Candle {
        model_config: CandleConfig,
        device: Device,
    },
}
```

**Development Flexibility:**
- **mistral.rs**: Production embedding, optimal performance
- **Ollama**: Development convenience, external service
- **Candle**: Future option when build tooling improves

### Model Configuration

**Primary Model: Gemma 3n-E4B-it 8B**
- **Memory Usage**: 4-6GB RAM with Q4K quantization
- **Context Length**: 8192 tokens
- **Capabilities**: Excellent reasoning for knowledge management tasks
- **Performance**: ~50-100 tokens/second on modern hardware
- **Format**: UQFF for fast loading (10-30 seconds vs 2-3 minutes)

**Quantization Strategy:**
- **Q4K**: 4-bit quantization, optimal balance of quality/size
- **Memory Efficiency**: ~50% reduction vs FP16 with minimal quality loss
- **Loading Speed**: UQFF format enables sub-30-second startup

## Database Architecture

### Embedded Turso Database (libSQL)

**Why Embedded Turso with Pure JSON:**
- **Desktop-First**: SQLite-compatible embedded database with no network overhead (0.1-5ms queries)
- **Zero Migration Risk**: Pure JSON schema eliminates ALTER TABLE on user machines (critical for 10,000+ installations)
- **Schema-as-Node**: Schemas stored as nodes, no dynamic table creation
- **Rule-Based Indexing**: JSON path indexes created based on query frequency (not LLM-driven)
- **Performance**: JSON path indexes (10-50ms) for frequently queried fields
- **Rust Integration**: Direct surrealdb integration (NOT sqlx or TypeScript clients)
- **Vector Search**: Built-in F32_BLOB vector support with similarity search
- **Sync Ready**: Embedded replicas enable seamless cloud sync (free → premium tier)
- **SQLite Compatibility**: Standard SQL interface with JSON operations

**Pure JSON Database Schema:**
```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct Node {
    pub id: String,                             // UUID or deterministic (YYYY-MM-DD for dates)
    pub node_type: String,                      // "task", "invoice", "schema", "date", etc.
    pub content: String,                        // Primary content/text
    pub parent_id: Option<String>,              // Where created (creation context)
    pub root_id: Option<String>,                // Root document (NULL = is root)
    pub before_sibling_id: Option<String>,      // Sibling ordering
    pub created_at: String,                     // ISO 8601 timestamp
    pub modified_at: String,                    // ISO 8601 timestamp
    pub properties: serde_json::Value,          // ALL entity-specific fields (Pure JSON)
    pub embedding_vector: Option<Vec<u8>>,      // F32_BLOB for AI embeddings
}

// Schema definition (stored as node with node_type = "schema", id = type_name)
#[derive(Debug, Deserialize, Serialize)]
pub struct SchemaDefinition {
    pub is_core: bool,
    pub description: String,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: String,         // "text", "number", "boolean", "date", "json", or schema name (auto-reference)
    pub indexed: bool,              // Triggers JSON path index creation
    pub required: bool,
}

// Rule-based index manager
pub struct IndexManager {
    db: Arc<Database>,
    // Creates JSON path indexes when query frequency > threshold
    // Example: CREATE INDEX idx_task_priority ON nodes((properties->>'$.priority')) WHERE node_type = 'task'
}
```

### Vector Embeddings

**Embedding Integration:**
- **Unified Queries**: Join structured data with vector similarity searches
- **Multi-Modal Storage**: Text, entity fields, and query results as vectors
- **Efficient Indexing**: Automatic index optimization for search performance
- **Batch Operations**: Efficient bulk embedding operations

**Embedding Strategy:**
```rust
pub struct EmbeddingStorage {
    pub node_id: String,
    pub content_type: ContentType,  // text, entity_field, query_result
    pub embedding: Vec<f32>,        // 768-dimensional embeddings
    pub metadata: EmbeddingMetadata,
    pub created_at: DateTime<Utc>,
}

pub enum ContentType {
    TextContent { full_text: String },
    EntityField { entity_id: String, field_name: String, field_value: String },
    QueryResult { query_id: String, result_summary: String },
}
```

### Vector Similarity Search

**Implementation**: SurrealDB native `vector::similarity::cosine()` function (Issue #495)

**Status**: ✅ Implemented and tested

**Usage**:
```rust
let results = store.search_by_embedding(&query_blob, limit, threshold).await?;
```

**Performance Characteristics**:
- 1,000 nodes: < 1500ms (measured ~950ms, linear scan)
- 10,000 nodes: < 15s (estimated ~9.5s, linear scan)
- 100,000 nodes: < 2.5 minutes (estimated, linear scan without index)
- Note: Current implementation uses O(n) linear scan without vector indexes

**Similarity Score Interpretation**:
- `1.0` = Identical content
- `0.7-0.9` = Highly similar (semantic equivalents)
- `0.5-0.7` = Moderately similar (related topics)
- `0.3-0.5` = Loosely related
- `< 0.3` = Unrelated content

**Default threshold**: 0.5 (moderate similarity, balances recall and precision)

## Development Tools

### Build System

**Cargo Workspace:**
```toml
[workspace]
members = [
    "crates/core",           # Core node types and traits
    "crates/ai",             # AI integration services
    "crates/storage",        # Database abstraction layer
    "crates/plugins",        # Plugin development framework
    "crates/server",         # HTTP API server
]
```

**Build Optimization:**
- **Incremental Compilation**: Fast development builds
- **Profile-Guided Optimization**: Release builds optimized for common usage patterns
- **Cross-Compilation**: Single command builds for all platforms
- **Dependency Caching**: Shared dependencies across workspace crates

### Testing Infrastructure

**Multi-Layer Testing:**
```rust
// Unit tests
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_creation() { /* ... */ }
}

// Integration tests with real services
#[tokio::test]
async fn test_ai_integration() {
    let services = create_test_services().await;  // Real Turso, AI model
    // ... test with actual services
}

// Performance benchmarks
#[bench]
fn bench_query_processing(b: &mut Bencher) {
    // ... performance testing
}
```

**Testing Philosophy:**
- **Real Services**: No mocking of databases or AI models
- **Comprehensive Coverage**: Unit, integration, and end-to-end tests
- **Performance Monitoring**: Automated performance regression detection
- **Property-Based Testing**: Automated edge case discovery

### Development Environment

**Required Tools:**
```bash
# Core development
rustc 1.80+
cargo 1.80+
node.js 20+
npm 10+

# AI model management
huggingface-hub (for model downloads)

# Platform-specific
# macOS: Xcode command line tools
# Linux: build-essential, pkg-config
# Windows: Visual Studio Build Tools
```

**IDE Integration:**
- **rust-analyzer**: Language server for excellent IDE support
- **Svelte for VS Code**: Frontend development tooling
- **Database extensions**: SQLite tooling and Turso dashboard
- **AI model viewers**: Tools for inspecting and debugging model behavior

## Performance Characteristics

### Startup Performance
- **Cold Start**: 15-30 seconds (model loading)
- **Warm Start**: 2-3 seconds (model cached)
- **UI Ready**: <1 second (while model loads in background)

### Runtime Performance
- **AI Inference**: 50-100 tokens/second (8B model on modern hardware)
- **Database Queries**: <10ms for typical node operations
- **Vector Search**: <50ms for similarity searches across 100K+ documents
- **Real-time Updates**: <5ms latency for query result updates

### Memory Usage
- **Base Application**: ~100MB (without AI model)
- **AI Model**: 4-6GB (Gemma 3n-E4B-it 8B Q4K)
- **Database Connections**: ~50MB (connection pools)
- **Total**: ~5-7GB for full operation

### Storage Requirements
- **Application**: ~50MB installed size
- **AI Model**: ~4GB (UQFF format)
- **Database**: Variable based on content (typically 100MB-10GB)
- **Indices**: ~20% overhead for search performance

## Security Considerations

### Application Security
- **Tauri Sandboxing**: Restricted system access with explicit permissions
- **Input Validation**: All user inputs validated before processing
- **SQL Injection Prevention**: Parameterized queries with sqlx
- **XSS Protection**: Svelte's automatic escaping

### AI Security
- **Model Isolation**: AI model runs in separate thread with limited access
- **Content Filtering**: Input/output sanitization for AI interactions
- **Rate Limiting**: Prevention of AI abuse or resource exhaustion
- **Local Processing**: No data sent to external AI services

### Data Security
- **Local Storage**: All data remains on user's machine
- **Encryption at Rest**: Sensitive data encrypted in database (future enhancement)
- **Backup Security**: User-controlled backup and restore procedures
- **Audit Logging**: Track all data modifications for debugging and compliance

## Deployment Strategy

### Development Builds
```bash
# Development with hot reload
cargo watch -x run
npm run dev  # Svelte frontend

# Testing builds
cargo test --workspace  # Run all tests
npm test               # Frontend tests
```

### Production Builds
```bash
# Optimized release build
cargo build --release
npm run build

# Platform-specific packaging
npm run tauri build -- --target x86_64-apple-darwin    # macOS Intel
npm run tauri build -- --target aarch64-apple-darwin   # macOS Apple Silicon
npm run tauri build -- --target x86_64-pc-windows-msvc # Windows
npm run tauri build -- --target x86_64-unknown-linux-gnu # Linux
```

### Distribution
- **Direct Downloads**: Platform-specific installers (.dmg, .msi, .AppImage)
- **Auto-Updates**: Built-in update mechanism with signature verification
- **Version Management**: Semantic versioning with migration support
- **Rollback Capability**: Ability to downgrade if issues occur

---

This technology stack provides the foundation for a high-performance, secure, and maintainable AI-native knowledge management system while supporting rapid development and deployment across multiple platforms.