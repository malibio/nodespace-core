# Data Storage Architecture

## Overview

NodeSpace uses a **universal node schema** with embedded LanceDB for simplified desktop architecture. This design provides flexibility, performance, and maintainability while supporting the hierarchical knowledge management system.

## Universal Node Schema

### Core Design Decision

All node types use a **single unified table structure** rather than separate tables per type. This approach provides:

- **Flexibility**: Easy to add new node types without schema changes
- **Performance**: Single table queries with no complex joins
- **Simplicity**: One service handles all node operations
- **Evolution**: Supports future requirements without breaking changes

### Schema Definition

```typescript
interface NodeSpaceNode {
  id: string;                             // Unique node identifier
  type: string;                           // Node type ("text", "task", "entity", etc.)
  content: string;                        // Primary content/text
  parent_id: string | null;               // Hierarchy parent (null for root nodes)
  root_id: string;                        // Root node of the hierarchy
  before_sibling_id: string | null;       // Single-pointer sibling ordering
  created_at: string;                     // ISO 8601 timestamp
  mentions: string[];                     // Array of referenced node IDs
  metadata: Record<string, unknown>;      // Type-specific JSON properties
  embedding_vector: Float32Array | null;  // AI/ML embeddings (computed)
}
```

### Field Descriptions

#### Core Identity Fields
- **`id`**: Unique identifier for the node
- **`type`**: Determines node behavior and rendering ("text", "task", "entity", "query", "ai-chat", etc.)
- **`content`**: Primary textual content of the node
- **`created_at`**: Creation timestamp in ISO 8601 format

#### Hierarchy Management
- **`parent_id`**: Direct parent in hierarchy (null for root nodes)
- **`root_id`**: Reference to the root node of the entire hierarchy
- **`before_sibling_id`**: Single-pointer approach for sibling ordering

#### Relationship System
- **`mentions`**: Array of node IDs that this node references or mentions
  - **This IS the backlink system** - provides bidirectional node relationships
  - Updates to mentions arrays maintain referential integrity
  - Used for @-references, links, and cross-node dependencies

#### Type-Specific Storage
- **`metadata`**: JSON field containing type-specific properties
  - TaskNode: `{ status, due_date, priority, assignee }`
  - EntityNode: `{ entity_type, stored_fields, calculated_fields }`
  - QueryNode: `{ query_definition, auto_refresh, refresh_triggers }`

#### AI Integration
- **`embedding_vector`**: Float32Array of embeddings for semantic search
  - Computed by AI service during node processing
  - Used for similarity search and content discovery

## Hierarchy Navigation System

### Bulk Root Hierarchy Loading (Primary Pattern)

NodeSpace uses a **bulk loading strategy** where entire hierarchies are fetched in a single query, then client-side reconstruction builds the tree structure:

```typescript
class HierarchyService {
  // Primary method used throughout the application
  async getAllNodesForRoot(rootId: string): Promise<NodeSpaceNode[]> {
    // Single query to fetch entire hierarchy
    const allNodes = await this.database.query({
      filter: `root_id = '${rootId}'`
    });
    
    return allNodes; // Client reconstructs hierarchy from parent_id and before_sibling_id
  }
  
  // Client-side hierarchy reconstruction
  buildHierarchyFromNodes(nodes: NodeSpaceNode[]): HierarchyTree {
    const nodeMap = new Map(nodes.map(n => [n.id, n]));
    const rootNode = nodes.find(n => n.parent_id === null);
    
    return this.buildTreeRecursively(rootNode, nodeMap);
  }
  
  private buildTreeRecursively(
    node: NodeSpaceNode, 
    nodeMap: Map<string, NodeSpaceNode>
  ): HierarchyNode {
    // Find direct children
    const children = Array.from(nodeMap.values())
      .filter(n => n.parent_id === node.id);
    
    // Order siblings using before_sibling_id chain
    const orderedChildren = this.orderSiblings(children);
    
    return {
      ...node,
      children: orderedChildren.map(child => 
        this.buildTreeRecursively(child, nodeMap)
      )
    };
  }
}
```

### Bulk Loading Benefits
- **Performance**: Single database query vs. multiple queries per level
- **Consistency**: All data fetched in single transaction, no race conditions
- **Caching**: Entire hierarchy cached in memory for O(1) navigation
- **Offline Support**: Complete hierarchy available for disconnected operation
- **Bandwidth**: Optimal for desktop apps with local database

### Single-Pointer Sibling Ordering

NodeSpace uses a **single-pointer approach** with `before_sibling_id` for sibling navigation:

