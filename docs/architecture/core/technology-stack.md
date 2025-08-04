# Technology Stack

## Overview

NodeSpace's technology stack is carefully chosen to provide optimal performance, developer experience, and maintainability for an AI-native desktop application. The combination of Rust backend, Svelte frontend, and Tauri framework creates a powerful foundation for building sophisticated knowledge management capabilities.

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

### Backend: Rust

**Why Rust:**
- **Memory Safety**: Zero-cost abstractions without garbage collection overhead
- **Performance**: Comparable to C++ with modern language features
- **Concurrency**: Built-in async/await with excellent ecosystem
- **Type Safety**: Compile-time error prevention and excellent tooling
- **AI Integration**: Growing ecosystem of ML/AI libraries

**Key Libraries:**
```toml
[dependencies]
# Core async runtime
tokio = { version = "1.39", features = ["full"] }

# AI Integration
mistralrs = { git = "https://github.com/EricLBuehler/mistral.rs.git", features = ["metal"] }

# Database connectivity
lance = "0.16"  # Vector database

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Web framework (for API endpoints)
axum = "0.7"

# Configuration management
config = "0.14"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Architecture Patterns:**
- **Service Injection**: Trait-based dependency injection for testability
- **Result-Based Error Handling**: Comprehensive error propagation with context
- **Async-First Design**: All I/O operations use async/await patterns
- **Type-Driven Development**: Strong typing prevents runtime errors

### Frontend: Svelte

**Why Svelte:**
- **Compile-Time Optimizations**: No virtual DOM overhead
- **Reactive by Default**: Automatic state management and updates
- **Small Bundle Size**: Minimal runtime footprint
- **Developer Experience**: Intuitive syntax and excellent tooling
- **Performance**: Faster than React/Vue for complex UIs

**Key Features:**
- **SvelteKit**: Full-stack framework with file-based routing
- **Stores**: Reactive state management across components
- **Component Composition**: Reusable UI components with clear APIs
- **CSS-in-JS**: Scoped styling with dynamic theme support
- **TypeScript Integration**: Type-safe frontend development

**UI Libraries:**
```json
{
  "dependencies": {
    "svelte": "^4.2.0",
    "@sveltejs/kit": "^2.0.0",
    "typescript": "^5.4.0",
    "@types/node": "^20.14.0",
    "vite": "^5.3.0",
    "tailwindcss": "^3.4.0",
    "lucide-svelte": "^0.400.0"
  }
}
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

### Unified Storage: LanceDB

**Why LanceDB-Only:**
- **Unified Storage**: Single database for both structured data and vector embeddings
- **Performance**: Columnar storage with SIMD optimizations
- **Rust Integration**: Native Rust client library
- **Vector-Optimized**: Purpose-built for embedding storage and search
- **Scalability**: Handles complex queries and millions of records efficiently
- **Versioning**: Built-in data versioning capabilities
- **ACID Support**: Transactional guarantees for data consistency

**Schema Design:**
```rust
// Core node storage schema
struct NodeRecord {
    id: String,
    node_type: String,
    content: serde_json::Value,
    parent_id: Option<String>,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}

// Entity-specific data
struct EntityRecord {
    node_id: String,
    entity_type: String,
    stored_fields: serde_json::Value,
    calculated_fields: Option<serde_json::Value>,
    schema_version: i32,
}

// Query subscriptions
struct QuerySubscription {
    id: String,
    node_id: String,
    query_definition: serde_json::Value,
    last_result_hash: Option<String>,
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
    let services = create_test_services().await;  // Real LanceDB, AI model
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
- **Database extensions**: LanceDB tooling and data viewers
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