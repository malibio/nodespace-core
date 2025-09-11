# ADR-006: Core Logic Migration from Rust to TypeScript

## Status
Accepted (2025-01-21)

## Context

Epic #69 Phase 1.3 (Issue #75) requires migrating core hierarchy and node operations from Rust backend to TypeScript frontend services. This migration is part of a broader architectural shift toward a simplified desktop application with embedded database.

### Current State
- **Rust Backend**: Core logic, hierarchy management, and node operations in Rust services
- **Complex Architecture**: Separate backend/frontend coordination with HTTP API boundaries
- **Heavy Caching**: Multi-tier caching system designed for network-distributed architecture
- **Development Overhead**: Dual-language development with complex build coordination

### Requirements
- Maintain O(1) hierarchy query performance
- Support >95% test coverage requirement
- Enable parallel development of Issues #73 (BacklinkService) and #74 (Decorations)
- Preserve all existing functionality while simplifying architecture
- Desktop-first optimization with embedded LanceDB

## Decision

### 1. Universal Node Schema
**Decision**: Use a single unified table structure for all node types instead of separate tables per type.

**Rationale**:
- **Flexibility**: Easy to add new node types without schema changes
- **Performance**: Single table queries with no complex joins
- **Simplicity**: One service handles all node operations
- **Evolution**: Supports future requirements without breaking changes

**Schema Structure**:
```typescript
interface NodeSpaceNode {
  id: string;                             // Unique identifier
  type: string;                           // Node type ("text", "task", etc.)
  content: string;                        // Primary content
  parent_id: string | null;               // Hierarchy parent
  root_id: string;                        // Root node reference
  before_sibling_id: string | null;       // Single-pointer sibling ordering
  created_at: string;                     // Creation timestamp
  mentions: string[];                     // Referenced node IDs (backlink system)
  metadata: Record<string, unknown>;      // Type-specific JSON properties
  embedding_vector: Float32Array | null;  // AI/ML embeddings
}
```

### 2. Service Extension Pattern
**Decision**: Extend existing NodeManager service rather than replacing it entirely.

**Rationale**:
- **Preserve Investment**: Leverage existing NodeManager functionality and patterns
- **Reduced Risk**: Incremental enhancement rather than complete rewrite
- **Compatibility**: Maintain existing API contracts and event patterns
- **Development Speed**: Build on proven foundation rather than starting from scratch

**Implementation Pattern**:
```typescript
export class EnhancedNodeManager extends NodeManager {
  private hierarchyService: HierarchyService;
  private nodeOperationsService: NodeOperationsService;
  
  constructor(events: NodeManagerEvents) {
    super(events);
    this.hierarchyService = new HierarchyService(this);
    this.nodeOperationsService = new NodeOperationsService(this);
  }
}
```

### 3. Simplified Caching Strategy
**Decision**: Use minimal in-memory caching focused on computed values only.

**Rationale**:
- **Desktop Context**: Embedded LanceDB eliminates network latency concerns
- **Reduced Complexity**: Fewer cache consistency issues and maintenance overhead
- **Performance**: LanceDB queries are fast enough without data duplication
- **Memory Efficiency**: Don't cache data that database already manages efficiently

**Caching Approach**:
```typescript
interface SimplifiedCache {
  // Cache expensive computations only, not raw data
  depthCache: Map<string, number>;           // Node hierarchy depths
  childrenCache: Map<string, string[]>;      // Direct children IDs
  siblingOrderCache: Map<string, string[]>;  // Computed sibling chains
}
```

### 4. Single-Pointer Sibling Navigation
**Decision**: Use `before_sibling_id` single-pointer approach for sibling ordering.

**Rationale**:
- **Database Compatibility**: Matches existing schema structure shown in user's database design
- **Simplicity**: Single field manages ordering without complex dual-pointer maintenance
- **Performance**: O(n) sibling reconstruction acceptable for typical sibling group sizes
- **Future Ready**: Can upgrade to dual-pointer system later without breaking changes

