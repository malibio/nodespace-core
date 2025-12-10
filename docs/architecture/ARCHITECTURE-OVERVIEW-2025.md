# NodeSpace Architecture Overview
## Current State Analysis - December 2025

---

## Executive Summary

NodeSpace is an **AI-native knowledge management system** built as a Tauri desktop application with a Rust backend, Svelte 5 frontend, and SurrealDB embedded database. The system provides hierarchical node-based content management with semantic search, MCP (Model Context Protocol) integration for AI agent access, and real-time reactive UI updates.

### Key Architecture Decisions (Current State)

| Layer | Technology | Status |
|-------|------------|--------|
| **Desktop Framework** | Tauri 2.0 | ✅ Production-ready |
| **Frontend** | Svelte 5 + TypeScript | ✅ Active development |
| **Backend** | Rust (nodespace-core) | ✅ Production-ready |
| **Database** | SurrealDB Embedded (RocksDB) | ✅ Migrated from libSQL |
| **NLP/Embeddings** | Candle + ONNX (bge-small-en-v1.5) | ✅ Implemented |
| **AI Agent Protocol** | MCP via HTTP (port 3200) | ✅ Implemented |
| **State Management** | Svelte 5 $state + reactive stores | ✅ Active development |

---

## Repository Structure

```
nodespace-core/
├── Cargo.toml                    # Rust workspace root
├── package.json                  # Bun workspace root
├── CLAUDE.md                     # AI agent development guide
├── docs/
│   └── architecture/             # Architecture documentation
│       ├── core/                 # System overview, tech stack
│       ├── components/           # Component specifications
│       ├── decisions/            # ADRs (Architecture Decision Records)
│       ├── development/          # Dev process, testing guides
│       └── business-logic/       # Node behaviors, MCP integration
├── packages/
│   ├── core/                     # Rust business logic library
│   │   └── src/
│   │       ├── behaviors/        # Node type trait system
│   │       ├── db/               # SurrealDB layer
│   │       ├── mcp/              # MCP protocol handlers
│   │       ├── models/           # Data structures
│   │       └── services/         # Business services
│   ├── nlp-engine/               # Rust embedding service
│   │   └── src/
│   │       ├── embedding.rs      # Candle/ONNX integration
│   │       └── config.rs         # Model configuration
│   ├── desktop-app/              # Tauri + Svelte application
│   │   ├── src/                  # Svelte frontend
│   │   │   └── lib/
│   │   │       ├── components/   # UI components
│   │   │       ├── design/       # Design system
│   │   │       ├── services/     # Frontend services
│   │   │       ├── stores/       # Svelte reactive stores
│   │   │       └── types/        # TypeScript types
│   │   └── src-tauri/            # Rust Tauri backend
│   │       └── src/
│   │           ├── commands/     # Tauri IPC commands
│   │           ├── services/     # Backend services
│   │           └── mcp_integration.rs
│   └── design-system/            # Shared design tokens (planned)
└── scripts/                      # Build & GitHub utilities
```

---

## Core Architecture

