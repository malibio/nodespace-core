# ADR-015: Data Layer Architecture - Local-First with Sync vs. Cloud-First Reactive

## Status
**Accepted** - January 2025

## Context

NodeSpace requires a data architecture that supports:
1. **AI-native features** - Vector embeddings, semantic search, multimodal content
2. **Local-first user experience** - Instant operations, offline capability
3. **Future collaboration** - Multi-user editing, real-time synchronization
4. **Desktop performance** - Leveraging local resources for speed and privacy

We evaluated two primary architectural approaches:
1. **Convex-style cloud-first reactive system** with real-time subscriptions
2. **Local-first with sync protocol** using embedded databases

### Convex Analysis

After analyzing Convex's architecture (`/Users/malibio/Zed Projects/convex-backend`), we identified their core patterns:
- **Subscription-based reactivity**: Clients subscribe to query results, automatically receive updates
- **Read set tracking**: Track which documents were accessed during queries
- **Write log architecture**: Changes tracked with timestamps for efficient propagation  
- **Optimistic Concurrency Control**: Transactions validate read sets at commit time

**Convex Strengths for Reactive Systems:**
- Automatic UI updates when data changes
- Consistent state across all clients
- Real-time collaboration built-in
- Sophisticated conflict resolution

**Convex Limitations for NodeSpace:**
- **No vector support**: Not designed for AI/ML workloads with embeddings
- **Cloud-dependent**: Requires network for all operations
- **JavaScript-only queries**: Cannot leverage Rust performance
- **Complex for single-user scenarios**: Over-engineered for individual knowledge work

## Decision

We chose **Local-First with Sync Protocol** architecture for the following reasons:

### Primary Storage: LanceDB Embedded
- **Vector-native**: Built specifically for AI/ML with native embedding storage and similarity search
- **Desktop-optimized**: Embedded database with no external dependencies
- **Columnar performance**: SIMD-optimized queries for analytical workloads
- **Unified schema**: Handles structured data and vectors in single queries

### Reactive Layer: Svelte Stores
- **UI reactivity**: Built-in Svelte reactive stores for automatic component updates
- **Simple and effective**: No complex subscription management needed
- **Local operations**: All updates are instant local operations

### Future Collaboration: Sync Protocol
- **Database synchronization**: Each user maintains complete local database
- **Conflict resolution**: Eventually consistent with automatic and manual resolution
- **Progressive enhancement**: Add collaboration without breaking local-first functionality

## Architecture Overview

### Current (Phase 1): Single User Local-First
```
NodeSpace Desktop App
├── Svelte Frontend (reactive stores)
├── TypeScript Services (business logic)  
├── LanceDB (embedded storage + vectors)
└── Local AI Models (mistral.rs)
```

### Future (Phase 2): Multi-User with Sync
```
User A: Local LanceDB ↔ Sync Protocol ↔ Cloud (PostgreSQL + pgvector) ↔ User B: Local LanceDB
```

### Future (Phase 3): Real-Time Collaboration
```
User A: Local + CRDT Layer ↔ WebSocket ↔ CRDT Layer + Local: User B
```

## Rationale

### Why Local-First?

1. **Knowledge work patterns**: Most operations are individual reflection and writing
2. **Privacy requirements**: Users want control over sensitive personal data
3. **Performance requirements**: AI operations benefit from local data and processing
4. **Offline necessity**: Essential for mobile knowledge workers
5. **Desktop-native advantages**: Leverage platform strengths vs. web limitations

### Why Not Pure Convex-Style Reactivity?

1. **Over-engineering for single user**: Complex reactive infrastructure not needed for individual work
2. **Vector storage gap**: Convex has no native vector support, critical for NodeSpace
3. **Network dependency**: Every operation requires cloud connectivity
4. **Cost and complexity**: Subscription management, real-time infrastructure overhead

### Why Sync Instead of Shared Database?

