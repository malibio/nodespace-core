# AI Integration Architecture Overview

## Executive Summary

NodeSpace implements AI capabilities through three complementary paths:

1. **External Agents (MCP)**: Developers use their existing AI tools (Claude Code, Cursor, etc.) which connect to NodeSpace via MCP
2. **Cloud Providers (ACP)**: NodeSpace connects to Anthropic, Gemini, OpenAI via Agent Client Protocol for powerful remote inference
3. **Embedded Agent (Native)**: A built-in AI assistant using local inference (Ministral 3 via llama.cpp) for offline/privacy-focused users

This architecture provides flexibility: use cloud providers for maximum capability, local inference for privacy/offline, or let developers use their own tools.

## What We Build vs What We Use

### Critical Distinction

| Component | Build or Use | Notes |
|-----------|--------------|-------|
| **Chat UI** | **Build** | Svelte components for conversation rendering |
| **MCP Server** | **Build** (exists) | NodeSpace tools for external agents |
| **ACP Client** | **Build** | Connect to Anthropic, Gemini, OpenAI |
| **Native Agent** | **Build** | Rust, integrated into Tauri, uses Ministral 3 |
| External agents | **Use as-is** | Claude Code, Cursor, etc. connect via MCP |
| Cloud LLM APIs | **Use as-is** | Anthropic, Gemini, OpenAI via their APIs |

**Key distinction**: MCP is for external agents connecting TO us. ACP is for us connecting OUT to cloud providers.

## Architecture Philosophy

### Three Communication Paths

| Path | Direction | Protocol | Purpose |
|------|-----------|----------|---------|
| **Outside → In** | External Agent → NodeSpace | MCP | Claude Code/Cursor accessing knowledge base |
| **Inside → Out** | NodeSpace → Cloud Provider | ACP | Anthropic, Gemini, OpenAI as remote LLM |
| **Inside (local)** | User ↔ Native Agent | Direct Rust | Ministral 3 via llama.cpp, no network |

```
┌─────────────────────────────────────────────────────────────────────┐
│                           NodeSpace                                  │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  Chat Panel (Svelte)                               [WE BUILD]  │ │
│  │  - Provider selector: Native | Claude | Gemini | GPT-4         │ │
│  │  - Conversation thread                                          │ │
│  │  - Tool call visualization                                      │ │
│  │  - Streaming response display                                   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                          │                                           │
│                    Tauri IPC                                         │
│                          │                                           │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  Tauri Backend (Rust)                              [WE BUILD]  │ │
│  │                                                                 │ │
│  │  ┌─────────────────────┐  ┌─────────────────────────────────┐  │ │
│  │  │  Native Agent       │  │  ACP Client                      │  │ │
│  │  │  - llama.cpp        │  │  - Anthropic adapter             │  │ │
│  │  │  - Ministral 3 8B   │  │  - Gemini adapter                │  │ │
│  │  │  - Direct Rust      │  │  - OpenAI adapter                │  │ │
│  │  │  - Offline capable  │  │  - Streaming support             │  │ │
│  │  └─────────────────────┘  └─────────────────────────────────┘  │ │
│  │           │                            │                        │ │
│  │           └────────────┬───────────────┘                        │ │
│  │                        │                                        │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  Unified Agent Interface                                  │  │ │
│  │  │  - Same agentic loop for all providers                    │  │ │
│  │  │  - Tool definitions & execution                           │  │ │
│  │  │  - Direct NodeService calls                               │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  │                          │                                      │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  NodeService                                              │  │ │
│  │  │  - Direct function calls from agent                       │  │ │
│  │  │  - No protocol overhead for tool execution                │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  │                                                                 │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  MCP Server (for external agents)             [WE BUILD]  │  │ │
│  │  │  - Exposes NodeSpace tools to Claude Code, Cursor, etc.   │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
         │                              │
         │ MCP                          │ ACP
         ▼                              ▼
   Claude Code, Cursor            Anthropic, Gemini, OpenAI
   (connects TO NodeSpace)        (NodeSpace connects OUT)
```

### Key Design Decisions

1. **ACP for Cloud Providers**: Standard protocol for connecting to Anthropic, Gemini, OpenAI - enables provider switching and consistent tool handling.

2. **No ACP for Native Agent**: We control both ends (llama.cpp + NodeSpace), so no protocol overhead needed. Direct Rust function calls.

3. **External Agents Use MCP Only**: Developers already have Claude Code, Cursor, etc. They connect via MCP like any other tool.

4. **Unified Agentic Loop**: Same tool definitions, same execution logic, regardless of provider. Only the LLM call differs.

5. **Single Binary**: The native agent is compiled into the Tauri app. No subprocess management, no separate runtime.

6. **Inference at C++ Level**: For local inference, actual LLM processing happens in llama.cpp (C++). Rust provides:
   - Minimal overhead (~10ms vs ~100ms for HTTP-based solutions)
   - No runtime dependencies (no Python, no Node.js)
   - Direct integration with NodeService

7. **Ministral 3 for Native Function Calling**: No prompt engineering for tool format - the model handles it natively.

## User Segmentation

| User Type | AI Experience |
|-----------|---------------|
| **Developers** | Use their existing AI tools (Claude Code, Cursor) via MCP |
| **Power users** | Use cloud providers (Claude, Gemini, GPT-4) via ACP for max capability |
| **Privacy-focused users** | Use the built-in native agent with local inference (Ministral 3) |
| **Offline users** | Native agent works without internet connection |

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

## Cloud Providers via ACP

### Why ACP for Cloud Providers?

ACP (Agent Client Protocol) provides a standard interface for connecting to cloud LLM providers:

