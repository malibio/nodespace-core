# AIChatNode Specification

## Overview

The `AIChatNode` is a specialized node type that represents an AI conversation session within NodeSpace. It serves as a **parent container** for the conversation, with messages stored as child nodes. This enables semantic search across chat history and integrates conversations into the knowledge graph.

## Design Philosophy

### Conversation as Node Hierarchy

Unlike external agent protocols where the agent owns conversation history, NodeSpace stores all chat messages as nodes:

```
AIChatNode (session container)
├── ChatMessageNode (role: "user", "Create a project plan")
├── ChatMessageNode (role: "assistant", "I'll create that for you...")
├── ChatToolCallNode (tool: "create_node", args: {...}, result: {...})
├── ChatMessageNode (role: "assistant", "Done! Here's your plan...")
└── ChatMessageNode (role: "user", "Thanks, add due dates")
```

**Benefits**:
- Semantic search across all conversations
- Chat sessions are first-class nodes in the knowledge graph
- Node references (`nodespace://`) create bidirectional relationships
- No external session state to manage
- Works offline (local inference)

### Session as Parent Container

Nodes created by the AI agent during a conversation become children of the AIChatNode:

```
PageNode: "Q4 Planning"
│
├── TextNode: "Overview of Q4 goals..."
│
└── AIChatNode: "Planning Discussion"
    │
    ├── [Chat Messages - see above]
    │
    └── [AI-Created Nodes]
        ├── HeaderNode: "Q4 Project Goals"
        │   ├── TaskNode: "Review budget"
        │   └── TaskNode: "Hire 2 engineers"
        │
        └── TextNode: "Action items summary"
```

## Node Schemas

### AIChatNode

```typescript
interface AIChatNode {
  // Base node properties
  id: string;                    // UUID
  node_type: "ai-chat";
  content: string;               // Chat title/description
  parent_id: string | null;      // Parent node (page/section)
  root_id: string | null;        // Root document
  created_at: string;            // ISO timestamp
  updated_at: string;            // ISO timestamp

  // AI Chat specific properties
  properties: {
    provider: AIProvider;        // Which AI provider
    status: ChatSessionStatus;   // Session lifecycle state
    last_active: string;         // ISO timestamp of last interaction
    message_count: number;       // Total messages in conversation

    // Model info
    model?: string;              // Model used (e.g., "ministral-3-8b", "claude-3-5-sonnet", "gpt-4o")

    // Context management
    context_tokens?: number;     // Current token usage estimate

    // Created nodes tracking
    created_nodes?: string[];    // IDs of nodes created in this session
  };
}

type AIProvider =
  | "native"        // Built-in Ministral 3 via llama.cpp (local, offline)
  | "anthropic"     // Claude via ACP (cloud)
  | "gemini"        // Gemini via ACP (cloud)
  | "openai"        // GPT via ACP (cloud)
  | "mcp:external"; // External agent connected via MCP (future)

type ChatSessionStatus =
  | "active"      // Session is live and can be continued
  | "archived";   // User archived the chat
```

### ChatMessageNode

```typescript
interface ChatMessageNode {
  id: string;
  node_type: "chat-message";
  content: string;               // Message text content
  parent_id: string;             // Always the AIChatNode ID
  root_id: string;               // Root document
  created_at: string;
  updated_at: string;

  properties: {
    role: "user" | "assistant";  // Who sent the message

    // For assistant messages with thinking
    thinking?: string;           // Reasoning content (optional display)

    // Node references in this message
    referenced_nodes?: string[]; // IDs of nodes mentioned

    // Streaming state (temporary)
    streaming?: boolean;         // True while tokens arriving
  };
}
```

### ChatToolCallNode

