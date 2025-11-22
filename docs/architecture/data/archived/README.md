# Archived Data Architecture Documents

These documents describe previous or alternative architectural approaches that are no longer current.

> **Note (Issue #614, 2025-11-22)**: References to `before_sibling_id` / `beforeSiblingId` in
> archived documents reflect historical architecture. Current architecture uses fractional
> `order` field on `has_child` edges for sibling ordering. See PR #616 for migration details.

## Archived Documents

### storage-architecture-turso.md
**Archived**: 2025-11-20
**Reason**: Described Turso/libSQL with Pure JSON schema and single universal `nodes` table. NodeSpace now uses SurrealDB with hub-and-spoke architecture using bidirectional Record Links.

**Historical Context**: Early exploration of local-first storage options before deciding on SurrealDB.

**Key differences from current architecture:**
- Used Turso (libSQL) instead of SurrealDB
- Single `nodes` table with JSON `properties` field for all type-specific data
- `parent_id` and `before_sibling_id` fields on nodes (not graph edges)
- No spoke tables - everything in one universal table

## Current Architecture

See these documents for the current target architecture:

### Primary Documentation
- **`../surrealdb-schema-design.md`** - Hub-and-spoke with bidirectional Record Links
  - Hub `node` table with `data` Record Link to spokes
  - Spoke tables (`task`, `date`, `schema`) with `node` reverse link
  - Graph relations (`has_child`, `mentions`) for relationships

- **`../surrealdb-only-architecture.md`** - SurrealDB as single backend
  - Direct access, zero abstraction overhead
  - Embedded-only (desktop application)

- **`../../development/hierarchy-reactivity-architecture-review.md`** - LIVE SELECT migration plan
  - Complete data-structure separation
  - Fractional ordering on edges
  - Dual reactive stores (data + structure)

### Supporting Documentation
- **`../optimistic-concurrency-control.md`** - OCC for concurrent edits
- **`../collaboration-strategy.md`** - Future multi-user collaboration
- **`../sync-protocol.md`** - Future sync capabilities

## Architecture Evolution

1. **Phase 1** (Archived): Turso with Pure JSON schema
2. **Phase 2** (Current Target): SurrealDB with hub-and-spoke Record Links
3. **Phase 3** (Future): Add LIVE SELECT reactivity + fractional ordering

## Migration from Archived Architecture

If you're looking at the archived docs for historical context, note that the current architecture differs significantly:

**Turso → SurrealDB:**
- Single table → Hub-and-spoke with spoke tables
- JSON properties → Typed spoke fields with indexes
- parent_id fields → has_child graph edges
- beforeSiblingId linked-list → Fractional ordering on edges

See issues #550-#562 for implementation details of the migration.
