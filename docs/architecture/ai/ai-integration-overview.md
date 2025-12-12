# AI Integration Architecture Overview

## Executive Summary

NodeSpace implements AI capabilities through two complementary paths:

1. **External Agents (MCP)**: Developers use their existing AI tools (Claude Code, Cursor, etc.) which connect to NodeSpace via MCP
2. **Embedded Agent (Native)**: A built-in AI assistant using local inference for non-technical users

This architecture prioritizes simplicity: external agents handle their own complexity, while our native agent is built directly into the Tauri app with no protocol overhead.

## What We Build vs What We Use

### Critical Distinction

| Component | Build or Use | Notes |
|-----------|--------------|-------|
| **Chat UI** | **Build** | Svelte components for conversation rendering |
| **MCP Server** | **Build** (exists) | NodeSpace tools for external agents |
| **Native Agent** | **Build** | Rust, integrated into Tauri, uses Ministral 3 |
| External agents | **Use as-is** | Claude Code, Cursor, etc. connect via MCP |

**We are NOT building ACP adapters or managing external agent protocols.** External agents connect to NodeSpace the same way they connect to any MCP server.

## Architecture Philosophy

### Two Communication Paths

| Path | Direction | Protocol | Purpose |
|------|-----------|----------|---------|
| **Outside → In** | External Agent → NodeSpace | MCP | Claude Code/Cursor accessing knowledge base |
| **Inside** | User ↔ Native Agent | Direct Rust calls | Embedded AI assistant for non-developers |

```
┌─────────────────────────────────────────────────────────────────────┐
│                           NodeSpace                                  │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  Native Chat Panel (Svelte)                        [WE BUILD]  │ │
│  │  - Conversation thread                                          │ │
│  │  - Tool call visualization                                      │ │
│  │  - Streaming response display                                   │ │
│  │  - Node creation previews                                       │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                          │                                           │
│                    Tauri IPC                                         │
│                          │                                           │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  Tauri Backend (Rust)                              [WE BUILD]  │ │
│  │                                                                 │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  Native Agent                                             │  │ │
│  │  │  - Agentic loop (inspired by OpenCode)                    │  │ │
│  │  │  - Tool definitions & execution                           │  │ │
│  │  │  - Direct NodeService calls (no MCP/HTTP)                 │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  │                          │                                      │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  LlamaEngine (llama.cpp Rust bindings)                    │  │ │
│  │  │  - Ministral 3 8B with native function calling            │  │ │
│  │  │  - Metal/CUDA acceleration                                 │  │ │
│  │  │  - Streaming token generation                              │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  │                          │                                      │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  NodeService                                              │  │ │
│  │  │  - Direct function calls from agent                       │  │ │
│  │  │  - No protocol overhead                                    │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  MCP Server                                        [WE BUILD]  │ │
│  │  - Exposes NodeSpace tools to EXTERNAL agents                  │ │
│  │  - Claude Code, Cursor, etc. connect here                      │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
            │
            │ MCP (External only)
            ▼
    Claude Code CLI, Cursor, etc.
    (user's own install, connects via MCP)
```

### Key Design Decisions

1. **No ACP for Native Agent**: We control both ends, so no need for protocol overhead. Direct Rust function calls.

2. **External Agents Use MCP Only**: Developers already have Claude Code, Cursor, etc. They connect via MCP like any other tool.

3. **Single Binary**: The native agent is compiled into the Tauri app. No subprocess management, no separate runtime.

4. **Inference at C++ Level**: Regardless of wrapper language (Rust, Python, TypeScript), actual LLM inference happens in llama.cpp (C++). Rust provides:
   - Minimal overhead (~10ms vs ~100ms for HTTP-based solutions)
   - No runtime dependencies (no Python, no Node.js)
   - Direct integration with NodeService

5. **Ministral 3 for Native Function Calling**: No prompt engineering for tool format - the model handles it natively.

## User Segmentation

| User Type | AI Experience |
|-----------|---------------|
| **Developers** | Use their existing AI tools (Claude Code, Cursor) via MCP |
| **Non-technical users** | Use the built-in native agent with local inference |

## Native Agent Architecture

### Why Rust + llama.cpp?

All LLM inference ultimately happens in C++ (llama.cpp). The wrapper language only affects:

| Aspect | Rust (Direct) | Python/Ollama |
|--------|---------------|---------------|
| **Runtime overhead** | ~5-10MB | ~100-200MB |
| **Startup latency** | ~10-50ms | ~200-500ms |
| **Tool execution** | Direct function call | HTTP round-trip |
| **IPC** | None | JSON over HTTP |
| **Subprocess management** | None | Must manage Ollama |
| **Actual inference speed** | **Same** | **Same** |

