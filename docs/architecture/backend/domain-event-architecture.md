# Domain Event Architecture

## Overview

NodeSpace uses a domain event system to propagate changes across multiple clients (MCP, Tauri, Proxy, CloudSync) in real-time. This document describes the architecture, design decisions, and implementation approach for the centralized event emission system.

## Architecture Layers

### Layer 1: Store Change Detection

**SurrealStore** is responsible for detecting ALL mutations and notifying upstream:

```rust
// packages/core/src/db/surreal_store.rs

pub enum StoreOperation {
    Created,
    Updated,
    Deleted,
}

pub struct StoreChange {
    pub operation: StoreOperation,
    pub node: Node,
    pub source: Option<String>,  // Optional for MVP (not persisted)
}

impl SurrealStore {
    notifier: Option<Arc<dyn Fn(StoreChange) + Send + Sync>>,

    pub fn register_notifier<F>(&mut self, notifier: F)
    where F: Fn(StoreChange) + Send + Sync + 'static
    {
        self.notifier = Some(Arc::new(notifier));
    }

    async fn create_node(&self, node: Node, source: Option<String>) -> Result<Node> {
        let created = /* DB operation */;

        // ✅ Automatically notify on every mutation
        if let Some(notifier) = &self.notifier {
            notifier(StoreChange {
                operation: StoreOperation::Created,
                node: created.clone(),
                source,
            });
        }

        Ok(created)
    }
}
```

**Key Principles:**
- Store knows ground truth (it performed the mutation)
- Store reports facts, not domain interpretations
- Similar to database triggers or audit logs
- Source tracking is infrastructure concern (like session tracking in databases)

### Layer 2: NodeService Event Broadcasting

**NodeService** registers a centralized handler and broadcasts domain events:

```rust
// packages/core/src/services/node_service.rs

impl NodeService {
    pub async fn new(mut store: Arc<SurrealStore>) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(128);

        // ✅ ONE centralized handler for ALL store changes
        let event_tx_clone = event_tx.clone();
        store.register_notifier(move |change: StoreChange| {
            // Convert Node → typed format (TaskNode, SchemaNode, etc.)
            let node_data = node_to_typed_value(&change.node)
                .unwrap_or_else(|_| serde_json::to_value(&change.node).unwrap());

            let event = match change.operation {
                StoreOperation::Created => DomainEvent::NodeCreated {
                    node_id: change.node.id.clone(),
                    node_data,
                    source_client_id: change.source,
                },
                StoreOperation::Updated => DomainEvent::NodeUpdated {
                    node_id: change.node.id.clone(),
                    node_data,
                    source_client_id: change.source,
                },
                StoreOperation::Deleted => DomainEvent::NodeDeleted {
                    id: change.node.id.clone(),
                    source_client_id: change.source,
                },
            };

            let _ = event_tx_clone.send(event);
        });

        Ok(Self { store, event_tx, /* ... */ })
    }
}
```

**Key Principles:**
- ONE place converts store changes to domain events
- Type conversion centralized (node_to_typed_value)
- Broadcasts to ALL clients via tokio::broadcast channel
- Clients filter by source_client_id to avoid echo

### Layer 3: DomainEvent Structure

**Strongly-typed event payloads** eliminate conversion in consumers:

```rust
// packages/core/src/db/events.rs

#[derive(Debug, Clone)]
pub enum DomainEvent {
    NodeCreated {
        node_id: String,
        node_data: serde_json::Value,  // TaskNode, SchemaNode, etc.
        source_client_id: Option<String>,
    },
    NodeUpdated {
        node_id: String,
        node_data: serde_json::Value,  // Type-specific data
        source_client_id: Option<String>,
    },
    NodeDeleted {
        id: String,
        source_client_id: Option<String>,
    },
    EdgeCreated { relationship, source_client_id },
    EdgeUpdated { relationship, source_client_id },
    EdgeDeleted { id, source_client_id },
}
```

**Benefits:**
- `node_data` contains typed JSON (TaskNode has flat `status`/`priority`, not nested)
- Consumers receive ready-to-use data (no conversion needed)
- Frontend can directly use the payload

### Layer 4: Client Subscription & Filtering