### 5. Mentions Array as Backlink System
**Decision**: Use existing `mentions` array field as the foundation for backlink functionality (Issue #73).

**Rationale**:
- **Schema Efficiency**: Leverages existing database field rather than adding new tables
- **Bidirectional Relationships**: Maintains forward and reverse reference capability
- **Parallel Development**: Enables Issue #73 mock implementation using this field
- **Performance**: Array indexing in LanceDB provides efficient backlink queries

### 6. EventBus Integration for UI Reactivity
**Decision**: Integrate with existing EventBus for UI coordination, not cache invalidation.

**Rationale**:
- **Desktop Context**: Cache invalidation less critical with embedded database
- **UI Reactivity**: EventBus provides excellent patterns for reactive updates
- **Existing Patterns**: Leverage proven EventBus coordination already in codebase
- **Simplified Logic**: Focus on UI updates rather than complex cache management

### 7. Bulk Root Hierarchy Loading Pattern
**Decision**: Implement `getAllNodesForRoot(rootId)` as the primary method used throughout the application.

**Rationale**:
- **Performance**: Single database query vs. multiple queries per hierarchy level
- **Consistency**: All data fetched in single transaction, eliminates race conditions
- **Desktop Optimization**: Optimal for embedded database with no network latency
- **Caching Efficiency**: Entire hierarchy cached in memory for O(1) navigation
- **Client-Side Reconstruction**: Leverage `parent_id` and `before_sibling_id` for tree building

**Implementation Pattern**:
```typescript
async getAllNodesForRoot(rootId: string): Promise<NodeSpaceNode[]> {
  // Single optimized query: WHERE root_id = rootId
  const allNodes = await this.database.query({
    filter: `root_id = '${rootId}'`
  });
  
  // Client reconstructs hierarchy from parent_id and before_sibling_id
  return allNodes;
}
```

## Consequences

### Positive Outcomes
- **Simplified Architecture**: Single language (TypeScript) for UI and business logic
- **Faster Development**: No backend/frontend coordination overhead
- **Desktop Optimization**: Architecture tailored for embedded database performance
- **Parallel Development**: Clean interfaces enable Issue #73/#74 parallel work
- **Reduced Complexity**: Fewer cache consistency issues and deployment concerns
- **Better Testing**: Unified testing approach with single language stack

### Potential Challenges
- **Performance Monitoring**: Need to validate TypeScript performance meets O(1) requirements
- **Migration Complexity**: Careful migration to avoid breaking existing functionality
- **Memory Management**: Monitor JavaScript garbage collection with large hierarchies
- **Error Handling**: Adapt Rust error patterns to TypeScript Result types

### Risk Mitigation
- **Performance Benchmarking**: Implement comprehensive performance testing from start
- **Incremental Migration**: Extend existing services rather than complete replacement
- **Mock Dependencies**: Use well-defined interfaces for parallel development
- **Comprehensive Testing**: Maintain >95% test coverage throughout migration

## Implementation Guidelines

### Service Architecture
```typescript
// Core service composition with bulk loading pattern
HierarchyService {
  // PRIMARY: Bulk hierarchy loading (used throughout application)
  getAllNodesForRoot(rootId): Single database query for entire hierarchy
  buildHierarchyFromNodes(nodes): Client-side tree reconstruction
  
  // Secondary operations (used less frequently)
  getNodeDepth(nodeId): O(1) cached operation  
  getSiblings(nodeId): O(n) sibling chain reconstruction from cached data
}

NodeOperationsService {
  upsertNode(node): Update mentions bidirectionally, invalidate hierarchy cache
  updateMentions(nodeId, mentions): Maintain referential integrity
  extractContent(content): Smart parsing for references
}
```

### Testing Strategy
- **Unit Tests**: Individual service method validation
- **Integration Tests**: Service coordination and EventBus integration
- **Performance Tests**: O(1) operation validation with 1,000-10,000 node datasets
- **Mock Testing**: Parallel development with Issues #73/#74 interfaces

### Migration Path
1. **Phase 1**: Create service interfaces and basic implementations
2. **Phase 2**: Implement hierarchy computation with caching
3. **Phase 3**: Add node operations with mention management
4. **Phase 4**: Integrate with existing EventBus and UI components
5. **Phase 5**: Performance optimization and comprehensive testing

## Alternative Considered

### Alternative 1: Keep Rust Backend
**Rejected**: Maintaining separate backend increases deployment complexity and doesn't leverage desktop application benefits.

### Alternative 2: Separate Tables Per Node Type
**Rejected**: Creates query complexity and schema evolution challenges. Universal schema provides better flexibility.

### Alternative 3: Complex Multi-Tier Caching
**Rejected**: Unnecessary complexity for embedded database context. Simple in-memory caching sufficient for desktop performance.

### Alternative 4: Complete NodeManager Replacement
**Rejected**: Higher risk and development overhead. Extension pattern preserves existing investment while adding capabilities.

## References
- Epic #69: 3-Phase Architecture Migration
- Issue #73: BacklinkService Implementation  
- Issue #74: Decorations System
- Existing NodeManager, EventBus, and CacheCoordinator implementations
- Universal node schema design from database planning

---

**Review Date**: 2025-07-21 (6 months)
**Reviewers**: Architecture Team, Issue #75 Implementation Engineer