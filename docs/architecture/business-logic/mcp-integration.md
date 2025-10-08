# NodeSpace MCP Integration Design

## Overview

NodeSpace integrates with the **Model Context Protocol (MCP)** to provide AI agents with programmatic access to the knowledge management system. This enables AI assistants to read, write, and manipulate nodes through a standardized protocol while maintaining reactive UI synchronization.

## MCP Architecture Strategy

### Dual-Mode Binary with Desktop-Managed Service

NodeSpace uses a **single binary** that can run in two modes, with the desktop app managing the MCP service as a background process:

1. **Desktop GUI Mode** (default): Normal Tauri application with UI
2. **MCP Server Mode**: Headless stdio server spawned and managed by desktop app

```rust
// Single binary, dual functionality
fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--mcp-server") => {
            // Run as MCP server (headless, stdio communication)
            // Spawned by desktop app as subprocess
            run_mcp_stdio_server();
        },
        Some("--help") => {
            print_usage();
        },
        _ => {
            // Run normal desktop application
            // Can spawn MCP server as subprocess when enabled
            run_desktop_app();
        }
    }
}
```

### Desktop-Managed Service Model

The desktop application provides a settings interface to enable/configure the MCP background service:

```
┌─────────────────────────────────────────────────────┐
│              Tauri Desktop App                      │
│  ┌─────────────────────────────────────────────┐   │
│  │  Settings UI                                 │   │
│  │  ☑ Enable MCP Server                        │   │
│  │  Port: 3000                                  │   │
│  │  ○ Stop when app closes                     │   │
│  │  ● Keep running in background               │   │
│  └─────────────────────────────────────────────┘   │
│                      ↓ Tauri command                │
│  ┌─────────────────────────────────────────────┐   │
│  │  Rust Backend - Service Manager             │   │
│  │  - Spawns MCP subprocess                    │   │
│  │  - Monitors health/status                   │   │
│  │  - Manages lifecycle                        │   │
│  └─────────────────┬───────────────────────────┘   │
└────────────────────┼───────────────────────────────┘
                     │ spawns subprocess
                     ↓
      ┌──────────────────────────────────┐
      │  MCP Server Process               │
      │  (nodespace --mcp-server)         │
      │  - stdio interface for AI agents  │
      │  - Runs independently             │
      │  - Can survive desktop close      │
      └──────────────┬────────────────────┘
                     │
                     ↓
      ┌──────────────────────────────────┐
      │  Shared Turso Database           │
      │  (Concurrent access via libSQL)  │
      └──────────────────────────────────┘
```

**Key Design Decisions:**
- **Single binary deployment**: Both modes use the same executable
- **Desktop app as control plane**: Users configure MCP via GUI settings
- **Subprocess spawning**: Desktop spawns MCP as separate process when enabled
- **Configurable lifecycle**: User chooses if MCP survives desktop close
- **Shared database**: Both processes access same Turso database concurrently
```

### Service Manager Implementation

The desktop app's Rust backend manages the MCP subprocess:

```rust
// Tauri command to start MCP service
#[tauri::command]
async fn start_mcp_service(config: McpConfig) -> Result<(), String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get executable path: {}", e))?;

    let child = Command::new(exe_path)
        .arg("--mcp-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn MCP server: {}", e))?;

    // Store child process handle for monitoring
    MCP_PROCESS_HANDLE.lock().await.replace(child);

    Ok(())
}

