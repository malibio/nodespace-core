# Aspirational Components Archive

This directory contains component specification documents that describe **planned features** not yet implemented. These are detailed designs for future capabilities.

---

## Why These Are Archived

**Current Reality**: Based on recent GitHub issues (#462-469, #447-450), active development focuses on:
- SurrealDB migration (NodeStore abstraction)
- Custom entity/schema system (schema-driven, not EntityNode)
- Placeholder persistence improvements

The documents below describe **more complex systems** that are **not currently being implemented**.

---

## Archived Component Specifications

### 1. Advanced Node Types
**Files**:
- `entity-management.md` - EntityNode with calculated fields, validation rules
- `validation-system.md` - Natural language validation engine
- `workflow-canvas-system.md` - Visual workflow editor (Google Opal-inspired)

**Status**: Documented as "📋 Planned" in IMPLEMENTATION_STATUS.md
**Why Archived**: These describe complex future features beyond current roadmap.

**Current Alternative**: Issue #449 describes simpler "schema-driven custom types" using existing node system, not separate EntityNode class.

---

### 2. NLP/AI Roadmaps
**Files**:
- `nlp-implementation-roadmap.md` - 3-4 month MVP timeline for function calling
- `nlp-strategy-roadmap.md` - Detailed NLP strategy
- `ai-integration.md` - AI integration architecture
- `ai-native-hybrid-approach.md` - Hybrid AI strategy

**Status**: Aspirational - describes LLM inference, function calling, complex AI workflows
**Why Archived**: These describe capabilities far beyond current embeddings-only implementation.

**Current Reality**:
- ✅ Implemented: Candle + ONNX for 384-dimensional embeddings only
- ❌ Not implemented: LLM inference, function calling, intent classification, workflow generation

---

### 3. Advanced AI Components
**Files**:
- `bert-implementation-guide.md` - BERT integration
- `confidence-ambiguity-system.md` - AI confidence scoring
- `hybrid-intent-classification.md` - Two-stage intent classification

**Status**: Describes sophisticated AI systems not in current roadmap
**Why Archived**: Beyond scope of current development focus.

**Current Reality**: Basic embedding generation only, no intent classification or confidence scoring.

---

## What IS Being Built

**Active work** (GitHub issues #447-450, #462-469):

1. **Schema System** (#449, #448, #447)
   - Schema-driven custom node types
   - Natural language schema creation via MCP
   - Hot-reload plugin registration
   - **Key difference**: Uses existing node system, not separate EntityNode

2. **SurrealDB Migration** (#462-469)
   - NodeStore trait abstraction
   - TursoStore and SurrealDBStore implementations
   - A/B testing framework
   - Gradual rollout system

3. **Placeholder Persistence** (#450)
   - Explicit persistence state tracking
   - Better placeholder node handling

---

## If You Need These Features

If you're planning to implement any of these aspirational components:

1. **Check GitHub issues first** - May already be tracked differently
2. **Review IMPLEMENTATION_STATUS.md** - Understand current state
3. **Consider simpler approaches** - Issue #449 shows schema-driven approach vs complex EntityNode
4. **Update status notices** - If starting work, move back to active docs with clear "🚧 In Progress" status

---

**Archive Date**: January 21, 2025
**Reason**: Describe future features far beyond current development focus, creating confusion about what's actually implemented or actively being built.
