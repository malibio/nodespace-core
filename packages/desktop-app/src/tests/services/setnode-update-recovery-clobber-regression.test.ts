/**
 * Regression coverage for `SharedNodeStore.setNode()`'s update-recovery
 * success handler (the "already persisted" branch of the persistence
 * closure) unconditionally `Object.assign`-ing every field from
 * `backendAdapter.updateNode()`'s response onto the local node, regardless
 * of whether this write's own request actually changed that field.
 *
 * This is the same clobber shape fixed in the generic `updateNode()`
 * method's own success handler (see `scopedFields` there): if a second
 * write for the same node races in while `setNode()`'s own `updateNode` RPC
 * is still in flight, and that second write changes a DIFFERENT field than
 * the first request touched, an unconditional spread of the first (now
 * stale) response would revert the second write's optimistic value for that
 * field — even though the second write's own optimistic apply already
 * landed in the store.
 *
 * `setNode()` posts the whole node as its payload rather than an explicit
 * patch, so the fix scopes the response by comparing each field against the
 * pre-RPC snapshot (`currentNode`, captured when the closure read state at
 * execution time) instead of an explicit `changedFields` list: a field is
 * only applied if the local node's current value for it still matches that
 * snapshot (nothing else moved it on since).
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  SharedNodeStore,
  SimplePersistenceCoordinator
} from '../../lib/services/shared-node-store.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import type { UpdateNodeInput } from '../../lib/services/backend-adapter';
import type { Node } from '../../lib/types';

function textNode(id: string, content: string, mentions: string[], version = 1): Node {
  return {
    id,
    nodeType: 'text',
    content,
    createdAt: '2026-01-01T00:00:00.000Z',
    modifiedAt: '2026-01-01T00:00:00.000Z',
    version,
    properties: {},
    mentions
  } as unknown as Node;
}

// `setNode()` only persists an already-persisted node for a non-'viewer'
// source (`shouldPersist = source.type !== 'viewer' || isNewNode`) — viewer
// edits to an existing node go through the debounced `updateNode()` path
// instead. Any non-'viewer', non-'database' source works here; 'mcp-server'
// was picked arbitrarily (it also incidentally sidesteps the 'database'-only
// skip-while-editing guard, though that guard would no-op anyway since
// neither write focuses the node).
const writeSource = { type: 'mcp-server' as const, serverId: 'test-server' };

describe("SharedNodeStore.setNode() update-recovery success handler — field clobber regression", () => {
  let store: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    SimplePersistenceCoordinator.resetInstance();
    store = SharedNodeStore.getInstance();
  });

  afterEach(() => {
    store.clearAll();
    SharedNodeStore.resetInstance();
    vi.restoreAllMocks();
  });

  it("does not revert a second write's field with a stale first-write response landing after it", async () => {
    const nodeId = 'setnode-clobber-1';
    // Seed as an already-persisted node so setNode()'s "already persisted"
    // UPDATE-recovery branch (not CREATE) is exercised.
    store.setNode(textNode(nodeId, 'hello', ['a']), { type: 'database', reason: 'seed' });

    let resolveFirst!: (node: Node) => void;
    const firstResponse = new Promise<Node>((resolve) => {
      resolveFirst = resolve;
    });

    let callCount = 0;
    vi.spyOn(backendAdapter, 'updateNode').mockImplementation(
      async (id: string, version: number, update: UpdateNodeInput) => {
        callCount += 1;
        if (callCount === 1) {
          // The FIRST call's response is held back so its (now-stale)
          // success handler resolves strictly after the second write's
          // optimistic apply.
          return firstResponse.then((n) => ({ ...n, id }));
        }
        // The second (queued) write's own RPC resolves immediately,
        // echoing its own request back with a bumped version.
        return { ...update, id, version: version + 1 } as unknown as Node;
      }
    );

    // First write: changes `content` only. Its RPC will stay pending until
    // `resolveFirst` is called below.
    store.setNode(textNode(nodeId, 'hello world', ['a'], 1), writeSource);

    // Second write for the SAME node, fired before the first write's RPC
    // resolves: changes `mentions` only, leaving `content` as the first
    // write left it. Because a write is already executing for this node,
    // PersistenceCoordinator collapses this into a queued slot — but the
    // optimistic apply to the store itself is synchronous and immediate.
    store.setNode(textNode(nodeId, 'hello world', ['a', 'b'], 1), writeSource);

    expect(store.getNode(nodeId)?.mentions).toEqual(['a', 'b']);

    // Now let the first write's (stale) response land. Its payload reflects
    // the pre-second-write snapshot: `mentions: ['a']`.
    resolveFirst(textNode(nodeId, 'hello world', ['a'], 2));

    // Let the first write's success handler run, then let the queued second
    // write's own RPC (and its success handler) settle too.
    await vi.waitFor(() => {
      expect(store.getNode(nodeId)?.version).toBeGreaterThanOrEqual(2);
    });
    // Drain microtasks/timers for the queued second write's own RPC.
    await new Promise((r) => setTimeout(r, 0));
    await vi.waitFor(() => {
      expect(backendAdapter.updateNode).toHaveBeenCalledTimes(2);
    });

    // The second write's `mentions` value must never have been reverted to
    // the first (stale) response's value, even momentarily.
    expect(store.getNode(nodeId)?.mentions).toEqual(['a', 'b']);
  });
});
