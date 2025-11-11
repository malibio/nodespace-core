# Future Planning & Business Strategy Archive

This directory contains future planning documents, business strategy, and aspirational roadmaps that describe **planned capabilities** not yet under active development.

---

## Archived Documents

### Business Strategy
**Status**: Business planning documents
**Archived**: 2025-01-21
**Files**:
- `monetization-strategy.md` - Creator economy pricing strategy
- `cost-analysis.md` - Infrastructure cost analysis and tiers

**Why Archived**: Business strategy documents that reference unimplemented features (mistral.rs, LanceDB, creator-focused features). Preserved for future business planning but not part of current development.

---

### Future Scaling & Infrastructure
**Status**: Future planning documents
**Archived**: 2025-01-21
**Files**:
- `scaling-strategy.md` - Plans for scaling to thousands of users
- `supabase-integration.md` - Supabase backend integration (alternative to current Turso)
- `tech-stack-roadmap.md` - Evolution from LanceDB to collaborative platform

**Why Archived**: These describe future infrastructure plans (Supabase, collaborative features, scaling strategies) that are not part of current roadmap. References outdated tech (LanceDB instead of Turso).

---

### Post-MVP Roadmap
**Status**: Aspirational enhancement plan
**Archived**: 2025-01-21
**File**: `post-mvp-roadmap.md`

**Why Archived**: Comprehensive roadmap describing enhancements beyond MVP including:
- Resilience & recovery systems
- Configuration management
- Advanced AI features (mistral.rs)
- Enterprise capabilities

Status notice already indicated "Many described features are not yet implemented" but keeping it active was confusing.

---

### Decision Records
**Status**: Future planning ADR
**Archived**: 2025-01-21
**File**: `future-tech-stack.md`

**Why Archived**: Architecture decision record about future technology choices, not current implementation.

---

## Current Actual Work

**Active development** (see GitHub issues):
- SurrealDB migration (Epic #461, #467) - NodeStore abstraction layer
- Custom entity/schema system (#449, #448, #447)
- Placeholder persistence improvements (#450)

**Current implemented tech**:
- ✅ Turso (libsql) - Working database layer
- ✅ Candle + ONNX - Working embedding generation
- ✅ Svelte 5 + Tauri 2.0 - Working frontend
- 🚧 SurrealDB - Migration in progress (NodeStore trait)

---

## Reference

For current implementation status, see:
- `/docs/IMPLEMENTATION_STATUS.md` - Master status document
- GitHub issues - Active development tracking
- `/docs/architecture/data/surrealdb-migration-roadmap.md` - Active migration work

---

**Archive Date**: January 21, 2025
**Reason**: Eliminate confusion between current implementation and future planning