#[tauri::command]
async fn stop_mcp_service() -> Result<(), String> {
    if let Some(mut child) = MCP_PROCESS_HANDLE.lock().await.take() {
        child.kill().map_err(|e| format!("Failed to stop MCP server: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
async fn get_mcp_status() -> Result<McpStatus, String> {
    let handle = MCP_PROCESS_HANDLE.lock().await;
    Ok(McpStatus {
        running: handle.is_some(),
        // Additional status info...
    })
}
```

### MCP Communication Flow

```
AI Agent Process                NodeSpace MCP Server Process
       |                                    |
       | Spawn subprocess with              |
       | --mcp-server flag                  |
       |                                    |
       | ─── JSON Request ──► stdin ──────► | Parse MCP request
       |                                    | ↓
       |                                    | Execute via NodeService
       |                                    | ↓
       |                                    | Access Turso DB
       |                                    | ↓
       | ◄── stdout ◄── JSON Response ◄─── | Return MCP response


Desktop App Process (Parallel)
       |
       | User interacts with UI
       | ↓
       | Tauri commands execute
       | ↓
       | Access same Turso DB (concurrent)
       | ↓
       | Update UI reactively
```

### Concurrent Database Access

Both desktop app and MCP server processes access the same Turso database file simultaneously. This is **safe and supported** by libSQL/Turso architecture:

**How Turso Handles Concurrency:**
- ✅ **WAL Mode**: Write-Ahead Logging enables concurrent readers and writers
- ✅ **Automatic Locking**: libSQL manages locks transparently
- ✅ **Read Concurrency**: Multiple processes can read simultaneously without blocking
- ✅ **Write Serialization**: Writes are automatically serialized via database locks
- ✅ **Transaction Safety**: ACID guarantees maintained across processes

**Example Concurrent Access:**
```
Time  Desktop App Process          MCP Server Process          Database
────────────────────────────────────────────────────────────────────────
T1    SELECT nodes WHERE...       -                           ✓ Read lock
T2    -                           SELECT nodes WHERE...       ✓ Read lock (concurrent)
T3    INSERT INTO nodes...        -                           ✓ Write lock (waits for reads)
T4    -                           INSERT INTO nodes...        ⏳ Waits for write lock
T5    COMMIT                      -                           ✓ Releases lock
T6    -                           [proceeds with write]       ✓ Acquires write lock
```

**Key Point**: No special synchronization code needed - libSQL handles it automatically. However, **UI reactivity requires explicit event coordination** (see Reactive State Synchronization section below).

### Why stdio Instead of HTTP

**Advantages of stdio for MCP:**
- **Standard protocol**: MCP defines stdio as primary transport
- **No network setup**: Direct process communication
- **Security**: No ports to expose, no HTTP attack surface
- **Simplicity**: JSON over text streams
- **Language agnostic**: Any language can read/write stdio

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
        "metadata": {
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

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

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

pub fn run_mcp_stdio_server() {
    let service = create_node_service().expect("Failed to initialize NodeService");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("Error reading from stdin: {}", e);
                continue;
            }
        };

        // Parse JSON-RPC request
        let request: MCPRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = MCPResponse {
                    jsonrpc: "2.0".to_string(),
                    id: 0, // Unknown ID
                    result: None,
                    error: Some(MCPError {
                        code: -32700, // Parse error
                        message: format!("Invalid JSON: {}", e),
                    }),
                };
                writeln!(stdout, "{}", serde_json::to_string(&error_response).unwrap()).unwrap();
                stdout.flush().unwrap();
                continue;
            }
        };

        // Handle request asynchronously
        let response = handle_mcp_request(&service, request).await;

        // Send response
        writeln!(stdout, "{}", serde_json::to_string(&response).unwrap()).unwrap();
        stdout.flush().unwrap();
    }
}

async fn handle_mcp_request(service: &NodeService, request: MCPRequest) -> MCPResponse {
    let result = match request.method.as_str() {
        "create_node" => handle_create_node(service, request.params).await,
        "get_node" => handle_get_node(service, request.params).await,
        "update_node" => handle_update_node(service, request.params).await,
        "delete_node" => handle_delete_node(service, request.params).await,
        "search_nodes" => handle_search_nodes(service, request.params).await,
        "get_children" => handle_get_children(service, request.params).await,
        "move_node" => handle_move_node(service, request.params).await,
        "semantic_search" => handle_semantic_search(service, request.params).await,
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
    metadata: Option<Value>,
}

async fn handle_create_node(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: CreateNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602, // Invalid params
            message: format!("Invalid parameters: {}", e),
        })?;

    let node = Node {
        id: uuid::Uuid::new_v4().to_string(),
        node_type: params.node_type,
        content: params.content,
        parent_id: params.parent_id,
        root_id: "".to_string(), // Will be computed
        before_sibling_id: None,
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        metadata: params.metadata.unwrap_or(Value::Null),
        embedding_vector: None,
    };

    let node_id = service.create_node(node).await
        .map_err(|e| MCPError {
            code: -32603, // Internal error
            message: format!("Failed to create node: {}", e),
        })?;

    Ok(json!({
        "node_id": node_id,
        "success": true
    }))
}

#[derive(Deserialize)]
struct GetNodeParams {
    node_id: String,
}

async fn handle_get_node(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
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
            code: -32000, // Custom error
            message: format!("Node not found: {}", params.node_id),
        }),
    }
}

#[derive(Deserialize)]
struct UpdateNodeParams {
    node_id: String,
    content: Option<String>,
    metadata: Option<Value>,
}

async fn handle_update_node(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: UpdateNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    let update = NodeUpdate {
        content: params.content,
        metadata: params.metadata,
    };

    service.update_node(&params.node_id, update).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to update node: {}", e),
        })?;

    Ok(json!({"success": true}))
}
```

### Hierarchy Operations

```rust
#[derive(Deserialize)]
struct GetChildrenParams {
    parent_id: String,
}

async fn handle_get_children(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: GetChildrenParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    let children = service.get_children(&params.parent_id).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to get children: {}", e),
        })?;

    Ok(serde_json::to_value(children).unwrap())
}

#[derive(Deserialize)]
struct MoveNodeParams {
    node_id: String,
    new_parent_id: Option<String>,
    before_sibling_id: Option<String>,
}

async fn handle_move_node(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: MoveNodeParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    service.move_node(
        &params.node_id,
        params.new_parent_id.as_deref(),
        params.before_sibling_id.as_deref()
    ).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to move node: {}", e),
        })?;

    Ok(json!({"success": true}))
}
```

### AI-Powered Operations

```rust
#[derive(Deserialize)]
struct SemanticSearchParams {
    query: String,
    limit: Option<usize>,
}

async fn handle_semantic_search(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: SemanticSearchParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    let results = service.semantic_search(
        &params.query,
        params.limit.unwrap_or(10)
    ).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to perform semantic search: {}", e),
        })?;

    Ok(serde_json::to_value(results).unwrap())
}

#[derive(Deserialize)]
struct GenerateContentParams {
    prompt: String,
    context_nodes: Option<Vec<String>>,
}

async fn handle_generate_content(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: GenerateContentParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    // Get context nodes if provided
    let context = if let Some(node_ids) = params.context_nodes {
        let nodes = service.get_nodes_by_ids(&node_ids).await
            .map_err(|e| MCPError {
                code: -32603,
                message: format!("Failed to get context nodes: {}", e),
            })?;
        Some(nodes.into_iter().map(|n| n.content).collect::<Vec<_>>().join("\n"))
    } else {
        None
    };

    let generated_content = service.generate_content(&params.prompt, context).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to generate content: {}", e),
        })?;

    Ok(json!({
        "generated_content": generated_content,
        "success": true
    }))
}

#[derive(Deserialize)]
struct NaturalLanguageQueryParams {
    query: String,
}

async fn handle_natural_language_query(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: NaturalLanguageQueryParams = serde_json::from_value(params)
        .map_err(|e| MCPError {
            code: -32602,
            message: format!("Invalid parameters: {}", e),
        })?;

    // Process through three-stage pipeline
    // Stage 1: Intent classification (validates clarity)
    // Stage 2: Function selection (SQL vs semantic search vs workflow)
    // Stage 3: Query execution (Gemma 3 4B-QAT generates SQL)
    let results = service.process_natural_language_query(&params.query).await
        .map_err(|e| MCPError {
            code: -32603,
            message: format!("Failed to process query: {}", e),
        })?;

    Ok(serde_json::to_value(results).unwrap())
}
```

## Natural Language Query Processing

### Architecture Overview

The MCP server supports natural language queries from AI agents through a three-stage pipeline:

```
AI Agent Query → MCP Server → Three-Stage Pipeline → Results
```

**Stage 1: Intent Classification** (< 50ms)
- Validates query clarity using DistilBERT
- Confidence scoring: Clear (>0.85) / Medium (0.65-0.85) / Unclear (<0.65)
- Multi-intent detection
- Rejects ambiguous queries early (before expensive LLM processing)

**Stage 2: Function Selection** (Gemma 3 4B-QAT)
- Determines query type: SQL query, semantic search, or workflow
- Uses validated intent from Stage 1 as context
- Selects appropriate execution strategy

**Stage 3: Query Execution**
- Text-to-SQL generation for structured queries
- Hybrid vector + SQL search for semantic queries
- Workflow generation for automation requests (Phase 2)

### Model: Gemma 3 4B-QAT

**Specifications:**
- **RAM**: 2.7GB (quantization-aware trained)
- **Context**: 128k tokens
- **Capabilities**: Text-to-SQL, function calling, multi-step reasoning, multimodal

**Why Gemma 3 4B-QAT:**
- Efficient for background MCP process (2.7GB RAM)
- Large context window (128k) for full schema + examples
- Handles both SQL generation and workflow automation
- Future-proof (multimodal capabilities)

### MCP Natural Language Query Flow

```
┌────────────────────────────────────────────────┐
│ AI Agent sends natural language query via MCP │
└─────────────────┬──────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────┐
│ Stage 1: Intent Classifier (DistilBERT)        │
│ - Validates clarity                             │
│ - Confidence > 0.85? Continue : Reject          │
└─────────────────┬───────────────────────────────┘
                  │ (only if clear)
                  ▼
┌─────────────────────────────────────────────────┐
│ Stage 2: Function Selector (Gemma 3 4B-QAT)    │
│ - Determines: SQL query vs semantic search     │
│ - Extracts parameters and filters              │
└─────────────────┬───────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────┐
│ Stage 3: Query Execution                        │
│                                                  │
│ IF SQL Query:                                   │
│   → Generate SQL via Gemma 3 4B-QAT             │
│   → Validate (prevent injection)                │
│   → Execute on Turso                            │
│                                                  │
│ IF Semantic Search:                             │
│   → Generate embedding                          │
│   → Hybrid vector + SQL search on Turso        │
│                                                  │
│ IF Workflow (Phase 2):                          │
│   → Generate workflow via Gemma 3 4B-QAT        │
└─────────────────┬───────────────────────────────┘
                  │
                  ▼
           Return structured results
```

### Example: Natural Language to SQL

**AI Agent Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "natural_language_query",
  "params": {
    "query": "Show me all meetings from last week where we discussed budget"
  }
}
```

**Server Processing:**

1. **Stage 1 (Intent Validation)**:
```rust
let classification = intent_classifier.classify(
    "Show me all meetings from last week where we discussed budget"
).await?;
// Result: Clear { intent: RETRIEVE_DATA, confidence: 0.94 }
```

2. **Stage 2 (Function Selection)**:
```rust
let function = gemma_model.select_function(query, classification).await?;
// Result: FunctionCall::SqlQuery
```

3. **Stage 3 (SQL Generation)**:
```rust
let sql = gemma_model.generate_sql(
    query,
    schema_context,  // Fits in 128k context!
    classification
).await?;
```

**Generated SQL:**
```sql
SELECT * FROM nodes
WHERE json_extract(properties, '$.entity_type') = 'Meeting'
  AND date >= '2025-09-30'
  AND date <= '2025-10-06'
  AND (
    content LIKE '%budget%'
    OR json_extract(properties, '$.budget_discussed') = true
  );
```

**MCP Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "id": "node-abc123",
      "node_type": "Meeting",
      "content": "Q4 budget planning discussion",
      "properties": {
        "entity_type": "Meeting",
        "budget_discussed": true,
        "date": "2025-10-02"
      }
    }
  ]
}
```

### Example: Semantic Search with Filters

**AI Agent Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "natural_language_query",
  "params": {
    "query": "Find notes about AI safety from the past month"
  }
}
```

**Execution: Hybrid Vector + SQL**
```sql
SELECT id, content,
       vec_distance_cosine(embedding_vector, ?) as similarity
FROM nodes
WHERE similarity > 0.7
  AND node_type = 'text'
  AND date >= '2025-09-08'
  AND date <= '2025-10-08'
ORDER BY similarity DESC
LIMIT 20;
```

### Configuration

MCP server needs access to three model files:
- `distilbert-intent-classifier.onnx` (Stage 1 - 200MB)
- `gemma-3-4b-it-qat.gguf` (Stages 2 & 3 - ~3GB)
- `bge-small-en-v1.5.onnx` (Embeddings for semantic search - 80MB)

**Startup:**
```rust
pub async fn start_mcp_server() -> Result<()> {
    // Initialize intent classifier (Stage 1)
    let intent_classifier = IntentClassifier::load(
        "models/distilbert-intent-classifier.onnx"
    ).await?;

    // Initialize LLM (Stages 2 & 3)
    let gemma_model = GemmaModel::load(
        "models/gemma-3-4b-it-qat.gguf",
        LlamaParams::default().with_n_ctx(128000)
    ).await?;

    // Initialize embedding engine (for semantic search)
    let embedding_engine = EmbeddingEngine::load(
        "models/bge-small-en-v1.5.onnx"
    ).await?;

    // Start MCP stdio server
    run_mcp_stdio_server(intent_classifier, gemma_model, embedding_engine).await
}
```

## Reactive State Synchronization

> **Related Issue**: [#111 - Implement reactive state synchronization](https://github.com/nodespace/nodespace-core/issues/111)
>
> This section describes the architecture that #111 will implement to keep desktop UI and MCP server in sync.

### The Challenge

When desktop app and MCP server both run simultaneously, they need bidirectional synchronization:

**Problem 1: MCP Changes Need UI Updates**
```
AI Agent → MCP Server → Database (INSERT node)
                            ↓
                    Desktop UI [still showing old data] ❌
```

**Problem 2: UI Changes Need MCP Awareness**
```
User → Desktop UI → Database (UPDATE node)
                        ↓
                MCP Server [may have stale cached data] ⚠️
```

### Solution: Inter-Process Event Bridge (Issue #111)

Two separate processes require **IPC (Inter-Process Communication)** for event coordination:

**Architecture Options:**

#### Option 1: File Watcher (Simple, Limited)
```rust
// Watch for database file changes
let watcher = notify::watcher(db_path)?;
watcher.watch(|event| {
    if event.is_write() {
        // Crude notification - reload everything
        reload_all_data();
    }
});
```
❌ **Issues**: No granularity, expensive full reloads, high latency

#### Option 2: IPC Event Channel (Recommended)

Desktop app and MCP server communicate via IPC mechanism:

**Implementation Approaches:**

**A. Unix Domain Sockets (Best for macOS/Linux)**
```rust
// Desktop app creates IPC server
let socket = UnixListener::bind("/tmp/nodespace-events.sock")?;

// MCP server connects as client
let stream = UnixStream::connect("/tmp/nodespace-events.sock")?;

// Bidirectional event messaging
send_event(&stream, Event::NodeCreated { node_id: "abc" })?;
```

**B. Named Pipes (Cross-platform)**
```rust
// Desktop creates named pipe
#[cfg(windows)]
let pipe = NamedPipe::create("\\\\.\\pipe\\nodespace-events")?;

#[cfg(unix)]
let pipe = NamedPipe::create("/tmp/nodespace-events.pipe")?;
```

**C. Shared Memory + Semaphores (High-performance)**
```rust
// For very high-frequency updates
let shm = SharedMemory::create("nodespace-events", 1024)?;
let semaphore = Semaphore::create("nodespace-sync")?;
```

### Recommended Implementation: Unix Domain Sockets + EventBus

**Architecture:**
```
Desktop App Process                    MCP Server Process
┌──────────────────┐                  ┌─────────────────┐
│ Tauri Backend    │                  │ MCP stdio loop  │
│                  │                  │                 │
│ NodeService ─────┼──── Database ────┼───── NodeService│
│      ↓           │     (shared)     │        ↓        │
│ EventBus ────────┼──── IPC Socket ──┼──── EventBus    │
│      ↓           │   (bidirectional)│        ↓        │
│ Tauri Events     │                  │  (no UI layer)  │
│      ↓           │                  │                 │
│ Svelte UI        │                  │                 │
└──────────────────┘                  └─────────────────┘
```

**Implementation Example:**

```rust
// Shared event types (used by both processes)
#[derive(Serialize, Deserialize, Debug)]
enum NodeSpaceEvent {
    NodeCreated { node_id: String, node_type: String },
    NodeUpdated { node_id: String },
    NodeDeleted { node_id: String },
    NodeMoved { node_id: String, new_parent_id: Option<String> },
}

// Desktop app: IPC server + event receiver
fn setup_desktop_ipc() -> Result<UnixListener, io::Error> {
    let socket_path = "/tmp/nodespace-events.sock";
    let _ = std::fs::remove_file(socket_path); // Clean up existing socket
    let listener = UnixListener::bind(socket_path)?;

    // Spawn thread to handle MCP events
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            tokio::spawn(handle_ipc_client(stream));
        }
    });

    Ok(listener)
}

