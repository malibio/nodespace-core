# AI Integration

> **Note**: See the detailed documentation at `/docs/architecture/ai/` for complete specifications.

## Current Architecture

NodeSpace's AI integration uses a **dual-path architecture**:

1. **External Agents (MCP)**: Developers use existing AI tools (Claude Code, Cursor, etc.) which connect to NodeSpace via MCP
2. **Native Agent (Embedded)**: Built-in AI assistant using Ministral 3 8B with local inference for non-technical users

### Key Design Decisions

- **No ACP/protocol overhead for native agent**: Direct Rust integration with llama.cpp
- **Inference at C++ level**: All LLM inference happens in llama.cpp regardless of wrapper language
- **Ministral 3 8B**: Native function calling, no prompt engineering needed
- **External agents use MCP**: Standard protocol, no custom adapters

## Documentation

| Document | Description |
|----------|-------------|
| [AI Integration Overview](../ai/ai-integration-overview.md) | Main architecture document |
| [AIChatNode Specification](../ai/ai-chat-node-specification.md) | Chat session node type |
| [Chat UI Implementation Guide](../ai/chat-ui-implementation-guide.md) | Frontend components |
| [Node Reference System in Chat](../ai/node-reference-system-in-chat.md) | @ mentions and links |

## Archived Documentation

The previous AI architecture documentation (custom agents, ACP adapters, workflow automation) has been moved to `/docs/architecture/archived/ai-agents/`.

See the [archived README](../archived/ai-agents/README.md) for details on why these documents were superseded.
