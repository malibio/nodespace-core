# MCP Integration - Current Gaps and Required Implementation

## Summary

**Status:** Types and basic infrastructure exist, but NO actual MCP integration is implemented.

**Risk Level:** 🔴 **HIGH** - Current architecture will cause data loss and conflicts when MCP is added.

## Critical Missing Pieces

### 1. Tauri IPC Event Channel (Rust → Frontend)

**Current:** Nothing
**Needed:** Rust backend emits events when MCP server modifies nodes

```rust
// packages/desktop-app/src-tauri/src/commands.rs

// NEW: Emit event when MCP server updates node
pub fn notify_frontend_of_mcp_update(
    app: tauri::AppHandle,
    node_id: String,
    node: Node,
    mcp_server_id: String,
) -> Result<(), String> {
    app.emit_all(
        "mcp:node:updated",
        McpUpdatePayload {
            node_id,
            node,
            server_id: mcp_server_id,
            timestamp: SystemTime::now(),
        },
    )?;
    Ok(())
}

// Call this after MCP server modifies database:
after_mcp_writes_to_database(node_id, node, server_id) {
    notify_frontend_of_mcp_update(app_handle, node_id, node, server_id);
}
```

### 2. Frontend Event Listener

**Current:** `handleExternalUpdate()` exists but nothing calls it
**Needed:** Event listener that calls handleExternalUpdate()

```typescript
// packages/desktop-app/src/lib/services/mcp-event-listener.ts

import { listen } from '@tauri-apps/api/event';
import { sharedNodeStore } from './shared-node-store';

export async function initializeMcpEventListener() {
  // Listen for MCP updates from Rust backend
  await listen('mcp:node:updated', (event) => {
    const { node_id, node, server_id, timestamp } = event.payload;

    console.log('[MCP] Received update from server:', server_id);

    // Forward to SharedNodeStore
    sharedNodeStore.handleExternalUpdate('mcp-server', {
      nodeId: node_id,
      changes: node,
      source: { type: 'mcp-server', serverId: server_id },
      timestamp,
      version: node.version  // Database version
    });
  });

  console.log('[MCP] Event listener initialized');
}

// Call on app startup
// In +layout.svelte or app initialization:
onMount(async () => {
  await initializeMcpEventListener();
});
```

### 3. Database Version Tracking

**Current:** Only in-memory versions tracked
**Needed:** Check database version before persisting

```typescript
// packages/desktop-app/src/lib/services/shared-node-store.ts

// Modify updateNode() to check database version
async updateNode(...) {
  // ... existing code ...

  // NEW: Check database version before persisting
  if (!options.skipPersistence && source.type !== 'database') {
    queueDatabaseWrite(nodeId, async () => {
      // Check database version FIRST
      const dbNode = await tauriNodeService.getNode(nodeId);

      if (dbNode && dbNode.version > updatedNode.version) {
        // Database was updated externally!
        console.warn('[SharedNodeStore] Database version conflict detected');

        // Emit conflict event
        eventBus.emit({
          type: 'persistence:conflict',
          nodeId,
          local: updatedNode,
          database: dbNode,
          source: source.type
        });

        // Don't persist - let user resolve
        throw new Error('Database version conflict');
      }

      // Proceed with persistence
      await tauriNodeService.updateNode(nodeId, updatedNode);
    });
  }
}
```

### 4. Cancel Pending Operations

**Current:** Debounced saves continue even after external update
**Needed:** Cancel pending operations when external update arrives

```typescript
// Modify handleExternalUpdate()
handleExternalUpdate(sourceType, update) {
  const nodeId = update.nodeId;

  // NEW: Cancel any pending debounced saves
  const pendingTimer = this.contentDebounceTimers.get(nodeId);
  if (pendingTimer) {
    clearTimeout(pendingTimer);
    this.contentDebounceTimers.delete(nodeId);
    console.log('[SharedNodeStore] Cancelled pending save due to external update');
  }

  // NEW: Cancel any pending structural updates
  if (this.pendingContentSaves.has(nodeId)) {
    console.warn('[SharedNodeStore] External update arrived during pending save');
    // Mark as cancelled, let operation complete but skip persistence
  }

  // Apply external update
  this.updateNode(update.nodeId, update.changes, update.source, {
    skipPersistence: sourceType === 'database',  // Already in DB
    force: true  // Override any version checks
  });

  // NEW: Notify user
  this.notifyUserOfExternalChange(nodeId, sourceType);
}

private notifyUserOfExternalChange(nodeId: string, sourceType: string) {
  // Show toast notification
  showToast({
    type: 'info',
    message: `Node updated by ${sourceType}`,
    action: {
      label: 'Undo',
      onClick: () => this.undoExternalChange(nodeId)
    },
    duration: 5000
  });
}
```

### 5. Conflict Resolution UI

**Current:** Last-Write-Wins (silent overwrite)
**Needed:** User prompt for conflict resolution