The inference speed is identical because it's the same C++ code. Rust wins on integration simplicity and reduced overhead per interaction.

### Model: Ministral 3 8B

| Capability | Details |
|------------|---------|
| **Native function calling** | No prompt hacking needed |
| **JSON output mode** | Structured responses |
| **Context window** | 256K tokens |
| **License** | Apache 2.0 (can bundle) |
| **Size** | ~4-5GB (Q4_K_M quantization) |

### Agentic Loop (Inspired by OpenCode)

The loop is simple because Ministral 3 handles tool calls natively:

```rust
async fn agent_loop(chat_node_id: &str, user_message: &str) -> String {
    let mut messages = load_history_from_chat_node(chat_node_id);
    messages.push(UserMessage(user_message));

    loop {
        // 1. Call LLM with tool definitions
        let response = llama_engine.generate_with_tools(&messages, &tools);

        // 2. If no tool calls, we're done
        if response.tool_calls.is_empty() {
            save_assistant_message(chat_node_id, &response.text);
            return response.text;
        }

        // 3. Execute tools (direct NodeService calls)
        for tool_call in response.tool_calls {
            let result = match tool_call.name {
                "create_node" => node_service.create_node(tool_call.args).await,
                "query_nodes" => node_service.query_nodes(tool_call.args).await,
                "search_semantic" => node_service.search_semantic(tool_call.args).await,
                // ... other tools
            };
            messages.push(ToolResult(tool_call.id, result));
        }

        // 4. Loop continues with tool results in context
    }
}
```

### Tools (Direct NodeService Calls)

No MCP for the native agent - direct function calls:

| Tool | NodeService Method |
|------|-------------------|
| `create_node` | `node_service.create_node()` |
| `update_node` | `node_service.update_node()` |
| `delete_node` | `node_service.delete_node()` |
| `query_nodes` | `node_service.query_nodes()` |
| `search_semantic` | `node_service.search_semantic()` |
| `get_children` | `node_service.get_children()` |
| `create_from_markdown` | `node_service.create_nodes_from_markdown()` |

## Session Management

### AIChatNode as Session

Each chat session is an AIChatNode. The node ID is the session ID.

| Stored in AIChatNode | Stored in Children |
|---------------------|-------------------|
| Provider (native/external) | User messages |
| Title/label | Assistant messages |
| Created/last active | Tool calls & results |
| Status (active/archived) | |

### Chat History Storage

Unlike ACP where the agent owns history, our native agent stores everything in NodeSpace:

```
AIChatNode (session)
├── TextNode (user: "Create a task for Q4 planning")
├── TextNode (assistant: "I'll create that task for you")
├── ToolCallNode (create_node: {...})
├── TextNode (assistant: "Done! Created task: Q4 Planning")
└── TextNode (user: "Thanks, add a due date")
```

This enables:
- Semantic search across chat history
- Node references (`nodespace://`) in conversations
- Chat sessions as first-class nodes in the knowledge graph

## MCP Server (External Agents Only)

The MCP server exposes NodeSpace tools to external agents (Claude Code, Cursor, etc.):

```json
{
  "name": "nodespace",
  "command": "nodespace-mcp",
  "args": ["--stdio"]
}
```

External agents discover and use these tools via standard MCP protocol. We don't control their agentic loops - they handle everything.

## Operational Considerations

### Performance Characteristics

Expected performance by hardware tier:

| Hardware | Model Load | First Token | Tokens/sec | RAM Usage |
|----------|------------|-------------|------------|-----------|
| M1 Mac (8GB) | ~15s | ~500ms | 10-15 t/s | ~6-8GB |
| M1 Pro (16GB) | ~10s | ~300ms | 20-30 t/s | ~8-10GB |
| M3 Max (32GB) | ~5s | ~200ms | 40-50 t/s | ~10-12GB |
| NVIDIA RTX 3080 | ~8s | ~250ms | 30-40 t/s | ~8GB VRAM |

**Minimum Requirements:**
- 8GB RAM (model runs but may swap)
- 10GB free disk space (model + temp files)
- Metal (Mac) or CUDA (NVIDIA) recommended for acceptable performance

### Error Recovery and Graceful Degradation

```rust
pub enum AgentError {
    ModelNotFound { path: PathBuf },
    InsufficientMemory { required_gb: f64, available_gb: f64 },
    GpuUnavailable,
    ContextOverflow { max_tokens: usize },
    ToolExecutionFailed { tool: String, error: String },
    InferenceTimeout { seconds: u64 },
}
```

**Recovery Strategies:**

