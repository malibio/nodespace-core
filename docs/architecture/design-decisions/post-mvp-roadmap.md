# Post-MVP Roadmap

## Overview

NodeSpace's architecture is solid. This roadmap outlines planned enhancements that build incrementally on the existing foundation.

**Current Strengths:**
- Trait-based plugin architecture with service injection
- Three-path AI integration (MCP for external agents, ACP for cloud providers, Native with Ministral 3)
- Schema-driven entity system with calculated fields
- SurrealDB with embedded RocksDB backend
- Desktop-first approach with Tauri

**Philosophy:**
- Evolutionary improvements, not architectural rewrites
- Generic systems that support any user-defined workflow
- Local-first: privacy, offline capability, no vendor lock-in

## Priority Tiers

### Tier 1: Core Value Proposition

These features define what makes NodeSpace unique: a local-first replacement for Jira, Linear, Confluence, and development workflow tools.

#### 1.1 Workflow Automation System

**Gap:** No event-driven automation for workflow progression.

**Goals:**
- Event-driven triggers based on property changes
- Automated actions (create nodes, update properties, assign)
- Phase-based workflow progression
- Generic engine with no methodology-specific code

**Key Design:**
```rust
pub struct WorkflowEngine {
    trigger_evaluator: TriggerEvaluator,
    action_executor: ActionExecutor,
}

impl WorkflowEngine {
    pub async fn on_node_changed(&self, event: NodeChangeEvent) -> Result<()> {
        let triggers = self.trigger_evaluator
            .find_matching_triggers(&event).await?;

        for trigger in triggers {
            self.action_executor
                .execute_actions(&trigger.actions, &event).await?;
        }
        Ok(())
    }
}
```

**Features:**
- **Workflow-as-Data**: Workflows stored as nodes, not code
- **Trigger Types**: property_change, node_created, node_deleted, condition_met, schedule
- **Action Types**: create_node, set_property, assign, add_to_collection
- **AI Agent Delegation**: Tasks can be delegated to AI with context assembly

**Validation Criteria:** The engine must support spec-driven, kanban, sprint-based, and custom workflows purely through user configuration - no special-case code for any methodology.

**Metrics:**
- <50ms trigger evaluation latency
- Support for 100+ concurrent workflow instances
- Zero missed trigger events

See [Workflow Automation System](../components/workflow-automation-system.md) for detailed specification.

#### 1.2 Collections System

**Gap:** No logical organization for root nodes beyond semantic search.

**Goals:**
- Hierarchical organization without breaking root node concept
- Multi-membership (nodes can belong to multiple collections)
- Team-friendly shared organizational structures

**Key Design:**
```rust
// Collections use member_of edges, not parent-child
// Nodes remain "root nodes" for querying purposes
pub async fn add_to_collection(&self, node_id: &str, collection_id: &str) -> Result<()> {
    self.edge_store.create_edge(Edge {
        from: node_id.to_string(),
        to: collection_id.to_string(),
        edge_type: "member_of".to_string(),
        properties: json!({ "added_at": Utc::now() }),
    }).await
}
```

**Features:**
- **Hierarchical Collections**: Nested like `hr/policy/vacation/berlin`
- **Multi-membership**: Nodes can exist in multiple collections
- **Root Preservation**: member_of edges don't affect root node status

**Metrics:**
- <10ms collection membership queries
- Support for 10,000+ nodes per collection
- Hierarchies up to 10 levels deep

See [Collections System](../components/collections-system.md) for detailed specification.

#### 1.3 Query Service Enhancements

**Gap:** `query_nodes` doesn't expose full NodeFilter capabilities via MCP.

**Goals:**
- `is_root: true` filter for root node discovery
- Property filters for any field value
- Sort options (modified_desc, created_desc, alpha, type)
- Collection membership filters

**Features:**
- Expose existing NodeFilter capabilities through MCP
- Add collection-aware queries
- Support for complex property filters (JSONPath)
- Cursor-based pagination

**Metrics:**
- All NodeFilter capabilities accessible via MCP
- <100ms for queries across 100,000+ nodes

### Tier 2: Production Readiness

These enhancements improve reliability and maintainability.

#### 2.1 Node Version History

**Gap:** No version tracking for node changes.

**Goals:**
- Track all changes to node content and properties
- View history and restore previous versions
- Diff between versions

**Design:**
```rust
// Each node change creates a version entry
pub struct NodeVersion {
    version: u32,
    content: String,
    properties: JsonValue,
    modified_at: DateTime<Utc>,
    modified_by: Option<String>,
}
```

**Features:**
- Linear version history (no branching)
- Restore to any previous version
- Diff view between versions
- Audit trail for compliance

**Metrics:**
- <10ms version lookup
- Configurable retention (default: unlimited)

#### 2.2 Resilience & Error Recovery

**Gap:** Limited error recovery mechanisms.

**Goals:**
- Automatic recovery from component failures
- Data integrity validation
- Graceful degradation when AI is unavailable

