/**
 * Tests for the skip-while-editing guard in SharedNodeStore.setNode().
 *
 * Repro context: nodespace-sync#76 (typing corruption) and #77 (Enter
 * relocates text). Root cause: daemon broadcasts of just-confirmed writes
 * arrive via the WatchNodes gRPC stream while the user is still typing.
 * The unguarded `setNode()` clobbers the optimistic store with the older
 * server-confirmed state.
 *
 * The guard skips the clobber when source.type === 'database' AND the
 * node is actively focused OR has unsaved local changes pending.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SharedNodeStore } from '../../lib/services/shared-node-store.svelte';
import { focusManager } from '../../lib/services/focus-manager.svelte';
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
  });

  afterEach(() => {
    store.clearAll();
    focusManager.clearEditing();
    SharedNodeStore.resetInstance();
  });

  it('skips clobbering the content of a focused node on a database event', () => {
    // User-typed optimistic state
    const optimisticNode = makeNode('n1', 'hello world', 1);
    store.setNode(optimisticNode, viewerSource);

    // User focuses the node (actively editing)
    focusManager.setEditingNode('n1', 'default');

    // Daemon broadcast lands with the older confirmed content
    const stalerNode = makeNode('n1', 'hell', 2);
    store.setNode(stalerNode, databaseSource);

    // Local content stays at the user's optimistic state. Crucially the
    // local node's `.version` is also unchanged — mutating it inside the
    // reactive Map would cause Svelte to re-render and remount the focused
    // textarea (resetting selectionStart → triggers sync#77).
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
    focusManager.setEditingNode('n4', 'default');

    // The user themselves is the source — this is their own typed change.
    const userEdit = makeNode('n4', 'after', 1);
    store.setNode(userEdit, viewerSource);

    expect(store.getNode('n4')?.content).toBe('after');
  });

  it('does apply database events when the node has never been seen locally', () => {
    // First time the local store sees this node — the guard's "existingNode"
    // check ensures we still accept the new state.
    focusManager.setEditingNode('n5', 'default'); // even with focus on the id
    const incoming = makeNode('n5', 'fresh from cloud', 1);
    store.setNode(incoming, databaseSource);

    expect(store.getNode('n5')?.content).toBe('fresh from cloud');
  });

  it('preserves the optimistic content AND leaves local version untouched (no reactive mutation)', () => {
    const optimistic = makeNode('n6', 'local-newer', 3);
    store.setNode(optimistic, viewerSource);
    focusManager.setEditingNode('n6', 'default');

    const broadcast = makeNode('n6', 'cloud-older', 7);
    store.setNode(broadcast, databaseSource);

    const after = store.getNode('n6');
    expect(after?.content).toBe('local-newer'); // content preserved
    expect(after?.version).toBe(3); // local version NOT touched
    // The server-confirmed version (7) is stashed in a non-reactive cache
    // and consumed by the persistence path; not observable via getNode.
  });

  it('clears the server-confirmed-version cache when a non-guarded setNode applies (no shadowing)', () => {
    // Seed via the database path so no persistence is scheduled — we want
    // to exercise just the focus/clear-cache flow without a pending op
    // tripping the guard.
    store.setNode(makeNode('n7', 'seed', 3), databaseSource);

    // Focus the node so the next database event hits the guard and stashes
    // its version in the non-reactive cache.
    focusManager.setEditingNode('n7', 'default');
    store.setNode(makeNode('n7', 'older', 9), databaseSource);
    // Verify the local view didn't change (guard fired).
    expect(store.getNode('n7')?.content).toBe('seed');
    expect(store.getNode('n7')?.version).toBe(3);

    // User blurs. Subsequent database event no longer matches the guard;
    // the normal setNode path runs, writes the new state, and MUST clear
    // the cache so a stale stashed version can't shadow it on the next
    // persistence.
    focusManager.clearEditing();
    store.setNode(makeNode('n7', 'cloud-current', 15), databaseSource);

    const after = store.getNode('n7');
    expect(after?.content).toBe('cloud-current');
    expect(after?.version).toBe(15);
  });
});
