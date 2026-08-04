/**
 * Optimistic collection creation.
 *
 * A newly created collection has zero members, and the collections tree hides
 * empty collections — so a create that waited on a backend round-trip and then
 * reloaded made the new collection invisible on the very refresh meant to
 * reveal it. `createCollection` therefore inserts the entry into local state
 * immediately (exempt from the hide-empty filter), reconciles its temporary id
 * with the backend's real one on success, and rolls it back out on failure.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

import type { CollectionInfo } from '$lib/services/collection-service';

const mockCreateCollection = vi.fn<(name: string, description?: string) => Promise<string>>();
const mockGetAllCollections = vi.fn<() => Promise<CollectionInfo[]>>();
vi.mock('$lib/services/collection-service', () => ({
  collectionService: {
    createCollection: (name: string, description?: string) =>
      mockCreateCollection(name, description),
    getAllCollections: () => mockGetAllCollections()
  }
}));

import { collectionsData } from '$lib/stores/collections.svelte';

function makeCollection(id: string, name: string, memberCount: number): CollectionInfo {
  return {
    id,
    content: name,
    nodeType: 'collection',
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1,
    properties: {},
    memberCount,
    parentCollectionIds: []
  };
}

describe('optimistic collection creation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetAllCollections.mockResolvedValue([]);
    collectionsData.reset();
  });

  it('shows the new collection in the tree before the backend create resolves', async () => {
    let resolveCreate: (id: string) => void = () => {};
    mockCreateCollection.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveCreate = resolve;
      })
    );

    const createPromise = collectionsData.createCollection('Architecture');

    // Visible immediately — before the create call has resolved.
    const treeWhilePending = collectionsData.collectionsTree;
    expect(treeWhilePending.map((c) => c.name)).toEqual(['Architecture']);
    expect(treeWhilePending[0].memberCount).toBe(0);
    expect(treeWhilePending[0].pending).toBe(true);

    resolveCreate('real-id-1');
    await createPromise;
  });

  it('reconciles the temporary id with the backend id on success', async () => {
    mockCreateCollection.mockResolvedValue('real-id-1');

    const id = await collectionsData.createCollection('Architecture');

    expect(id).toBe('real-id-1');
    const tree = collectionsData.collectionsTree;
    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe('real-id-1');
    expect(tree[0].name).toBe('Architecture');
    // No longer pending once confirmed.
    expect(tree[0].pending).toBe(false);
  });

  it('keeps the confirmed collection visible even though it has zero members', async () => {
    mockCreateCollection.mockResolvedValue('real-id-1');
    await collectionsData.createCollection('Architecture');

    // A reload returns the real, still-empty collection. The hide-empty filter
    // must not swallow it — this is the regression the issue reported.
    mockGetAllCollections.mockResolvedValue([makeCollection('real-id-1', 'Architecture', 0)]);
    await collectionsData.loadCollections();

    const tree = collectionsData.collectionsTree;
    expect(tree.map((c) => c.id)).toEqual(['real-id-1']);
  });

  it('still hides empty collections the user did not create in this session', async () => {
    mockGetAllCollections.mockResolvedValue([
      makeCollection('other-empty', 'Someone Elses Empty', 0),
      makeCollection('populated', 'Populated', 3)
    ]);
    await collectionsData.loadCollections();

    expect(collectionsData.collectionsTree.map((c) => c.id)).toEqual(['populated']);
  });

  it('rolls the entry back out of the tree when the create fails', async () => {
    mockCreateCollection.mockRejectedValue(new Error('COLLECTION_EXISTS'));

    const id = await collectionsData.createCollection('Duplicate');

    expect(id).toBeNull();
    expect(collectionsData.collectionsTree).toEqual([]);
    expect(collectionsData.state.error).toContain('COLLECTION_EXISTS');
  });

  it('does not leave a rolled-back collection exempt from the hide-empty filter', async () => {
    mockCreateCollection.mockRejectedValue(new Error('boom'));
    await collectionsData.createCollection('Ghost');

    // The same name later arrives from the backend as a genuinely empty
    // collection; the failed create must not have left a lingering exemption.
    mockGetAllCollections.mockResolvedValue([makeCollection('ghost-id', 'Ghost', 0)]);
    await collectionsData.loadCollections();

    expect(collectionsData.collectionsTree).toEqual([]);
  });

  it('does not drop an unconfirmed entry when a reload races the create', async () => {
    let resolveCreate: (id: string) => void = () => {};
    mockCreateCollection.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveCreate = resolve;
      })
    );

    const createPromise = collectionsData.createCollection('Architecture');

    // A refresh lands mid-flight and the backend does not know about the new
    // collection yet. The optimistic entry must survive it.
    mockGetAllCollections.mockResolvedValue([makeCollection('existing', 'Existing', 2)]);
    await collectionsData.loadCollections();

    expect(collectionsData.collectionsTree.map((c) => c.name)).toEqual(
      expect.arrayContaining(['Architecture', 'Existing'])
    );

    resolveCreate('real-id-1');
    await createPromise;

    const ids = collectionsData.collectionsTree.map((c) => c.id);
    expect(ids).toEqual(expect.arrayContaining(['real-id-1', 'existing']));
    // Exactly one entry for the new collection — no duplicate placeholder.
    expect(ids.filter((id) => id === 'real-id-1')).toHaveLength(1);
    expect(ids.some((id) => id.startsWith('pending-collection-'))).toBe(false);
  });

  it('drops the placeholder rather than duplicating when a reload already brought the real row in', async () => {
    let resolveCreate: (id: string) => void = () => {};
    mockCreateCollection.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveCreate = resolve;
      })
    );

    const createPromise = collectionsData.createCollection('Architecture');

    // The reload sees the backend row before the create call returns its id.
    mockGetAllCollections.mockResolvedValue([makeCollection('real-id-1', 'Architecture', 0)]);
    await collectionsData.loadCollections();

    resolveCreate('real-id-1');
    await createPromise;

    const tree = collectionsData.collectionsTree;
    expect(tree.map((c) => c.id)).toEqual(['real-id-1']);
  });

  it('resets the locally-created exemption on store reset', async () => {
    mockCreateCollection.mockResolvedValue('real-id-1');
    await collectionsData.createCollection('Architecture');

    collectionsData.reset();

    mockGetAllCollections.mockResolvedValue([makeCollection('real-id-1', 'Architecture', 0)]);
    await collectionsData.loadCollections();

    // After a reset (e.g. database switch) the exemption is gone, so the empty
    // collection is filtered like any other.
    expect(collectionsData.collectionsTree).toEqual([]);
  });
});
