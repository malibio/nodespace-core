/**
 * Regression tests for issue #1492: edit persistence amplifies writes and
 * self-conflicts during fast typing.
 *
 * Covers the SimplePersistenceCoordinator's per-node serial writer: a
 * keystroke arriving while an UpdateNode RPC is in flight must collapse into
 * a single latest-wins write that runs only after the in-flight write's
 * version confirmation lands, never re-fired per RPC round-trip and never
 * OCC-conflicting against the same client's own prior write.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';
import type { Node } from '../../lib/types';

const makeNode = (id: string, content: string, version = 1): Node => ({
  id,
  nodeType: 'text',
  content,
  createdAt: '2024-01-01T00:00:00.000Z',
  modifiedAt: '2024-01-01T00:00:00.000Z',
  version,
  properties: {},
  mentions: []
});

const dbSource = { type: 'database' as const, reason: 'initial-load' };
const viewerSource = { type: 'viewer' as const, viewerId: 'pane-1' };

describe('Persistence serial writer regression (#1492)', () => {
  let store: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    store = SharedNodeStore.getInstance();
    conflictNotifications.dismissAll();
  });

  afterEach(() => {
    store.clearAll();
    SharedNodeStore.resetInstance();
    conflictNotifications.dismissAll();
    vi.restoreAllMocks();
  });

  it('a keystroke arriving mid-RPC collapses into a single follow-up write, not a re-fire per round-trip', async () => {
    const nodeId = 'serial-writer-1';
    const initialNode = makeNode(nodeId, 'a', 1);

    let callCount = 0;
    // Each RPC takes a tick to resolve, simulating an in-flight round-trip.
    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockImplementation(async (_id, version, node) => {
      callCount++;
      await new Promise((resolve) => setTimeout(resolve, 20));
      return {
        id: nodeId,
        nodeType: 'text',
        content: String(node.content ?? ''),
        createdAt: '2024-01-01T00:00:00.000Z',
        modifiedAt: new Date().toISOString(),
        version: version + 1,
        properties: {},
        mentions: []
      };
    });

    store.setNode(initialNode, dbSource);

    // First edit: debounce fires immediately in test env or after 500ms; force
    // it into flight by using immediate-mode structural updates is not
    // representative, so instead simulate the debounce firing and then queue
    // a second edit while the RPC for the first is in flight.
    store.updateNode(nodeId, { content: 'ab' }, viewerSource);

    // Wait past the 500ms debounce so the first RPC starts (but not past the
    // 20ms RPC latency, so it's still in flight).
    await new Promise((resolve) => setTimeout(resolve, 520));
    expect(store.isNodePersistenceExecuting(nodeId)).toBe(true);

    // A burst of further keystrokes arrives while the RPC is in flight.
    store.updateNode(nodeId, { content: 'abc' }, viewerSource);
    store.updateNode(nodeId, { content: 'abcd' }, viewerSource);
    store.updateNode(nodeId, { content: 'abcde' }, viewerSource);

    await store.flushAllPendingSaves(3000);

    // The in-flight write plus the collapsed latest-wins follow-up write is
    // exactly two RPCs — not one per queued keystroke.
    expect(callCount).toBe(2);
    const lastCall = updateSpy.mock.calls[updateSpy.mock.calls.length - 1];
    expect(lastCall[2].content).toBe('abcde');

    const stored = store.getNode(nodeId);
    expect(stored?.content).toBe('abcde');
    expect(stored?.version).toBe(3);
  }, 10000);

  it('the follow-up write always reads the version confirmed by the prior write — no self-conflict', async () => {
    const nodeId = 'serial-writer-2';
    const initialNode = makeNode(nodeId, 'x', 5);

    const versionsSeen: number[] = [];
    vi.spyOn(backendAdapter, 'updateNode').mockImplementation(async (_id, version, node) => {
      versionsSeen.push(version);
      await new Promise((resolve) => setTimeout(resolve, 20));
      return {
        id: nodeId,
        nodeType: 'text',
        content: String(node.content ?? ''),
        createdAt: '2024-01-01T00:00:00.000Z',
        modifiedAt: new Date().toISOString(),
        version: version + 1,
        properties: {},
        mentions: []
      };
    });

    store.setNode(initialNode, dbSource);

    store.updateNode(nodeId, { content: 'xy' }, viewerSource);
    await new Promise((resolve) => setTimeout(resolve, 520));

    // Second edit queued while first RPC (version 5) is in flight.
    store.updateNode(nodeId, { content: 'xyz' }, viewerSource);

    await store.flushAllPendingSaves(3000);

    // The second RPC must carry the version the first RPC's response
    // confirmed (6), not the stale version read before confirmation (5).
    expect(versionsSeen).toEqual([5, 6]);

    // No OCC conflict notification should have been raised.
    const versionMismatches = conflictNotifications.notifications.filter(
      (n) => n.conflictType === 'version-mismatch'
    );
    expect(versionMismatches).toHaveLength(0);
  }, 10000);
});
