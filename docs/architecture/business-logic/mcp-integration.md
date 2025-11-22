# NodeSpace MCP Integration Design

## Overview

NodeSpace integrates with the **Model Context Protocol (MCP)** to provide AI agents with programmatic access to the knowledge management system. This enables AI assistants to read, write, and manipulate nodes through a standardized protocol while maintaining reactive UI synchronization.

## MCP Architecture Strategy

### Single-Process Architecture

NodeSpace runs the MCP server as an **async Tokio task** within the Tauri application process:

```
Tauri Application (Single Process)
┌────────────────────────────────────────────┐
│  Tauri Runtime                              │
│  ├─ Svelte Frontend (reactive stores)       │
│  └─ Event System (existing EventBus)        │
│                                              │
│  Rust Backend (Tokio async)                 │
│  ├─ NodeService (shared Arc<T>)             │
│  ├─ Database Connection Pool                │
│  ├─ Tauri Command Handlers                  │
│  └─ MCP stdio Task (background)             │
│      ├─ stdin reader (async)                │
│      ├─ JSON-RPC handler                    │
│      └─ stdout writer (async)               │
└────────────────────────────────────────────┘
         ↑                    ↑
         │                    │
    AI Agent              Tauri Events
   (stdio I/O)         (reactive updates)
```

### Key Design Decisions

1. **Same-Process Execution**: MCP runs as Tokio task, not separate process
2. **Shared NodeService**: Single instance accessed by both Tauri commands and MCP
3. **Existing EventBus**: Frontend reactivity handled by established EventBus system
4. **Tauri Event Bridge**: MCP operations emit Tauri events for UI updates
5. **No IPC Required**: Same-process communication is trivial and fast

### Why Single-Process?

**Advantages:**
- **Simplicity**: No IPC layer needed - same-process communication is trivial
- **Existing Infrastructure**: Leverages working EventBus (already tested, proven)
- **Zero Latency**: Direct function calls, no socket overhead
- **Easier Debugging**: Single process to attach debugger, unified logs
- **Memory Efficiency**: Single NodeService instance, no duplicate data structures
- **Atomic Operations**: Shared database connection pool handles concurrency via tokio
- **Proven Pattern**: Tauri apps commonly run background async tasks

**Trade-offs:**
- **Lifecycle Coupling**: MCP stops when desktop app closes (matches primary use case)
- **Resource Sharing**: Both desktop UI and MCP share same process memory/CPU

**Archived Alternative:** See `/docs/architecture/decisions/archived/mcp-dual-process-approach.md` for the rejected dual-process architecture.

### MCP Communication Flow

```
AI Agent Process → spawn NodeSpace with stdio piping
       |
       | ─── JSON-RPC Request ──► stdin ──────► MCP Task (Tokio)
       |                                               ↓
       |                                        Parse JSON-RPC
       |                                               ↓
       |                                        Execute via NodeService
       |                                               ↓
       |                                        Database Update
       |                                               ↓
       |                                        Emit Tauri Event
       |                                               ↓
       | ◄── stdout ◄── JSON-RPC Response ◄─── Return result


Parallel: Desktop App UI
       |
       | Listens for Tauri events
       | ↓
       | EventBus receives event
       | ↓
       | Svelte stores update
       | ↓
       | UI re-renders automatically
```

### Reactivity Flow

When an AI agent performs an operation via MCP while the user has NodeSpace open:

```
MCP Request (AI Agent via stdio)
  ↓
JSON-RPC Parser (MCP Task)
  ↓
NodeService Operation (shared instance)
  ↓
Database Update (Turso/libSQL)
  ↓
Tauri Event Emission
  ↓
Frontend EventBus (existing)
  ↓
Svelte Store Update (reactive)
  ↓
UI Re-render (automatic - user sees changes immediately)
```

**No special synchronization needed!** The existing EventBus + reactive stores handle UI updates automatically.

### Concurrent Database Access

Both Tauri command handlers and MCP handlers access the same SurrealDB through a shared connection pool. This is **safe and supported** by libSQL/Turso architecture:

**How Turso Handles Concurrency:**
- ✅ **WAL Mode**: Write-Ahead Logging enables concurrent readers and writers
- ✅ **Automatic Locking**: libSQL manages locks transparently
- ✅ **Read Concurrency**: Multiple operations can read simultaneously without blocking
- ✅ **Write Serialization**: Writes are automatically serialized via database locks
- ✅ **Transaction Safety**: ACID guarantees maintained

**Example Concurrent Access:**
```
Time  Tauri Command Handler    MCP Handler                Database
──────────────────────────────────────────────────────────────────
T1    SELECT nodes WHERE...    -                          ✓ Read lock
T2    -                        SELECT nodes WHERE...      ✓ Read lock (concurrent)
T3    INSERT INTO nodes...     -                          ✓ Write lock
T4    -                        INSERT INTO nodes...       ⏳ Waits for write lock
T5    COMMIT                   -                          ✓ Releases lock
T6    -                        [proceeds with write]      ✓ Acquires write lock
```

**Key Point**: From the database's perspective, whether requests come from Tauri commands or MCP handlers makes no difference - both use the same connection pool and async runtime.

### Transport Options: stdio vs HTTP

#### stdio Transport (CLI and Headless Mode)

**Advantages of stdio for MCP:**
- **Standard protocol**: MCP defines stdio as primary transport
- **No network setup**: Direct process communication via pipes
- **Security**: No ports to expose, no HTTP attack surface
- **Simplicity**: JSON over text streams
- **Language agnostic**: Any language can read/write stdio
- **Built-in piping**: AI agent frameworks automatically pipe stdin/stdout

#### HTTP Transport (GUI Mode with Claude Code Bridge)

**Problem Statement:**
- NodeSpace runs as a **GUI application** (Tauri desktop app)
- Claude Code **cannot pipe stdin/stdout** to an already-running GUI app
- stdio transport is incompatible with GUI application lifecycle

**Solution: HTTP Transport (Port 3001)**

The MCP server now supports **both** stdio and HTTP transports:

```
CLI/Headless Mode: Claude Code → stdio bridge (existing)

GUI Mode: Claude Code → Bridge Script → HTTP POST localhost:3001/mcp → MCP Server
                                                        ↓
                                                  Tauri Application
```

**HTTP Configuration:**
- **Port**: 3001 (hardcoded for now, future: configurable)
- **Binding**: 127.0.0.1 only (localhost, no network exposure)
- **Protocol**: Standard MCP over HTTP (JSON-RPC 2.0)
- **Endpoint**: POST `/mcp` with JSON-RPC 2.0 body
- **Security**: None needed (localhost-only access)

**Why Both Transports?**
- **stdio**: Works for CLI tools and headless servers
- **HTTP**: Works for GUI apps (NodeSpace desktop, Claude Code bridge)
- **Shared logic**: Same handlers, different transports
- **Zero duplication**: Single business logic layer handles both

**Claude Code Integration with Bridge Script:**

Create bridge script (`~/.config/claude/mcp-servers/nodespace-bridge.sh`):
```bash
#!/usr/bin/env bash
# Translates stdio to HTTP
while IFS= read -r line; do
    response=$(curl -s -X POST http://localhost:3001/mcp \
        -H "Content-Type: application/json" \
        -d "$line" 2>/dev/null)
    echo "$response"
done
```

Configure Claude Code (`~/.claude.json`):
```json
{
  "mcpServers": {
    "nodespace": {
      "type": "stdio",
      "command": "/path/to/mcp-bridge.sh"
    }
  }
}
```

**Workflow:**
1. Launch NodeSpace app (HTTP server starts on port 3001)
2. Claude Code loads bridge script configuration
3. Claude Code sends MCP requests to bridge script via stdio
4. Bridge script POSTs requests to http://localhost:3001/mcp
5. MCP server processes request and returns JSON-RPC response
6. Bridge script forwards response back to Claude Code via stdout
7. Tauri events trigger UI updates in real-time

