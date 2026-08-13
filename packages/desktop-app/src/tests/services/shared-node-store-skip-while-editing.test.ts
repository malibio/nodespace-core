/**
 * Tests for the skip-while-editing guard in SharedNodeStore.setNode().
 *
 * Repro context: typing corruption and Enter relocating text. Root cause:
 * daemon broadcasts of just-confirmed writes
 * arrive via the WatchNodes gRPC stream while the user is still typing.
 * The unguarded `setNode()` clobbers the optimistic store with the older
 * server-confirmed state.
 *
 * The guard skips the clobber when source.type === 'database' AND the
 * node is actively focused OR has unsaved local changes pending.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { focusManager } from '../../lib/services/focus-manager.svelte';
import { structureTree } from '../../lib/stores/reactive-structure-tree.svelte';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';
import { backendAdapter } from '../../lib/services/backend-adapter';
import type { Node } from '../../lib/types';
import type { UpdateSource } from '../../lib/types/update-protocol';

describe('SharedNodeStore — skip-while-editing guard', () => {
  let store: SharedNodeStore;

  const makeNode = (id: string, content: string, version = 1): Node => ({
    id,
    nodeType: 'text',
    content,
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version,
    properties: {},
    mentions: []
  });

  const viewerSource: UpdateSource = {
    type: 'viewer',
    viewerId: 'viewer-1'
  };

  const databaseSource: UpdateSource = {
    type: 'database',
    reason: 'domain-event'
  };

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    store = SharedNodeStore.getInstance();
    focusManager.clearEditing();
    conflictNotifications.dismissAll();
  });

  afterEach(() => {
    store.clearAll();
    focusManager.clearEditing();
    SharedNodeStore.resetInstance();
    vi.restoreAllMocks();
  });

  it('skips clobbering the content of a focused node on a database event', () => {
    // User-typed optimistic state
    const optimisticNode = makeNode('n1', 'hello world', 1);
    store.setNode(optimisticNode, viewerSource);

    // User focuses the node (actively editing)
    focusManager.focusNode('n1', 'default');

    // Daemon broadcast lands with the older confirmed content
    const stalerNode = makeNode('n1', 'hell', 2);
    store.setNode(stalerNode, databaseSource);

    // Local content stays at the user's optimistic state. Crucially the
    // local node's `.version` is also unchanged — mutating it inside the
    // reactive Map would cause Svelte to re-render and remount the focused
    // textarea (resetting selectionStart → triggers the text-relocation bug).
    const after = store.getNode('n1');
    expect(after?.content).toBe('hello world');
    expect(after?.version).toBe(1);
  });

  it('skips clobbering when the node has pending persistence even if unfocused', () => {
    // Plant a node and put it into the user-edit path so the persistence
    // coordinator is engaged with a debounced write for it.
    const initial = makeNode('n2', 'initial', 1);
    store.setNode(initial, viewerSource);

    // Trigger a viewer-side update that schedules a debounced persist.
    // Persistence stays in the "pending" bucket because we don't flush.
    store.updateNode('n2', { content: 'user-typing-this' }, viewerSource);

    // Daemon broadcast lands with the older confirmed content.
    const stalerNode = makeNode('n2', 'initial', 2);
    store.setNode(stalerNode, databaseSource);

    // Optimistic content survives the broadcast; local .version untouched.
    const after = store.getNode('n2');
    expect(after?.content).toBe('user-typing-this');
    expect(after?.version).toBe(1);
  });

  it('does apply database events for non-focused, non-dirty nodes (regression check)', () => {
    // Seed via the database path so no persistence is scheduled — mirrors
    // the production case where the node arrived from the daemon and the
    // user hasn't touched it yet.
    const initial = makeNode('n3', 'before', 1);
    store.setNode(initial, databaseSource);

    // No focus, no pending writes → genuine remote update should land.
    const remoteUpdate = makeNode('n3', 'after', 5);
    store.setNode(remoteUpdate, databaseSource);

    expect(store.getNode('n3')?.content).toBe('after');
    expect(store.getNode('n3')?.version).toBe(5);
  });

  it('does apply viewer-source updates to a focused node (user actions are authoritative)', () => {
    const initial = makeNode('n4', 'before', 1);
    store.setNode(initial, viewerSource);
    focusManager.focusNode('n4', 'default');

    // The user themselves is the source — this is their own typed change.
    const userEdit = makeNode('n4', 'after', 1);
    store.setNode(userEdit, viewerSource);

    expect(store.getNode('n4')?.content).toBe('after');
  });

  it('does apply database events when the node has never been seen locally', () => {
    // First time the local store sees this node — the guard's "existingNode"
    // check ensures we still accept the new state.
    focusManager.focusNode('n5', 'default'); // even with focus on the id
    const incoming = makeNode('n5', 'fresh from cloud', 1);
    store.setNode(incoming, databaseSource);

    expect(store.getNode('n5')?.content).toBe('fresh from cloud');
  });

  it('preserves the optimistic content AND leaves local version untouched (no reactive mutation)', () => {
    const optimistic = makeNode('n6', 'local-newer', 3);
    store.setNode(optimistic, viewerSource);
    focusManager.focusNode('n6', 'default');

    const broadcast = makeNode('n6', 'cloud-older', 7);
    store.setNode(broadcast, databaseSource);

    const after = store.getNode('n6');
    expect(after?.content).toBe('local-newer'); // content preserved
    expect(after?.version).toBe(3); // local version NOT touched
  });

  it('never stashes a broadcast version, preserving OCC for every database-sourced event (ADR-026 C5 extension)', () => {
    // Before the ADR-026 C5 extension, the guard tried to distinguish "my own write
    // echoing back" from "a different client's write" by comparing the
    // broadcast's content against what this client last sent, and stashed
    // the broadcast version only for a plausible own-echo. The daemon now
    // suppresses a connection's own write echoes before they ever reach
    // WatchNodes (`packages/daemon/src/services/node_service.rs`), so every
    // database-sourced broadcast the frontend receives is guaranteed
    // foreign — there is no case left where stashing a broadcast version is
    // safe, and the guard never does so. The next UpdateNode always carries
    // this client's own local version, so a real conflict surfaces via OCC
    // instead of silently overwriting the foreign write.
    const optimistic = makeNode('foreign', 'alice typed this', 3);
    store.setNode(optimistic, viewerSource);
    focusManager.focusNode('foreign', 'default');

    const foreignBroadcast = makeNode('foreign', 'bob wrote something else', 9);
    store.setNode(foreignBroadcast, databaseSource);

    // Local content is preserved (guard still fired — we keep optimistic).
    expect(store.getNode('foreign')?.content).toBe('alice typed this');
    expect(store.getNode('foreign')?.version).toBe(3);
    // The next UpdateNode still uses the local version — an OCC conflict
    // against the backend's v9 will surface it rather than silently losing it.
    expect(store.computeOccVersionForUpdate('foreign')).toBe(3);
  });

  it('preserves insertAfterNodeId when structureTree agrees on parent (sync#77)', () => {
    // The persistence-time stale-sibling check consults `structureTree`
    // (the authoritative source for hierarchy via has_child edges) so the
    // hint survives whenever the tree confirms the same parent.
    // Node.parentId no longer exists — structureTree is the only source.
    //
    // Tests the decision via `shouldClearStaleInsertAfter`, the helper
    // the persistence closure calls — locks in the production code path
    // without mocking the Tauri-side IPC.
    structureTree.clear();
    const existingA = makeNode('a', 'existing', 1);
    store.setNode(existingA, databaseSource);
    structureTree.addChild({ parentId: 'D', childId: 'a', order: 1 });

    // structureTree says sibling 'a' is under 'D'. New node 'b' is being
    // inserted with currentParentId='D'. The hint must be preserved.
    expect(store.shouldClearStaleInsertAfter('a', 'D')).toBe(false);

    // Sanity: if structureTree disagrees, the hint IS cleared.
    expect(store.shouldClearStaleInsertAfter('a', 'OTHER_PARENT')).toBe(true);

    // Sanity: if structureTree has no opinion, the hint is preserved
    // (backend retry loop will handle it).
    expect(store.shouldClearStaleInsertAfter('unknown', 'D')).toBe(false);

    structureTree.clear();
  });

  it('applies the next database event normally once the user blurs (guard only fires while actively editing)', () => {
    store.setNode(makeNode('n7', 'seed', 3), databaseSource);

    // Focus the node so the next database event hits the guard.
    focusManager.focusNode('n7', 'default');
    store.setNode(makeNode('n7', 'older', 9), databaseSource);
    // Verify the local view didn't change (guard fired).
    expect(store.getNode('n7')?.content).toBe('seed');
    expect(store.getNode('n7')?.version).toBe(3);

    // User blurs. Subsequent database event no longer matches the guard;
    // the normal setNode path runs and writes the new state.
    focusManager.clearEditing();
    store.setNode(makeNode('n7', 'cloud-current', 15), databaseSource);

    const after = store.getNode('n7');
    expect(after?.content).toBe('cloud-current');
    expect(after?.version).toBe(15);
  });

  // batchSetNodes (used by doLoadChildrenTree with a database source)
  // must apply the SAME skip-while-editing guard as setNode — a concurrent tree
  // reload would otherwise overwrite a child being edited mid-keystroke.
  describe('batchSetNodes guard (#1436)', () => {
    it('skips clobbering a focused node in a database-source batch', () => {
      const optimistic = makeNode('b1', 'hello world', 1);
      store.setNode(optimistic, viewerSource);
      focusManager.focusNode('b1', 'default');

      // A tree (re)load batch lands with the older confirmed snapshot.
      store.batchSetNodes([makeNode('b1', 'hell', 2)], databaseSource);

      const after = store.getNode('b1');
      expect(after?.content).toBe('hello world');
      expect(after?.version).toBe(1);
    });

    it('skips clobbering a node with pending persistence in a batch even if unfocused', () => {
      store.setNode(makeNode('b2', 'initial', 1), viewerSource);
      store.updateNode('b2', { content: 'user-typing-this' }, viewerSource);

      store.batchSetNodes([makeNode('b2', 'initial', 2)], databaseSource);

      const after = store.getNode('b2');
      expect(after?.content).toBe('user-typing-this');
      expect(after?.version).toBe(1);
    });

    it('applies database-source batch updates for non-focused, non-dirty nodes (regression)', () => {
      store.setNode(makeNode('b3', 'before', 1), databaseSource);

      store.batchSetNodes([makeNode('b3', 'after', 5)], databaseSource);

      expect(store.getNode('b3')?.content).toBe('after');
      expect(store.getNode('b3')?.version).toBe(5);
    });

    it('guards only the edited node, applying the rest of the batch', () => {
      // b4 is being edited; b5 is untouched. One batch reload touches both.
      store.setNode(makeNode('b4', 'editing-this', 1), viewerSource);
      focusManager.focusNode('b4', 'default');
      store.setNode(makeNode('b5', 'old', 1), databaseSource);

      store.batchSetNodes(
        [makeNode('b4', 'stale', 2), makeNode('b5', 'new', 7)],
        databaseSource
      );

      // b4 preserved (guard fired), b5 applied (no guard).
      expect(store.getNode('b4')?.content).toBe('editing-this');
      expect(store.getNode('b4')?.version).toBe(1);
      expect(store.getNode('b5')?.content).toBe('new');
      expect(store.getNode('b5')?.version).toBe(7);
    });
  });

  // A foreign write to an actively-edited node is skipped to protect the
  // optimistic text — but it must NOT be silent. Surface a version-mismatch
  // conflict notification so the user knows another writer changed the node.
  describe('foreign-write conflict signal (#1437)', () => {
    const versionMismatchFor = (nodeId: string) =>
      conflictNotifications.notifications.filter(
        (n) => n.nodeId === nodeId && n.conflictType === 'version-mismatch'
      );

    it('raises a version-mismatch notification when a foreign broadcast hits a focused node', () => {
      store.setNode(makeNode('shared', 'hello world', 5), viewerSource);
      focusManager.focusNode('shared', 'default');

      // Bob's foreign write.
      store.setNode(makeNode('shared', 'totally different text', 7), databaseSource);

      expect(versionMismatchFor('shared')).toHaveLength(1);
      // Optimistic content still protected (the clobber was skipped).
      expect(store.getNode('shared')?.content).toBe('hello world');
    });

    it('dedupes repeated foreign broadcasts for the same node', () => {
      store.setNode(makeNode('dup', 'hello world', 5), viewerSource);
      focusManager.focusNode('dup', 'default');

      store.setNode(makeNode('dup', 'foreign one', 7), databaseSource);
      store.setNode(makeNode('dup', 'foreign two', 8), databaseSource);

      expect(versionMismatchFor('dup')).toHaveLength(1);
    });

    // ADR-026's C5 extension: the daemon suppresses a connection's own write echoes
    // before they ever reach WatchNodes, so the frontend never receives a
    // database-sourced broadcast that is its own echo — there is no "own
    // echo, don't notify" case left to test here. Every database-sourced
    // broadcast reaching an actively-edited node is a genuine foreign write
    // and always raises the conflict notification, including one whose
    // content happens to be identical to what the client last saw (the old
    // content-comparison heuristic would have misclassified this as an echo
    // — the daemon's authoritative signal does not need content at all).
    it('still notifies even when the broadcast content is identical to what was last seen (no content-based echo guessing)', () => {
      store.setNode(makeNode('hydrated', 'hello world', 5), databaseSource);
      focusManager.focusNode('hydrated', 'default');

      store.setNode(makeNode('hydrated', 'hello world', 6), databaseSource);

      expect(versionMismatchFor('hydrated')).toHaveLength(1);
    });

    it('notifies on a genuine foreign write to a focused node', () => {
      store.setNode(makeNode('hydrated2', 'hello world', 5), databaseSource);
      focusManager.focusNode('hydrated2', 'default');

      store.setNode(makeNode('hydrated2', 'a foreign edit', 7), databaseSource);

      expect(versionMismatchFor('hydrated2')).toHaveLength(1);
      // The local content is still protected (the clobber was skipped).
      expect(store.getNode('hydrated2')?.content).toBe('hello world');
    });
  });

  // resyncNodeFromServer() writes the fetched row via the same guard-free
  // path a `database`-sourced setNode() would, but historically bypassed
  // the skip-while-editing guard entirely (it calls the private `nodesSet`
  // directly). It's used both by the pre-existing OCC-conflict fallback and
  // by the non-OCC write-failure recovery path (core#1985 follow-up) — the
  // latter fires on ordinary transient failures (network blips, timeouts),
  // far more common than a genuine version conflict, so an unguarded resync
  // there would routinely revert a user's in-progress typing to stale
  // server content on nothing more than a brief network hiccup.
  //
  // Guards on `isFocused` only — deliberately NOT `hasPending`, unlike
  // `setNode()`'s use of the same underlying policy. `PersistenceCoordinator
  // .hasPending()` is true while an operation is executing, and there is
  // only one executing slot per node: when resyncNodeFromServer is called
  // from a failed write's own catch handler, that very write still occupies
  // it (its `finally` hasn't run yet) — so `hasPending` at that moment is
  // this failing write's own not-yet-cleared bookkeeping, not evidence of a
  // genuinely different pending edit. That's self-referential and racy
  // (depends on mock-vs-real network timing), and checking it would defeat
  // this recovery path for its single most common case: an isolated
  // failure with nothing else in flight. `isFocused` has no such
  // self-reference and covers the scenario the guard exists for.
  describe('resyncNodeFromServer guard', () => {
    it("does not clobber a focused node's optimistic content with a stale server snapshot", async () => {
      store.setNode(makeNode('resync-1', 'server-old', 1), databaseSource);
      focusManager.focusNode('resync-1', 'default');
      // User keeps typing locally (optimistic, not yet confirmed).
      store.updateNode('resync-1', { content: 'user is typing this' }, viewerSource, {
        skipPersistence: true
      });

      vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(makeNode('resync-1', 'server-old', 1));

      await store.resyncNodeFromServer('resync-1');

      const after = store.getNode('resync-1');
      expect(after?.content).toBe('user is typing this');
    });

    // Documents the scope boundary above, rather than asserting protection
    // that (deliberately) doesn't exist here: an unfocused node with an
    // unrelated pending debounced write is NOT shielded from a resync by
    // this guard — only `setNode()` gets that (via the real, non-racy
    // `hasPending` check, safe there because setNode is never called from
    // inside the very operation it might be racing against).
    it('does NOT protect a merely-pending (not focused) node — known scope limit, see guard comment above', async () => {
      store.setNode(makeNode('resync-2', 'server-old', 1), databaseSource);
      store.updateNode('resync-2', { content: 'debounced-edit-in-flight' }, viewerSource);

      vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(makeNode('resync-2', 'server-fresh', 5));

      await store.resyncNodeFromServer('resync-2');

      expect(store.getNode('resync-2')?.content).toBe('server-fresh');
    });

    it('still resyncs normally for a non-focused, non-pending node (regression check)', async () => {
      store.setNode(makeNode('resync-3', 'local-stale', 1), databaseSource);

      vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(makeNode('resync-3', 'server-fresh', 9));

      await store.resyncNodeFromServer('resync-3');

      const after = store.getNode('resync-3');
      expect(after?.content).toBe('server-fresh');
      expect(after?.version).toBe(9);
    });

    it('applies normally once the user blurs (guard only fires while actively editing)', async () => {
      store.setNode(makeNode('resync-4', 'server-old', 1), databaseSource);
      focusManager.focusNode('resync-4', 'default');

      vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(makeNode('resync-4', 'server-fresh', 4));

      // Still focused — the resync must be skipped.
      await store.resyncNodeFromServer('resync-4');
      expect(store.getNode('resync-4')?.content).toBe('server-old');

      // User blurs — the same resync call now applies normally.
      focusManager.clearEditing();
      await store.resyncNodeFromServer('resync-4');
      expect(store.getNode('resync-4')?.content).toBe('server-fresh');
    });
  });
});