### Three-Layer Design

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         TAURI DESKTOP APP                               │
├─────────────────────────────────────────────────────────────────────────┤
│  FRONTEND (Svelte 5 + TypeScript)                                       │
│  ┌─────────────────────┬──────────────────────┬────────────────────┐   │
│  │   Components        │   Services           │   Stores           │   │
│  │   - TextNode        │   - ReactiveNodeSvc  │   - StructureTree  │   │
│  │   - TaskNode        │   - BackendAdapter   │   - Navigation     │   │
│  │   - NodeTree        │   - FocusManager     │   - Layout         │   │
│  │   - BaseNode        │   - ContentProcessor │   - ScrollState    │   │
│  └─────────────────────┴──────────────────────┴────────────────────┘   │
│                              ↕ Tauri IPC Commands                       │
├─────────────────────────────────────────────────────────────────────────┤
│  BACKEND (Rust/Tauri)                                                   │
│  ┌─────────────────────┬──────────────────────┬────────────────────┐   │
│  │   Commands          │   Services           │   Integration      │   │
│  │   - nodes::*        │   - DomainEventFwd   │   - MCP Server     │   │
│  │   - schemas::*      │   - EmbeddingState   │   - Event Emitter  │   │
│  │   - embeddings::*   │   - Preferences      │                    │   │
│  └─────────────────────┴──────────────────────┴────────────────────┘   │
│                              ↕ Direct Rust Calls                        │
├─────────────────────────────────────────────────────────────────────────┤
│  CORE LIBRARY (nodespace-core)                                          │
│  ┌─────────────────────┬──────────────────────┬────────────────────┐   │
│  │   NodeService       │   Behaviors          │   MCP Handlers     │   │
│  │   - CRUD ops        │   - TextNodeBehavior │   - create_node    │   │
│  │   - Hierarchy       │   - TaskNodeBehavior │   - search_nodes   │   │
│  │   - Queries         │   - DateNodeBehavior │   - update_node    │   │
│  └─────────────────────┴──────────────────────┴────────────────────┘   │
│                              ↕ SurrealDB Driver                         │
├─────────────────────────────────────────────────────────────────────────┤
│  DATABASE (SurrealDB Embedded + RocksDB)                                │
│  ┌─────────────────────┬──────────────────────┬────────────────────┐   │
│  │   Tables            │   Edge Tables        │   Indexes          │   │
│  │   - node (hub)      │   - has_child        │   - node_type      │   │
│  │   - schema (spoke)  │   - mentions         │   - modified_at    │   │
│  │   - embedding       │                      │   - child_order    │   │
│  └─────────────────────┴──────────────────────┴────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Database Schema (SurrealDB)

### Hub-and-Spoke Architecture

NodeSpace uses a **hub-and-spoke** design where:
- **Hub (`node` table)**: Universal metadata for ALL node types
- **Spokes**: Type-specific queryable data (e.g., `schema`, `task`)
- **Record Links**: Bidirectional 1-to-1 composition between hub and spoke
- **Graph Edges**: `has_child` and `mentions` for relationships

### Core Tables

```sql
-- HUB TABLE (SCHEMAFULL)
DEFINE TABLE node SCHEMAFULL;
DEFINE FIELD content ON TABLE node TYPE string DEFAULT "";
DEFINE FIELD node_type ON TABLE node TYPE string ASSERT $value != NONE;
DEFINE FIELD data ON TABLE node TYPE option<record>;  -- Link to spoke
DEFINE FIELD version ON TABLE node TYPE int DEFAULT 1;
DEFINE FIELD created_at ON TABLE node TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE node TYPE datetime DEFAULT time::now();

-- HIERARCHY EDGE TABLE
DEFINE TABLE has_child SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD order ON TABLE has_child TYPE float ASSERT $value != NONE;
DEFINE FIELD version ON TABLE has_child TYPE int DEFAULT 1;

-- MENTIONS EDGE TABLE  
DEFINE TABLE mentions SCHEMAFULL TYPE RELATION IN node OUT node;
DEFINE FIELD context ON TABLE mentions TYPE string DEFAULT "";
DEFINE FIELD offset ON TABLE mentions TYPE int DEFAULT 0;
DEFINE FIELD root_id ON TABLE mentions TYPE option<string>;

-- EMBEDDING TABLE
DEFINE TABLE embedding SCHEMAFULL;
DEFINE FIELD node ON TABLE embedding TYPE record<node>;
DEFINE FIELD vector ON TABLE embedding TYPE array<float>;
DEFINE FIELD dimension ON TABLE embedding TYPE int DEFAULT 384;
DEFINE FIELD stale ON TABLE embedding TYPE bool DEFAULT true;
DEFINE FIELD chunk_index ON TABLE embedding TYPE int DEFAULT 0;
```

