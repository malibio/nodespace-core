# ADR: Dual Persistence Architecture

**Status**: Accepted
**Date**: 2025-10-07
**Decision Makers**: Frontend Architect, Development Team
**Related**: [Persistence System](../persistence-system.md) (consolidated persistence guide)

## Context

During bug fix work for node deletion persistence, we discovered that NodeSpace uses two different patterns for data operations:

1. **Direct Persistence**: UI components use Svelte $effect watchers to directly call `databaseService` methods
2. **Event Bus Coordination**: Services emit and subscribe to events for coordination

This raised the question: Should we consolidate to a single pattern (pure Event Bus) for consistency?

### Current Implementation

**Pattern 1 - Direct Persistence** (`base-node-viewer.svelte`):
```typescript
$effect(() => {
  for (const node of visibleNodes) {
    if (contentChanged) {
      databaseService.saveNodeWithParent(node.id, data);
    }
  }
});
```

**Pattern 2 - Event Bus** (various services):
```typescript
// Emit
events.nodeDeleted(nodeId);

// Subscribe
eventBus.subscribe('node:deleted', (event) => {
  cleanupReferences(event.nodeId);
});
```

## Decision

**We will maintain the dual-pattern architecture** and NOT migrate to pure Event Bus.

The two patterns serve fundamentally different purposes:
- **Direct persistence**: For UI → Database CRUD operations
- **Event Bus**: For Service ↔ Service coordination

## Rationale

### Performance

**Direct Persistence**:
- UI → Database: ~0.1ms overhead
- Simple function call

**Event Bus Alternative**:
- UI → Event → Handler → Database: ~0.5-1ms overhead
- 5-10x slower for high-frequency operations

For a knowledge management system with constant editing, this overhead is significant.

### Simplicity

**Direct Persistence**:
```typescript
// 2 steps - easy to understand
UI change → databaseService.save()
```

**Event Bus Alternative**:
```typescript
// 4 steps - unnecessary complexity
UI change → eventBus.emit() → DatabaseSyncService → databaseService.save()
```

The direct approach is simpler and more maintainable.

### Framework Alignment

Svelte 5's `$effect` is **designed** for direct side effects:

```typescript
// This is idiomatic Svelte 5
$effect(() => {
  // Watch reactive state
  const data = $state.value;

  // Perform side effect
  saveToDatabase(data);  // Direct call is natural
});
```

Using events here fights the framework's design.

### Clear Ownership

**Direct Persistence**:
- Component owns its data
- Component responsible for persistence
- Clear, single responsibility

**Event Bus Alternative**:
- Unclear who's responsible for persistence
- DatabaseSyncService becomes god object
- Breaks component ownership model

### Debugging

**Direct Persistence**:
```
UI change → $effect → databaseService.save()
✅ Simple call stack
✅ Easy to debug
✅ Errors propagate naturally to UI
```

**Event Bus Alternative**:
```
UI change → $effect → eventBus.emit() → async handler → service → database
❌ Complex async flow
❌ Harder to trace issues
❌ Error handling complications
```

### Architectural Separation of Concerns

The dual approach **correctly** separates:

- **Persistence concerns**: Component owns saving its state (UI layer)
- **Coordination concerns**: Services coordinate via events (Service layer)

Pure Event Bus would **conflate** these concerns.

## Consequences

### Positive

✅ **Performance**: No event overhead for high-frequency saves
✅ **Simplicity**: Direct calls are easy to understand
✅ **Framework-aligned**: Leverages Svelte 5 design
✅ **Clear ownership**: Components own their persistence
✅ **Easy debugging**: Direct call stacks
✅ **Future-ready**: Event Bus available for new features (AI, sync, analytics)

### Negative

⚠️ **Pattern awareness**: Developers must understand when to use each pattern
⚠️ **Documentation debt**: Must document the dual approach clearly

### Mitigation

- ✅ Create `persistence-system.md` documentation (consolidated guide)
- ✅ Add this ADR to explain the decision
- ✅ Add code comments in key files
- ✅ Update developer onboarding guide

## Alternative Considered

### Pure Event Bus Architecture

**Proposal**: Create `DatabaseSyncService` that subscribes to all persistence events.

```typescript
class DatabaseSyncService {
  constructor(databaseService, eventBus) {
    eventBus.subscribe('node:created', this.handleCreate);
    eventBus.subscribe('node:updated', this.handleUpdate);
    eventBus.subscribe('node:deleted', this.handleDelete);
  }
}
```

**Why Rejected**:
- ❌ 5-10x performance overhead
- ❌ Unnecessary complexity
- ❌ Fights Svelte framework design
- ❌ Unclear ownership of persistence
- ❌ No meaningful benefits

The consistency gained doesn't outweigh the costs.

## Implementation

No code changes required. The existing implementation is correct.

### Required Documentation

1. ✅ **Persistence System Guide** (`docs/architecture/persistence-system.md`)
   - When to use direct persistence
   - When to use Event Bus
   - Complete service layer explanations
   - Placeholder nodes documentation
   - Code examples and testing patterns
   - *(Note: Replaces previous `persistence-architecture.md` and consolidates 4 overlapping docs)*

2. ✅ **This ADR** (`docs/architecture/decisions/dual-persistence-architecture.md`)
   - Decision rationale
   - Trade-offs considered

3. 🔲 **Code Comments** (if needed)
   - Add explanatory comments to key files
   - Explain pattern choice

4. 🔲 **Developer Guide Updates**
   - Add section on persistence patterns
   - Include in onboarding

## Future Considerations

### When to Revisit This Decision

Consider revisiting if:
- Svelte framework fundamentally changes $effect design
- Performance becomes an issue (unlikely)
- New architectural patterns emerge (e.g., CQRS, Event Sourcing)

### Evolution Path

The current architecture is ready for future features:

**Real-time Sync** (just add subscriber):
```typescript
eventBus.subscribe('node:updated', syncToCloud);
```

**AI Embeddings** (just add subscriber):
```typescript
eventBus.subscribe('node:updated', generateEmbedding);
```

**Analytics** (just add subscriber):
```typescript
eventBus.subscribe('node:created', trackAnalytics);
```

No architectural changes needed.

## Related Decisions

- [Component Architecture](../components/component-architecture-guide.md)
- [Event Bus Design](../core/event-bus.md)
- [Frontend Architecture](../frontend-architecture.md)

## References

- Original bug fix: Node deletion not persisting to database (2025-10-07)
- Frontend Architect review: Keep dual approach (2025-10-07)
- Svelte 5 reactivity documentation: https://svelte.dev/docs/svelte/$effect

## Status History

| Date | Status | Notes |
|------|--------|-------|
| 2025-10-07 | Proposed | During bug fix investigation |
| 2025-10-07 | Accepted | After frontend architect review |