```typescript
interface ChatToolCallNode {
  id: string;
  node_type: "chat-tool-call";
  content: string;               // Tool name for display
  parent_id: string;             // The AIChatNode ID
  root_id: string;
  created_at: string;
  updated_at: string;

  properties: {
    tool: string;                // Tool name (e.g., "create_node")
    args: Record<string, any>;   // Tool arguments

    status: "pending" | "running" | "completed" | "error";

    // Results
    result?: {
      success: boolean;
      output: string;
      created_node_id?: string;  // If tool created a node
      error?: string;
    };

    // Timing
    started_at?: string;
    completed_at?: string;
  };
}
```

### Database Schema (SurrealDB)

```sql
-- AIChatNode
DEFINE TABLE ai_chat_node SCHEMAFULL;

DEFINE FIELD content ON ai_chat_node TYPE string;
DEFINE FIELD parent_id ON ai_chat_node TYPE option<string>;
DEFINE FIELD root_id ON ai_chat_node TYPE option<string>;
DEFINE FIELD created_at ON ai_chat_node TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON ai_chat_node TYPE datetime DEFAULT time::now();

DEFINE FIELD properties.provider ON ai_chat_node TYPE string
  ASSERT $value IN ["native", "mcp:external"];
DEFINE FIELD properties.status ON ai_chat_node TYPE string
  ASSERT $value IN ["active", "archived"];
DEFINE FIELD properties.last_active ON ai_chat_node TYPE datetime;
DEFINE FIELD properties.message_count ON ai_chat_node TYPE int DEFAULT 0;
DEFINE FIELD properties.model ON ai_chat_node TYPE option<string>;
DEFINE FIELD properties.context_tokens ON ai_chat_node TYPE option<int>;
DEFINE FIELD properties.created_nodes ON ai_chat_node TYPE option<array<string>>;

DEFINE INDEX ai_chat_provider_idx ON ai_chat_node FIELDS properties.provider;
DEFINE INDEX ai_chat_status_idx ON ai_chat_node FIELDS properties.status;
DEFINE INDEX ai_chat_last_active_idx ON ai_chat_node FIELDS properties.last_active;

-- ChatMessageNode
DEFINE TABLE chat_message_node SCHEMAFULL;

DEFINE FIELD content ON chat_message_node TYPE string;
DEFINE FIELD parent_id ON chat_message_node TYPE string;  -- Required: AIChatNode ID
DEFINE FIELD root_id ON chat_message_node TYPE option<string>;
DEFINE FIELD created_at ON chat_message_node TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON chat_message_node TYPE datetime DEFAULT time::now();

DEFINE FIELD properties.role ON chat_message_node TYPE string
  ASSERT $value IN ["user", "assistant"];
DEFINE FIELD properties.thinking ON chat_message_node TYPE option<string>;
DEFINE FIELD properties.referenced_nodes ON chat_message_node TYPE option<array<string>>;
DEFINE FIELD properties.streaming ON chat_message_node TYPE option<bool>;

DEFINE INDEX chat_msg_parent_idx ON chat_message_node FIELDS parent_id;
DEFINE INDEX chat_msg_role_idx ON chat_message_node FIELDS properties.role;

-- ChatToolCallNode
DEFINE TABLE chat_tool_call_node SCHEMAFULL;

DEFINE FIELD content ON chat_tool_call_node TYPE string;
DEFINE FIELD parent_id ON chat_tool_call_node TYPE string;
DEFINE FIELD root_id ON chat_tool_call_node TYPE option<string>;
DEFINE FIELD created_at ON chat_tool_call_node TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON chat_tool_call_node TYPE datetime DEFAULT time::now();

DEFINE FIELD properties.tool ON chat_tool_call_node TYPE string;
DEFINE FIELD properties.args ON chat_tool_call_node TYPE object;
DEFINE FIELD properties.status ON chat_tool_call_node TYPE string
  ASSERT $value IN ["pending", "running", "completed", "error"];
DEFINE FIELD properties.result ON chat_tool_call_node TYPE option<object>;
DEFINE FIELD properties.started_at ON chat_tool_call_node TYPE option<datetime>;
DEFINE FIELD properties.completed_at ON chat_tool_call_node TYPE option<datetime>;

DEFINE INDEX tool_call_parent_idx ON chat_tool_call_node FIELDS parent_id;
DEFINE INDEX tool_call_status_idx ON chat_tool_call_node FIELDS properties.status;
```