### Key Design Decisions

1. **Fractional Ordering**: Sibling order stored on `has_child.order` (float), not on nodes
2. **Schema-as-Node**: Schema definitions stored as nodes with `node_type = "schema"`
3. **Pure JSON Properties**: All type-specific data in flexible `properties` field
4. **OCC (Optimistic Concurrency)**: Version fields for conflict detection

---

## Node Type System

### Trait-Based Behaviors

```rust
// Core trait that all node types implement
pub trait NodeBehavior: Send + Sync {
    fn node_type(&self) -> &'static str;
    fn validate(&self, node: &Node) -> Result<(), ValidationError>;
    fn default_properties(&self) -> serde_json::Value;
    fn can_have_children(&self) -> bool;
    fn is_embeddable(&self) -> bool;
}
```

### Implemented Node Types

| Type | Behavior | Can Have Children | Embeddable | Description |
|------|----------|-------------------|------------|-------------|
| `text` | TextNodeBehavior | ✅ | ✅ | Markdown-enabled text blocks |
| `task` | TaskNodeBehavior | ✅ | ❌ | Tasks with status, priority, due date |
| `date` | DateNodeBehavior | ✅ | ❌ | Date entries (YYYY-MM-DD ID) |
| `schema` | SchemaNodeBehavior | ❌ | ❌ | Type definitions |
| `header` | HeaderNodeBehavior | ✅ | ✅ | Heading blocks (H1-H6) |
| `code-block` | CodeBlockNodeBehavior | ❌ | ✅ | Code snippets |
| Custom | CustomNodeBehavior | Configurable | Configurable | User-defined types |

### Schema Definition Structure

```rust
pub struct SchemaDefinition {
    pub is_core: bool,           // Protected from modification
    pub description: String,
    pub fields: Vec<FieldDefinition>,
    pub relationships: Vec<SchemaRelationship>,
}

pub struct FieldDefinition {
    pub name: String,
    pub field_type: String,      // text, number, boolean, date, json
    pub indexed: bool,
    pub required: bool,
}
```

---

## Frontend Architecture

### Svelte 5 Reactive System

The frontend uses Svelte 5's new reactivity system with `$state` and `$derived`:

```typescript
// ReactiveStructureTree - Core hierarchy store
class ReactiveStructureTree {
  // Using $state.raw() for Map reactivity
  children = $state.raw(new Map<string, ChildInfo[]>());
  
  getChildren(parentId: string): string[] {
    const childInfos = this.children.get(parentId) || [];
    return childInfos.map(c => c.nodeId);
  }
}
```

### Component Architecture

```
BaseNode (design/components/base-node.svelte)
├── Configuration Props: multiline, markdown, contentEditable
├── ContentEditable with dual-representation (focus=syntax, blur=formatted)
└── Event delegation pattern

Specialized Nodes (extend via props, not inheritance):
├── TextNode → multiline=true, markdown=true
├── TaskNode → multiline=false, markdown=false + task features
├── DateNode → specialized date handling
└── HeaderNode → heading level support
```

### Key Frontend Services

| Service | Purpose |
|---------|---------|
| `ReactiveNodeService` | Svelte 5 reactive wrapper for node operations |
| `BackendAdapter` | Tauri IPC command abstraction |
| `SharedNodeStore` | Centralized node data cache |
| `FocusManager` | Node focus/blur coordination |
| `ContentProcessor` | Markdown parsing and formatting |
| `BrowserSyncService` | SSE-based real-time sync (browser mode) |
| `NavigationService` | Tab and panel navigation |

---

## MCP Integration

### Architecture

NodeSpace exposes its operations via MCP (Model Context Protocol) for AI agent integration:

