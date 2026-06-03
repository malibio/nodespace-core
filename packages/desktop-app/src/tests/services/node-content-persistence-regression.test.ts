/**
 * Regression tests for issue #1307: node content not persisting to database.
 *
 * Covers the write → flush → read-back cycle so that edits made in the Tauri
 * desktop app reliably reach the backend.
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

describe('Node content persistence regression (#1307)', () => {
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

  it('write → flush → read-back: content written via updateNode reaches the backend', async () => {
    const nodeId = 'persist-regression-1';
    const initialNode = makeNode(nodeId, 'Original content', 1);
    const updatedContent = 'Edited content that must persist';

    // Simulate the backend storing the update and returning an incremented version
    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockResolvedValueOnce({
      ...initialNode,
      content: updatedContent,
      version: 2
    });

    // Load the node as if the daemon pushed it
    store.setNode(initialNode, dbSource);

    // User edits the node in the viewer
    store.updateNode(nodeId, { content: updatedContent }, viewerSource);

    // Wait for the debounced persistence to flush
    await store.flushAllPendingSaves(3000);

    // The backend must have received the correct content
    expect(updateSpy).toHaveBeenCalledOnce();
    const [calledNodeId, , calledNode] = updateSpy.mock.calls[0];
    expect(calledNodeId).toBe(nodeId);
    expect(calledNode.content).toBe(updatedContent);

    // The in-memory store must reflect the backend-confirmed version
    const stored = store.getNode(nodeId);
    expect(stored?.content).toBe(updatedContent);
    expect(stored?.version).toBe(2);
  }, 5000);

  it('write → flush → read-back: content persists across multiple sequential edits', async () => {
    const nodeId = 'persist-regression-2';
    const initialNode = makeNode(nodeId, 'Start', 1);

    let callCount = 0;
    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockImplementation(async (_id, _v, node) => {
      callCount++;
      return {
        id: nodeId,
        nodeType: node.nodeType ?? 'text',
        content: node.content ?? '',
        createdAt: '2024-01-01T00:00:00.000Z',
        modifiedAt: new Date().toISOString(),
        version: 1 + callCount,
        properties: node.properties ?? {}
      };
    });

    store.setNode(initialNode, dbSource);

    store.updateNode(nodeId, { content: 'Edit 1' }, viewerSource);
    store.updateNode(nodeId, { content: 'Edit 2' }, viewerSource);
    store.updateNode(nodeId, { content: 'Edit 3 — final' }, viewerSource);

    await store.flushAllPendingSaves(3000);

    // PersistenceCoordinator debounces and coalesces rapid edits into a single
    // backend call. The exact call count is non-deterministic (could be 1 or 3
    // depending on timer resolution), so we assert only that it was called at
    // all and that the final write carried the last content.
    expect(updateSpy).toHaveBeenCalled();
    const lastCall = updateSpy.mock.calls[updateSpy.mock.calls.length - 1];
    expect(lastCall[2].content).toBe('Edit 3 — final');
  }, 5000);

  it('write → flush: a new node created via setNode is persisted via createNode', async () => {
    const nodeId = 'persist-regression-3';
    const newNode = makeNode(nodeId, 'Brand new node content', 1);

    // createNode returns the node ID string, not a Node object
    const createSpy = vi.spyOn(backendAdapter, 'createNode').mockResolvedValueOnce(nodeId);

    // New node arrives from viewer (not yet in DB)
    store.setNode(newNode, viewerSource);

    await store.flushAllPendingSaves(3000);

    expect(createSpy).toHaveBeenCalledOnce();
    const [createdInput] = createSpy.mock.calls[0];
    expect((createdInput as { id?: string }).id).toBe(nodeId);
    expect((createdInput as { content?: string }).content).toBe('Brand new node content');
  }, 5000);

  it('write failure surfaces a write-failure conflict notification', async () => {
    const nodeId = 'persist-regression-4';
    const initialNode = makeNode(nodeId, 'Content', 1);

    vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
      new Error('Network error: daemon unreachable')
    );

    store.setNode(initialNode, dbSource);
    store.updateNode(nodeId, { content: 'Edit that will fail' }, viewerSource);

    await store.flushAllPendingSaves(3000);

    const notifications = conflictNotifications.notifications;
    expect(notifications.length).toBeGreaterThanOrEqual(1);
    const writeFailure = notifications.find((n) => n.conflictType === 'write-failure');
    expect(writeFailure).toBeDefined();
    expect(writeFailure?.nodeId).toBe(nodeId);
  }, 5000);

  it('database broadcast does not clobber an actively-edited node', async () => {
    const nodeId = 'persist-regression-5';
    const initialNode = makeNode(nodeId, 'Server state', 1);
    const userTyped = 'User is typing this right now';

    vi.spyOn(backendAdapter, 'updateNode').mockResolvedValue({
      ...initialNode,
      version: 2
    });

    // Load initial node
    store.setNode(initialNode, dbSource);

    // User starts editing (makes the node "focused" for OCC purposes via pending op)
    store.updateNode(nodeId, { content: userTyped }, viewerSource);

    // Daemon echoes back an older server snapshot while the user is still typing
    const olderServerSnapshot = makeNode(nodeId, 'Older server snapshot', 1);
    store.setNode(olderServerSnapshot, dbSource);

    // The in-memory store must still hold the user's latest content
    expect(store.getNode(nodeId)?.content).toBe(userTyped);

    await store.flushAllPendingSaves(3000);
  }, 5000);
});