| Benefit | Description |
|---------|-------------|
| **Provider abstraction** | Switch between Anthropic, Gemini, OpenAI without code changes |
| **Consistent tool handling** | Same tool format across all providers |
| **Streaming support** | Unified streaming interface |
| **Auth management** | Standard API key handling |
| **Error handling** | Consistent error types across providers |

### Supported Providers

| Provider | Model Examples | Strengths |
|----------|---------------|-----------|
| **Anthropic** | Claude 3.5 Sonnet, Claude 3 Opus | Best reasoning, safety |
| **Google** | Gemini 1.5 Pro, Gemini 2.0 Flash | Multimodal, speed |
| **OpenAI** | GPT-4o, o1 | Broad capabilities |

### ACP Client Architecture

```rust
/// Unified interface for all LLM providers
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Response, ProviderError>;

    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<impl Stream<Item = StreamEvent>, ProviderError>;
}

/// Provider implementations
pub struct AnthropicProvider { api_key: String, model: String }
pub struct GeminiProvider { api_key: String, model: String }
pub struct OpenAIProvider { api_key: String, model: String }
pub struct NativeProvider { engine: LlamaEngine }  // Local, no ACP
```

### Provider Selection

Users select their provider in the chat UI:

```typescript
type AIProvider =
  | { type: "native" }                              // Local Ministral 3
  | { type: "anthropic"; model: string }            // Claude via ACP
  | { type: "gemini"; model: string }               // Gemini via ACP
  | { type: "openai"; model: string };              // GPT via ACP
```

### Authentication

Each provider uses its standard authentication mechanism - we don't manage API keys:

| Provider | Auth Method |
|----------|-------------|
| **Anthropic** | `ANTHROPIC_API_KEY` env var or SDK config |
| **Google** | `GOOGLE_API_KEY` env var or Google Cloud auth |
| **OpenAI** | `OPENAI_API_KEY` env var or SDK config |
| **Native** | No auth needed (local inference) |

Users configure their keys via standard methods (environment variables, provider CLI tools, etc.). NodeSpace just uses the provider SDKs which handle auth automatically.

## Session Management

### AIChatNode as Session

Each chat session is an AIChatNode. The node ID is the session ID.

| Stored in AIChatNode | Stored in Children |
|---------------------|-------------------|
| Provider (native/anthropic/gemini/openai) | User messages |
| Model (e.g., "claude-3-5-sonnet") | Assistant messages |
| Title/label | Tool calls & results |
| Created/last active | |
| Status (active/archived) | |

### Chat History Storage

All providers (native and cloud) store chat history in NodeSpace as nodes:

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

**Native Agent (Local Inference):**
- All inference happens on device via llama.cpp
- No telemetry, no license checks, no network calls during inference
- Model downloaded once from Hugging Face, verified by SHA256
- Complete privacy - conversations never leave your machine

**Cloud Providers:**
- Conversations sent to provider (Anthropic, Google, OpenAI) per their privacy policies
- Clear UI indicator when using cloud vs local
- User configures API keys via standard provider mechanisms (env vars, SDK config)
- Provider selection persisted per-session

**Data Residency (All Providers):**
- Chat history stored in local SurrealDB
- No NodeSpace cloud sync for conversations
- User can export/delete all AI data

**Auditability:**
- Native agent code is part of open-source NodeSpace
- llama.cpp is open-source (MIT license)
- Ministral 3 is Apache 2.0 licensed
- ACP client code is open-source

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
│       │       │       ├── chat-panel.svelte
│       │       │       └── provider-selector.svelte
│       │       └── services/
│       │           └── chat-service.ts    # Tauri IPC for chat
│       │
│       └── src-tauri/
│           └── src/
│               ├── agent/                 # Unified agent system
│               │   ├── mod.rs             # Agentic loop (shared)
│               │   ├── tools.rs           # Tool definitions
│               │   └── provider.rs        # LLMProvider trait
│               │
│               ├── providers/             # LLM provider implementations
│               │   ├── mod.rs             # Provider registry
│               │   ├── native.rs          # llama.cpp / Ministral 3
│               │   ├── anthropic.rs       # Claude via ACP
│               │   ├── gemini.rs          # Gemini via ACP
│               │   └── openai.rs          # GPT via ACP
│               │
│               ├── node_service.rs        # Existing NodeService
│               └── mcp_server.rs          # MCP for external agents
│
│   └── nodespace-mcp/                     # Standalone MCP server (optional)
│       └── ...                            # For non-Tauri usage
```

## Implementation Phases

### Phase 1: Unified Agent Core
- Define LLMProvider trait and tool definitions
- Port LlamaEngine from `nodespace-experiment-nlp` to `src-tauri`
- Implement basic agentic loop (provider-agnostic)
- Wire up Tauri commands for frontend

### Phase 2: Chat UI
- ChatPanel component with message rendering
- Provider selector (Native | Claude | Gemini | GPT-4)
- Streaming response display
- Tool call visualization (collapsible)
- AIChatNode creation and management

### Phase 3: Cloud Providers (ACP)
- Implement AnthropicProvider (Claude)
- Implement GeminiProvider
- Implement OpenAIProvider
- Provider-specific error handling (missing API key, rate limits, etc.)

### Phase 4: Native Model & First Run
- Model download UI (~4-5GB)
- Progress indicator
- Hardware capability detection
- First-run experience with provider choice

### Phase 5: Node Integration
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
- [ACP Specification](https://agentclientprotocol.com/) - For cloud provider integration
- [Anthropic API](https://docs.anthropic.com/en/api) - Claude API documentation
- [Gemini API](https://ai.google.dev/gemini-api/docs) - Google Gemini documentation
- [OpenAI API](https://platform.openai.com/docs) - GPT API documentation