## Lifecycle

### Creating a New Chat Session

```typescript
// Frontend: ChatService.ts
async function createNewChat(parentId: string): Promise<AIChatNode> {
  // Create the AIChatNode
  const chatNode = await invoke<AIChatNode>("create_chat_session", {
    parentId,
    title: "New Chat"
  });

  return chatNode;
}

// Backend: Rust Tauri command
#[tauri::command]
async fn create_chat_session(
    parent_id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<AIChatNode, Error> {
    let node_service = &state.node_service;

    let chat_node = node_service.create_node(CreateNodeParams {
        node_type: "ai-chat".to_string(),
        content: title,
        parent_id: Some(parent_id),
        properties: json!({
            "provider": "native",
            "status": "active",
            "last_active": Utc::now().to_rfc3339(),
            "message_count": 0,
            "model": "ministral-3-8b"
        }),
    }).await?;

    Ok(chat_node)
}
```

### Sending a Message

```typescript
// Frontend: Initiate message send
async function sendMessage(chatNodeId: string, userMessage: string): Promise<void> {
  // Stream response from backend
  await invoke("send_chat_message", {
    chatNodeId,
    message: userMessage
  });
}

// Backend: Rust agentic loop
#[tauri::command]
async fn send_chat_message(
    chat_node_id: String,
    message: String,
    state: State<'_, AppState>,
    window: Window,
) -> Result<(), Error> {
    let node_service = &state.node_service;
    let agent = &state.native_agent;

    // 1. Save user message as node
    let user_msg = node_service.create_node(CreateNodeParams {
        node_type: "chat-message".to_string(),
        content: message.clone(),
        parent_id: Some(chat_node_id.clone()),
        properties: json!({ "role": "user" }),
    }).await?;

    // 2. Load conversation history
    let history = load_chat_history(&node_service, &chat_node_id).await?;

    // 3. Run agentic loop (streams events to frontend)
    agent.run_loop(
        &chat_node_id,
        history,
        message,
        |event| {
            // Stream events to frontend
            window.emit("chat-event", event)?;
            Ok(())
        }
    ).await?;

    // 4. Update chat metadata
    node_service.update_node(&chat_node_id, UpdateNodeParams {
        properties: Some(json!({
            "last_active": Utc::now().to_rfc3339(),
            "message_count": history.len() + 2  // +user +assistant
        })),
        ..Default::default()
    }).await?;

    Ok(())
}
```

### Loading Chat History

```typescript
// Frontend: Load existing chat
async function loadChatHistory(chatNodeId: string): Promise<ChatHistory> {
  const messages = await invoke<ChatMessage[]>("get_chat_history", {
    chatNodeId
  });

  return { messages };
}

// Backend: Query child nodes
#[tauri::command]
async fn get_chat_history(
    chat_node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, Error> {
    let node_service = &state.node_service;

    // Get all children of the chat node, ordered by creation time
    let children = node_service.get_children(&chat_node_id).await?;

    // Filter to message and tool call nodes
    let messages: Vec<ChatMessage> = children
        .into_iter()
        .filter(|n| n.node_type == "chat-message" || n.node_type == "chat-tool-call")
        .map(|n| n.into())
        .collect();

    Ok(messages)
}
```

## Context Management

### Token Counting and Limits

The native agent manages context window limits (256K for Ministral 3):