**Transport Abstraction (Code):**

```rust
pub enum McpTransport {
    Stdio,              // CLI tools, testing
    Http { port: u16 }, // GUI + Claude Code
}

pub async fn run_mcp_server_with_callback(
    services: McpServices,
    transport: McpTransport,
    callback: Option<ResponseCallback>,
) -> anyhow::Result<()> {
    match transport {
        McpTransport::Stdio => run_stdio_server(services, callback).await,
        McpTransport::Http { port } => run_http_server(services, port, callback).await,
    }
}
```

**Implementation Details:**

GUI Mode (Tauri):
```rust
// packages/desktop-app/src-tauri/src/mcp_integration.rs
pub async fn run_mcp_server_with_events(...) -> anyhow::Result<()> {
    // Use HTTP transport for GUI app
    mcp::run_mcp_server_with_callback(
        services,
        McpTransport::Http { port: 3001 },
        Some(callback),
    ).await
}
```

HTTP Server (using axum):
```rust
// packages/core/src/mcp/server.rs
async fn run_http_server(
    services: McpServices,
    port: u16,
    callback: Option<ResponseCallback>,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/mcp", post(handle_http_mcp_request))
        .with_state((Arc::new(services), callback));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

## MCP Protocol Implementation

### JSON-RPC Message Format

**Request Structure:**
```json
{
    "jsonrpc": "2.0",
    "id": 123,
    "method": "create_node",
    "params": {
        "node_type": "task",
        "content": "Review the quarterly reports",
        "properties": {
            "priority": 3,
            "due_date": "2024-02-15"
        }
    }
}
```

**Response Structure:**
```json
{
    "jsonrpc": "2.0",
    "id": 123,
    "result": {
        "node_id": "task-abc123",
        "success": true
    }
}
```

**Error Response:**
```json
{
    "jsonrpc": "2.0",
    "id": 123,
    "error": {
        "code": -32600,
        "message": "Invalid node type: unknown_type"
    }
}
```

### MCP Server Implementation

The MCP server runs as an async Tokio task spawned during Tauri setup:

```rust
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// Tauri setup - spawn MCP task
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Initialize shared NodeService
            let node_service = Arc::new(NodeService::new(db_pool));
            app.manage(node_service.clone());

            // Spawn MCP stdio task in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_mcp_server(node_service, app_handle).await {
                    eprintln!("MCP server error: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![...])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// MCP stdio server loop
async fn run_mcp_server(
    service: Arc<NodeService>,
    app: AppHandle
) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        // Parse JSON-RPC request
        let request: MCPRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = MCPResponse {
                    jsonrpc: "2.0".to_string(),
                    id: 0,
                    result: None,
                    error: Some(MCPError {
                        code: -32700, // Parse error
                        message: format!("Invalid JSON: {}", e),
                    }),
                };
                writer.write_all(serde_json::to_string(&error_response)?.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                continue;
            }
        };

        // Handle request
        let response = handle_mcp_request(&service, &app, request).await;

        // Write response
        writer.write_all(serde_json::to_string(&response)?.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

// Request/response types
#[derive(Debug, Deserialize)]
struct MCPRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Serialize)]
struct MCPResponse {
    jsonrpc: String,
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<MCPError>,
}

#[derive(Debug, Serialize)]
struct MCPError {
    code: i32,
    message: String,
}

// Request dispatcher
async fn handle_mcp_request(
    service: &NodeService,
    app: &AppHandle,
    request: MCPRequest
) -> MCPResponse {
    let result = match request.method.as_str() {
        "create_node" => handle_create_node(service, app, request.params).await,
        "get_node" => handle_get_node(service, request.params).await,
        "update_node" => handle_update_node(service, app, request.params).await,
        "delete_node" => handle_delete_node(service, app, request.params).await,
        "query_nodes" => handle_query_nodes(service, request.params).await,
        "create_nodes_from_markdown" => handle_create_nodes_from_markdown(service, app, request.params).await,
        _ => Err(MCPError {
            code: -32601, // Method not found
            message: format!("Unknown method: {}", request.method),
        }),
    };

    match result {
        Ok(result) => MCPResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(error) => MCPResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(error),
        },
    }
}
```

## MCP Method Implementations

### Node CRUD Operations

```rust
#[derive(Deserialize)]
struct CreateNodeParams {
    node_type: String,
    content: String,
    parent_id: Option<String>,
    root_id: Option<String>,
    properties: Value,
}