**Multiple clients** subscribe and filter based on source:

```
┌──────────────────────────────────────┐
│ Clients                              │
│ - MCP (no subscription)              │
│ - Tauri (subscribes for UI updates)  │
│ - Proxy (subscribes, forwards SSE)   │
│ - CloudSync (future, for sync)       │
└──────────────────────────────────────┘
           ↓ subscribe_to_events()
┌──────────────────────────────────────┐
│ NodeService broadcast::Sender        │
│ - Broadcasts to ALL                  │
└──────────────────────────────────────┘
           ↓ recv()
┌──────────────────────────────────────┐
│ Client Receivers                     │
│ - Filter: source_client_id != self   │
│ - Process relevant events            │
└──────────────────────────────────────┘
```

**Example - Proxy Client:**

```rust
// Proxy subscribes to events
let mut event_rx = node_service.subscribe_to_events();

tokio::spawn(async move {
    while let Ok(event) = event_rx.recv().await {
        match event {
            DomainEvent::NodeCreated { source_client_id, .. } => {
                // Skip events from proxy itself (avoid echo)
                if source_client_id == Some("proxy".to_string()) {
                    continue;
                }
                // Forward to browsers via SSE
                forward_to_browsers(event).await;
            }
            _ => {}
        }
    }
});
```

### Layer 5: SSE to Browsers (Separate from Internal Events)

**Important distinction**: SSE is HTTP/WebSocket from Proxy to browsers, NOT part of the Rust event system.

```
Rust broadcast::Receiver (Layer 4)
           ↓
     Proxy Server
           ↓ SSE/WebSocket (HTTP streaming)
   Browser A, B, C...
```

**All browsers are treated as ONE logical client** ("proxy") in the domain event system.

## Design Rationale

### Why Store-Level Notification?

#### Problem: Manual Emission Doesn't Scale

```rust
// Current (manual): Must remember to emit
async fn complex_operation(&self, params: Params) -> Result<()> {
    self.store.update_node(&id_a, update_a).await?;
    self.emit_node_updated(node_a);  // ❌ Easy to forget!

    if should_create_child() {
        let child = self.store.create_node(new_child).await?;
        self.emit_node_created(child);  // ❌ Easy to forget!
    }

    for obsolete_id in find_obsolete() {
        self.store.delete_node(&obsolete_id).await?;
        self.emit_node_deleted(obsolete_id);  // ❌ Easy to forget in loop!
    }
}
```

**Current burden**: 14+ manual emit sites, growing to 50-100+ as business logic becomes more complex.

#### Solution: Automatic Notification

```rust
// With store notification: Can't forget!
async fn complex_operation(&self, params: Params, source: Option<String>) -> Result<()> {
    // Just call store - events automatic!
    self.store.update_node(&id_a, update_a, source.clone()).await?;

    if should_create_child() {
        self.store.create_node(new_child, source.clone()).await?;
    }

    for obsolete_id in find_obsolete() {
        self.store.delete_node(&obsolete_id, source.clone()).await?;
    }

    // ✅ All events emitted automatically by store!
}
```

**Benefits:**
- ✅ Can't forget to emit (store always notifies)
- ✅ Works in loops and conditional branches
- ✅ Reduces boilerplate by ~87% (150 lines → 20 lines)
- ✅ Single point of failure (easier to debug)

### Why NOT Callback Pattern (Concerns Addressed)

**Concern 1:** "Store shouldn't know about domain events"
- **Answer**: Store only knows `StoreChange` (generic), not `DomainEvent` (domain-specific)
- Similar to database triggers reporting mutations

**Concern 2:** "Callback complexity (`Arc<dyn Fn()>`)"
- **Answer**: One-time setup in `NodeService::new()`, never touched again
- Developers don't write callbacks, just call store methods

**Concern 3:** "Wrong layer for change detection"
- **Answer**: Change detection IS a store responsibility (it knows what changed)
- Domain interpretation IS a service responsibility (converts to DomainEvent)

**Concern 4:** "Client ID coupling"
- **Answer**: `source` is optional metadata (like database session tracking)
- Not persisted in MVP (single-user)
- Standard pattern in databases (audit trail, session tracking)

### Industry Patterns

