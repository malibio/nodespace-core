# Documentation Archive

This directory contains documentation that has been archived for the following reasons:

## Archive Categories

### 1. AI Agents (`ai-agents/`)
**Status**: Aspirational - not implemented
**Archived**: 2025-01-21
**Reason**: These documents describe comprehensive AI agent systems, workflow automation engines, and creator-economy features that are not currently implemented and not on the immediate roadmap.

**Current Reality**: NodeSpace uses Candle + ONNX for **embedding generation only** (384-dimensional vectors with BAAI/bge-small-en-v1.5 model). No LLM inference, no workflow engines, no AI agents.

**Files Archived**:
- adapter-management-strategy.md
- agentic-architecture-overview.md
- creator-business-intelligence.md
- creator-knowledge-management.md
- creator-onboarding-experience.md
- creator-workflows.md
- hybrid-llm-agent-architecture.md
- implementation-guide.md
- local-ai-implementation.md
- natural-language-workflow-engine.md
- personal-knowledge-agents.md
- training-data-evolution.md

**Reference**: See [`/docs/IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md#ai-integration) for actual AI implementation status.

---

### 2. Plugin Architecture (`plugins/`)
**Status**: Aspirational - not implemented
**Archived**: 2025-01-21
**Reason**: These documents describe a comprehensive build-time plugin system with service injection, Rust+Svelte integration, and external plugin development. The actual system uses hardcoded node types.

**Current Reality**: 7 core node types hardcoded in the codebase (text, task, date, code-block, quote-block, ordered-list, header). No plugin system exists.

**Files Archived**:
- development-guide.md
- external-development-guide.md
- future-requirements.md
- overview.md
- plugin-architecture.md
- unified-plugin-registry.md

**Reference**: See [`/docs/architecture/core/system-overview.md`](../architecture/core/system-overview.md#node-types) for actual node type implementation.

---

### 3. NLP Model Selection (`nlp/`)
**Status**: Aspirational - describes planned model, not current implementation
**Archived**: 2025-01-21
**Reason**: This document describes Gemma 3 4B-QAT for Text-to-SQL and workflow generation, but the actual implementation uses only Candle + ONNX for embedding generation.

**Current Reality**: BAAI/bge-small-en-v1.5 for 384-dimensional embeddings only. No LLM inference, no Text-to-SQL, no workflow generation.

**Files Archived**:
- model-selection.md

**Reference**: See [`/docs/architecture/core/technology-stack.md`](../architecture/core/technology-stack.md#ai-integration-stack) for actual AI stack.

---

### 4. Research Documents (`research/`)
**Status**: Historical research/analysis
**Archived**: 2025-01-21
**Reason**: Analysis of external systems (Logseq editor) for potential adoption. Useful historical context but not current architecture documentation.

**Files Archived**:
- logseq-editor-analysis.md - Comprehensive analysis of Logseq's textarea-based editor approach

---

### 5. Deprecated Persistence Docs (`deprecated-persistence/`)
**Status**: Deprecated - consolidated
**Archived**: 2025-01-21
**Reason**: These documents were consolidated into [`/docs/architecture/persistence-system.md`](../architecture/persistence-system.md) to eliminate confusion from overlapping documentation.

**Files Archived**:
- persistence-architecture.md
- persistence-layer.md
- dependency-based-persistence.md
- elegant-persistence-solution.md

**Reference**: See [`/docs/architecture/persistence-system.md`](../architecture/persistence-system.md) for consolidated persistence documentation.

---

## Why Archive Instead of Delete?

These documents represent valuable:
- **Design thinking** - Understanding of what was considered and why
- **Future roadmap** - Potential features that may be implemented later
- **Historical context** - Evolution of architectural decisions

Archiving preserves this knowledge while removing confusion about what is actually implemented vs planned.

---

## How to Use Archived Documentation

**If you're looking for current architecture:**
- Start with [`/docs/IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md) - Master status document
- See [`/docs/architecture/core/system-overview.md`](../architecture/core/system-overview.md) - Current architecture overview
- Check [`/docs/architecture/core/technology-stack.md`](../architecture/core/technology-stack.md) - Actual technology stack

**If you're planning future features:**
- Archived documents contain detailed designs for planned capabilities
- Review with understanding that these require significant implementation work
- Cross-reference with current implementation status before making decisions

---

### 6. Resolved Investigations (`resolved-investigations/`)
**Status**: Historical investigation/handoff documents
**Archived**: 2025-01-21
**Reason**: These documents tracked debugging and investigation processes for issues that are now resolved and closed.

**Files Archived**:
- TEST-STATE-ANALYSIS.md - Issue #409 analysis (fixed in PR #410)
- threads-4-investigation-handoff.md - Issue #411 handoff (resolved with threads=2)
- test-suite-concurrency-investigation.md - Issue #398 investigation (fixed in PR #408)

**Reference**: These were temporary analysis/handoff documents that served their purpose. Issues are closed and fixes are merged.

---

## Archive Date
**January 21, 2025**

## Archived By
Claude Code (Documentation Organization Task)

## Restoration
If any archived documentation becomes relevant again (feature implementation begins), it can be restored to the main documentation tree with appropriate status updates.
