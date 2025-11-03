# Optimistic Concurrency Control (OCC)

## Overview

NodeSpace implements lightweight optimistic concurrency control (OCC) to prevent race conditions when multiple clients (MCP servers, Frontend UI, AI assistants) modify the same node concurrently. This document describes the design principles, implementation details, and integration patterns.

## Design Principles

### Why Optimistic Concurrency Control?

**Problem**: Without concurrency control, concurrent modifications lead to silent data loss:

```
Time  | Frontend UI              | MCP Client
------|--------------------------|---------------------------
T1    | Read node (content: "A") |
T2    |                          | Read same node (content: "A")
T3    | Update content → "B"     |
T4    |                          | Update properties → OVERWRITES ❌
```

Result: Frontend's content change is lost because MCP client's update overwrote the entire node.

**Solution**: Version-based conflict detection ensures clients explicitly acknowledge concurrent modifications:

```
Time  | Frontend UI                    | MCP Client
------|--------------------------------|--------------------------------
T1    | Read node (v=1, content: "A")  |
T2    |                                | Read node (v=1, content: "A")
T3    | Update with v=1 → SUCCESS (v→2)|
T4    |                                | Update with v=1 → CONFLICT ❌
T5    |                                | Re-read (v=2), merge, retry
```

Result: MCP client detects conflict, fetches latest state, merges changes intelligently, and retries.

### Design Choices

#### 1. Lightweight OCC (Not MVCC)

**What we chose**: Single `version` column that increments on each update

**What we didn't choose**: Multi-Version Concurrency Control (MVCC) with transaction IDs and snapshot isolation

**Rationale**:
- NodeSpace conflicts are rare (few concurrent editors per node)
- MVCC overhead (history storage, garbage collection) not justified
- Version-based detection is sufficient and performant (< 5ms overhead)

#### 2. Per-Node Versioning

**What we chose**: Each node has its own version counter

**What we didn't choose**: Document-wide or workspace-wide versioning

**Rationale**:
- Enables concurrent edits to different nodes without conflicts
- Only conflicts when the exact same node is modified
- Scales better with large documents (parallel operations)

#### 3. Client-Side Conflict Resolution

**What we chose**: Server returns conflict error with current node state; clients decide how to merge

**What we didn't choose**: Server-side automatic merge strategies

**Rationale**:
- Different clients have different merge priorities (UI vs AI assistants)
- Frontend can show rich merge UI for user decisions
- MCP clients can implement domain-specific merge logic
- Flexibility beats one-size-fits-all approach

## Implementation Architecture

### Database Schema

```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,  -- OCC version counter
    node_type TEXT NOT NULL,
    content TEXT NOT NULL,
    properties TEXT,
    parent_id TEXT,
    before_sibling_id TEXT,
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Index for efficient version checks
CREATE INDEX idx_nodes_version ON nodes(id, version);
```

**Version Semantics**:
- Starts at 1 for new nodes
- Increments atomically on every UPDATE operation
- Never decrements (monotonically increasing)
- Survives node moves, reorders, and property updates

### Core Update Pattern

All update operations follow this pattern:

```rust
// 1. Client provides expected version from their last read
pub async fn update_node(
    &self,
    node_id: &str,
    expected_version: i64,  // Version client thinks is current
    update: NodeUpdate,
) -> Result<Node, NodeOperationError> {
    // 2. Atomic version check + update
    let rows_affected = self.db.execute(
        "UPDATE nodes
         SET content = ?,
             version = version + 1,  -- Atomic increment
             modified_at = ?
         WHERE id = ? AND version = ?",  -- Version check
        params![update.content, now, node_id, expected_version]
    ).await?;

    // 3. If no rows affected, version mismatch occurred
    if rows_affected == 0 {
        let current = self.get_node(node_id).await?
            .ok_or_else(|| NodeOperationError::node_not_found(node_id))?;

        return Err(NodeOperationError::VersionConflict {
            node_id: node_id.to_string(),
            expected_version,
            actual_version: current.version,
            current_node: current,  // Provide current state for merge
        });
    }

    // 4. Return updated node
    Ok(/* fetch and return updated node */)
}
```

**Why This Works**:
- SQL `WHERE version = ?` ensures atomicity (no TOCTOU)
- `rows_affected = 0` unambiguously signals version mismatch
- No database locks required (optimistic approach)
- Transaction isolation handles concurrent updates safely

### Operations Affected

All mutating operations use version checking:

| Operation | Version Check Location | Notes |
|-----------|------------------------|-------|
| `update_node()` | Content/properties update | Direct version check |
| `delete_node()` | Node deletion | Prevents deleting modified node |
| `move_node()` | Hierarchy change | Checks node being moved |
| `reorder_node()` | Sibling position change | Checks node being reordered |
| Sibling chain fixes | Sibling updates | Uses retry loop (see below) |