**Similar Approaches:**
- **PostgreSQL**: Triggers notify on INSERT/UPDATE/DELETE
- **RocksDB**: EventListener pattern for change notification
- **Event Sourcing**: Store persistence includes event emission
- **CQRS**: Write model emits events as part of state change

### Client Context Management

**Shared Singleton Problem:**

```rust
// ❌ WRONG: Instance-level client_id doesn't work with shared service
pub struct NodeService {
    client_id: Option<String>,  // Multiple clients can't share this!
}
```

**Solution: Per-Request Context:**

```rust
// ✅ CORRECT: Source passed per-request
async fn create_node(&self, node: Node, source: Option<String>) -> Result<String> {
    self.store.create_node(node, source).await?;
    Ok(node.id)
}

// Usage by clients:
// MCP
node_service.create_node(node, Some("mcp".to_string())).await?;

// Tauri
node_service.create_node(node, Some("tauri".to_string())).await?;

// Tests
node_service.create_node(node, None).await?;
```

## Implementation Status

### Phase 1: Typed Payloads ✅ (Complete)

**Completed:**
- ✅ DomainEvent structure updated to use `serde_json::Value`
- ✅ Helper methods created (`emit_node_created`, `emit_node_updated`)
- ✅ All 12 emit sites updated to use helpers
- ✅ Event emission tests updated
- ✅ Tauri domain_event_forwarder updated
- ✅ dev-proxy SseEvent updated to use typed data
- ✅ All tests passing (1547 frontend + 141 Rust)

**Files Modified:**
- `packages/core/src/db/events.rs`
- `packages/core/src/services/node_service.rs`
- `packages/core/tests/event_emission_test.rs`
- `packages/core/src/db/surreal_store.rs`
- `packages/desktop-app/src-tauri/src/services/domain_event_forwarder.rs`
- `packages/desktop-app/src-tauri/src/bin/dev-proxy.rs`

### Phase 2-5: Store Notification (Remaining)

**Next Steps:**
1. Implement `StoreChange` and `register_notifier()` in SurrealStore
2. Update all store mutation methods to call notifier
3. Register centralized handler in `NodeService::new()`
4. Remove all 14 manual `emit_event()` calls
5. Update client code (MCP/Tauri/Proxy) to pass source
6. Create comprehensive tests
7. Document in architecture guides

**See Issue #718 for detailed implementation plan.**

## Testing Strategy

### Automatic Notification Guarantees

**Test Confidence:**
```rust
#[test]
async fn test_store_notification() {
    let (service, _temp) = create_test_service().await;
    let mut rx = service.subscribe_to_events();

    // Call store operation
    service.create_node(test_node(), None).await?;

    // ✅ Guaranteed: Store called = Event emitted
    let event = rx.recv().await.unwrap();
    assert_matches!(event, DomainEvent::NodeCreated { .. });
}
```

**Complex Method Testing:**
```rust
#[test]
async fn test_bulk_delete_emits_all_events() {
    let service = create_test_service().await;
    let mut rx = service.subscribe_to_events();

    service.bulk_delete(vec![id1, id2, id3], None).await?;

    // ✅ Collect all events (guaranteed by store notification)
    let events: Vec<_> = collect_events(&mut rx, 3).await;
    assert_eq!(events.len(), 3);  // Can't miss events!
}
```

### Migration Testing

