# ADR 023: Eliminate Ephemeral Nodes During Editing

**Date:** 2025-11-13
**Status:** Accepted and Implemented
**Deciders:** Development Team
**Issues:** #479 (Phase 1), #480 (Phase 2)
**PR:** #483

## Context

The NodeSpace persistence system maintained ephemeral (non-persisted) nodes during editing, causing:
- UNIQUE constraint violations when indenting blank nodes
- Complex deferred update queue (~700 lines) for handling ephemeral references
- Race conditions and timing bugs
- 5+ layers of state tracking trying to determine "does this node exist in database?"

The system allowed blank nodes created by user actions (Enter key, Tab key) to remain ephemeral until content was added, requiring complex dependency tracking and deferred persistence logic.

## Decision

**Eliminate ephemeral-during-editing behavior.** All nodes created by user actions now persist immediately (with 500ms debounce for content changes).

### What Changed

**Before:**
```
Empty viewer → ONE placeholder (ephemeral)
User presses Enter → creates ephemeral node
User adds content → transitions to pending → persisted
Complex deferred update queue handles ephemeral references
```

**After:**
```
Empty viewer → ONE viewer-local placeholder (never enters SharedNodeStore)
User presses Enter → creates real node, persists immediately
User adds content → simple UPDATE operation
No deferred updates needed
```

### Key Principle

**Only ONE ephemeral node exists:** The viewer-local placeholder shown in BaseNodeViewer when a parent has no children. This placeholder never enters SharedNodeStore and is purely a UI concern.

All other nodes—even blank nodes created during editing—are real nodes that persist to the database.

## Consequences

### Positive

- ✅ **Eliminated entire class of bugs:** No more UNIQUE constraint violations from indent operations
- ✅ **Simpler codebase:** Removed ~107 lines of `isPlaceholder` checking logic from persistence layer
- ✅ **Clearer mental model:** "All real nodes persist immediately" is easier to understand than ephemeral state transitions
- ✅ **Better testability:** No complex ephemeral lifecycle to test
- ✅ **Foundation for future work:** Removed need for deferred update queue (Phase 2)

### Negative

- ⚠️ **More database writes:** Blank nodes now persist to database (mitigated by 500ms debounce)
- ⚠️ **Backend accepts blank nodes:** Changed backend validation to allow empty text nodes (intentional)

### Neutral

- 📝 **No UX change:** Users see identical behavior; change is purely internal architecture

## Implementation

### Files Modified (6 files)

1. **Backend Validation** (`packages/core/src/behaviors/mod.rs`)
   - Removed validation that rejected blank text nodes
   - Backend now accepts blank content (frontend controls persistence)

2. **Viewer Logic** (`packages/desktop-app/src/lib/design/components/base-node-viewer.svelte`)
   - Removed placeholder checks from structural watcher
   - Viewer-local placeholder never enters SharedNodeStore
   - All structural changes (indent/outdent) persist immediately

3. **Node Service** (`packages/desktop-app/src/lib/services/reactive-node-service.svelte.ts`)
   - Added `isInitialPlaceholder` flag to distinguish viewer placeholder from real nodes
   - Only viewer placeholder skips persistence

4. **Persistence Logic** (`packages/desktop-app/src/lib/services/shared-node-store.ts`)
   - Removed all `isPlaceholderNode()` checks from persistence decisions
   - Removed FOREIGN KEY validation placeholder checks
   - Removed content-only update skip for placeholders
   - Removed content field stripping for header nodes
   - Removed ancestor chain placeholder skip
   - Removed batch commit placeholder check

5. **Utility** (`packages/desktop-app/src/lib/utils/placeholder-detection.ts`)
   - Updated documentation: function now used for UI styling only, NOT persistence decisions

6. **Backend Service** (`packages/core/src/services/node_service.rs`)
   - Minor cleanup (removed debug logging)

### Test Results

- ✅ All 707 frontend tests passing
- ✅ All 504 backend tests passing (3 previously failing tests fixed)
- ✅ Zero UNIQUE constraint violations
- ✅ Zero FOREIGN KEY violations

## Alternatives Considered

### Alternative 1: Fix persistence state tracking in-place

Keep ephemeral nodes but add better state tracking (e.g., `everPersisted` field).

**Rejected because:**
- Adds more complexity instead of removing it
- Band-aid on architectural problem, not root cause fix
- Would still require deferred update queue

### Alternative 2: Use feature flag for gradual rollout

Add feature flag to test new behavior alongside old.

**Not needed because:**
- Changes are internal (no UX impact)
- Tests provide sufficient validation
- Can revert via git if needed

## Related Decisions

- **ADR 019:** Quote Block Prefix Persistence - Established pattern of persisting syntax
- **ADR 021:** MCP Single Process Architecture - Simplified state management aligns with this decision

## References

- **Issue #479:** Phase 1: Eliminate ephemeral nodes during editing
- **Issue #480:** Phase 2: Remove deferred update queue (completed as part of Phase 1)
- **PR #483:** Implementation and code review
- **Commit:** `0cdb9e5` - Squash merge to main
- **Architecture Doc:** `docs/architecture/persistence-layer.md`