### Sibling Chain Race Condition Fix

**Problem**: Reordering nodes requires updating multiple siblings' `before_sibling_id` pointers. Without OCC, concurrent operations can corrupt the chain.

**Scenario**:
```
Initial: A → B → C
Thread 1: Move B to first (B → A → C) - needs to fix A's before_sibling_id
Thread 2: Update C's properties concurrently
Result: Thread 1 overwrites C's property changes when fixing chain ❌
```

**Solution**: Retry loop with exponential backoff

```rust
// When fixing sibling pointers during reorder/delete
for attempt in 0..MAX_RETRIES {
    // Fetch fresh sibling state (with current version)
    let fresh_sibling = self.node_service.get_node(&sibling_id).await?;

    // Attempt update with current version
    let rows_affected = self.node_service
        .update_with_version_check(
            &sibling_id,
            fresh_sibling.version,  // Use fresh version
            NodeUpdate::new().with_before_sibling_id(new_position)
        )
        .await?;

    // Success - break out of retry loop
    if rows_affected > 0 {
        break;
    }

    // Conflict - exponential backoff before retry
    tokio::time::sleep(Duration::from_millis(10 * (1 << attempt))).await;
}

// If all retries exhausted, return error
if attempt == MAX_RETRIES {
    return Err(NodeOperationError::max_retries_exceeded());
}
```

**Parameters**:
- `MAX_RETRIES = 3`
- Backoff: 10ms, 20ms, 40ms
- Total max delay: 70ms (acceptable for rare conflicts)

**Why This Works**:
- Each retry fetches fresh node state (including updated version)
- Conflicts are rare, so retries typically succeed on first attempt
- Exponential backoff reduces contention under load
- Max retries prevents infinite loops

## Security Considerations

### TOCTOU Vulnerability Prevention

**CRITICAL**: Version parameter must be MANDATORY, not optional.

**Vulnerable Pattern (DO NOT USE)**:
```rust
// ❌ INSECURE - Optional version creates TOCTOU race
pub async fn update_node(
    node_id: &str,
    version: Option<i64>,  // ❌ Optional allows bypass
    update: NodeUpdate,
) -> Result<Node> {
    let version = match version {
        Some(v) => v,
        None => {
            // ❌ Auto-fetch creates race condition
            let node = self.get_node(node_id).await?;
            node.version
        }
    };
    // ... rest of update
}
```

**Attack Scenario**:
```
Time  | Client A                     | Client B
------|------------------------------|------------------------------
T1    | Read node (v=5)              | Read node (v=5)
T2    | Update WITHOUT version →     |
      | auto-fetches v5 → writes v6  |
T3    |                              | Update WITHOUT version →
      |                              | auto-fetches v6 → writes v7 ❌
```

Result: Client B silently overwrites Client A's changes (no conflict detected).

**Secure Pattern (CURRENT IMPLEMENTATION)**:
```rust
// ✅ SECURE - Version is mandatory
pub async fn update_node(
    node_id: &str,
    version: i64,  // ✅ Required, no auto-fetch
    update: NodeUpdate,
) -> Result<Node> {
    // Version check is always explicit
    // ... update with version check
}
```

**Why This Matters**:
- Eliminates Time-of-Check-Time-of-Use (TOCTOU) race condition
- Forces clients to explicitly acknowledge their read state
- Makes conflict detection reliable and deterministic

### Version Exhaustion

**Non-Issue**: `i64` version counter supports ~9.2 quintillion updates per node. At 1 million updates/second, this lasts 292 million years.

### Version Spoofing

**Non-Issue**: Version checks prevent spoofing:
- Attacker cannot guess future version (must match current)
- Lower version → conflict error
- Higher version → conflict error
- Only exact match succeeds

## MCP Client Integration

### Request Format

All update/delete/move/reorder operations require version:

```json
{
  "method": "nodes/update",
  "params": {
    "id": "node-abc123",
    "version": 5,  // REQUIRED: Version from last read
    "content": "Updated text",
    "properties": {
      "tags": ["important"]
    }
  }
}
```

### Conflict Error Response

```json
{
  "error": {
    "code": -32001,
    "message": "Version conflict: expected version 5, but current version is 7",
    "data": {
      "expected_version": 5,
      "actual_version": 7,
      "current_node": {
        "id": "node-abc123",
        "version": 7,
        "content": "Someone else's edit",
        "properties": { "tags": ["urgent"] }
      }
    }
  }
}
```

### Client Retry Pattern

