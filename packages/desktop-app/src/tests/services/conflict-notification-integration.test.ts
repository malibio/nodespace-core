/**
 * Integration tests: SharedNodeStore OCC conflict handling → conflictNotifications store
 *
 * Verifies that when the daemon returns a VERSION_CONFLICT CommandError,
 * the store rolls back, hydrates from current_node, and surfaces a notification.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import type { Node } from '../../lib/types';

const makeNode = (id: string, version = 1): Node => ({
  id,
  nodeType: 'text',
  content: 'Original content',
  createdAt: '2024-01-01T00:00:00.000Z',
  modifiedAt: '2024-01-01T00:00:00.000Z',
  version,
  properties: {}
});

const makeVersionConflictError = (nodeId: string, currentNode: Node | null = null) => ({
  message: `Version conflict on ${nodeId}: expected 1, got 2`,
  code: 'VERSION_CONFLICT',
  details: 'Aborted',
  conflictData: {
    node_id: nodeId,
    expected: 1,
    actual: 2,
    current_node: currentNode
  }
});

const dbSource = { type: 'database' as const, reason: 'test-load' };
const viewerSource = { type: 'viewer' as const, viewerId: 'pane-A' };

describe('SharedNodeStore → conflictNotifications (OCC)', () => {
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

  it('emits a version-mismatch notification when daemon returns VERSION_CONFLICT', async () => {
    const nodeId = 'node-occ-1';
    const node = makeNode(nodeId, 1);
    const serverNode = makeNode(nodeId, 2);
    serverNode.content = 'Server content';

    vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
      makeVersionConflictError(nodeId, serverNode)
    );

    store.setNode(node, dbSource);
    store.updateNode(nodeId, { content: 'My edit' }, viewerSource);

    await new Promise((resolve) => setTimeout(resolve, 1000));

    expect(conflictNotifications.notifications.length).toBeGreaterThanOrEqual(1);
    const n = conflictNotifications.notifications[0];
    expect(n.nodeId).toBe(nodeId);
    expect(n.conflictType).toBe('version-mismatch');
  }, 5000);

  it('hydrates from current_node when daemon provides it', async () => {
    const nodeId = 'node-occ-2';
    const node = makeNode(nodeId, 1);
    const serverNode = makeNode(nodeId, 2);
    serverNode.content = 'Authoritative server content';

    vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
      makeVersionConflictError(nodeId, serverNode)
    );

    store.setNode(node, dbSource);
    store.updateNode(nodeId, { content: 'My optimistic edit' }, viewerSource);

    await new Promise((resolve) => setTimeout(resolve, 1000));

    const stored = store.getNode(nodeId);
    expect(stored?.content).toBe('Authoritative server content');
    expect(stored?.version).toBe(2);
  }, 5000);

  it('does not emit a notification when skipPersistence is set', () => {
    const nodeId = 'node-no-conflict';
    const node = makeNode(nodeId);

    store.setNode(node, dbSource);
    store.updateNode(nodeId, { content: 'Pane A edit' }, viewerSource, { skipPersistence: true });
    store.updateNode(nodeId, { content: 'Pane B edit' }, { type: 'viewer', viewerId: 'pane-B' }, { skipPersistence: true });

    expect(conflictNotifications.notifications).toHaveLength(0);
  });

  it('increments rollbackCount metric on OCC error', async () => {
    const nodeId = 'node-metric';
    const node = makeNode(nodeId, 1);
    const serverNode = makeNode(nodeId, 2);

    vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
      makeVersionConflictError(nodeId, serverNode)
    );

    store.setNode(node, dbSource);

    const metricsBefore = store.getMetrics();

    store.updateNode(nodeId, { content: 'My edit' }, viewerSource);

    await new Promise((resolve) => setTimeout(resolve, 1000));

    expect(conflictNotifications.notifications.length).toBeGreaterThanOrEqual(1);
    const metricsAfter = store.getMetrics();
    expect(metricsAfter.rollbackCount).toBeGreaterThan(metricsBefore.rollbackCount);
  }, 5000);
});
