/**
 * Integration tests: SharedNodeStore conflict detection → conflictNotifications store
 *
 * Verifies that when a concurrent-edit or version-mismatch conflict is detected,
 * the UI notification store receives a corresponding entry (Issue #642).
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';
import type { Node } from '../../lib/types';

vi.mock('../../lib/services/tauri-commands', () => ({
  updateNode: vi.fn().mockResolvedValue(undefined),
  createNode: vi.fn().mockResolvedValue(undefined),
  deleteNode: vi.fn().mockResolvedValue(undefined),
  getNode: vi.fn().mockResolvedValue(null),
  getChildren: vi.fn().mockResolvedValue([]),
  getParents: vi.fn().mockResolvedValue([]),
  getRootNodes: vi.fn().mockResolvedValue([]),
  updateTaskNode: vi.fn().mockResolvedValue(undefined)
}));

const makeNode = (id: string): Node => ({
  id,
  nodeType: 'text',
  content: 'Original content',
  createdAt: new Date().toISOString(),
  modifiedAt: new Date().toISOString(),
  version: 1,
  properties: {}
});

describe('SharedNodeStore → conflictNotifications', () => {
  let store: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    store = SharedNodeStore.getInstance();
    conflictNotifications.dismissAll();
    store.setConflictWindow(5000);
  });

  afterEach(() => {
    store.clearAll();
    SharedNodeStore.resetInstance();
    conflictNotifications.dismissAll();
  });

  it('emits a notification when a concurrent-edit conflict is detected', () => {
    const node = makeNode('node-conflict-1');
    const viewer1 = { type: 'viewer' as const, viewerId: 'pane-A' };
    const viewer2 = { type: 'viewer' as const, viewerId: 'pane-B' };

    store.setNode(node, viewer1);

    // First update from pane A — creates a pending update entry
    store.updateNode(node.id, { content: 'Pane A edit' }, viewer1, {
      skipConflictDetection: false,
      skipPersistence: true
    });

    // Second update from pane B on the same field within the conflict window
    store.updateNode(node.id, { content: 'Pane B edit' }, viewer2, {
      skipConflictDetection: false,
      skipPersistence: true
    });

    expect(conflictNotifications.notifications.length).toBeGreaterThanOrEqual(1);
    const n = conflictNotifications.notifications[0];
    expect(n.nodeId).toBe(node.id);
    expect(n.message).toBe('Your edit was overwritten by another pane');
  });

  it('does not emit a notification when conflict detection is skipped', () => {
    const node = makeNode('node-no-conflict');
    const viewer1 = { type: 'viewer' as const, viewerId: 'pane-A' };
    const viewer2 = { type: 'viewer' as const, viewerId: 'pane-B' };

    store.setNode(node, viewer1);
    store.updateNode(node.id, { content: 'Pane A edit' }, viewer1, {
      skipConflictDetection: true,
      skipPersistence: true
    });
    store.updateNode(node.id, { content: 'Pane B edit' }, viewer2, {
      skipConflictDetection: true,
      skipPersistence: true
    });

    expect(conflictNotifications.notifications).toHaveLength(0);
  });

  it('does not emit a notification for updates on different fields', () => {
    const node = makeNode('node-diff-fields');
    const viewer1 = { type: 'viewer' as const, viewerId: 'pane-A' };
    const viewer2 = { type: 'viewer' as const, viewerId: 'pane-B' };

    store.setNode(node, viewer1);
    store.updateNode(node.id, { content: 'New content' }, viewer1, {
      skipConflictDetection: false,
      skipPersistence: true
    });
    store.updateNode(node.id, { properties: { tag: 'work' } }, viewer2, {
      skipConflictDetection: false,
      skipPersistence: true
    });

    expect(conflictNotifications.notifications).toHaveLength(0);
  });

  it('increments conflictCount metric when a notification is emitted', () => {
    const node = makeNode('node-metric');
    const viewer1 = { type: 'viewer' as const, viewerId: 'pane-A' };
    const viewer2 = { type: 'viewer' as const, viewerId: 'pane-B' };

    store.setNode(node, viewer1);

    const metricsBefore = store.getMetrics();

    store.updateNode(node.id, { content: 'Pane A' }, viewer1, {
      skipConflictDetection: false,
      skipPersistence: true
    });
    store.updateNode(node.id, { content: 'Pane B' }, viewer2, {
      skipConflictDetection: false,
      skipPersistence: true
    });

    const metricsAfter = store.getMetrics();

    if (conflictNotifications.notifications.length > 0) {
      expect(metricsAfter.conflictCount).toBeGreaterThan(metricsBefore.conflictCount);
    }
  });
});