async fn handle_create_node(
    service: &NodeService,
    app: &AppHandle,
    params: Value
) -> Result<Value, MCPError> {
    let params: CreateNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602, // Invalid params
            message: format!("Invalid parameters: {}", e),
        })?;

    let node = Node {
        id: uuid::Uuid::new_v4().to_string(),
        node_type: params.node_type.clone(),
        content: params.content,
        parent_id: params.parent_id,
        root_id: params.root_id,
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        properties: params.properties,
        embedding_vector: None,
        mentions: Vec::new(),
        mentioned_by: Vec::new(),
    };
    // Note: Sibling ordering is handled via has_child edge `order` field (Issue #614)

    let node_id = service.create_node(node.clone()).await
        .map_err(|e| MCPError {
            code: -32603, // Internal error
            message: format!("Failed to create node: {}", e),
        })?;

    // Emit Tauri event for UI reactivity
    app.emit("node-created", &node).ok();

    Ok(json!({
        "node_id": node_id,
        "success": true
    }))
}

async fn handle_get_node(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    #[derive(Deserialize)]
    struct GetNodeParams {
        node_id: String,
    }

    let params: GetNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    let node = service.get_node(&params.node_id).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to get node: {}", e),
        })?;

    match node {
        Some(node) => Ok(serde_json::to_value(node).unwrap()),
        None => Err(MCPError {
            code: -32000,
            message: format!("Node not found: {}", params.node_id),
        }),
    }
}

async fn handle_update_node(
    service: &NodeService,
    app: &AppHandle,
    params: Value
) -> Result<Value, MCPError> {
    #[derive(Deserialize)]
    struct UpdateNodeParams {
        node_id: String,
        content: Option<String>,
        properties: Option<Value>,
    }

    let params: UpdateNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    let update = NodeUpdate {
        content: params.content,
        properties: params.properties,
    };

    service.update_node(&params.node_id, update).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to update node: {}", e),
        })?;

    // Emit Tauri event for UI reactivity
    app.emit("node-updated", json!({ "node_id": params.node_id })).ok();

    Ok(json!({ "success": true }))
}

async fn handle_delete_node(
    service: &NodeService,
    app: &AppHandle,
    params: Value
) -> Result<Value, MCPError> {
    #[derive(Deserialize)]
    struct DeleteNodeParams {
        node_id: String,
    }

    let params: DeleteNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    service.delete_node(&params.node_id).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to delete node: {}", e),
        })?;

    // Emit Tauri event for UI reactivity
    app.emit("node-deleted", json!({ "node_id": params.node_id })).ok();

    Ok(json!({ "success": true }))
}
```

## AI Agent Integration Examples

### Claude Code Integration

AI agents can spawn NodeSpace with stdio piping to communicate via MCP:

```typescript
// Claude Code MCP client configuration
const mcp = {
  name: "nodespace",
  command: "/Applications/NodeSpace.app/Contents/MacOS/NodeSpace",
  args: [],
  transport: "stdio"
};

// Example: Create a node
const request = {
  jsonrpc: "2.0",
  id: 1,
  method: "create_node",
  params: {
    node_type: "text",
    content: "Meeting notes from AI agent",
    root_id: "daily-2025-01-19",
    properties: {}
  }
};

// Send via stdin, receive response via stdout
```

### Example AI Interaction Flow

```
1. User in Claude Code: "Create a task to review Q1 reports"

2. AI Agent processes request and calls MCP:
   {
     "method": "create_node",
     "params": {
       "node_type": "task",
       "content": "Review Q1 reports",
       "properties": { "status": "pending", "priority": 3 }
     }
   }

