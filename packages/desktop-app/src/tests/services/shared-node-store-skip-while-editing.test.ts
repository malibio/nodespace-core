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
import { structureTree } from '../../lib/stores/reactive-structure-tree.svelte';
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

  it('does NOT stash the server-confirmed version when the broadcast looks like a foreign write (preserves OCC)', () => {
    // The guard's whole point is to avoid clobbering local optimistic
    // content with the server-confirmed snapshot. But the broadcast
    // mechanism doesn't distinguish "my-own-write echoing back" from
    // "another client's write to the same node arriving via cloud
    // round-trip". For the latter case, blindly stashing the foreign
    // writer's version would defeat OCC: alice's next UpdateNode would
    // carry bob's version against alice's content and the backend would
    // silently overwrite bob's change.
    //
    // The "is plausibly an own echo" heuristic resolves this: stash only
    // when `incoming.content` matches this client's recorded last-sent
    // content for the node. For any other broadcast (including the
    // alice="hello world" + bob="hello" prefix-compatible race the
    // earlier startsWith heuristic mis-classified), treat as foreign.
    const optimistic = makeNode('foreign', 'alice typed this', 3);
    store.setNode(optimistic, viewerSource);
    focusManager.setEditingNode('foreign', 'default');
    // Alice last persisted "alice typed this" to backend.
    store.__test_setLastPersistedContent('foreign', 'alice typed this');

    // Foreign-looking broadcast (different writer altered the node).
    const foreignBroadcast = makeNode('foreign', 'bob wrote something else', 9);
    store.setNode(foreignBroadcast, databaseSource);

    // Local content is preserved (guard still fired — we keep optimistic).
    expect(store.getNode('foreign')?.content).toBe('alice typed this');
    expect(store.getNode('foreign')?.version).toBe(3);
    // Foreign version was NOT stashed: next UpdateNode uses local v3,
    // backend has v9, OCC conflict surfaces.
    expect(store.peekServerConfirmedVersion('foreign')).toBeUndefined();
  });

  it('does NOT mis-classify a prefix-compatible foreign write as an own-echo (regression for the startsWith hole)', () => {
    // Previously the heuristic was `localContent.startsWith(incomingContent)`.
    // That would mis-classify the following case as own-echo and stash
    // bob's version, defeating OCC for bob's change:
    //
    //   alice's optimistic state: "hello world"
    //   bob writes (in a parallel window): "hello"
    //   bob's broadcast hits alice with content="hello", version=v_bob
    //
    // `"hello world".startsWith("hello")` is true, so the old code
    // stashed v_bob. Next UpdateNode from alice would have carried
    // v_bob against "hello world" and silently overwritten bob.
    //
    // The current heuristic compares against this client's recorded
    // last-sent content. Alice never sent "hello" — her last persisted
    // value (if any) is whatever she actually persisted. So bob's
    // broadcast is correctly classified as foreign.
    const aliceOptimistic = makeNode('shared', 'hello world', 5);
    store.setNode(aliceOptimistic, viewerSource);
    focusManager.setEditingNode('shared', 'default');
    // Alice has never persisted "hello" — only "hello world" via her
    // own typing. (For the regression check the exact prior value
    // doesn't matter — what matters is that it ISN'T "hello".)
    store.__test_setLastPersistedContent('shared', 'hello world');

    const bobsBroadcast = makeNode('shared', 'hello', 7);
    store.setNode(bobsBroadcast, databaseSource);

    // Bob's version must NOT be stashed.
    expect(store.peekServerConfirmedVersion('shared')).toBeUndefined();
    // Alice's content and version are preserved (no clobber).
    expect(store.getNode('shared')?.content).toBe('hello world');
    expect(store.getNode('shared')?.version).toBe(5);
  });

  it('persistence path uses the stashed server-confirmed version for OCC (not the stale local node.version)', () => {
    // Contract test for the cache's read site. The actual persistence
    // closure inside SharedNodeStore calls `computeOccVersionForUpdate`,
    // which routes through the same `serverConfirmedVersions` cache the
    // skip-while-editing guard populates. Without this test, a future
    // refactor that drops that read would silently reintroduce the
    // OCC-defeat from nodespace-sync#76 — the unit tests above only
    // assert cache *population*, not *consumption*.
    store.setNode(makeNode('persist', 'abc', 1), databaseSource);
    // Simulate that the local client just persisted 'abc' (own echo
    // gate). Without this, the guard correctly treats the next
    // broadcast as foreign and does NOT stash the version.
    store.__test_setLastPersistedContent('persist', 'abc');

    // Before the guard fires, the OCC version is the local one.
    expect(store.computeOccVersionForUpdate('persist')).toBe(1);

    // Guard fires for an own-echo broadcast (content matches what we sent).
    focusManager.setEditingNode('persist', 'default');
    store.setNode(makeNode('persist', 'abc', 5), databaseSource);

    // Local .version unchanged, but the persistence path now picks 5.
    expect(store.getNode('persist')?.version).toBe(1);
    expect(store.computeOccVersionForUpdate('persist')).toBe(5);
  });

  it('preserves insertAfterNodeId when sibling.parentId is null but structureTree agrees on parent (sync#77)', () => {
    // The persistence-time stale-sibling check used to compare the bare
    // `Node.parentId` field. Nodes loaded via `getChildrenTree` come back
    // with `parentId: null` on the wire, so every Enter-key insertion
    // looked "stale" and the hint got cleared — the backend then
    // defaulted to "insert at beginning". The fix consults `structureTree`
    // (the authoritative source for hierarchy via has_child edges) so the
    // hint survives whenever the tree confirms the same parent.
    //
    // Tests the decision via `shouldClearStaleInsertAfter`, the helper
    // the persistence closure calls — locks in the production code path
    // without mocking the Tauri-side IPC.
    structureTree.clear();
    const existingA = {
      ...makeNode('a', 'existing', 1),
      parentId: null as string | null
    };
    store.setNode(existingA, databaseSource);
    structureTree.addChild({ parentId: 'D', childId: 'a', order: 1 });

    // Sibling 'a' has `parentId: null` on the node object but
    // structureTree says its parent is 'D'. New node 'b' is being
    // inserted with parentId='D'. The hint must be preserved.
    expect(store.shouldClearStaleInsertAfter('a', 'D')).toBe(false);

    // Sanity: if structureTree disagrees, the hint IS cleared.
    expect(store.shouldClearStaleInsertAfter('a', 'OTHER_PARENT')).toBe(true);

    // Sanity: if structureTree has no opinion, the hint is preserved
    // (backend retry loop will handle it).
    expect(store.shouldClearStaleInsertAfter('unknown', 'D')).toBe(false);

    structureTree.clear();
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