| Error | Recovery |
|-------|----------|
| GPU unavailable | Fall back to CPU with performance warning |
| Insufficient memory | Reduce context window, warn user |
| Context overflow | Summarize older messages, continue |
| Tool failure | Report error to user, continue conversation |
| Model load failure | Show download/repair UI |

### Tool Call Safety

```rust
pub struct ToolCallPolicy {
    /// Maximum tool calls per turn (prevent infinite loops)
    max_calls_per_turn: usize,  // Default: 10

    /// Tools requiring user confirmation
    require_confirmation: Vec<String>,  // e.g., ["delete_node"]

    /// Rate limiting
    max_creates_per_minute: usize,  // Default: 20
    max_deletes_per_minute: usize,  // Default: 5
}
```

**Safety Measures:**
- "Doom loop" detection: same tool + same args 3x → pause and ask user
- Destructive operations (`delete_node`) require confirmation
- Rate limiting prevents runaway agents
- All tool calls logged for audit

### Chat History Management

**Context Window Strategy (256K tokens):**
- Reserve 8K tokens for response
- Load messages newest-first until limit approached
- Insert `[Earlier conversation summarized]` marker when truncating

**Long-term Storage:**
- All messages stored as nodes (no loss)
- Automatic summarization after 100+ messages (future)
- Archive inactive chats after 90 days (future)
- Full-text search via existing NodeSpace search

### Privacy Guarantees

**Local-Only Inference:**
- All inference happens on device via llama.cpp
- No telemetry, no license checks, no network calls during inference
- Model downloaded once from Hugging Face, verified by SHA256

**Data Residency:**
- Chat history stored in local SurrealDB
- No cloud sync for conversations
- User can export/delete all AI data

**Auditability:**
- Native agent code is part of open-source NodeSpace
- llama.cpp is open-source (MIT license)
- Ministral 3 is Apache 2.0 licensed

### Model Distribution

**Download Strategy:**
- Source: Hugging Face Hub (reliable CDN)
- Format: GGUF (Q4_K_M quantization, ~4.5GB)
- Verification: SHA256 hash check
- Resumable: HTTP range requests for interrupted downloads

**First-Run Experience:**
1. Detect missing model
2. Show disk space check (~10GB needed)
3. Download with progress bar (background capable)
4. Verify integrity
5. Load model and run quick test
6. Ready to chat

**Update Policy:**
- Model version pinned to app version
- Updates bundled with app updates
- No automatic model updates (user control)

## Package Structure

```
nodespace-core/
├── packages/
│   └── desktop-app/
│       ├── src/
│       │   └── lib/
│       │       ├── components/
│       │       │   └── chat/              # Chat UI components
│       │       └── services/
│       │           └── chat-service.ts    # Tauri IPC for chat
│       │
│       └── src-tauri/
│           └── src/
│               ├── agent/                 # Native agent
│               │   ├── mod.rs             # Agentic loop
│               │   ├── tools.rs           # Tool definitions
│               │   └── llama_engine.rs    # llama.cpp integration
│               │
│               ├── node_service.rs        # Existing NodeService
│               └── mcp_server.rs          # MCP for external agents
│
│   └── nodespace-mcp/                     # Standalone MCP server (optional)
│       └── ...                            # For non-Tauri usage
```

## Implementation Phases

### Phase 1: Native Agent Core
- Port LlamaEngine from `nodespace-experiment-nlp` to `src-tauri`
- Implement basic agentic loop
- Define core tools (create_node, query_nodes, search_semantic)
- Wire up Tauri commands for frontend

### Phase 2: Chat UI
- ChatPanel component with message rendering
- Streaming response display
- Tool call visualization (collapsible)
- AIChatNode creation and management

### Phase 3: Model & First Run
- Model download UI (~4-5GB)
- Progress indicator
- Model selection (if multiple)
- First-run experience

### Phase 4: Node Integration
- Node reference parsing (`nodespace://`) in responses
- Clickable node pills with type-aware decoration
- @ mention autocomplete in prompt editor
- Chat sessions in knowledge graph

## Related Documents

- [AIChatNode Specification](./ai-chat-node-specification.md) - Chat session node type
- [Chat UI Implementation Guide](./chat-ui-implementation-guide.md) - Frontend components
- [Node Reference System](./node-reference-system-in-chat.md) - @ mentions and node links

## References

- [llama.cpp](https://github.com/ggml-org/llama.cpp) - C++ inference engine
- [llama-cpp-2](https://crates.io/crates/llama-cpp-2) - Rust bindings
- [Ministral 3](https://docs.unsloth.ai/models/ministral-3) - Model documentation
- [OpenCode](https://github.com/sst/opencode) - Agentic loop reference (MIT license)
- [MCP Specification](https://modelcontextprotocol.io/) - For external agent integration