```rust
impl NativeAgent {
    const MAX_CONTEXT_TOKENS: usize = 256_000;
    const RESERVE_FOR_RESPONSE: usize = 8_000;

    async fn prepare_context(&self, history: Vec<ChatMessage>) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut token_count = self.system_prompt_tokens();

        // Add messages from newest to oldest until we hit limit
        for msg in history.into_iter().rev() {
            let msg_tokens = self.count_tokens(&msg.content);

            if token_count + msg_tokens > Self::MAX_CONTEXT_TOKENS - Self::RESERVE_FOR_RESPONSE {
                // Add summarization marker
                messages.insert(0, Message::system(
                    "[Earlier conversation summarized for brevity]"
                ));
                break;
            }

            token_count += msg_tokens;
            messages.insert(0, msg.into());
        }

        messages
    }
}
```

### Conversation Summarization (Future)

For very long conversations that exceed context limits:

```rust
async fn summarize_old_messages(
    &self,
    chat_node_id: &str,
    messages_to_summarize: Vec<ChatMessage>,
) -> Result<String, Error> {
    // Use the agent to summarize older messages
    let summary = self.llama_engine.generate(
        "Summarize this conversation concisely, preserving key decisions and context:",
        &messages_to_summarize
    ).await?;

    // Store summary as a special message node
    self.node_service.create_node(CreateNodeParams {
        node_type: "chat-message".to_string(),
        content: summary.clone(),
        parent_id: Some(chat_node_id.to_string()),
        properties: json!({
            "role": "system",
            "is_summary": true,
            "summarized_count": messages_to_summarize.len()
        }),
    }).await?;

    Ok(summary)
}
```

## Provenance Tracking

### Nodes Created by AI

When the native agent creates nodes, they're tracked for provenance:

```rust
// In tool execution
async fn execute_create_node(&self, args: CreateNodeArgs, chat_node_id: &str) -> ToolResult {
    let node = self.node_service.create_node(CreateNodeParams {
        node_type: args.node_type,
        content: args.content,
        parent_id: args.parent_id.or(Some(chat_node_id.to_string())),
        properties: json!({
            ...args.properties,
            "created_by": "ai",
            "source_chat_node": chat_node_id,
        }),
    }).await?;

    // Track in chat node's created_nodes list
    self.track_created_node(chat_node_id, &node.id).await?;

    ToolResult {
        success: true,
        output: format!("Created {} nodespace://{}", node.node_type, node.id),
        created_node_id: Some(node.id),
    }
}
```

### Querying Created Nodes

```typescript
// Find all nodes created by a specific chat session
async function getNodesCreatedByChat(chatNodeId: string): Promise<Node[]> {
  return await nodeService.query({
    filters: [
      { field: "properties.source_chat_node", op: "=", value: chatNodeId }
    ]
  });
}

// Find all AI-created nodes
async function getAllAICreatedNodes(): Promise<Node[]> {
  return await nodeService.query({
    filters: [
      { field: "properties.created_by", op: "=", value: "ai" }
    ]
  });
}
```

## UI States

### Active Session

```
┌─────────────────────────────────────────────────────────────┐
│  Chat: Q4 Planning Discussion                    [Archive]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  You: Create a project plan for Q4                          │
│                                                             │
│  Assistant: I'll create the following structure...          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ ⚙ create_node                              [expand] │   │
│  │   Created: Q4 Project Plan                          │   │
│  └─────────────────────────────────────────────────────┘   │
│  • [# Q4 Project Plan]                                      │
│  • [☐ Review budget]                                        │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  [@] Type a message...                              [Send]  │
└─────────────────────────────────────────────────────────────┘
```

### Streaming Response

```
┌─────────────────────────────────────────────────────────────┐
│  Chat: Q4 Planning Discussion                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  You: Add due dates to all tasks                            │
│                                                             │
│  Assistant: I'll update the tasks with due dates...█        │
│  [Streaming...]                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  [Cancel]                                                   │
└─────────────────────────────────────────────────────────────┘
```

### Model Not Installed

