# NodeSpace Data Storage Architecture

## Executive Summary

NodeSpace employs a **local-first data architecture** with embedded Turso (libSQL) for high-performance, offline-capable knowledge management. This design prioritizes user experience through instant local operations while supporting future collaborative features via database synchronization rather than cloud-first reactivity.

## Current Architecture (Local-First + Pure JSON Schema)

### Core Storage: Turso Embedded (libSQL)

NodeSpace uses [Turso](https://turso.tech/) embedded mode as its primary database for several key advantages:

#### Why Turso?
- **SQLite Compatibility**: Battle-tested foundation with modern enhancements
- **Embedded Mode**: Runs in-process with no network overhead (0.1-5ms queries)
- **JSON Operations**: Native JSON field support with JSON path indexing capabilities
- **Vector Search**: F32_BLOB support for AI embeddings with similarity search
- **Sync Ready**: Built-in embedded replicas for future cloud collaboration
- **Desktop-Optimized**: No ALTER TABLE migrations required - critical for 10,000+ user machines
- **Local-First**: Complete offline functionality with optional sync

#### Pure JSON Schema Architecture

NodeSpace uses a **Pure JSON schema** with a single universal nodes table - no complementary tables:

```sql
-- Universal nodes table (ALL entities stored here)
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,                    -- "task", "invoice", "schema", "date", etc.
    content TEXT NOT NULL,                      -- Primary content/text
    parent_id TEXT,                             -- Where node was created (creation context)
    root_id TEXT,                               -- Root document for bulk fetch (NULL = is root)
    before_sibling_id TEXT,                     -- Single-pointer sibling ordering
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    properties JSON NOT NULL DEFAULT '{}',      -- ALL entity-specific fields (includes auto-detected references)
    embedding_vector BLOB,                      -- F32_BLOB vector for AI similarity search

    FOREIGN KEY (parent_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (root_id) REFERENCES nodes(id)
);

-- Separate mentions table for backlink queries
CREATE TABLE node_mentions (
    node_id TEXT NOT NULL,
    mentions_node_id TEXT NOT NULL,
    PRIMARY KEY (node_id, mentions_node_id),
    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (mentions_node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Core indexes
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_nodes_parent ON nodes(parent_id);
CREATE INDEX idx_nodes_root ON nodes(root_id);
CREATE INDEX idx_nodes_modified ON nodes(modified_at);
CREATE INDEX idx_mentions_source ON node_mentions(node_id);
CREATE INDEX idx_mentions_target ON node_mentions(mentions_node_id);
```

#### Schema-as-Node Pattern

Schemas are stored as nodes with `node_type = "schema"` and `id = type_name`. Schemas include field definitions with protection levels and enum support:

```sql
-- Example: Task schema definition with protection levels
INSERT INTO nodes (
    id,                     -- "task" (schema id = type name)
    node_type,             -- "schema"
    content,               -- "Task"
    properties
) VALUES (
    'task',
    'schema',
    'Task',
    '{
        "is_core": true,
        "version": 1,
        "description": "Task tracking with status and due dates",
        "fields": [
            {
                "name": "status",
                "type": "enum",
                "protection": "core",
                "core_values": ["OPEN", "IN_PROGRESS", "DONE"],
                "user_values": [],
                "indexed": true,
                "required": true,
                "extensible": true,
                "default": "OPEN"
            },
            {
                "name": "due_date",
                "type": "date",
                "protection": "core",
                "indexed": true,
                "description": "Date node ID (YYYY-MM-DD format)"
            }
        ]
    }'
);

-- Example: Task instance
INSERT INTO nodes (
    id,                     -- UUID or custom identifier
    node_type,             -- "task" (references schema)
    content,               -- "Implement Pure JSON schema"
    parent_id,             -- "2025-01-03" (created in daily note)
    root_id,               -- "2025-01-03" (root document for bulk fetch)
    properties
) VALUES (
    'uuid-123',
    'task',
    'Implement Pure JSON schema',
    '2025-01-03',
    '2025-01-03',
    '{
        "status": "IN_PROGRESS",
        "due_date": "2025-01-10",
        "started_at": "2025-01-03"
    }'
);
```

**Protection Levels:**
- `core`: Cannot be deleted, type cannot change (UI components depend on these)
- `user`: User-added fields, fully modifiable/deletable
- `system`: Auto-managed fields, read-only (future use)

**Enum Support:**
- `core_values`: Protected enum values (cannot be removed - UI depends on these)
- `user_values`: User-extensible values (can be added/removed via SchemaService)
- Validation ensures only allowed values are used

**Schema Lookup Convention:**
- Schema nodes: `WHERE id = <type_name> AND node_type = 'schema'`
- Instance lookup: `WHERE node_type = <type_name>`

#### Storage Benefits

**Pure JSON Architecture Advantages:**
- **Zero Migration Risk**: No ALTER TABLE on user machines - catastrophic failure prevention
- **Infinite Flexibility**: Add any field to any entity without schema changes
- **Parent-Based Linking**: parent_id = creation context, root_id = bulk fetch optimization
- **Auto-Reference Detection**: Non-primitive field types automatically treated as entity references
- **Rule-Based Indexing**: Dynamic JSON path indexes based on query patterns (not LLM-driven)
- **Desktop-First**: Optimized for 0-100K nodes locally, premium tier for 100K+ with cloud sync

**Performance Characteristics:**
- **Indexed JSON fields**: 10-50ms (JSON path indexes)
- **Unindexed JSON fields**: 50-200ms (acceptable for rare queries)
- **Vector similarity**: 10-50ms (F32_BLOB with indexes)
- **Hierarchy queries**: 5-30ms (parent_id/root_id indexes)
- **Schema evolution**: No migrations - just index creation/removal

**Hierarchy Semantics:**
- **parent_id**: "Where was this node created?" (creation context, not ownership)
- **root_id**: "What document does this belong to?" (for bulk fetch, NULL = is root node/page)
- **node_mentions**: Separate table for bidirectional reference queries (backlinks)

## Rule-Based Index Management

### Dynamic JSON Path Indexing

NodeSpace uses rule-based index management to optimize query performance without ALTER TABLE migrations:

```rust
pub struct IndexManager {
    db: Arc<libsql::Database>,
    index_registry: HashMap<String, IndexDefinition>,
}

impl IndexManager {
    // Rule: Create JSON path index when query frequency exceeds threshold
    pub async fn analyze_and_optimize(&self, query_stats: &QueryStatistics) -> Result<()> {
        for (field_path, stats) in query_stats.json_field_access.iter() {
            if stats.queries_per_day > 10 && !self.has_index(field_path) {
                self.create_json_path_index(field_path).await?;
            }
        }
        Ok(())
    }

    async fn create_json_path_index(&self, json_path: &str) -> Result<()> {
        // Example: "task.priority" -> CREATE INDEX idx_task_priority ON nodes((properties->>'$.priority')) WHERE node_type = 'task';
        let index_name = format!("idx_{}", json_path.replace(".", "_"));
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON nodes((properties->>'$.{}')) WHERE node_type = '{}'",
            index_name,
            json_path.split('.').last().unwrap(),
            json_path.split('.').next().unwrap()
        );
        self.db.execute(&sql, ()).await?;
        Ok(())
    }
}
```

**Index Creation Rules:**
1. **Frequency-based**: Create index if field queried 10+ times/day
2. **Type-specific**: JSON path indexes scoped to node_type (WHERE clause)
3. **Composite indexes**: For common multi-field queries
4. **Automatic removal**: Drop indexes unused for 30+ days

**Performance Impact:**
- **Before index**: 200-1000ms (table scan with JSON extraction)
- **After index**: 10-50ms (JSON path index lookup)
- **Index creation**: <1 second (non-blocking background operation)

### Auto-Reference Detection Pattern

```rust
const PRIMITIVE_TYPES: &[&str] = &["text", "number", "boolean", "date", "json"];

impl SchemaService {
    pub async fn detect_field_type(&self, field_type: &str) -> Result<FieldType> {
        if PRIMITIVE_TYPES.contains(&field_type) {
            Ok(FieldType::Primitive(field_type.to_string()))
        } else {
            // Check if schema exists
            if self.schema_exists(field_type).await? {
                Ok(FieldType::Reference {
                    schema_name: field_type.to_string()
                })
            } else {
                Err(anyhow!("Unknown type: {}", field_type))
            }
        }
    }
}

// Example usage:
// {"name": "customer", "type": "person"} -> Auto-detected as reference to person schema
// {"name": "priority", "type": "text"} -> Primitive text field
```

### Frontend Reactivity: Svelte Stores

For UI reactivity, NodeSpace leverages Svelte's built-in reactive stores:

```typescript
// Simple, effective UI reactivity
class NodeManager {
    nodes = writable<Map<NodeId, Node>>(new Map());

    async updateNode(node: Node) {
        // 1. Update Turso (local, fast)
        await this.turso.execute(
            "UPDATE nodes SET properties = ?, modified_at = ? WHERE id = ?",
            [JSON.stringify(node.properties), new Date().toISOString(), node.id]
        );

        // 2. Update Svelte store (automatic UI updates)
        this.nodes.update(n => {
            n.set(node.id, node);
            return n;
        });

        // 3. Queue for future sync (background)
        if (this.syncEnabled) {
            this.syncQueue.push({ operation: 'upsert', node });
        }
    }
}
```

## Future Architecture (Local-First + Collaboration)

### Cloud Storage Stack

When collaborative features are implemented, NodeSpace will use a **sync-based approach** rather than migrating to cloud-first storage:

#### PostgreSQL + pgvector
```sql
-- Cloud sync storage (not primary storage)
CREATE TABLE sync_log (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    node_id UUID NOT NULL,
    operation TEXT NOT NULL,           -- 'create', 'update', 'delete'
    node_data JSONB NOT NULL,          -- Full node content
    embedding vector(384),             -- Vector embeddings
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    version INTEGER NOT NULL,
    parent_version INTEGER,            -- For conflict resolution
    resolved BOOLEAN DEFAULT FALSE,
    
    INDEX idx_user_timestamp (user_id, timestamp),
    INDEX idx_node_versions (node_id, version),
    INDEX embedding_idx USING ivfflat (embedding vector_cosine_ops)
);
```

#### Object Storage (S3-compatible)
- **Large Files**: PDFs, images, documents
- **Model Weights**: Custom AI models
- **Backup Snapshots**: Full database exports
- **Archive Data**: Historical versions

#### Redis/Valkey Cache Layer
- **Session Management**: User authentication state
- **Real-Time Presence**: Who's editing what
- **Pub/Sub**: Collaboration event coordination
- **Rate Limiting**: API protection

### Sync Protocol Architecture

```
┌─────────────────┐    Embedded Replicas    ┌─────────────────┐
│   User A        │◄──────────────────────►│   User B        │
│   Turso Local   │                        │   Turso Local   │
│   (Embedded)    │                        │   (Embedded)    │
└─────────────────┘                        └─────────────────┘
         ▲                                           ▲
         │           ┌─────────────────────┐         │
         └──────────►│  Turso Cloud        │◄────────┘
                     │  (Sync Coordination)│
                     │  Embedded Replicas  │
                     └─────────────────────┘
```

**Unified Stack Benefits:**
- Same database (Turso) for free and premium tiers
- Built-in embedded replicas for sync (no custom implementation)
- Seamless transition from local to cloud sync
- Consistent query interface across tiers

## Architectural Principles

### 1. Local-First Design
- **Every operation** works offline
- **Instant feedback** - no network latency
- **Data ownership** - users control their data
- **Sync is additive** - local functionality never depends on cloud

### 2. Progressive Enhancement
```rust
// Start simple (free tier)
struct NodeSpace {
    storage: TursoEmbedded,
}

// Add sync capability without breaking existing code (premium tier)
struct NodeSpace {
    storage: TursoEmbedded,
    sync: Option<TursoReplica>,  // Optional collaborative features via embedded replicas
}
```

### 3. Database Sync, Not Shared Database
- Each user maintains **complete local database**
- Cloud coordinates **changes between databases**
- **Conflict resolution** rather than real-time consistency
- **Eventually consistent** across users

### 4. Graceful Degradation
- **Online**: Full collaboration features
- **Offline**: Complete local functionality
- **Poor Connection**: Background sync, no blocking

## Storage Performance Characteristics

### Local Operations (Current)
- **Node Creation**: <1ms (in-memory + disk write)
- **Vector Search**: <10ms for 100K+ nodes
- **Hierarchy Queries**: <5ms with proper indexing
- **Full-Text Search**: <20ms with content indexing
- **Startup Time**: <3 seconds (database init + index loading)

### Sync Operations (Future)
- **Change Detection**: <1ms (timestamp-based)
- **Sync Preparation**: <100ms for typical changesets
- **Network Transfer**: Depends on connection and changes
- **Conflict Resolution**: <50ms for simple conflicts
- **Background Sync**: Non-blocking, user-configurable intervals

## Technology Comparison

### NodeSpace (Local-First + Sync)
```
User Operation → Local Turso Embedded → Instant Result
                      ↓
                Background Sync → Turso Cloud → Other Users
```
**Benefits**: Always fast, works offline, unified database stack, embedded replicas

### Convex/Supabase (Cloud-First + Reactive)
```
User Operation → Network → Cloud DB → Real-time Subscriptions → All Users
```
**Benefits**: Real-time consistency, automatic caching

### Why Local-First for NodeSpace?
1. **Knowledge work is personal** - most operations are individual
2. **Privacy-sensitive** - users want local data control
3. **AI processing** - local models benefit from local data
4. **Desktop-native** - leverages platform strengths
5. **Offline requirements** - essential for mobile knowledge workers

## Implementation Patterns

### Future-Proof Design
```typescript
// Operation pattern for future sync compatibility
interface NodeOperation {
    type: 'create' | 'update' | 'delete';
    nodeId: string;
    timestamp: Date;
    userId: string;
    data?: Node;
    metadata?: Record<string, unknown>;
}

class OperationLog {
    async applyOperation(op: NodeOperation): Promise<void> {
        // Apply to local LanceDB
        await this.storage.applyOperation(op);
        
        // Queue for sync when available
        if (this.syncEnabled) {
            await this.syncQueue.enqueue(op);
        }
    }
}
```

### Storage Abstraction
```rust
#[async_trait]
pub trait NodeStorage {
    async fn get_node(&self, id: &NodeId) -> Result<Option<Node>>;
    async fn upsert_node(&self, node: &Node) -> Result<()>;
    async fn search_similar(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;
    async fn query_hierarchy(&self, parent_id: &NodeId) -> Result<Vec<Node>>;
}

// Current implementation (free tier)
impl NodeStorage for TursoEmbedded { /* ... */ }

// Future: premium tier with sync
impl NodeStorage for TursoWithReplica { /* embedded + cloud sync */ }
```

## Migration Considerations

### Schema Evolution

NodeSpace handles schema changes through lazy migration to preserve the Pure JSON architecture. See [Schema Management Implementation Guide](../development/schema-management-implementation-guide.md) for complete details on:

- **Version tracking** on node instances (`_schema_version` field)
- **Migration registry** for schema transform functions
- **Protection level enforcement** (core/user/system fields)
- **Lazy migration strategy** - nodes upgrade on first access
- **Breaking vs. additive changes** - how to handle each type

This approach enables controlled schema evolution without ALTER TABLE operations, maintaining the desktop-safe architecture while allowing schemas to evolve over time.

### Phase 1: Current (Single User, Local)
- Turso embedded storage (Pure JSON schema)
- Svelte stores for UI reactivity
- Simple CRUD operations with JSON path indexes
- Rule-based index management
- No ALTER TABLE migrations
- Schema versioning with lazy migration support

### Phase 2: Add Sync Infrastructure (Multi-User - Premium Tier)
- Enable Turso embedded replicas
- Configure cloud sync endpoint
- Implement conflict resolution (Turso handles replication)
- Premium tier upsell at 100K+ nodes

### Phase 3: Real-Time Collaboration
- WebSocket connections for live editing
- CRDT implementation for conflict-free updates
- Presence awareness (cursors, selections)
- Collaborative session management

### Data Migration Strategy
```typescript
// Pure JSON schema eliminates migration risk
// Turso embedded replicas handle sync automatically
// No schema changes needed - just enable replication
interface TursoSyncConfig {
    enabled: boolean;
    cloudEndpoint: string;
    conflictResolution: 'local-wins' | 'cloud-wins' | 'manual';
}
```

## Security and Privacy

### Local Security
- **Data encryption at rest** (planned)
- **Local-only AI processing** - no data sent to cloud
- **User-controlled backups** - no forced cloud storage

### Sync Security
- **End-to-end encryption** for sync data
- **User authentication** via Supabase Auth
- **Data minimization** - only sync deltas, not full content
- **Right to be forgotten** - complete local deletion

## Monitoring and Observability

### Local Metrics
- **Storage size** and growth patterns
- **Query performance** and optimization opportunities
- **AI model performance** and resource usage
- **User interaction patterns** for UX optimization

### Sync Metrics (Future)
- **Sync frequency** and data volume
- **Conflict resolution** patterns
- **Network performance** and optimization
- **User collaboration** patterns

## Schema Evolution

**Complete Implementation**: [Issue #106 - Schema Management System](https://github.com/malibio/nodespace-core/issues/106)

NodeSpace implements a **lazy migration strategy** for safe schema evolution across desktop installations:

### Lazy Migration Benefits

- **Desktop-Safe**: No bulk migrations on user machines (critical for 10,000+ installations)
- **Version Tracking**: Each node stores `_schema_version` in properties field
- **On-Access Migration**: Nodes auto-upgrade when read, distributing load over time
- **No Coordination**: No need for synchronized migrations across installations
- **Pure Functions**: Migration transforms are deterministic (v1→v2, v2→v3, etc.)

### Protection Levels

- **`core` fields**: Cannot be deleted, type cannot change (UI components depend on these)
- **`user` fields**: User-added fields, fully modifiable/deletable
- **`system` fields**: Auto-managed fields, read-only (future use)

### Enum Extensibility

- **`core_values`**: Protected enum values (cannot be removed - UI depends on these)
- **`user_values`**: User-extensible values (can be added/removed via SchemaService)
- **Validation**: Ensures only allowed values are used

See [Issue #106](https://github.com/malibio/nodespace-core/issues/106) for complete 7-phase implementation including SchemaService, MigrationRegistry, MCP tools, and frontend TypeScript wrappers.

---

## Conclusion

NodeSpace's local-first architecture with Turso embedded and Pure JSON schema provides the optimal foundation for AI-native knowledge management:

1. **Immediate Performance**: All operations are local-first and instant (0.1-5ms queries)
2. **Zero Migration Risk**: Pure JSON schema eliminates ALTER TABLE on user machines
3. **AI-Optimized**: F32_BLOB vector storage with JSON path indexes for semantic search
4. **Offline Capable**: Complete functionality without network dependency
5. **Future-Ready**: Turso embedded replicas enable seamless sync (free → premium tier)
6. **Rule-Based Intelligence**: Dynamic index management based on query patterns (not LLM)
7. **User-Centric**: Users own and control their data completely
8. **Safe Evolution**: Lazy migration strategy enables schema changes without desktop coordination

This approach avoids the catastrophic risk of desktop migrations while providing a unified database stack from free tier (0-100K nodes) to premium tier (100K+ with cloud sync). The result is a fast, reliable, and privacy-respecting knowledge management system that scales from individual use to team collaboration.