```
AI Agent (Claude Code, Cursor, etc.)
        ↓ HTTP JSON-RPC
┌───────────────────────────────┐
│   MCP HTTP Server             │
│   (Port 3200 default)         │
├───────────────────────────────┤
│   MCP Handlers                │
│   - create_node               │
│   - search_nodes              │
│   - update_node               │
│   - delete_node               │
│   - get_children              │
├───────────────────────────────┤
│   NodeService                 │
│   (Shared with Tauri)         │
└───────────────────────────────┘
```

### MCP Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "create_node",
  "params": {
    "node_type": "task",
    "content": "Review quarterly reports",
    "parent_id": "node:abc123"
  }
}
```

### Tauri Integration

The MCP server shares the same `NodeService` instance as Tauri commands, ensuring consistency:

```rust
pub fn initialize_mcp_server(app: tauri::AppHandle) -> anyhow::Result<()> {
    let node_service: tauri::State<NodeService> = app.state();
    let node_service_arc = Arc::new(node_service.inner().clone());
    
    let (mcp_service, callback) = mcp_integration::create_mcp_service_with_events(
        node_service_arc,
        embedding_service_arc,
        app.clone(),
    );
    
    // MCP events are forwarded to frontend via Tauri events
    tauri::async_runtime::spawn(mcp_service.start_with_callback(callback));
}
```

---

## Embedding & Semantic Search

### NLP Engine

```rust
// packages/nlp-engine/src/embedding.rs
pub struct EmbeddingService {
    model: Option<BertModel>,
    tokenizer: Option<Tokenizer>,
    cache: LruCache<String, Vec<f32>>,
}

impl EmbeddingService {
    pub fn generate_embedding(&mut self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        if let Some(embedding) = self.cache.get(&hash(text)) {
            return Ok(embedding.clone());
        }
        
        // Generate via Candle + ONNX
        let tokens = self.tokenizer.encode(text)?;
        let embedding = self.model.forward(&tokens)?;
        
        // Cache and return
        self.cache.put(hash(text), embedding.clone());
        Ok(embedding)
    }
}
```

### Model Configuration

| Property | Value |
|----------|-------|
| Model | `bge-small-en-v1.5` |
| Dimensions | 384 |
| Backend | Candle + ONNX |
| GPU Support | Metal (macOS), CPU fallback |
| Cache | LRU with automatic eviction |

### Root-Aggregate Embedding Model

Only **root nodes** (no parent edge) of embeddable types get embedded. The embedding represents the semantic content of the entire subtree.

**Embeddable types**: `text`, `header`, `code-block`, `schema`
**NOT embeddable**: `task`, `date`, `person`, child nodes

---

## Real-Time Sync

### Domain Event System

```rust
pub enum DomainEvent {
    NodeCreated { node: Node, edge: Option<EdgeRecord> },
    NodeUpdated { node: Node },
    NodeDeleted { id: String },
    EdgeCreated { edge: EdgeRecord },
    EdgeUpdated { edge: EdgeRecord },
    EdgeDeleted { id: String },
}
```

### Event Flow

```
NodeService emits DomainEvent
        ↓
DomainEventForwarder subscribes (background task)
        ↓
Events forwarded via Tauri emit()
        ↓
Frontend listens via Tauri event listeners
        ↓
ReactiveStructureTree updates
        ↓