```typescript
async function updateNodeWithRetry(
  nodeId: string,
  update: NodeUpdate,
  maxRetries: number = 3
): Promise<Node> {
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    // Always fetch latest state before update
    const currentNode = await mcpClient.getNode(nodeId);

    try {
      // Attempt update with current version
      return await mcpClient.updateNode(nodeId, currentNode.version, update);
    } catch (error) {
      if (error.code === -32001) { // Version conflict
        // Merge logic here (see below)
        const merged = attemptAutoMerge(update, error.data.current_node);
        if (merged.success) {
          update = merged.update; // Retry with merged changes
          continue;
        } else {
          throw new Error("Manual merge required");
        }
      }
      throw error; // Non-conflict error
    }
  }

  throw new Error(`Max retries (${maxRetries}) exceeded`);
}
```

### Auto-Merge Strategies

**Strategy 1: Non-Overlapping Field Updates**

```typescript
function attemptAutoMerge(
  yourUpdate: NodeUpdate,
  currentNode: Node
): MergeResult {
  // If you only changed content, and server only changed properties
  if (yourUpdate.content && !yourUpdate.properties) {
    if (currentNode.properties !== originalNode.properties) {
      return {
        success: true,
        update: {
          content: yourUpdate.content,
          properties: currentNode.properties // Keep server's properties
        }
      };
    }
  }

  return { success: false }; // Overlapping changes require manual merge
}
```

**Strategy 2: Last-Write-Wins (Use Sparingly)**

```typescript
// Only appropriate for idempotent operations
function forceUpdate(nodeId: string, update: NodeUpdate): Promise<Node> {
  const currentNode = await mcpClient.getNode(nodeId);
  return await mcpClient.updateNode(nodeId, currentNode.version, update);
  // No retry needed - always uses latest version
}
```

## Frontend Conflict Resolution

### Automatic Merge (Strategy 1)

```typescript
// packages/desktop-app/src/lib/services/version-conflict-resolver.ts

export async function resolveVersionConflict(
  nodeId: string,
  yourChanges: Partial<Node>,
  conflict: VersionConflictError
): Promise<Node> {
  const currentNode = conflict.current_node;

  // Strategy 1: Auto-merge if no overlapping changes
  if (canAutoMerge(yourChanges, originalNode, currentNode)) {
    const merged = {
      ...currentNode,
      ...deepMerge(currentNode, yourChanges), // Deep merge properties
      version: currentNode.version // Use current version
    };

    return await tauriCommands.updateNode(nodeId, merged.version, merged);
  }

  // Strategy 2: Show conflict modal for overlapping changes
  return await showConflictModal(yourChanges, currentNode);
}

function canAutoMerge(
  yourChanges: Partial<Node>,
  original: Node,
  current: Node
): boolean {
  // Content-only change + properties changed on server = auto-merge
  if (yourChanges.content && !yourChanges.properties) {
    return current.content === original.content;
  }

  // Properties-only change + content changed on server = auto-merge
  if (yourChanges.properties && !yourChanges.content) {
    return current.properties === original.properties;
  }

  // Overlapping changes = require manual resolution
  return false;
}
```

### Manual Merge UI

```svelte
<!-- packages/desktop-app/src/lib/components/conflict-modal.svelte -->
<script lang="ts">
  export let yourChanges: Partial<Node>;
  export let currentNode: Node;

  let resolution: 'yours' | 'current' | 'manual' = 'manual';
</script>

<Modal title="Conflict Detected">
  <div class="conflict-details">
    <p>Node was modified by another client while you were editing</p>

    <div class="comparison">
      <div class="your-version">
        <h4>Your Changes</h4>
        <pre>{JSON.stringify(yourChanges, null, 2)}</pre>
      </div>

      <div class="current-version">
        <h4>Current Version</h4>
        <pre>{JSON.stringify(currentNode, null, 2)}</pre>
      </div>
    </div>
  </div>

  <div class="actions">
    <button on:click={() => resolution = 'yours'}>
      Use Your Changes
    </button>
    <button on:click={() => resolution = 'current'}>
      Use Current Version
    </button>
    <button on:click={() => resolution = 'manual'}>
      Merge Manually
    </button>
  </div>
</Modal>
```

## Performance Characteristics

### Measured Overhead

Performance benchmark results (`packages/core/src/mcp/handlers/nodes_test.rs:1467`):

| Metric | Value | Notes |
|--------|-------|-------|
| Average update time | 0.5-2ms | 100 sequential updates with version checking |
| Acceptance criterion | < 5ms | Met with significant margin |
| Overhead vs non-OCC | ~1-2ms | Version check + atomic increment |

### Scalability

**Concurrent Clients**: Linear performance (no locks)
- 1 client: 2ms per update
- 10 clients: 2ms per update (no contention if different nodes)
- 100 clients: Slight increase only on conflicting updates (rare)

