/**
 * OCC/subtree-access-denied errors lose their shape before reaching the
 * outer `handle.promise.catch()` (#2080).
 *
 * The real command errors crossing the Tauri/gRPC boundary (VERSION_CONFLICT,
 * SUBTREE_ACCESS_DENIED — see `isVersionConflict`/`isSubtreeAccessDenied` in
 * `$lib/types/errors`) are plain objects, not `instanceof Error`. Every
 * `PersistenceCoordinator.persist()` closure's own `catch (dbError)` wraps
 * whatever it re-throws via `dbError instanceof Error ? dbError : new
 * Error(String(dbError))` — losing `.code`/`.conflictData` entirely — before
 * `throw error`. The OUTER `handle.promise.catch((err) => ...)` then can't
 * classify `err` by shape anymore, so a naive `if (isVersionConflict(err))
 * return;` guard there can never match, regardless of what happened inside
 * the closure.
 *
 * `updateNode()`'s closure DOES raise its own specific `version-mismatch`
 * notification for a genuine OCC conflict — so the outer guard failing to
 * match meant a SECOND, generic `write-failure` notification piled on top
 * of it every time. Fixed the same way #2079 fixed the analogous bug for
 * `deleteNode()`: a boolean captured inside the closure, set right where the
 * specific notification is raised, checked by the outer catch instead of
 * re-deriving the classification from the already-stripped `err`.
 *
 * `setNode()`'s closure, investigated as part of this fix, turns out NOT to
 * have the presumed twin bug: its own `catch (dbError)` has no OCC-specific
 * branch at all — no specific notification is ever raised there for an OCC
 * failure to duplicate. The dead `isVersionConflict(err)` check that used to
 * sit in its outer catch was removed as misleading dead code rather than
 * "fixed" with a captured flag, since there was never a first notification
 * to guard against duplicating. The second describe block below locks in
 * that setNode()'s OCC failures correctly surface exactly one (generic)
 * notification — confirmed here rather than just asserted in a comment.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';
import type { Node } from '../../lib/types';
import type { UpdateSource } from '../../lib/types/update-protocol';

describe('SharedNodeStore — OCC notification dedup (#2080)', () => {
  let store: SharedNodeStore;

  const viewerSource: UpdateSource = { type: 'viewer', viewerId: 'viewer-1' };
  const databaseSource: UpdateSource = { type: 'database', reason: 'seed' };
  // updateNode()'s persistence closure only reaches the actual write for a
  // non-viewer source or an already-persisted node — but its OCC-conflict
  // catch handling under test here doesn't depend on source, so viewerSource
  // is fine for it. setNode()'s persistence closure specifically requires a
  // non-viewer source to reach the UPDATE branch for an already-persisted
  // node (`shouldPersist = source.type !== 'viewer' || isNewNode` —
  // shared-node-store.svelte.ts) — see the setNode block below.
  const mcpSource: UpdateSource = { type: 'mcp-server', serverId: 'test-mcp' };

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

  // Real daemon error shape (plain object, NOT instanceof Error) — same
  // convention every other OCC test in this suite uses (e.g.
  // ai-chat-occ-conflict-regression.test.ts's makeVersionConflictError).
  const makeVersionConflictError = (currentNode: Node | null) => ({
    message: 'Version conflict',
    code: 'VERSION_CONFLICT' as const,
    details: 'Aborted',
    conflictData: {
      node_id: currentNode?.id ?? 'unknown',
      expected: 1,
      actual: 2,
      current_node: currentNode
    }
  });

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

  describe('updateNode()', () => {
    it('raises exactly one notification (version-mismatch) on an OCC conflict, not a duplicate write-failure', async () => {
      store.setNode(makeNode('u-occ-1', 'seed', 1), databaseSource);
      store.setNode(makeNode('u-occ-1', 'seed', 1), databaseSource);

      // Fallback path (no current_node embedded) — exercises
      // resyncNodeFromServer, not the direct-hydration branch, but the
      // notification-dedup bug is in the shared outer catch, not either
      // hydration branch.
      vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
        makeVersionConflictError(null)
      );
      vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(makeNode('u-occ-1', 'server-fresh', 2));

      store.updateNode('u-occ-1', { content: 'user-edit', properties: {} }, viewerSource);
      await new Promise((resolve) => setTimeout(resolve, 100));

      const notifications = conflictNotifications.notifications.filter(
        (n) => n.nodeId === 'u-occ-1'
      );
      expect(notifications).toHaveLength(1);
      expect(notifications[0].conflictType).toBe('version-mismatch');
    });

    it('still raises the generic write-failure notification for a genuine non-OCC failure (regression check)', async () => {
      store.setNode(makeNode('u-occ-2', 'seed', 1), databaseSource);
      store.setNode(makeNode('u-occ-2', 'seed', 1), databaseSource);

      vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
        new Error('network error: connection refused')
      );

      store.updateNode('u-occ-2', { content: 'user-edit', properties: {} }, viewerSource);
      await new Promise((resolve) => setTimeout(resolve, 100));

      const notifications = conflictNotifications.notifications.filter(
        (n) => n.nodeId === 'u-occ-2'
      );
      expect(notifications).toHaveLength(1);
      expect(notifications[0].conflictType).toBe('write-failure');
    });
  });

  describe('setNode()', () => {
    it('raises exactly one (generic) notification for an OCC conflict — no specific notification exists here to duplicate', async () => {
      // mcp-server source so the UPDATE branch runs regardless of isNewNode
      // (see the class-level comment on mcpSource above).
      store.setNode(makeNode('s-occ-1', 'seed', 1), databaseSource);
      store.setNode(makeNode('s-occ-1', 'seed', 1), databaseSource);

      vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
        makeVersionConflictError(null)
      );

      store.setNode(makeNode('s-occ-1', 'external-edit', 1), mcpSource);
      await new Promise((resolve) => setTimeout(resolve, 100));

      const notifications = conflictNotifications.notifications.filter(
        (n) => n.nodeId === 's-occ-1'
      );
      // setNode()'s persistence closure has no OCC-specific branch, so an
      // OCC failure surfaces through the same generic path any other
      // failure does — exactly one notification, not zero (silently
      // dropped) and not two (duplicated).
      expect(notifications).toHaveLength(1);
      expect(notifications[0].conflictType).toBe('write-failure');
    });

    it('still raises exactly one notification for a genuine non-OCC failure (regression check)', async () => {
      store.setNode(makeNode('s-occ-2', 'seed', 1), databaseSource);
      store.setNode(makeNode('s-occ-2', 'seed', 1), databaseSource);

      vi.spyOn(backendAdapter, 'updateNode').mockRejectedValueOnce(
        new Error('network error: connection refused')
      );

      store.setNode(makeNode('s-occ-2', 'external-edit', 1), mcpSource);
      await new Promise((resolve) => setTimeout(resolve, 100));

      const notifications = conflictNotifications.notifications.filter(
        (n) => n.nodeId === 's-occ-2'
      );
      expect(notifications).toHaveLength(1);
      expect(notifications[0].conflictType).toBe('write-failure');
    });
  });
});