```typescript
// Sibling ordering chain example:
// Node A (before_sibling_id: null) -> First sibling
// Node B (before_sibling_id: A.id) -> Second sibling  
// Node C (before_sibling_id: B.id) -> Third sibling

class HierarchyService {
  async getSiblings(nodeId: string): Promise<string[]> {
    const node = await this.getNode(nodeId);
    if (!node.parent_id) return [nodeId]; // Root node
    
    // Build complete sibling chain from before_sibling_id pointers
    return await this.buildSiblingChain(node.parent_id);
  }
  
  private async buildSiblingChain(parentId: string): Promise<string[]> {
    const children = await this.getNodesByParent(parentId);
    
    // Build ordered chain following before_sibling_id pointers
    const siblingOrder: string[] = [];
    const nodeMap = new Map(children.map(c => [c.id, c]));
    
    // Find first sibling (before_sibling_id is null)
    const firstSibling = children.find(c => c.before_sibling_id === null);
    if (!firstSibling) return [];
    
    // Follow the chain
    let currentNode = firstSibling;
    while (currentNode) {
      siblingOrder.push(currentNode.id);
      currentNode = children.find(c => c.before_sibling_id === currentNode!.id);
    }
    
    return siblingOrder;
  }
}
```

### Hierarchy Benefits
- **Performance**: O(n) sibling reconstruction where n = number of siblings
- **Simplicity**: Single field manages ordering
- **Integrity**: Clear parent-child relationships
- **Future-Ready**: Can upgrade to dual-pointer system later

## Backlink System Integration

### Mentions Array as Backlinks

The `mentions` array serves as NodeSpace's **bidirectional relationship system**:

```typescript
class NodeOperationsService {
  async updateNodeMentions(nodeId: string, newMentions: string[]) {
    const oldNode = await this.getNode(nodeId);
    const oldMentions = oldNode.mentions || [];
    
    // Update the node's mentions array
    await this.updateNode({...oldNode, mentions: newMentions});
    
    // Maintain bidirectional consistency
    await this.updateBacklinkReferences(nodeId, oldMentions, newMentions);
  }
  
  private async updateBacklinkReferences(
    sourceId: string, 
    oldMentions: string[], 
    newMentions: string[]
  ) {
    // Remove old backlink references
    for (const removedId of oldMentions.filter(id => !newMentions.includes(id))) {
      await this.removeBacklinkReference(removedId, sourceId);
    }
    
    // Add new backlink references  
    for (const addedId of newMentions.filter(id => !oldMentions.includes(id))) {
      await this.addBacklinkReference(addedId, sourceId);
    }
  }
  
  async getNodesReferencingNode(targetId: string): Promise<NodeSpaceNode[]> {
    // Find all nodes that mention the target node
    return await this.database.query({
      filter: `array_contains(mentions, '${targetId}')`
    });
  }
}
```

### Backlink Use Cases
- **@-references**: `@person-001` in content updates mentions array
- **Link relationships**: Cross-document references and citations
- **Dependency tracking**: Task dependencies and project relationships
- **Semantic connections**: AI-discovered content relationships

## Type-Specific Metadata Patterns

### TaskNode Metadata
```typescript
const taskMetadata = {
  status: "in_progress" | "completed" | "pending" | "cancelled",
  due_date: "2025-02-01T10:00:00Z",
  priority: "high" | "medium" | "low" | "critical",
  assignee: "user-001",
  estimated_hours: 8.5,
  actual_hours: 6.2,
  tags: ["urgent", "feature", "backend"]
}
```

### EntityNode Metadata
```typescript
const entityMetadata = {
  entity_type: "employee",
  schema_version: 2,
  stored_fields: {
    first_name: "John",
    last_name: "Doe", 
    email: "john@company.com",
    salary: 95000,
    start_date: "2024-01-15T00:00:00Z"
  },
  calculated_fields: {
    full_name: "John Doe",                    // first_name + " " + last_name
    total_compensation: 110000,               // salary + bonus
    years_employed: 1.2                       // DAYS(start_date, TODAY()) / 365.25
  }
}
```

### QueryNode Metadata
```typescript
const queryMetadata = {
  query_definition: {
    name: "Overdue Tasks",
    entity_types: ["task"],
    filters: [
      { field: "due_date", operator: "<", value: "NOW()" },
      { field: "status", operator: "!=", value: "completed" }
    ],
    sort_by: "due_date",
    limit: 100
  },
  auto_refresh: true,
  refresh_interval: 300000,  // 5 minutes
  last_executed: "2025-01-21T10:30:00Z",
  result_count: 23
}
```

## Database Integration Patterns

### LanceDB Operations

```typescript
interface DatabaseAdapter {
  // PRIMARY: Bulk hierarchy loading (most important method)
  async getAllNodesForRoot(rootId: string): Promise<NodeSpaceNode[]>;
  
  // Core node operations
  async getNode(id: string): Promise<NodeSpaceNode | null>;
  async upsertNode(node: NodeSpaceNode): Promise<void>;
  async deleteNode(id: string): Promise<void>;
  
  // Hierarchy queries (used less frequently due to bulk loading)
  async getNodesByParent(parentId: string): Promise<NodeSpaceNode[]>;
  async getRootNodes(): Promise<NodeSpaceNode[]>;
  
  // Relationship queries
  async getNodesByMention(targetId: string): Promise<NodeSpaceNode[]>;
  async getNodesOfType(type: string): Promise<NodeSpaceNode[]>;
  
  // Search operations
  async searchByContent(query: string): Promise<NodeSpaceNode[]>;
  async similaritySearch(embedding: Float32Array): Promise<NodeSpaceNode[]>;
  
  // Batch operations
  async upsertNodes(nodes: NodeSpaceNode[]): Promise<void>;
  async bulkUpdateMentions(updates: MentionUpdate[]): Promise<void>;
}
```

