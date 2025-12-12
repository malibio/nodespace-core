# Archived AI Documentation

## Status: Superseded

The documents in this folder represent **earlier visions** of NodeSpace's AI architecture that have been superseded by the current **dual-path architecture**.

## Why Archived?

The original architectures explored:
- Building custom AI agents internally
- Local LLM inference (Gemma 3, mistral.rs) with custom prompt engineering
- Intent classification and routing within NodeSpace
- Custom RAG, content generation, and entity CRUD processors
- ACP (Agent Client Protocol) integration with external agents

The **current architecture** instead:
- **External agents (MCP)**: Developers use Claude Code, Cursor, etc. via standard MCP
- **Native agent (embedded)**: Rust + llama.cpp + Ministral 3 8B for local inference
- **No ACP**: Direct integration, no protocol overhead for native agent
- **Inference at C++ level**: llama.cpp handles all inference regardless of wrapper language

## Current Documentation

See the active AI documentation in:
- `/docs/architecture/ai/ai-integration-overview.md` - Main architecture document
- `/docs/architecture/ai/ai-chat-node-specification.md` - AIChatNode type spec
- `/docs/architecture/ai/chat-ui-implementation-guide.md` - Frontend implementation
- `/docs/architecture/ai/node-reference-system-in-chat.md` - @ mentions and node links

## Archived Documents

| Document | Original Purpose |
|----------|-----------------|
| `agentic-architecture-overview.md` | Custom workflow automation with local LLM |
| `hybrid-llm-agent-architecture.md` | Local vs cloud LLM strategy |
| `local-ai-implementation.md` | mistral.rs integration plans |
| `implementation-guide.md` | Building custom AI processors |
| `natural-language-workflow-engine.md` | NL workflow creation |
| `creator-*.md` | Creator economy specific features |
| `personal-knowledge-agents.md` | Personal AI agent concepts |
| `training-data-evolution.md` | Model training strategies |
| `adapter-management-strategy.md` | Custom adapter patterns |

## Historical Reference

These documents may still be useful for:
- Understanding NodeSpace's AI vision evolution
- Reference for potential future hybrid approaches
- Context on domain-specific requirements (creator economy, workflows)

The creator economy focus and workflow automation concepts may be revisited in the future as features built on top of the native agent integration.