**Conflict Frequency**: Depends on editing patterns
- Single user: 0% conflicts (sequential edits)
- AI assistant + user: < 1% conflicts (rare simultaneous edits)
- Multiple users: Varies (OCC optimized for low-conflict scenarios)

### When Conflicts Are Acceptable

OCC is ideal for NodeSpace because:
1. **Rare conflicts**: Most edits are to different nodes
2. **User tolerance**: UI can show merge modal (not blocking)
3. **AI assistants**: Can retry automatically with merge logic
4. **No locks**: Doesn't block readers or other writers

### When OCC Would Not Work

If NodeSpace had:
- High conflict rate (e.g., real-time collaborative editing of same node)
- Need for snapshot isolation (not just conflict detection)
- Complex transaction dependencies across multiple nodes

Then we would need MVCC, operational transforms, or CRDTs instead.

## Testing Strategy

### Unit Tests

```rust
// packages/core/src/mcp/handlers/nodes_test.rs

#[tokio::test]
async fn test_concurrent_update_version_conflict() {
    // Simulates two clients reading same node, one updates successfully,
    // other gets conflict error
}

#[tokio::test]
async fn test_version_increments_on_update() {
    // Verifies version increments atomically
}

#[tokio::test]
async fn test_delete_with_version_check() {
    // Ensures deletes also check version
}

#[tokio::test]
async fn test_rapid_sequential_updates() {
    // Stress test: 100 rapid updates, all should succeed
}

#[tokio::test]
async fn test_occ_performance_overhead() {
    // Validates < 5ms per operation acceptance criterion
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_mcp_and_frontend_concurrent_updates() {
    // Simulates MCP client and Tauri command updating same node
}

#[tokio::test]
async fn test_sibling_chain_integrity_under_concurrency() {
    // Multiple clients reordering same sibling chain
}
```

### Frontend Tests

```typescript
// packages/desktop-app/src/tests/services/version-conflict-resolver.test.ts

describe("Version Conflict Resolver", () => {
  it("should auto-merge non-overlapping changes", () => {
    // Your content change + their property change = auto-merge
  });

  it("should require manual merge for overlapping changes", () => {
    // Your content change + their content change = manual
  });

  it("should deep-merge properties object", () => {
    // Ensures nested properties are merged correctly
  });
});
```

## Troubleshooting

### Common Issues

**Issue**: "Version conflict" errors on every update

**Cause**: Client not fetching latest node before update

**Solution**: Always read node immediately before update:
```typescript
const node = await getNode(nodeId);  // Fresh read
await updateNode(nodeId, node.version, changes);  // Use fresh version
```

---

**Issue**: Sibling chain corruption despite OCC

**Cause**: Legacy code using non-versioned `update_node()` instead of `update_with_version_check()`

**Solution**: Ensure ALL updates (including sibling fixes) use version checking:
```rust
// ❌ Wrong
self.node_service.update_node(&id, update).await?;

// ✅ Correct
self.node_service.update_with_version_check(&id, version, update).await?;
```

---

**Issue**: Max retries exceeded during sibling operations

**Cause**: Extremely high contention on sibling chain (rare)

**Solution**: Increase `MAX_RETRIES` or backoff duration in `packages/core/src/operations/mod.rs`

---

**Issue**: Performance degradation with OCC enabled

**Cause**: Likely unrelated to OCC (< 2ms overhead)

**Solution**: Profile with `cargo flamegraph` - likely database indexes or query optimization needed

## Future Enhancements

### Potential Improvements (Not Currently Needed)

1. **MVCC for High-Conflict Scenarios**
   - If conflict rate exceeds 5%, consider MVCC with snapshot isolation
   - Would add history storage and garbage collection overhead

2. **Operation Transforms for Real-Time Collab**
   - If we add real-time collaborative editing of same node
   - Requires transformation functions for conflicting operations

3. **CRDTs for Convergence**
   - If we need eventual consistency without conflicts
   - Would fundamentally change data model (no version numbers)

4. **Version Vector for Distributed Sync**
   - If we add multi-device sync with offline support
   - Would replace single version counter with per-device vectors

None of these are needed for current NodeSpace use cases. OCC is sufficient.

## Summary

NodeSpace's OCC implementation provides:

✅ **Correctness**: Detects and prevents concurrent modification race conditions
✅ **Security**: Eliminates TOCTOU vulnerability with mandatory version checks
✅ **Performance**: < 5ms overhead per operation (typically 1-2ms)
✅ **Flexibility**: Client-side merge strategies for different use cases
✅ **Simplicity**: Lightweight design without MVCC complexity

The design is well-suited for NodeSpace's low-conflict, multi-client editing environment.
