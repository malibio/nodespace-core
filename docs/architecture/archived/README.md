# Archived Architecture Documentation

This directory contains historical architecture documentation that is no longer relevant to the current NodeSpace implementation but is preserved for historical context and reference.

## Why These Documents Are Archived

**Date Archived**: 2025-11-13
**Reason**: Completed SurrealDB migration (PR #485, Issue #470)

NodeSpace completed a major architectural simplification by:
- Removing the NodeStore trait abstraction layer
- Removing the TursoStore wrapper implementation
- Removing DatabaseService
- Making SurrealDB the single, direct database backend

**Net Result**: -8,860 lines of code, 87% simplification of database layer

## Archived Documents

### Migration Documentation (Completed)

1. **node-store-abstraction.md**
   - Documents the NodeStore trait abstraction layer
   - **Status**: Abstraction removed in PR #485
   - **Reference**: Issue #470

2. **surrealdb-migration-guide.md**
   - Step-by-step migration guide from Turso to SurrealDB
   - **Status**: Migration completed in PR #485
   - **Reference**: Issues #461, #470

3. **surrealdb-migration-roadmap.md**
   - Phased migration roadmap and timeline
   - **Status**: All phases completed in PR #485
   - **Reference**: Issue #461 (Epic)

4. **turso-performance-analysis.md**
   - Performance analysis of Turso database
   - **Status**: Replaced by SurrealDB-only architecture
   - **Reference**: Issue #460 (SurrealDB PoC)

## Current Architecture Documentation

For current architecture information, see:
- `docs/architecture/data/surrealdb-schema-design.md` - SurrealDB schema and design
- `docs/architecture/core/system-overview.md` - Overall system architecture
- `docs/architecture/core/technology-stack.md` - Current technology choices

## Historical Context

These documents represent the **hybrid migration phase** (October-November 2025) when NodeSpace was transitioning from:
- **Before**: Turso (libsql) with NodeStore abstraction
- **After**: SurrealDB embedded with direct access

The migration was completed successfully with:
- ✅ 455/455 tests passing (100% pass rate)
- ✅ Zero regressions
- ✅ Massive code simplification (-87%)
- ✅ Eliminated 2 layers of indirection

### AI Agents Architecture (Superseded)

**Date Archived**: 2025-12-11
**Reason**: Transitioned to dual-path AI architecture (Native Agent + MCP)

The original AI architecture explored multiple approaches that have been superseded:

1. **Custom AI processors** - Complex internal agent architecture with intent classification
2. **ACP (Agent Client Protocol)** - External agent spawning via ACP adapters
3. **mistral.rs integration** - Custom Rust inference engine

The **current architecture** instead uses:
- **Native Agent**: Built-in Rust + llama.cpp + Ministral 3 8B for local inference
- **External Agents via MCP**: Developers use their existing tools (Claude Code, Cursor) which connect to NodeSpace via MCP
- **Direct integration**: No protocol overhead for native agent (direct Rust calls)
- **Chat history as nodes**: Conversations stored in NodeSpace (not external agent state)

**Archived Documents** (in `ai-agents/` subfolder):
- `agentic-architecture-overview.md` - Custom workflow automation
- `hybrid-llm-agent-architecture.md` - Local vs cloud LLM strategy
- `local-ai-implementation.md` - mistral.rs integration plans
- `implementation-guide.md` - Building custom AI processors
- `natural-language-workflow-engine.md` - NL workflow creation
- `creator-*.md` - Creator economy specific features
- `personal-knowledge-agents.md` - Personal AI agent concepts
- `training-data-evolution.md` - Model training strategies
- `adapter-management-strategy.md` - Custom adapter patterns

**Current AI Documentation**: See `docs/architecture/ai/`

## Related Issues

- **#470**: Remove NodeStore abstraction, use SurrealDB directly
- **#461**: Epic - SurrealDB Migration (original)
- **#460**: SurrealDB PoC (performance validation)
- **#485**: PR - Complete SurrealDB Migration
- **#486**: Documentation cleanup (this work)