**Features:**
- **AI Model Recovery**: Automatic model reloading on failure
- **Data Integrity Checks**: Startup validation of relationships
- **Graceful Degradation**: System works without AI (manual workflows only)

**Metrics:**
- <30 second recovery from AI model failures
- Zero data corruption incidents

#### 2.3 Observability

**Gap:** Minimal telemetry for debugging.

**Goals:**
- Performance metrics for key operations
- Request tracing for debugging
- Health status indicators

**Features:**
- Operation timing metrics
- Error rate tracking
- AI inference monitoring (latency, token usage)
- Optional structured logging

**Metrics:**
- <100ms metric collection overhead
- Complete request tracing for debugging

### Tier 3: Advanced Features

These provide differentiation and advanced use cases.

#### 3.1 Data Migration System

**Gap:** No strategy for schema evolution.

**Goals:**
- Zero-downtime schema migrations
- Automated migration from schema changes
- Rollback capability

**Features:**
- Automatic migration generation from schema diffs
- Migration testing before apply
- Rollback within configurable window

#### 3.2 Multi-Model AI Routing

**Gap:** Single model selection per session.

**Goals:**
- Route queries to optimal model based on task type
- Support specialized models (coding, analysis, writing)
- Learn which models perform best

**Features:**
- Task classification for model routing
- Performance tracking per model/task combination
- User override for model selection

#### 3.3 Advanced Security

**Gap:** Basic security model.

**Goals:**
- Data encryption at rest
- Fine-grained access control (for team use)
- Audit trails for sensitive operations

**Features:**
- User-controlled encryption keys
- Permission system for shared collections
- Comprehensive audit logging

#### 3.4 Team Collaboration

**Gap:** Single-user only, no shared workspaces.

**Goals:**
- Shared NodeSpace instances for teams
- Real-time collaboration on documents
- Sync between team members with offline support

**Key Features:**
- **Dual-Mode Design**: Solo mode (local-first) with optional live collaboration mode
- **CRDT-Based Sync**: Yjs for conflict-free real-time editing
- **Presence Awareness**: See collaborator cursors, selections, and avatars
- **Graceful Degradation**: Full offline capability, sync when reconnected

**Metrics:**
- <500ms sync latency for changes
- Conflict-free merging for concurrent edits
- Full offline capability with eventual consistency

See [Collaboration Strategy](../data/collaboration-strategy.md) and [Sync Protocol](../data/sync-protocol.md) for detailed specifications.

#### 3.5 Codebase Semantic Search

**Gap:** No integration with code repositories.

**Goals:**
- Index local git repositories with tree-sitter parsing
- Semantic search across code using natural language
- Connect code symbols to specs, plans, and tasks

**Key Features:**
- **Tree-sitter Integration**: Parse code into CodeRepository → CodeFile → CodeSymbol nodes
- **Humanized Embeddings**: Transform code into natural language for better semantic matching
- **MCP Tools**: `search_code`, `get_symbol`, `get_file_symbols` for AI agent access
- **File Watching**: Automatic re-indexing on file changes

**Value Proposition:**
| Approach | Query | Result |
|----------|-------|--------|
| grep | `grep "auth"` | 847 matches, mostly noise |
| Semantic | "where do we handle authentication?" | Top 3 relevant functions |

**Metrics:**
- <30 seconds for small repo indexing (~500 files)
- <200ms semantic search across codebases
- Support for 10,000+ files per workspace

See [Semantic Code Search](../features/semantic-code-search.md) for detailed specification including tree-sitter queries, node schemas, and MCP tool definitions.

## Implementation Priority

**Phase 1: Core Value (Current Focus)**
1. Workflow Automation System
2. Collections System
3. Query Service Enhancements

**Phase 2: Production Readiness**
4. Node Version History
5. Resilience & Error Recovery
6. Observability

**Phase 3: Advanced**
7. Data Migration System
8. Multi-Model AI Routing
9. Advanced Security
10. Team Collaboration
11. Codebase Semantic Search

## Success Metrics

### Core Functionality
- **Workflow Flexibility**: Any user-defined workflow works without code changes
- **Organization**: Users can browse/discover documents effectively
- **Query Power**: All filtering/sorting needs met via MCP

### Reliability
- **Data Integrity**: Zero corruption incidents
- **Availability**: System usable even when AI is unavailable
- **Recovery**: Quick recovery from component failures

### Performance
- **Query Speed**: <100ms for typical queries
- **Trigger Latency**: <50ms for workflow triggers
- **Collection Queries**: <10ms for membership lookups

### Collaboration (Future)
- **Sync Latency**: <500ms for team sync
- **Conflict Resolution**: Zero data loss from concurrent edits
- **Code Search**: <200ms semantic search across codebases

## Non-Goals (Explicitly Out of Scope)

- **Methodology-specific code**: No Spec Kit, BMAD, OpenSpec specific implementations
- **Cloud-hosted sync**: We provide sync protocol, not hosted infrastructure
- **Plugin marketplace**: Focus on core before ecosystem

---

**Next Steps**: Focus on Tier 1 features - these define NodeSpace's unique value proposition.
