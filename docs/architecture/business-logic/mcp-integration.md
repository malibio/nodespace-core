# NodeSpace MCP Integration Design

## Overview

NodeSpace integrates with the **Model Context Protocol (MCP)** to provide AI agents with programmatic access to the knowledge management system. This enables AI assistants to read, write, and manipulate nodes through a standardized protocol while maintaining reactive UI synchronization.

## MCP Architecture Strategy

### Dual-Mode Application Design

NodeSpace desktop app can run in two modes:

1. **Desktop GUI Mode** (default): Normal Tauri application with UI
2. **MCP Server Mode**: Headless mode responding to AI agents via stdio

```rust
// Single binary, dual functionality
fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--mcp-stdio") => {
            // Run as MCP server (headless, stdio communication)
            run_mcp_stdio_server();
        },
        Some("--help") => {
            print_usage();
        },
        _ => {
            // Run normal desktop application
            run_desktop_app();
        }
    }
}
```

### MCP Communication Flow

```
AI Agent Process                     NodeSpace Process
       |                                    |
       | Spawn subprocess with              |
       | --mcp-stdio flag                   |
       |                                    |
       | ─── JSON Request ──► stdin ──────► | Parse MCP request
       |                                    | ↓
       |                                    | Execute via NodeService
       |                                    | ↓
       | ◄── stdout ◄── JSON Response ◄─── | Return MCP response
```

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
```

## Reactive State Synchronization

### The Challenge

When AI agents modify nodes via MCP, the desktop UI needs to update reactively:

```
AI Agent → MCP Server → NodeService → Database
                            ↓
                    Desktop UI needs update
```

### Solution: Event Bridge Pattern

```rust
// In Tauri desktop app
fn run_desktop_app_with_mcp() {
    tauri::Builder::default()
        .setup(|app| {
            let service = Arc::new(create_node_service()?);
            let app_handle = app.handle();

            // Store service for Tauri commands
            app.manage(service.clone());

            // Spawn MCP server in background thread
            let mcp_service = service.clone();
            let mcp_app_handle = app_handle.clone();
            std::thread::spawn(move || {
                run_mcp_server_with_events(mcp_service, mcp_app_handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_node_gui,
            get_node_gui,
            // ... other GUI commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// MCP server that emits events to GUI
fn run_mcp_server_with_events(
    service: Arc<NodeService>,
    app_handle: tauri::AppHandle
) {
    // Same stdio loop, but emit events after operations
    for line in io::stdin().lock().lines() {
        // ... parse request ...

        let response = handle_mcp_request_with_events(&service, request, &app_handle).await;

        // ... send response ...
    }
}

async fn handle_mcp_request_with_events(
    service: &NodeService,
    request: MCPRequest,
    app_handle: &tauri::AppHandle
) -> MCPResponse {
    let response = handle_mcp_request(service, request).await;

    // Emit events for successful operations
    if response.error.is_none() {
        match request.method.as_str() {
            "create_node" => {
                app_handle.emit_all("node-created", &response.result).unwrap();
            },
            "update_node" => {
                app_handle.emit_all("node-updated", json!({
                    "node_id": request.params["node_id"]
                })).unwrap();
            },
            "delete_node" => {
                app_handle.emit_all("node-deleted", json!({
                    "node_id": request.params["node_id"]
                })).unwrap();
            },
            _ => {}
        }
    }

    response
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

This MCP integration design enables AI agents to interact naturally with NodeSpace while maintaining reactive UI updates and system consistency.