UI reactively updates via Svelte 5 $state
```

### Client ID Filtering

Events originating from the same Tauri client are filtered to prevent feedback loops:

```rust
// Domain events include source client_id
// Forwarder skips events from same client
if event.client_id == self.client_id {
    continue; // Skip our own events
}
```

---

## Development Modes

### Tauri Mode (Production)

```bash
bun run dev:tauri
# MCP_PORT=3100 bunx tauri dev
```

- Full native desktop experience
- Embedded SurrealDB with RocksDB storage
- Direct Rust service access
- MCP server on configurable port

### Browser Mode (Development)

```bash
bun run dev:browser
# Starts: SurrealDB server + Dev Proxy + Vite frontend
```

- External SurrealDB server (port 8000)
- Rust dev-proxy for SSE sync
- Hot reload for frontend development
- Useful for rapid UI iteration

---

## Testing Strategy

### Multi-Layer Testing

| Layer | Command | Description |
|-------|---------|-------------|
| Frontend Unit | `bun run test` | Vitest + Happy-DOM (fast) |
| Frontend Browser | `bun run test:browser` | Vitest + Playwright |
| Rust Unit | `bun run rust:test` | Cargo test |
| Performance | `bun run test:perf` | Large dataset validation |
| Full Integration | `bun run test:all` | All tests combined |

### Test Coverage

- **Current**: 728+ tests with 98.8% coverage
- **Philosophy**: Real services, no mocking of databases or AI models

---

## Key Architecture Decisions (ADRs)

| ADR | Decision | Rationale |
|-----|----------|-----------|
| ADR-001 | ContentEditable over CodeMirror | Cursor positioning reliability |
| ADR-002 | Dual-Representation System | Focus=syntax, blur=formatted |
| ADR-003 | ContentEditableController Pattern | Separate DOM from Svelte reactivity |
| ADR-005 | Node Reference Decoration | Universal `@` triggers with `nodespace://` URIs |
| ADR-017 | Managed Sync Only | No user cloud folder sync, managed service only |

---

## Technology Stack Summary

### Runtime Versions

| Technology | Version | Notes |
|------------|---------|-------|
| Rust | 1.80+ | Workspace with cargo |
| Bun | 1.0+ | Node.js replacement |
| Svelte | 5.39+ | New reactivity system |
| SvelteKit | 2.42+ | File-based routing |
| Tauri | 2.8+ | Desktop framework |
| SurrealDB | 2.3+ | Embedded with RocksDB |
| TypeScript | 5.6 | Strict mode |

### Key Dependencies

**Rust:**
- `surrealdb` - Database driver
- `tokio` - Async runtime
- `serde/serde_json` - Serialization
- `anyhow/thiserror` - Error handling
- `tracing` - Logging

**Frontend:**
- `@tauri-apps/api` - Tauri IPC
- `marked` - Markdown parsing
- `bits-ui` - UI components (shadcn-svelte compatible)
- `tailwindcss` - Styling
- `@lucide/svelte` - Icons

---

## Future Roadmap

### Planned Features

1. **Workflow Canvas System** - Visual AI workflow creation
2. **Collaborative Sync** - Multi-user real-time collaboration via Turso
3. **Playbook Marketplace** - Shareable methodology templates
4. **Advanced AI Integration** - Local LLM inference via mistral.rs
5. **Mobile Companion** - React Native or Capacitor mobile app

### Migration Considerations

- **libSQL → SurrealDB**: Completed (improved graph capabilities)
- **Turso Sync**: Planned for cloud tier (managed service)
- **Vector Search**: Currently O(n) linear scan, index optimization planned

---

## Quick Reference

### Common Commands

```bash
# Development
bun run dev              # Browser mode (default)
bun run dev:tauri        # Tauri desktop mode

# Testing
bun run test             # Frontend unit tests
bun run rust:test        # Rust tests
bun run test:all         # All tests

# Quality
bun run quality:fix      # ESLint + Svelte check

# Build
bun run build            # Frontend build
bun run tauri:build      # Desktop app build
```

### File Locations

| Need | Location |
|------|----------|
| Rust business logic | `packages/core/src/` |
| Database schema | `packages/core/src/db/schema.surql` |
| Frontend components | `packages/desktop-app/src/lib/components/` |
| Design system | `packages/desktop-app/src/lib/design/` |
| Tauri commands | `packages/desktop-app/src-tauri/src/commands/` |
| Architecture docs | `docs/architecture/` |
| Agent guide | `CLAUDE.md` |

---

*Document generated: December 2025*
*Based on codebase analysis of nodespace-core repository*