3. NodeSpace MCP server:
   - Creates node in database
   - Emits Tauri event "node-created"
   - Returns node ID

4. NodeSpace UI (if open):
   - EventBus receives Tauri event
   - Svelte stores update reactively
   - User sees new task appear immediately

5. AI Agent receives response:
   { "result": { "node_id": "uuid-123", "success": true } }
```

## Code Organization: Layered Architecture

NodeSpace MCP implementation follows a **layered architecture** that separates pure protocol logic from framework-specific integration.

### Architecture Layers

```
┌─────────────────────────────────────────────────────┐
│  packages/desktop-app/src-tauri/src/                │
│  ┌───────────────────────────────────────────────┐  │
│  │  Tauri Integration Layer                      │  │
│  │  - mcp_integration.rs (Tauri event wrapper)   │  │
│  │  - lib.rs (spawns MCP task)                   │  │
│  └───────────────────────────────────────────────┘  │
│                        ↓ uses                        │
│  ┌───────────────────────────────────────────────┐  │
│  │  packages/core/src/mcp/ (Pure Protocol)      │  │
│  │  - server.rs (stdio loop, JSON-RPC)           │  │
│  │  - types.rs (protocol types)                  │  │
│  │  - handlers/ (business logic)                 │  │
│  │    ├── initialize.rs (capability negotiation) │  │
│  │    ├── nodes.rs (CRUD operations)             │  │
│  │    └── markdown.rs (import/export)            │  │
│  └───────────────────────────────────────────────┘  │
│                        ↓ uses                        │
│  ┌───────────────────────────────────────────────┐  │
│  │  packages/core/src/services/                  │  │
│  │  - NodeService (database operations)          │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

### Why This Separation?

**Core Package (`packages/core/src/mcp/`):**
- ✅ **Framework-agnostic** - No Tauri dependencies
- ✅ **Reusable** - Could be used in CLI tools, HTTP servers, etc.
- ✅ **Testable** - Pure Rust logic, easier to unit test
- ✅ **Portable** - Works anywhere Rust runs

**Tauri Integration (`packages/desktop-app/src-tauri/src/`):**
- ✅ **Minimal wrapper** - Only handles Tauri-specific concerns
- ✅ **Event emissions** - Bridges MCP operations to UI reactivity
- ✅ **App lifecycle** - Spawns MCP task in Tauri setup

### Concrete Example: create_node Flow

```rust
// 1. Core protocol layer (packages/core/src/mcp/handlers/nodes.rs)
pub async fn handle_create_node(
    service: &Arc<NodeService>,
    params: Value,
) -> Result<Value, MCPError> {
    // Pure business logic - no Tauri dependencies
    let params: CreateNodeParams = serde_json::from_value(params)?;
    let node_id = service.create_node(node).await?;
    Ok(json!({ "node_id": node_id }))
}

// 2. Core server layer (packages/core/src/mcp/server.rs)
async fn handle_request(
    service: &Arc<NodeService>,
    request: MCPRequest,
) -> MCPResponse {
    match request.method.as_str() {
        "create_node" => handle_create_node(service, request.params).await,
        // ... other methods
    }
}

// 3. Tauri integration layer (packages/desktop-app/src-tauri/src/mcp_integration.rs)
pub async fn run_mcp_server_with_events(
    node_service: Arc<NodeService>,
    app: AppHandle,
) -> anyhow::Result<()> {
    // Wraps core MCP server with Tauri event callback
    let callback = Arc::new(move |method: &str, result: &Value| {
        if method == "create_node" {
            // Emit Tauri event for UI reactivity
            app.emit("node-created", result).ok();
        }
    });

    // Run core MCP server with event-emitting callback
    mcp::run_mcp_server_with_callback(node_service, Some(callback)).await
}
```

### Design Benefits