async fn handle_ipc_client(stream: UnixStream) {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await.ok().flatten() {
        if let Ok(event) = serde_json::from_str::<NodeSpaceEvent>(&line) {
            // Forward to Tauri frontend via EventBus
            DESKTOP_EVENT_BUS.emit(event);
        }
    }
}

// MCP server: IPC client + event sender
async fn setup_mcp_ipc() -> Result<UnixStream, io::Error> {
    let socket_path = "/tmp/nodespace-events.sock";
    let stream = UnixStream::connect(socket_path).await?;
    Ok(stream)
}

async fn send_event_to_desktop(
    stream: &mut UnixStream,
    event: NodeSpaceEvent
) -> Result<(), io::Error> {
    let json = serde_json::to_string(&event)?;
    stream.write_all(format!("{}\n", json).as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

// In MCP request handler
async fn handle_create_node_with_events(
    service: &NodeService,
    params: CreateNodeParams,
    ipc_stream: &mut UnixStream
) -> Result<Value, MCPError> {
    // Create node in database
    let node_id = service.create_node(params).await?;

    // Notify desktop app via IPC
    send_event_to_desktop(
        ipc_stream,
        NodeSpaceEvent::NodeCreated {
            node_id: node_id.clone(),
            node_type: params.node_type.clone(),
        }
    ).await.ok(); // Don't fail if desktop not running

    Ok(json!({ "node_id": node_id, "success": true }))
}
```

### Frontend Event Handling

```typescript
// In Svelte frontend
import { listen } from '@tauri-apps/api/event';
import { nodeStore } from '$lib/stores/nodeStore';

// Listen for MCP-triggered changes
listen('node-created', (event) => {
    const nodeData = event.payload;
    nodeStore.update(nodes => [...nodes, nodeData]);
});

listen('node-updated', async (event) => {
    const { node_id } = event.payload;

    // Refresh the specific node
    const updatedNode = await invoke('get_node_gui', { nodeId: node_id });

    nodeStore.update(nodes => {
        const index = nodes.findIndex(n => n.id === node_id);
        if (index !== -1) {
            nodes[index] = updatedNode;
        }
        return nodes;
    });
});

listen('node-deleted', (event) => {
    const { node_id } = event.payload;
    nodeStore.update(nodes => nodes.filter(n => n.id !== node_id));
});
```

## AI Agent Integration Examples

### Claude Desktop Integration

```json
{
  "name": "nodespace",
  "description": "Access NodeSpace knowledge management system",
  "run": {
    "command": "/Applications/NodeSpace.app/Contents/MacOS/NodeSpace",
    "args": ["--mcp-stdio"],
    "transport": "stdio"
  },
  "methods": [
    "create_node",
    "get_node",
    "update_node",
    "delete_node",
    "search_nodes",
    "semantic_search",
    "get_children",
    "move_node"
  ]
}
```

### Example AI Interaction

**AI Agent Request:**
```
Human: "Create a task to review the Q1 reports, due next Friday"

AI Agent processes and calls:
```

```json
{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "create_node",
    "params": {
        "node_type": "task",
        "content": "Review Q1 reports",
        "metadata": {
            "status": "pending",
            "due_date": "2024-02-09",
            "priority": 3
        }
    }
}
```

**NodeSpace Response:**
```json
{
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
        "node_id": "task-abc123",
        "success": true
    }
}
```

**Reactive UI Update:** Desktop app automatically shows new task in UI

## Security Considerations

### Process Isolation
- MCP server runs in same process as desktop app (shared security context)
- No network exposure - stdio communication only
- Process permissions limited by OS-level restrictions

### Input Validation
- All MCP parameters validated against schemas
- SQL injection prevented by parameterized queries
- File system access controlled by Tauri security policies

### Authentication
- Future enhancement: API keys for MCP access
- Process-level authentication via OS user context
- Optional encryption for sensitive operations

## Performance Optimizations

### Batch Operations
```rust
#[derive(Deserialize)]
struct BatchCreateParams {
    nodes: Vec<CreateNodeParams>,
}

async fn handle_batch_create(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    let params: BatchCreateParams = serde_json::from_value(params)?;

    let mut results = Vec::new();

    // Process in transaction
    let tx = service.begin_transaction().await?;

    for node_params in params.nodes {
        let node_id = service.create_node_in_tx(&tx, node_params).await?;
        results.push(node_id);
    }

    tx.commit().await?;

    Ok(json!({
        "node_ids": results,
        "success": true
    }))
}
```

### Streaming Responses
For large result sets:

```rust
// Future enhancement: streaming JSON responses
async fn handle_large_search(
    service: &NodeService,
    params: Value
) -> Result<Value, MCPError> {
    // Stream results as JSONL instead of single JSON array
    // Useful for searches returning thousands of nodes
}
```

## Architecture Summary

### Key Design Decisions

1. **Single Binary, Dual Modes**
   - Desktop GUI mode (default)
   - MCP server mode (`--mcp-server` flag)
   - Same executable for both, simplifies distribution

2. **Desktop-Managed Service**
   - Desktop app provides settings UI for MCP
   - Spawns MCP as subprocess when enabled
   - Monitors health and lifecycle
   - User controls if MCP survives desktop close

3. **Concurrent Database Access**
   - Both processes access same Turso database
   - libSQL/Turso handles locking automatically
   - WAL mode enables concurrent reads
   - Write operations automatically serialized
   - No special synchronization needed for database access

4. **IPC for Reactive Synchronization (Issue #111)**
   - Unix Domain Sockets for cross-process events
   - EventBus in both processes coordinates state
   - Desktop app runs IPC server, MCP connects as client
   - Bidirectional event flow keeps both processes in sync
   - Tauri events bridge to Svelte UI for reactive updates

### Implementation Phases

**Phase 1: Basic MCP Server (#112)**
- Implement dual-mode binary
- Add MCP protocol handlers (JSON-RPC over stdio)
- Expose all NodeService operations as MCP methods
- Test with Claude Desktop integration

**Phase 2: Desktop Service Manager (#112 continued)**
- Add MCP settings UI
- Implement subprocess spawning/monitoring
- Add lifecycle management (start/stop/status)
- Configuration persistence

**Phase 3: Reactive Synchronization (#111)**
- Implement IPC event channel (Unix Domain Sockets)
- Add EventBus integration for both processes
- Wire Tauri events to Svelte stores
- Test concurrent UI and MCP operations

**Phase 4: Production Hardening**
- Add authentication/authorization
- Implement rate limiting
- Add comprehensive error handling
- Performance optimization and monitoring

### Related Issues

- **#112**: Add MCP stdio server for AI agent access (Phases 1-2)
- **#111**: Implement reactive state synchronization (Phase 3)

This MCP integration design enables AI agents to interact naturally with NodeSpace while maintaining reactive UI updates and system consistency through concurrent database access and IPC event coordination.