### Query Optimization Strategies

```typescript
// Index strategies optimized for bulk loading pattern
const indexConfiguration = {
  // Primary access patterns
  primary_key: ["id"],
  
  // CRITICAL: Bulk hierarchy loading (most important index)
  root_hierarchy: ["root_id", "parent_id", "before_sibling_id"],
  
  // Secondary hierarchy navigation (used less frequently)
  parent_index: ["parent_id", "before_sibling_id"],
  
  // Type-based queries
  type_index: ["type", "created_at"],
  
  // Relationship queries
  mentions_index: ["mentions"],  // Array index for backlink queries
  
  // Search optimization
  content_fts: ["content"],      // Full-text search
  embedding_vector: ["embedding_vector"],  // Vector similarity
  
  // Composite indexes for complex queries
  root_type: ["root_id", "type"],  // Filter by type within root hierarchy
  mentions_root: ["mentions", "root_id"]  // Cross-hierarchy backlinks
}

// Typical usage patterns with bulk loading
class HierarchyService {
  private hierarchyCache = new Map<string, NodeSpaceNode[]>();
  
  async loadHierarchy(rootId: string): Promise<HierarchyTree> {
    // Check cache first
    if (this.hierarchyCache.has(rootId)) {
      const cachedNodes = this.hierarchyCache.get(rootId)!;
      return this.buildHierarchyFromNodes(cachedNodes);
    }
    
    // Single optimized query for entire hierarchy
    const allNodes = await this.database.getAllNodesForRoot(rootId);
    
    // Cache the flat node list
    this.hierarchyCache.set(rootId, allNodes);
    
    // Build tree structure client-side
    return this.buildHierarchyFromNodes(allNodes);
  }
  
  // Invalidate cache when hierarchy changes
  invalidateHierarchyCache(rootId: string) {
    this.hierarchyCache.delete(rootId);
  }
}
```

## Migration and Evolution Strategy

### Schema Versioning

```typescript
interface SchemaVersion {
  version: number;
  applied_at: string;
  changes: SchemaChange[];
}

interface SchemaChange {
  type: "add_field" | "remove_field" | "modify_field" | "add_index";
  field_name?: string;
  description: string;
  migration_script?: string;
}

// Example migration: Adding new field
const migration_v2: SchemaChange = {
  type: "add_field",
  field_name: "last_modified_at",
  description: "Track last modification timestamp",
  migration_script: "UPDATE nodes SET last_modified_at = created_at WHERE last_modified_at IS NULL"
}
```

### Backwards Compatibility

- **Metadata Evolution**: New properties added to metadata without breaking existing nodes
- **Field Defaults**: New schema fields have sensible defaults for existing data
- **Gradual Migration**: Background processes update nodes to new schema versions
- **Rollback Support**: Database migrations can be reversed if needed

### Future Enhancements

#### Planned Schema Extensions
1. **Dual-pointer sibling navigation**: Add `next_sibling_id` for O(1) traversal
2. **Versioning support**: Add `version` and `previous_version_id` fields
3. **Access control**: Add `permissions` and `owner_id` fields
4. **Rich metadata**: Structured metadata schemas per node type
5. **Temporal data**: Track field-level change history

#### Performance Optimizations
1. **Computed columns**: Database-level calculated fields for common queries
2. **Materialized views**: Pre-computed hierarchy and relationship views
3. **Partitioning**: Separate hot/cold data based on access patterns
4. **Caching layers**: Redis/in-memory caching for frequently accessed nodes

## Best Practices

### Data Consistency
- **Transactional updates**: Use database transactions for multi-node operations
- **Referential integrity**: Validate mentions arrays before updates
- **Cascade operations**: Handle node deletion with cleanup of references
- **Concurrency control**: Optimistic locking for concurrent node edits

### Performance Guidelines
- **Batch operations**: Group multiple updates into single transactions
- **Lazy loading**: Load node metadata on-demand rather than eagerly
- **Index usage**: Design queries to leverage existing indexes
- **Embedding management**: Cache and reuse embeddings when content unchanged

### Security Considerations
- **Input validation**: Sanitize all content before storage
- **Injection prevention**: Use parameterized queries for dynamic filters
- **Access logging**: Track all node access and modification operations
- **Data encryption**: Encrypt sensitive metadata fields at application level

---

This universal node schema provides the foundation for NodeSpace's flexible, performant, and maintainable data architecture while supporting all current and planned features for hierarchical knowledge management.