**Testability:**
```rust
// Test core protocol without Tauri
#[tokio::test]
async fn test_create_node() {
    let service = setup_test_service();
    let params = json!({ "node_type": "text", "content": "Test" });
    let result = handle_create_node(&service, params).await.unwrap();
    assert!(result["node_id"].is_string());
}
```

**Reusability:**
```rust
// Example: Could use same core in CLI tool
async fn cli_main() {
    let service = Arc::new(NodeService::new(db_pool));

    // Use core MCP server in CLI context (no Tauri)
    mcp::run_mcp_server(service).await?;
}
```

**Maintainability:**
- Protocol changes isolated to `packages/core/src/mcp/`
- Tauri upgrades only affect thin integration layer
- Clear separation of concerns

### File Structure

**Core MCP Protocol (`packages/core/src/mcp/`):**
```
packages/core/src/mcp/
├── mod.rs                      # Public API exports
├── server.rs                   # stdio server loop, JSON-RPC dispatcher
├── types.rs                    # MCPRequest, MCPResponse, MCPError, MCPNotification
└── handlers/
    ├── mod.rs                  # Handler module exports
    ├── initialize.rs           # Capability negotiation (Issue #308)
    ├── nodes.rs                # Node CRUD operations
    └── markdown.rs             # Markdown import/export (Issues #296, #309)
```

**Tauri Integration (`packages/desktop-app/src-tauri/src/`):**
```
packages/desktop-app/src-tauri/src/
├── lib.rs                      # Spawns MCP task in Tauri setup
├── mcp_integration.rs          # Wraps core MCP with Tauri events
└── commands/                   # Tauri IPC command handlers (separate from MCP)
    ├── db.rs
    ├── nodes.rs
    └── ...
```

### Key Principle: Single Responsibility

**Core Package** = "What" (Protocol logic, business rules)
**Tauri Package** = "How" (Desktop app integration, event emissions)

This separation ensures:
- Core protocol can be tested independently
- Tauri-specific code is minimal and focused
- Future frameworks (CLI, HTTP, etc.) can reuse core

## Implementation Phases

### Phase 1: MCP Server Foundation (Issue #112)
- Create MCP types (request/response/error)
- Implement stdio server loop with tokio::io
- Add JSON-RPC request dispatcher
- Spawn MCP task during Tauri setup
- Implement basic node CRUD methods
- Add Tauri event emissions after operations
- Test with simple MCP client

### Phase 2: Markdown Node Creation
- ~~Add `pulldown-cmark` dependency~~ → **Custom line-by-line parser** (see architecture decision below)
- Implement markdown parser with hierarchy tracking
- Create `create_nodes_from_markdown` handler
- Handle heading hierarchy (h1 → h2 → h3)
- Handle list nesting (indentation-based)
- Test with sample markdown documents
- Claude Code integration testing

**Markdown Parser Architecture Decision:**

NodeSpace uses a **custom line-by-line markdown parser** instead of pulldown-cmark for specific architectural reasons:

1. **Inline formatting preservation**: Stores `**bold**` and `*italic*` syntax intact (not stripped), allowing users to edit markdown directly in the outliner
2. **Hierarchical node creation**: Designed specifically for parent-child-sibling database relationships, not HTML rendering
3. **Round-trip fidelity**: Import → export preserves original markdown formatting exactly
4. **Explicit sibling ordering**: Maintains sibling order via `has_child` edge `order` field (fractional ordering, Issue #614)

The custom parser is ~100 lines with helper functions and handles:
- Headers (#, ##, ###)
- Tasks (- [ ], - [x])
- Bullets (- item) with correct hierarchy
- Ordered lists (1., 2., 3.)
- Code blocks (```)
- Quote blocks (>)
- Text paragraphs with inline markdown

**Known limitations** (acceptable for v1, can add incrementally):
- No escaped character support (`\*`)
- No link references (`[text][ref]`)
- No inline HTML (`<strong>`)
- Basic markdown only (sufficient for note-taking use case)

See `/packages/core/src/mcp/handlers/markdown.rs` for implementation details.

### Phase 3: Advanced Features (Future)
- Natural language query processing
- Semantic search operations
- Batch operations for performance
- Authentication and permissions

## Migration Guide: Container Model Change (v0.2.0)

### Breaking Change: First Element as Root

**What Changed:**

In the markdown import implementation, we changed how root nodes are created:

- **Old behavior**: `create_nodes_from_markdown` created a wrapper root node with `container_title` as content, then all parsed markdown elements became children of that wrapper
- **New behavior**: The first markdown element becomes the root node itself (no wrapper created)

**Visual Example:**

Given this markdown:
```markdown
# My Document
Some text
```

**Old Model (pre-v0.2.0):**
```
Root (node_type="text", content="Test Title")  ← wrapper node
  └─ Header (node_type="header", content="# My Document")
     └─ Text (node_type="text", content="Some text")
```

**New Model (v0.2.0+):**
```
Header (node_type="header", content="# My Document")  ← IS the root
  └─ Text (node_type="text", content="Some text")
```

**Key Differences:**

1. **No wrapper node**: The first parsed element becomes the root
2. **container_title parameter deprecated**: Still accepted for backward compatibility but effectively ignored
3. **Root node structure**: Root has `root_id = None`, children have `root_id = <root_id>`
4. **Export format**: When exporting, you get the actual first element content, not a wrapper title

**Why This Change:**

1. **Semantic correctness**: The document structure matches the markdown structure exactly
2. **Database efficiency**: One less node per markdown import
3. **Export clarity**: When exporting markdown, you get the actual content, not a synthetic wrapper
4. **Simpler mental model**: What you see in markdown is what you get in the node structure

**Migration Steps:**

If you have existing code that depends on the old root model:

1. **Update root queries**: Code expecting `root_id` to point to a wrapper node should expect it to point to the first markdown element instead
2. **Re-import existing content**: To migrate existing markdown roots to the new structure, export and re-import them
3. **Update tests**: Any tests asserting on root content should expect the first markdown element, not the `container_title` value
4. **Export workflows**: If you're exporting markdown, the root node will now be the first element, not a wrapper with arbitrary title

**Example Code Migration:**

```rust
// OLD: Expecting wrapper container
let container = service.get_node(&container_id).await?;
assert_eq!(container.node_type, "text"); // wrapper was always "text"
assert_eq!(container.content, "My Title"); // container_title parameter

// NEW: First element is container
let container = service.get_node(&container_id).await?;
assert_eq!(container.node_type, "header"); // could be header, text, code-block, etc.
assert_eq!(container.content, "# My Document"); // actual first markdown element
```

**Backward Compatibility:**

- The `container_title` parameter is still accepted in `create_nodes_from_markdown` to avoid breaking the API, but it has no effect on the node structure
- Existing code that doesn't make assumptions about container structure will continue to work
- The change is only breaking if you relied on the wrapper node existing or having specific content

**Timeline:**

- **Introduced**: Issue #309 (Markdown export feature)
- **Rationale**: Commits e251b06 and ef1456c in PR #309 detail the refactoring
- **Status**: Implemented as of v0.2.0

---

## Related Issues

### Completed
- **#112**: Add MCP stdio server for AI agent access (Foundation) - ✅ **MERGED**
- **#111**: ~~Implement reactive state synchronization~~ (Closed - automatic with single-process)
- **#296**: Add create_nodes_from_markdown MCP method - ✅ **MERGED**

### In Progress / Planned
- **#308**: Implement MCP initialization handshake and capability discovery (CRITICAL)
- **#309**: Add get_markdown_from_node_id for markdown export (AI reading workflow)
- **#310**: Add batch node operations (get_nodes_batch, update_nodes_batch, update_container_from_markdown)
- **#312**: Auto-generate MCP tool schemas from Rust types (Future enhancement)

## References

- **Archived Dual-Process Approach**: `/docs/architecture/decisions/archived/mcp-dual-process-approach.md`
- **Architecture Decision**: `/docs/architecture/decisions/021-mcp-single-process-architecture.md`
- **MCP Specification**: https://modelcontextprotocol.io
