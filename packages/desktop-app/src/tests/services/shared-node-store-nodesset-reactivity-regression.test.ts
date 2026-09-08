/**
 * Regression coverage for a stale-reactivity bug where a Kanban board (or any
 * other `$derived`/`$effect` consumer of `sharedNodeStore`) stopped reacting
 * to a node after its first successful write, only recovering on remount.
 *
 * `SvelteMap.set(key, value)` only increments that key's reactive signal
 * when `value` is a DIFFERENT reference from what's already stored —
 * reference equality, not a deep comparison (see
 * svelte/src/reactivity/map.js). Several write paths in
 * `shared-node-store.svelte.ts` (the generic `updateNode()` success handler,
 * `updateTaskNode()`'s success handler, and others) read the CURRENT node
 * object via `this.nodes.get(nodeId)`, mutate it in place with
 * `Object.assign(...)`, and re-set that SAME reference via the private
 * `nodesSet()` helper to "confirm" the backend's response.
 *
 * Passed straight through to `SvelteMap.set()`, that re-set is a silent
 * no-op for reactivity: any `$derived`/`$effect` consumer that already read
 * this node (e.g. a Kanban board's per-column bucketing) never re-runs,
 * because the map's per-key signal was never incremented. The node's data in
 * the store IS correct — `getNode()` returns the right value if read fresh —
 * but nothing tells existing reactive consumers to re-read it. A remount
 * (which re-subscribes from scratch) "fixes" it, matching the "works once
 * per mount, then stuck" symptom this covers.
 *
 * Fixed by having `nodesSet()` always store a shallow copy, so `set()` sees
 * a genuine reference change and signals every consumer regardless of
 * whether the caller mutated in place or built a fresh object.
 *
 * This test exercises the private `nodesSet()` behavior indirectly through
 * `updateTaskNode()`'s confirm branch (`Object.assign(localNode,
 * confirmedFields); this.nodesSet(nodeId, localNode);`), which mutates and
 * re-sets the exact SAME reference every time — the precise shape the
 * Kanban board's real bug went through.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  SharedNodeStore,
  SimplePersistenceCoordinator
} from '../../lib/services/shared-node-store.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import type { UpdateNodeInput } from '../../lib/services/backend-adapter';
import type { Node, TaskNode, TaskNodeUpdate } from '../../lib/types';

type TaskLikeNode = Node & { status: string };

const makeTaskNode = (id: string, status: string, version = 1): TaskLikeNode =>
  ({
    id,
    nodeType: 'task',
    content: '- [ ] seed task',
    createdAt: '2024-01-01T00:00:00.000Z',
    modifiedAt: '2024-01-01T00:00:00.000Z',
    version,
    properties: {},
    mentions: [],
    status
  }) as unknown as TaskLikeNode;

const viewerSource = { type: 'viewer' as const, viewerId: 'kanban-view' };

describe('SharedNodeStore.nodesSet() — reference-identity reactivity regression', () => {
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

  it('re-sets a DIFFERENT object reference when a write confirms by mutating the currently-stored node in place', async () => {
    // Exercises the private `nodesSet()` helper directly via `setNode()`'s
    // internal call path is awkward from outside the class, so instead this
    // drives the exact shape the real bug went through: read the current
    // node, mutate it with `Object.assign`, hand that SAME reference to a
    // public method that ultimately calls `nodesSet()`. `updateTaskNode()`'s
    // confirm branch does precisely this
    // (`Object.assign(localNode, confirmedFields); this.nodesSet(nodeId,
    // localNode)`), so asserting on its outcome proves the fix without
    // reaching into store internals.
    const nodeId = 'reactivity-1';
    store.setNode(makeTaskNode(nodeId, 'open', 1), { type: 'database', reason: 'seed' });

    vi.spyOn(backendAdapter, 'updateTaskNode').mockImplementation(
      async (id: string, version: number, update: TaskNodeUpdate) => {
        return {
          id,
          nodeType: 'task',
          content: '- [ ] seed task',
          createdAt: '2024-01-01T00:00:00.000Z',
          modifiedAt: new Date().toISOString(),
          version: version + 1,
          status: update.status ?? 'open'
        } as unknown as TaskNode;
      }
    );

    store.updateTaskNode(nodeId, { status: 'in_progress' }, viewerSource);
    const refBeforeConfirm = store.getNode(nodeId);

    // Poll for the actual condition (the confirmed version landing) rather
    // than a fixed delay — deterministic regardless of how long the mocked
    // RPC's microtask chain takes to resolve.
    await vi.waitFor(() => {
      expect(store.getNode(nodeId)?.version).toBe(2);
    });
    const refAfterConfirm = store.getNode(nodeId);

    // The store's data must be correct...
    expect((refAfterConfirm as TaskLikeNode | undefined)?.status).toBe('in_progress');
    // ...AND the confirm step's mutate-then-reset must have installed a
    // genuinely different object reference, not the SAME one it mutated in
    // place — otherwise a `$derived` consumer holding `refBeforeConfirm`
    // (from reading the node during the optimistic-apply render) never sees
    // the confirmed value (SvelteMap.set() reference-equality gate).
    expect(refAfterConfirm).not.toBe(refBeforeConfirm);
  });

  it('produces a fresh reference on every generic updateNode() confirm, not just the optimistic apply', async () => {
    // Uses a `text` node (no registered type-specific updater), and a
    // `properties` change — `task`'s `status` would route through the
    // type-specific `updateTaskNode` path instead (see the other test in
    // this file), never reaching `backendAdapter.updateNode` at all.
    const nodeId = 'reactivity-2';
    store.setNode(
      {
        id: nodeId,
        nodeType: 'text',
        content: 'hello',
        createdAt: '2024-01-01T00:00:00.000Z',
        modifiedAt: '2024-01-01T00:00:00.000Z',
        version: 1,
        properties: {},
        mentions: []
      } as unknown as Node,
      { type: 'database', reason: 'seed' }
    );

    vi.spyOn(backendAdapter, 'updateNode').mockImplementation(
      async (id: string, version: number, update: UpdateNodeInput) => {
        return {
          id,
          nodeType: 'text',
          content: 'hello',
          createdAt: '2024-01-01T00:00:00.000Z',
          modifiedAt: new Date().toISOString(),
          // `...update` first: `UpdateNodeInput` echoes back whatever the
          // caller sent (here, `{ properties: {...} }`), which does NOT
          // include a version — but if it ever did, `version` below must
          // win, not be silently overwritten by an echoed field.
          ...update,
          version: version + 1
        } as unknown as Node;
      }
    );

    const beforeWrite = store.getNode(nodeId);
    store.updateNode(
      nodeId,
      { properties: { color: 'red' } } as Partial<Node>,
      viewerSource,
      {}
    );
    const afterOptimistic = store.getNode(nodeId);
    expect(afterOptimistic).not.toBe(beforeWrite);

    // Poll for the confirmed version rather than a fixed delay — see the
    // matching comment in the `updateTaskNode()` test above.
    await vi.waitFor(() => {
      expect(store.getNode(nodeId)?.version).toBe(2);
    });
    const afterConfirm = store.getNode(nodeId);
    // The confirm step re-sets the node too (to apply the backend's
    // authoritative version) — it must also be a new reference so a
    // `$derived` consumer that read `afterOptimistic` re-runs and picks up
    // the confirmed version/value rather than staying stuck on the
    // optimistic one.
    expect(afterConfirm).not.toBe(afterOptimistic);
  });
});