```
┌─────────────────────────────────────────────────────────────┐
│  New Chat                                                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  The AI assistant requires a local model to be installed.   │
│                                                             │
│  Model: Ministral 3 8B                                      │
│  Size: ~4.5 GB                                              │
│  Disk space available: 128 GB                               │
│                                                             │
│  [Download and Install]                                     │
│                                                             │
│  ────────────────────────────────────────────────────────   │
│                                                             │
│  Or connect an external AI via MCP:                         │
│  [Configure External Agent]                                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Mention Relationships

When users or AI reference nodes in chat, mention edges are created:

```typescript
// Process mentions in user message
async function processUserMentions(
  chatNodeId: string,
  message: string,
  mentionedNodeIds: string[]
): Promise<void> {
  // Save to message node
  await nodeService.update(messageId, {
    properties: { referenced_nodes: mentionedNodeIds }
  });

  // Create mention edges
  for (const nodeId of mentionedNodeIds) {
    await createMentionEdge({
      from: chatNodeId,
      to: nodeId,
      context: "chat_mention"
    });
  }
}

// Process references in AI response
async function processAIReferences(
  chatNodeId: string,
  assistantMessageId: string,
  responseContent: string
): Promise<void> {
  const referencedIds = extractNodespaceLinks(responseContent);

  await nodeService.update(assistantMessageId, {
    properties: { referenced_nodes: referencedIds }
  });

  for (const nodeId of referencedIds) {
    await createMentionEdge({
      from: chatNodeId,
      to: nodeId,
      context: "ai_reference"
    });
  }
}
```

## Error Handling

### Model Loading Failure

```rust
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Model not found: {path}")]
    ModelNotFound { path: PathBuf },

    #[error("Insufficient memory: need {required}GB, have {available}GB")]
    InsufficientMemory { required: f64, available: f64 },

    #[error("GPU acceleration unavailable, falling back to CPU")]
    GpuUnavailable,

    #[error("Context overflow: conversation exceeds {max_tokens} tokens")]
    ContextOverflow { max_tokens: usize },

    #[error("Tool execution failed: {tool} - {message}")]
    ToolError { tool: String, message: String },
}
```

### Graceful Degradation

```rust
impl NativeAgent {
    async fn initialize(&mut self) -> Result<(), AgentError> {
        // Try GPU first
        match self.llama_engine.load_with_gpu().await {
            Ok(_) => return Ok(()),
            Err(e) => {
                log::warn!("GPU unavailable: {}, falling back to CPU", e);
            }
        }

        // Fall back to CPU
        self.llama_engine.load_cpu_only().await?;

        // Warn user about performance
        self.emit_warning("Running on CPU - responses may be slower");

        Ok(())
    }
}
```

## Implementation Checklist

### Phase 1: Basic AIChatNode
- [ ] Define node schemas in SurrealDB
- [ ] Add "ai-chat", "chat-message", "chat-tool-call" to node type enum
- [ ] Implement Tauri commands for chat CRUD
- [ ] Basic ChatPanel component

### Phase 2: Native Agent Integration
- [ ] Port LlamaEngine from nodespace-experiment-nlp
- [ ] Implement agentic loop with tool execution
- [ ] Wire up streaming events to frontend
- [ ] Save messages and tool calls as nodes

### Phase 3: Context & History
- [ ] Token counting and context management
- [ ] Load/display conversation history
- [ ] Handle context overflow (truncation/summarization)

### Phase 4: Provenance & Relationships
- [ ] Track created_by and source_chat_node on nodes
- [ ] Create mention edges for references
- [ ] Query nodes created by specific chat
- [ ] Show created nodes in chat UI

### Phase 5: Model Distribution
- [ ] Model download UI with progress
- [ ] First-run experience
- [ ] Error handling and recovery
- [ ] Update/version management

## Related Documents

- [AI Integration Overview](./ai-integration-overview.md)
- [Chat UI Implementation Guide](./chat-ui-implementation-guide.md)
- [Node Reference System in Chat](./node-reference-system-in-chat.md)