```svelte
<!-- packages/desktop-app/src/lib/components/ConflictDialog.svelte -->

<script lang="ts">
  import { eventBus } from '$lib/services/event-bus';

  let conflictDialogOpen = false;
  let localVersion: Node;
  let remoteVersion: Node;
  let resolveCallback: (resolution: Node) => void;

  eventBus.subscribe('persistence:conflict', (event) => {
    localVersion = event.local;
    remoteVersion = event.database;
    conflictDialogOpen = true;

    resolveCallback = (resolution) => {
      sharedNodeStore.updateNode(event.nodeId, resolution, source, {
        force: true  // Override version check
      });
      conflictDialogOpen = false;
    };
  });
</script>

{#if conflictDialogOpen}
  <dialog open>
    <h2>Conflict Detected</h2>
    <p>This node was modified by an external source while you were editing.</p>

    <div class="versions">
      <div class="local">
        <h3>Your Version</h3>
        <pre>{localVersion.content}</pre>
        <button on:click={() => resolveCallback(localVersion)}>
          Keep Mine
        </button>
      </div>

      <div class="remote">
        <h3>External Version</h3>
        <pre>{remoteVersion.content}</pre>
        <button on:click={() => resolveCallback(remoteVersion)}>
          Use Theirs
        </button>
      </div>
    </div>

    <button on:click={() => conflictDialogOpen = false}>
      Cancel
    </button>
  </dialog>
{/if}
```

### 6. Dependency Coordination with MCP

**Current:** No coordination between user operations and MCP updates
**Needed:** Ensure MCP updates don't break ongoing operations

```typescript
// With dependency-based architecture:

// User's combineNodes() operation
const mergeHandle = updateNode('GrandChild1', { content }, source, {
  persistenceMode: 'immediate'
});

// MCP update arrives during merge
handleExternalUpdate('mcp-server', {
  nodeId: 'Child3',  // One of the nodes being promoted
  changes: { content: 'MCP modified' }
});

// Problem: Child3 promotion depends on merge completing
// If MCP modifies Child3, it should:
// 1. Wait for merge to complete
// 2. OR cancel the merge
// 3. OR merge with user's changes

// Solution: Queue MCP updates behind ongoing operations
handleExternalUpdate(sourceType, update) {
  // Check if node is involved in pending operation
  const pendingOp = this.pendingOperations.get(update.nodeId);

  if (pendingOp) {
    console.log('[MCP] Queuing update behind pending operation');

    // Wait for pending operation to complete
    pendingOp.promise.finally(() => {
      // Then apply MCP update
      this.applyExternalUpdate(update);
    });
  } else {
    // No pending operation - apply immediately
    this.applyExternalUpdate(update);
  }
}
```

## Implementation Priority

### Phase 1: Basic MCP Integration (Required for Issue #112)
1. ✅ Rust event emitter when MCP modifies database
2. ✅ Frontend event listener
3. ✅ Call `handleExternalUpdate()` from listener
4. ✅ Cancel pending debounced saves
5. ✅ Basic user notification (toast)

### Phase 2: Conflict Detection (Required for multi-user)
6. ✅ Database version checking before persistence
7. ✅ Conflict event emission
8. ✅ Basic conflict resolution UI
9. ✅ Undo stack for external changes

### Phase 3: Advanced Coordination (Optional, future)
10. ⏳ Queue MCP updates behind ongoing operations
11. ⏳ Operational Transform for text merging
12. ⏳ CRDT for conflict-free coordination

## Testing Strategy

```typescript
// Test: MCP update during user edit
test('MCP update cancels pending debounced save', async () => {
  // User types
  sharedNodeStore.updateNode('node1', { content: 'User edit' }, viewerSource);

  // MCP updates same node (within debounce window)
  sharedNodeStore.handleExternalUpdate('mcp-server', {
    nodeId: 'node1',
    changes: { content: 'MCP edit' },
    source: { type: 'mcp-server' },
    timestamp: Date.now() + 100
  });

  // Wait for debounce
  await sleep(600);

  // Verify: MCP version persisted, user version cancelled
  const persisted = await tauriNodeService.getNode('node1');
  expect(persisted.content).toBe('MCP edit');
});

// Test: User edit during MCP operation
test('User edit waits for MCP operation to complete', async () => {
  // MCP starts update
  const mcpPromise = sharedNodeStore.handleExternalUpdate('mcp-server', {
    nodeId: 'node1',
    changes: { content: 'MCP edit' }
  });

  // User edits same node immediately
  sharedNodeStore.updateNode('node1', { content: 'User edit' }, viewerSource, {
    dependsOn: [mcpPromise]  // Wait for MCP
  });

  // Verify: User edit happens AFTER MCP
  const order = [];
  // ... track order ...
  expect(order).toEqual(['mcp', 'user']);
});
```

## Conclusion

**Current state:** Architecture is **NOT sufficient** for MCP integration.

**Required work:**
- 🔴 **Critical:** Rust → Frontend event channel (Phase 1)
- 🔴 **Critical:** Cancel pending operations on external update (Phase 1)
- 🟡 **Important:** Database version checking (Phase 2)
- 🟡 **Important:** Conflict resolution UI (Phase 2)
- 🟢 **Nice-to-have:** Advanced coordination (Phase 3)

**Estimated effort:**
- Phase 1: 2-3 days
- Phase 2: 3-5 days
- Phase 3: 1-2 weeks (ongoing)

**Recommendation:** Complete Phase 1 before deploying MCP server to prevent data loss.
