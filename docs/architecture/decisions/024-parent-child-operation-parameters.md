# ADR-024: Parent-Child Relationship as Operation Parameters (Not Stored Fields)

**Status**: Accepted
**Date**: 2025-11-17
**Related Issues**: #528, #523, #514
**Context**: Graph-native hierarchy migration broke parent-child edge creation

---

## Context and Problem Statement

**Issue #514** removed `parent_id` and `container_node_id` from the Node storage model to embrace a pure graph-native hierarchy using edges. This was the correct architectural direction.

**However, PR #523** went too far by also removing these fields from the HTTP API and operation layer, which **broke parent-child edge creation entirely**. Nodes were being created but orphaned (no parent relationship established).

**The Bug (Issue #528)**: When typing in a placeholder node:
1. Placeholder promoted to real node
2. Node created in database ✅
3. **Parent-child edge NOT created** ❌ (missing parentId/containerNodeId parameters)
4. Node orphaned in graph
5. Frontend children cache not updated
6. Component re-rendered without node → textarea disappeared
7. Result: Focus loss, typing impossible

---

## Decision

**We will maintain a hybrid approach:**

1. **Storage Layer (Database)**: NO `parent_id` or `container_node_id` fields
   - Pure graph-native using edges only
   - Follows Issue #514 decision ✅

2. **Operations Layer (HTTP API, NodeOperations)**: YES to `parentId` and `containerNodeId` parameters
   - Required for edge creation during operations
   - Transient parameters, not persisted fields
   - Enables `NodeOperations::create_node()` to create parent-child edges atomically

3. **Frontend (TypeScript)**: Transient fields `_parentId` and `_containerId`
   - Prefixed with `_` to indicate non-persisted nature
   - Stripped from node before storage, sent as operation parameters
   - Enables frontend to communicate parent context without polluting data model

---

## Implementation Details

### Rust Operations Layer

```rust
// packages/core/src/operations/mod.rs
pub struct CreateNodeParams {
    pub id: String,
    pub node_type: String,
    pub content: String,
    pub parent_id: Option<String>,      // ✅ Operation parameter
    pub container_node_id: Option<String>, // ✅ Operation parameter
    // ... other fields
}

impl<S: NodeService> NodeOperations<S> {
    pub async fn create_node(&self, params: CreateNodeParams) -> Result<String> {
        // 1. Create the node (NO parent_id in storage)
        let node_id = self.node_service.create_node(...).await?;

        // 2. Create parent-child edge if parent_id provided
        if let Some(parent_id) = params.parent_id {
            self.ensure_parent_child_edge(node_id, parent_id).await?;
        }

        Ok(node_id)
    }
}
```

### HTTP Dev-Proxy

```rust
// packages/desktop-app/src-tauri/src/bin/dev-proxy.rs
#[derive(Deserialize)]
struct CreateNodeRequest {
    pub id: String,
    pub nodeType: String,
    pub content: String,
    pub parentId: Option<String>,        // ✅ HTTP API parameter
    pub containerNodeId: Option<String>, // ✅ HTTP API parameter
}

// Map HTTP request to NodeOperations::CreateNodeParams
let params = CreateNodeParams {
    id: req.id,
    node_type: req.nodeType,
    content: req.content,
    parent_id: req.parentId,             // ✅ Pass to operations
    container_node_id: req.containerNodeId, // ✅ Pass to operations
    // ...
};
```

### Frontend (TypeScript)

```typescript
// packages/desktop-app/src/lib/services/backend-adapter.ts
async createNode(node: Node): Promise<string> {
  const requestBody: any = {
    id: node.id,
    nodeType: node.nodeType,
    content: node.content,
    // ... other fields
  };

  // Extract transient parent fields (don't persist them)
  if ((node as any)._parentId) {
    requestBody.parentId = (node as any)._parentId; // ✅ Send as operation parameter
  }

  if ((node as any)._containerId) {
    requestBody.containerNodeId = (node as any)._containerId; // ✅ Send as operation parameter
  }

  return await fetch('/api/nodes', {
    method: 'POST',
    body: JSON.stringify(requestBody)
  });
}
```

### Component Layer (Svelte)

```svelte
// packages/desktop-app/src/lib/design/components/base-node-viewer.svelte
const promotedNode = {
  ...placeholder,
  content: newContent,
  _parentId: parentNodeId,      // ✅ Transient field
  _containerId: containerNodeId // ✅ Transient field
};

await nodeService.createNode(promotedNode);

// Update children cache (frontend only)
const existingChildren = sharedNodeStore.getNodesForParent(parentNodeId).map(n => n.id);
sharedNodeStore.updateChildrenCache(parentNodeId, [...existingChildren, promotedNode.id]);
```

---

## Rationale

### Why NOT Store These Fields?

1. **Graph-native architecture**: Edges are the source of truth for relationships
2. **Flexibility**: Multiple parent types, multiple relationship types
3. **Query power**: Leverage graph database edge queries
4. **Consistency**: Single source of truth (edges, not duplicated fields)

### Why KEEP as Operation Parameters?

1. **Atomic edge creation**: Create node + edge in single operation
2. **Performance**: Avoid extra round-trips for edge creation
3. **Consistency**: Ensure node is never orphaned
4. **Developer ergonomics**: Natural API (`createNode({ parentId: '...' })`)

### Why Transient Fields in Frontend?

1. **Context passing**: Components need to communicate parent context
2. **Type safety**: TypeScript can validate the pattern
3. **Clear intent**: `_` prefix signals "this is temporary, don't persist"
4. **HTTP adapter**: Easy to extract and send as parameters

---

## Consequences

### Positive

- ✅ Pure graph storage model (no denormalized fields)
- ✅ Atomic node+edge creation (no orphaned nodes)
- ✅ Clear separation: storage vs operations
- ✅ Developer-friendly API
- ✅ Maintains backward compatibility with frontend code

### Negative

- ⚠️ Requires careful documentation (why fields are transient)
- ⚠️ Frontend developers must understand the `_` prefix convention
- ⚠️ HTTP adapter has mapping logic (transient → parameters)
- ⚠️ Type definitions need to reflect transient vs persistent

### Risks

- **Risk**: Developers might try to persist `_parentId`/`_containerId` fields
- **Mitigation**: Clear naming convention, linter rules, code review

- **Risk**: Confusion between storage model and operation parameters
- **Mitigation**: This ADR, inline code comments, architecture docs

---

## Related Decisions

- **ADR-XXX** (Issue #514): Remove parent_id from storage (graph-native)
- **PR #523**: Initial (incomplete) graph migration that broke edge creation
- **PR #529** (Issue #528): This fix - re-add as operation parameters

---

## Testing Strategy

See `/packages/desktop-app/src/tests/integration/parent-child-edge-creation.test.ts`:

1. ✅ Parent-child edges created when `_parentId` provided
2. ✅ Children cache updated in frontend
3. ✅ Edges persist to database (via HTTP adapter)
4. ✅ No duplicate edges created
5. ✅ Nodes remain visible after promotion (no orphaning)
6. ✅ Parent relationship maintained across page reload

---

## Migration Notes

**For Issue #514 (Graph Migration)**:
- The storage model change was **correct** ✅
- The operation layer change was **incomplete** ❌
- This ADR completes the migration properly

**For Future Work**:
- Consider creating dedicated `CreateNodeContext` type instead of transient fields
- Evaluate if TypeScript decorators could enforce the `_` prefix convention
- Document pattern in frontend architecture guide

---

## References

- [Issue #528: Placeholder typing focus loss](https://github.com/malibio/nodespace-core/issues/528)
- [Issue #523: Graph-native hierarchy migration](https://github.com/malibio/nodespace-core/pull/523)
- [Issue #514: Remove parent_id from storage](https://github.com/malibio/nodespace-core/issues/514)
- [Component Architecture Guide](../components/component-architecture-guide.md)
- [Node Hierarchy System](../components/node-hierarchy-system.md)