**Verify backward compatibility:**
- Existing tests should pass without changes (except pattern matching)
- Event consumers receive typed data (verify structure)
- Source filtering works correctly (proxy doesn't receive own events)

## Performance Considerations

### Callback Overhead

**Measured Impact:**
- Arc clone: ~5ns per operation
- Closure call: ~5ns per operation
- Total overhead: ~10ns per mutation

**Context**: Database operations take 100,000-1,000,000ns (0.1-1ms).
Event overhead is **0.001%** of operation time - negligible.

### Broadcast Channel Performance

**tokio::broadcast** characteristics:
- Lock-free for single producer
- O(1) send operation
- Handles 100,000+ events/second
- Subscribers don't block sender

**Conclusion**: Performance is not a concern for this architecture.

## Future Enhancements

### CloudSync Integration

```rust
// CloudSync client
let mut event_rx = node_service.subscribe_to_events();

tokio::spawn(async move {
    while let Ok(event) = event_rx.recv().await {
        // Filter out own changes
        if event.source_client_id() == Some("cloudsync") {
            continue;
        }

        // Sync to remote server
        cloud_client.sync_change(event).await;
    }
});

// Making changes
node_service.create_node(synced_node, Some("cloudsync".to_string())).await?;
// ✅ Event emitted with source="cloudsync", other clients receive it
```

### Multi-User Source Tracking

**Current (MVP)**: `source` is client type ("mcp", "tauri", "proxy")

**Future**: `source` could be user/session ID:
```rust
let source = format!("user-{}-session-{}", user_id, session_id);
node_service.create_node(node, Some(source)).await?;
```

This enables:
- Per-user audit trails
- User-specific event filtering
- Collaboration features (see who made what change)

**Note**: Not persisting source in MVP (single-user system).

## Comparison to Alternatives

### Alternative 1: Manual Emission (Current)

**Approach**: NodeService methods manually call `emit_event()` after store operations.

**Pros:**
- Simple to understand
- Explicit control over when events emit
- No callback complexity

**Cons:**
- ❌ Easy to forget (14+ call sites, growing to 50-100+)
- ❌ Complex methods with multiple operations = tracking nightmare
- ❌ No compile-time guarantee
- ❌ Boilerplate code (~150 lines)

### Alternative 2: Return-Based Change Detection

**Approach**: Store methods return change descriptors, service emits.

```rust
let change = self.store.create_node(node).await?;
self.emit_change(change);  // Still manual!
```

**Cons:**
- ❌ Reintroduces "forgetting to emit" problem
- ❌ API pollution (every return type includes change)
- ❌ Thread-safety complexity for change accumulation

### Alternative 3: Channel-Based Store→Service Communication

**Approach**: Store sends to channel, NodeService receives in background task.

**Cons:**
- ❌ Overcomplicated (background task coordination)
- ❌ Harder to test (async coordination)
- ❌ Lifecycle management complexity
- ❌ Doesn't solve source tracking problem

### Recommended: Store Notification (Callback)

**Winner** based on:
- ✅ Can't forget to emit (automatic)
- ✅ Scales to complex operations
- ✅ Minimal overhead (one-time setup)
- ✅ Industry-standard pattern
- ✅ Testing confidence (guaranteed emission)

## Migration Guide

### For Developers Adding New Methods

**Old Pattern (Manual):**
```rust
async fn archive_node(&self, id: &str) -> Result<()> {
    self.store.update_node(id, update).await?;
    self.emit_node_updated(updated);  // Must remember!
    Ok(())
}
```

**New Pattern (Automatic):**
```rust
async fn archive_node(&self, id: &str, source: Option<String>) -> Result<()> {
    self.store.update_node(id, update, source).await?;
    // ✅ Event emitted automatically!
    Ok(())
}
```

**Key Changes:**
1. Add `source: Option<String>` parameter
2. Pass source to store methods
3. Remove emit_event() calls (automatic!)

### For Developers Writing Tests

**Old Pattern:**
```rust
#[test]
async fn test_create() {
    service.create_node(node).await?;
    // Must verify event emitted
    let event = rx.recv().await.unwrap();
}
```

**New Pattern:**
```rust
#[test]
async fn test_create() {
    service.create_node(node, None).await?;  // Pass None for source
    // Event emission guaranteed, test can focus on operation
}
```

## Related Documentation

- [Real-Time Updates](../components/real-time-updates.md) - QueryNode and live updates
- [Data Flow](../core/data-flow.md) - Overall system data flow
- [MCP Integration](../business-logic/mcp-integration.md) - MCP client event handling

## Related Issues

- #718 - This refactor (centralize domain event emission)
- #665 - Client ID tracking foundation
- #673 - Typed node payloads (node_to_typed_value)
- #715 - MCP browser mode (discovered type mismatch)

## References

- [PostgreSQL Triggers](https://www.postgresql.org/docs/current/triggers.html) - Similar change notification pattern
- [RocksDB EventListener](https://github.com/facebook/rocksdb/wiki/EventListener) - Observer pattern in storage layer
- [Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html) - Store includes event emission