1. **Better offline support**: Complete functionality without network
2. **Faster local operations**: No network latency for any operation
3. **Simpler conflict model**: Eventually consistent vs. real-time consistency
4. **Scales from 1 to many users**: Same architecture for individual and collaborative use

## Implementation Strategy

### Phase 1: Local-First Foundation (Current)
```typescript
// Simple, effective local operations
class NodeManager {
    nodes = writable<Map<NodeId, Node>>(new Map());
    
    async updateNode(node: Node) {
        // 1. Update LanceDB (instant, local)
        await this.lanceDB.upsert(node);
        
        // 2. Update Svelte store (automatic UI updates)
        this.nodes.update(n => {
            n.set(node.id, node);
            return n;
        });
    }
}
```

### Phase 2: Add Sync Infrastructure (Future)
```rust
// Operation pattern for future sync compatibility
#[derive(Debug, Clone)]
pub struct NodeOperation {
    pub operation_type: OperationType,
    pub node_id: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub data: Node,
}

impl NodeStorage {
    async fn apply_operation(&self, op: &NodeOperation) -> Result<()> {
        // Apply to local LanceDB
        self.lance_db.upsert(&op.data).await?;
        
        // Queue for sync when enabled
        if let Some(sync) = &self.sync_adapter {
            sync.queue_operation(op).await?;
        }
        
        Ok(())
    }
}
```

### Phase 3: Real-Time Collaboration (Future)
```typescript
// Hybrid: Local-first + Real-time when collaborating
class HybridEditor {
    async enterCollaborativeMode(nodeId: string) {
        // Initialize CRDT layer on top of local storage
        this.ydoc = new Y.Doc();
        this.ytext = this.ydoc.getText('content');
        
        // WebSocket for real-time coordination
        this.provider = new WebsocketProvider(
            'wss://collab-server.com',
            `node-${nodeId}`,
            this.ydoc
        );
        
        // Local LanceDB remains source of truth
        this.bindCollaborativeLayer();
    }
}
```

## Technology Stack Decisions

### Local Storage: LanceDB
- **Chosen over**: SQLite + separate vector DB, Qdrant embedded, custom storage
- **Reasons**: Native vectors, columnar performance, unified queries, desktop-optimized

### Cloud Sync Storage: PostgreSQL + pgvector  
- **Chosen over**: Using LanceDB in cloud, NoSQL alternatives
- **Reasons**: Multi-tenant proven, native vectors (pgvector), ACID transactions, tooling

### Real-Time Infrastructure: Supabase Realtime + Yjs
- **Chosen over**: Custom WebSocket server, Socket.io, other CRDT libraries
- **Reasons**: Proven collaboration stack, managed infrastructure, integrated auth

### UI Reactivity: Svelte Stores
- **Chosen over**: Complex subscription systems, Redux-like state management
- **Reasons**: Built-in reactivity, performance, simplicity for local-first operations

## Consequences

### Positive Consequences

1. **Immediate Performance**: All operations are local-first and instant
2. **AI-Optimized Stack**: LanceDB perfect for vector operations and semantic search
3. **Offline Capability**: Complete functionality without network dependency
4. **Progressive Enhancement**: Can add collaboration without breaking core functionality
5. **User Data Control**: Users own their data completely, privacy-first
6. **Cost Efficiency**: Minimal cloud costs for individual users
7. **Future-Ready**: Clean architecture supports adding sync and collaboration

### Negative Consequences

1. **Delayed Real-Time Updates**: Not immediate like pure cloud-reactive systems
2. **Conflict Resolution Complexity**: Eventually consistent requires conflict handling
3. **Storage Duplication**: Each user maintains complete database copy
4. **Sync Implementation**: Additional complexity for multi-user features
5. **Limited Cross-Device**: Requires sync implementation for device switching

### Mitigation Strategies

1. **Progressive Enhancement**: Start simple, add complexity only when needed
2. **Abstract Storage Operations**: Use traits/interfaces for future flexibility
3. **Operation-Based Design**: Design for sync from the beginning
4. **Automated Conflict Resolution**: Handle common cases automatically
5. **User Education**: Clear indication of sync status and offline capabilities

