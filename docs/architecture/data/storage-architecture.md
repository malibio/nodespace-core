# NodeSpace Data Storage Architecture

## Executive Summary

NodeSpace employs a **local-first data architecture** with embedded Turso (libSQL) for high-performance, offline-capable knowledge management. This design prioritizes user experience through instant local operations while supporting future collaborative features via database synchronization rather than cloud-first reactivity.

## Current Architecture (Local-First + LLM Intelligence)

### Core Storage: Turso Embedded (libSQL)

NodeSpace uses [Turso](https://turso.tech/) embedded mode as its primary database for several key advantages:

#### Why Turso?
- **SQLite Compatibility**: Battle-tested foundation with modern enhancements
- **Embedded Mode**: Runs in-process with no network overhead (0.1-5ms queries)
- **JSON Operations**: Native JSON field support with indexing capabilities
- **Vector Search**: F32_BLOB support for AI embeddings with similarity search
- **Sync Ready**: Built-in replication for future cloud collaboration
- **LLM-Optimized**: Perfect for AI-driven schema design and optimization
- **Local-First**: Complete offline functionality with optional sync

#### Hybrid Database Schema

NodeSpace uses a **hybrid architecture** combining universal nodes with LLM-optimized entity tables:

```sql
-- Universal nodes table (foundation for all entities)
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,                     -- Entity type identifier
    content TEXT NOT NULL,                  -- Primary content/text
    parent_id TEXT REFERENCES nodes(id),    -- Hierarchy parent
    root_id TEXT REFERENCES nodes(id),      -- Root node reference
    before_sibling_id TEXT REFERENCES nodes(id), -- Single-pointer sibling ordering
    created_at TEXT NOT NULL,               -- ISO 8601 timestamp
    mentions TEXT,                          -- JSON array of referenced node IDs
    embedding_vector BLOB                   -- F32_BLOB vector for AI similarity search
);

-- LLM-generated entity tables (example: task management)
CREATE TABLE task_properties (
    node_id TEXT PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    
    -- Hot fields (LLM identified as frequently queried)
    priority TEXT CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    due_date DATE,
    status TEXT CHECK (status IN ('todo', 'in_progress', 'completed')),
    assignee_id TEXT,
    project_id TEXT,
    
    -- Cold fields (flexible JSON for rare queries)
    metadata JSON,
    
    -- LLM-recommended indexes
    INDEX(priority),
    INDEX(due_date), 
    INDEX(status),
    INDEX(assignee_id)
);

-- LLM-generated entity tables (example: recipe collection)
CREATE TABLE recipe_properties (
    node_id TEXT PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    
    -- Hot fields (LLM identified from "cooking time, dietary restrictions")
    cooking_time INTEGER,
    difficulty TEXT CHECK (difficulty IN ('easy', 'medium', 'hard')),
    cuisine TEXT,
    dietary_tags TEXT, -- Comma-separated for indexing
    
    -- Cold fields (complex data structures)
    metadata JSON, -- ingredients, instructions, nutrition_facts
    
    -- LLM-recommended indexes  
    INDEX(cooking_time),
    INDEX(cuisine),
    INDEX(dietary_tags)
);
```

#### Storage Benefits

**Hybrid Architecture Advantages:**
- **Performance Optimization**: Hot fields in normalized columns (1-10ms queries)
- **Schema Flexibility**: Cold fields in JSON for unknown structures  
- **LLM Intelligence**: AI-driven field classification and index recommendations
- **Query Efficiency**: Entity-specific tables eliminate unnecessary data scanning
- **Type Safety**: Database-level constraints and validation
- **Storage Efficiency**: No repeated JSON field names, optimal column types

**Performance Characteristics:**
- **Hot field queries**: 1-10ms (native column indexes)
- **Cold field queries**: 20-100ms (JSON operations)
- **Vector similarity**: 10-50ms (F32_BLOB with indexes)
- **Complex joins**: 5-30ms (foreign key optimization)
- **Schema evolution**: Real-time field migration based on usage patterns

## LLM-Driven Database Intelligence

### Natural Language Schema Design

NodeSpace leverages Large Language Models to automatically optimize database schemas based on natural language descriptions:

```typescript
// User describes entity in natural language
const userRequest = "Create a task tracking system for my development team with priorities, deadlines, and assignees";

// LLM analyzes and generates optimal schema
interface LLMSchemaAnalysis {
    entityType: "task";
    hotFields: {
        priority: {
            type: "TEXT",
            constraint: "CHECK (priority IN ('low', 'medium', 'high', 'urgent'))",
            indexed: true,
            reasoning: "Priority filtering is common in task management dashboards"
        },
        due_date: {
            type: "DATE", 
            indexed: true,
            reasoning: "Date range queries frequent (overdue, due this week, etc.)"
        },
        assignee_id: {
            type: "TEXT",
            indexed: true,
            reasoning: "Users will filter tasks by assignee for personal views"
        }
    },
    coldFields: {
        description: "Long text descriptions - keep in JSON metadata",
        attachments: "File references - JSON array for flexibility"
    },
    predictedQueryPatterns: [
        "Tasks due this week",
        "High priority tasks by assignee", 
        "Overdue tasks by project"
    ]
}
```

### Dynamic QueryNode Optimization

When users create QueryNodes through natural language, the LLM optimizes database indexes:

```typescript
// User creates query: "Show me all high-priority tasks due in the next week"
const queryOptimization = {
    detectedFields: ["priority", "due_date"],
    recommendedIndexes: [
        "CREATE INDEX idx_task_priority ON task_properties(priority)",
        "CREATE INDEX idx_task_due_date ON task_properties(due_date)",
        // Composite index for common combined queries
        "CREATE INDEX idx_task_priority_due ON task_properties(priority, due_date)"
    ],
    performanceImpact: {
        before: "200-500ms (table scan)",
        after: "5-15ms (indexed lookup)"
    }
}
```

### Schema Evolution Intelligence

The LLM continuously monitors usage patterns and evolves the schema:

```typescript
interface SchemaEvolutionRecommendation {
    analysis: {
        fieldUsageStats: {
            "task.project_id": { queries: 150, lastMonth: true },
            "task.legacy_field": { queries: 0, lastMonth: false }
        }
    },
    recommendations: [
        {
            action: "PROMOTE_TO_HOT",
            field: "project_id", 
            reasoning: "Now queried 150+ times/month, move from JSON to column",
            migration: "ALTER TABLE task_properties ADD COLUMN project_id TEXT"
        },
        {
            action: "DEMOTE_TO_COLD",
            field: "legacy_field",
            reasoning: "Zero usage in 3 months, move to JSON metadata" 
        }
    ]
}
```

### Performance Intelligence

The LLM provides real-time performance insights and optimizations:

```typescript
interface PerformanceOptimization {
    queryAnalysis: {
        query: "Find recipes under 30 minutes with high protein",
        currentPerformance: "180ms (JSON extraction on 10K recipes)",
        recommendation: {
            createIndex: "CREATE INDEX idx_recipe_cooking_time ON recipe_properties(cooking_time)",
            expectedImprovement: "15ms (12x faster)",
            migrationCost: "2 seconds to create index"
        }
    },
    autoOptimize: true // Automatically apply if benefit > cost
}
```

### Frontend Reactivity: Svelte Stores

For UI reactivity, NodeSpace leverages Svelte's built-in reactive stores rather than complex database subscription systems:

```typescript
// Simple, effective UI reactivity
class NodeManager {
    nodes = writable<Map<NodeId, Node>>(new Map());
    
    async updateNode(node: Node) {
        // 1. Update LanceDB (local, fast)
        await this.lanceDB.upsert(node);
        
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
┌─────────────────┐    Sync Protocol    ┌─────────────────┐
│   User A        │◄──────────────────►│   User B        │
│   LanceDB       │                    │   LanceDB       │
│   (Local)       │                    │   (Local)       │
└─────────────────┘                    └─────────────────┘
         ▲                                       ▲
         │           ┌─────────────────┐         │
         └──────────►│  Cloud Sync     │◄────────┘
                     │  PostgreSQL     │
                     │  (Coordination) │
                     └─────────────────┘
```

## Architectural Principles

### 1. Local-First Design
- **Every operation** works offline
- **Instant feedback** - no network latency
- **Data ownership** - users control their data
- **Sync is additive** - local functionality never depends on cloud

### 2. Progressive Enhancement
```rust
// Start simple
struct NodeSpace {
    storage: LanceDB,
}

// Add sync capability without breaking existing code
struct NodeSpace {
    storage: LanceDB,
    sync: Option<SyncAdapter>,  // Optional collaborative features
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
User Operation → Local LanceDB → Instant Result
                      ↓
                Background Sync → Cloud → Other Users
```
**Benefits**: Always fast, works offline, simple architecture

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

// Current implementation
impl NodeStorage for LanceDataStore { /* ... */ }

// Future: can add cloud adapter without changing interfaces
impl NodeStorage for HybridStorage { /* local + sync */ }
```

## Migration Considerations

### Phase 1: Current (Single User, Local)
- LanceDB embedded storage
- Svelte stores for UI reactivity
- Simple CRUD operations
- No over-engineering

### Phase 2: Add Sync Infrastructure (Multi-User)
- Add operation logging
- Implement sync protocol
- Cloud storage setup
- Conflict resolution

### Phase 3: Real-Time Collaboration
- WebSocket connections for live editing
- CRDT implementation for conflict-free updates
- Presence awareness (cursors, selections)
- Collaborative session management

### Data Migration Strategy
```typescript
// Existing data remains in LanceDB
// Add sync metadata without changing core schema
interface NodeWithSync extends Node {
    _sync?: {
        version: number;
        lastSyncTime: Date;
        syncStatus: 'pending' | 'synced' | 'conflict';
    };
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

## Conclusion

NodeSpace's local-first architecture with LanceDB provides the optimal foundation for AI-native knowledge management:

1. **Immediate Performance**: All operations are local-first and instant
2. **AI-Optimized**: Native vector storage perfect for embeddings and semantic search  
3. **Offline Capable**: Complete functionality without network dependency
4. **Future-Ready**: Clean architecture for adding sync and collaboration
5. **User-Centric**: Users own and control their data completely

This approach avoids the complexity of cloud-first reactive systems while maintaining the flexibility to add collaborative features when needed. The result is a fast, reliable, and privacy-respecting knowledge management system that scales from individual use to team collaboration.