## Comparison with Alternatives

### vs. Convex-Style Cloud Reactive
| Aspect | Local-First + Sync | Cloud-First Reactive |
|--------|-------------------|---------------------|
| **Single User Performance** | ⭐⭐⭐⭐⭐ Instant | ⭐⭐⭐ Network dependent |
| **Offline Capability** | ⭐⭐⭐⭐⭐ Full functionality | ⭐ Limited/none |
| **Vector/AI Operations** | ⭐⭐⭐⭐⭐ Native support | ⭐⭐ External service needed |
| **Real-Time Collaboration** | ⭐⭐⭐ Eventually consistent | ⭐⭐⭐⭐⭐ Instant updates |
| **Privacy/Data Control** | ⭐⭐⭐⭐⭐ Full user control | ⭐⭐ Cloud dependent |
| **Development Complexity** | ⭐⭐⭐⭐ Simple to start | ⭐⭐ Complex from day 1 |
| **Operational Costs** | ⭐⭐⭐⭐⭐ Minimal | ⭐⭐⭐ Ongoing cloud costs |

### vs. Traditional Client-Server
| Aspect | Local-First + Sync | Traditional Client-Server |
|--------|-------------------|-------------------------|
| **Performance** | ⭐⭐⭐⭐⭐ No round trips | ⭐⭐⭐ Network latency |
| **Offline Support** | ⭐⭐⭐⭐⭐ Full offline | ⭐⭐ Limited caching |
| **Data Consistency** | ⭐⭐⭐⭐ Eventually consistent | ⭐⭐⭐⭐⭐ Strong consistency |
| **Scalability** | ⭐⭐⭐⭐ Distributed load | ⭐⭐⭐ Server bottlenecks |

## Success Metrics

### Phase 1 Success Indicators
- [ ] All local operations complete in <50ms
- [ ] Vector search returns results in <100ms for 100K+ nodes  
- [ ] Application startup time <3 seconds
- [ ] Zero network dependencies for core functionality
- [ ] User data remains 100% local and private

### Phase 2 Success Indicators  
- [ ] Sync operations complete in <2 seconds for typical changesets
- [ ] <5% of syncs result in manual conflict resolution
- [ ] Offline→online sync recovers gracefully in 90%+ of cases
- [ ] Multi-user collaboration maintains individual performance

### Phase 3 Success Indicators
- [ ] Real-time collaborative editing latency <200ms
- [ ] Graceful fallback from collaborative to sync mode
- [ ] User presence and cursor tracking performs smoothly
- [ ] Collaborative sessions support 5+ concurrent users

## Future Considerations

### When to Revisit This Decision

1. **Vector database ecosystem changes**: If better cloud-native vector solutions emerge
2. **Real-time collaboration becomes critical**: If users demand instant collaboration
3. **AI processing shifts to cloud**: If local AI models become insufficient
4. **Regulatory requirements**: If data residency laws require cloud storage

### Potential Evolution Paths

1. **Hybrid cloud processing**: Keep data local, optionally process in cloud
2. **Distributed storage**: P2P sync without centralized cloud
3. **Edge computing integration**: Combine local-first with edge AI services
4. **Advanced CRDT adoption**: More sophisticated real-time conflict resolution

## References

- [Convex Architecture Analysis](../data/storage-architecture.md#comparison-with-convex)
- [Collaboration Strategy](../data/collaboration-strategy.md)  
- [Sync Protocol Specification](../data/sync-protocol.md)
- [Local-First Software Principles](https://www.inkandswitch.com/local-first/)
- [LanceDB Documentation](https://lancedb.github.io/lancedb/)
- [Yjs CRDT Library](https://docs.yjs.dev/)

---

**Decision Made By**: Development Team  
**Date**: January 2025  
**Review Date**: July 2025 (or when Phase 2 implementation begins)