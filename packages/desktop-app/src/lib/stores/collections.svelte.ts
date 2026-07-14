import { collectionService, type CollectionInfo } from '$lib/services/collection-service';
import type { Node } from '$lib/types';
import { createLogger } from '$lib/utils/logger';
import { onDaemonReconnect } from '$lib/services/daemon-status';
import { stripMarkdown } from '$lib/services/markdown-utils';

const log = createLogger('CollectionsStore');

/**
 * Collections Store
 *
 * Manages the state for the Collections browser in the navigation sidebar.
 * Collections are AI-managed groupings of related nodes that span across the hierarchy.
 *
 * ## Architecture
 *
 * - Collections are flat (globally unique names, no parent-child hierarchy)
 * - Paths like "hr:policy:vacation" are navigation conventions, not structure
 * - Nodes can belong to multiple collections (many-to-many via member_of edges)
 *
 * ## Data Flow
 *
 * 1. `collectionsData` - Reactive store with all collections from backend
 * 2. `collectionsState` - UI state (selection, sub-panel, expanded items)
 * 3. Computed getters for selected collection and its members
 *
 * Svelte 5 rune store (ADR-049): state lives on the classes as `$state`;
 * `collectionsTree` / `selectedCollection` / `selectedCollectionMembers` are
 * computed getters, not `derived` stores.
 */

// ============================================================================
// Types
// ============================================================================

export interface CollectionItem {
  id: string;
  name: string;
  memberCount: number;
  children?: CollectionItem[];
}

export interface CollectionMember {
  id: string;
  name: string;
  nodeType: string;
}

export interface CollectionsState {
  /** Currently selected collection ID (for sub-panel display) */
  selectedCollectionId: string | null;
  /** Whether the sub-panel is open */
  subPanelOpen: boolean;
  /** Set of expanded collection IDs (for nested tree state) */
  expandedCollectionIds: Set<string>;
}

// ============================================================================
// Collections Data Store (from backend)
// ============================================================================

interface CollectionsDataState {
  collections: CollectionInfo[];
  /** Collection members - full Node data fetched in single query */
  members: Map<string, Node[]>;
  loading: boolean;
  error: string | null;
}

const initialDataState: CollectionsDataState = {
  collections: [],
  members: new Map(),
  loading: false,
  error: null,
};

class CollectionsDataStore {
  state = $state<CollectionsDataState>({
    ...initialDataState,
    members: new Map(),
  });

  /**
   * Transform flat collections into tree structure for UI display.
   * Uses parentCollectionIds to build proper hierarchy. Hides collections with
   * no member nodes visible to the current user.
   */
  get collectionsTree(): CollectionItem[] {
    return buildCollectionsTree(this.state.collections);
  }

  /** Load all collections from backend */
  async loadCollections(): Promise<void> {
    this.state = { ...this.state, loading: true, error: null };

    try {
      const collections = await collectionService.getAllCollections();
      log.debug('Loaded collections', { count: collections.length });
      this.state = { ...this.state, collections, loading: false };
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load collections';
      log.error('Failed to load collections', { error: message });
      this.state = { ...this.state, loading: false, error: message };
    }
  }

  /** Load members for a specific collection (single query for full Node data) */
  async loadMembers(collectionId: string): Promise<void> {
    try {
      const members = await collectionService.getCollectionMembers(collectionId);
      log.debug('Loaded collection members', { collectionId, count: members.length });

      const newMembers = new Map(this.state.members);
      newMembers.set(collectionId, members);
      this.state = { ...this.state, members: newMembers };
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to load members';
      log.error('Failed to load collection members', { collectionId, error: message });
    }
  }

  /** Invalidate cached members for a collection (triggers reload on next access) */
  invalidateMembers(collectionId: string): void {
    const newMembers = new Map(this.state.members);
    newMembers.delete(collectionId);
    this.state = { ...this.state, members: newMembers };
  }

  /** Invalidate all cached members (e.g., on node update that might affect any collection) */
  invalidateAllMembers(): void {
    this.state = { ...this.state, members: new Map() };
  }

  /** Get cached members for a collection */
  getMembers(collectionId: string): Node[] {
    return this.state.members.get(collectionId) ?? [];
  }

  /** Get a collection by ID from cached data */
  getCollectionById(collectionId: string): CollectionInfo | undefined {
    return this.state.collections.find((c) => c.id === collectionId);
  }

  /** Clear all cached data */
  reset(): void {
    this.state = { ...initialDataState, members: new Map() };
  }

  /** Set test data directly (for testing purposes only) */
  _setTestData(collections: CollectionInfo[], members: Map<string, Node[]>): void {
    this.state = {
      collections,
      members,
      loading: false,
      error: null,
    };
  }
}

export const collectionsData = new CollectionsDataStore();

// Registered once at module load (this file is a singleton — ES modules only
// evaluate once), not per component mount. Retries loadCollections whenever
// the daemon becomes reachable, so a load that failed while the daemon was
// still starting up recovers automatically without a manual reload.
onDaemonReconnect(() => collectionsData.loadCollections());

// ============================================================================
// Collections UI State Store
// ============================================================================

const initialState: CollectionsState = {
  selectedCollectionId: null,
  subPanelOpen: false,
  expandedCollectionIds: new Set(),
};

class CollectionsStore {
  state = $state<CollectionsState>({
    ...initialState,
    expandedCollectionIds: new Set(),
  });

  /** The currently selected collection, normalized for UI display */
  get selectedCollection(): SelectedCollectionInfo | undefined {
    if (!this.state.selectedCollectionId) return undefined;

    const collection = collectionsData.state.collections.find(
      (c) => c.id === this.state.selectedCollectionId
    );
    if (collection) {
      return {
        id: collection.id,
        name: collection.content, // CollectionInfo uses content for name
        content: collection.content,
        memberCount: collection.memberCount,
      };
    }

    return undefined;
  }

  /** Members of the currently selected collection as CollectionMember format */
  get selectedCollectionMembers(): CollectionMember[] {
    if (!this.state.selectedCollectionId) return [];

    const members = collectionsData.state.members.get(this.state.selectedCollectionId);
    if (members && members.length > 0) {
      return (
        members
          // Filter out collection nodes - they're already shown in the collection tree
          .filter((node) => node.nodeType !== 'collection')
          .map((node) => ({
            id: node.id,
            // Prefer the cleaned title; fall back to the node content with its
            // markdown stripped so an imported header root shows "ACP Integration
            // Architecture", not the raw "# ACP Integration Architecture".
            name: node.title || stripMarkdown(node.content),
            nodeType: node.nodeType,
          }))
      );
    }

    return [];
  }

  /** Select a collection and open the sub-panel */
  async selectCollection(collectionId: string): Promise<void> {
    this.state = {
      ...this.state,
      selectedCollectionId: collectionId,
      subPanelOpen: true,
    };

    // Load members when selecting
    await collectionsData.loadMembers(collectionId);
  }

  /** Close the sub-panel */
  closeSubPanel(): void {
    this.state = {
      ...this.state,
      subPanelOpen: false,
      // Keep selectedCollectionId for visual context in the list
    };
  }

  /** Clear selection and close sub-panel */
  clearSelection(): void {
    this.state = {
      ...this.state,
      selectedCollectionId: null,
      subPanelOpen: false,
    };
  }

  /** Toggle a collection's expanded state in the tree */
  toggleCollectionExpanded(collectionId: string): void {
    const newExpanded = new Set(this.state.expandedCollectionIds);
    if (newExpanded.has(collectionId)) {
      newExpanded.delete(collectionId);
    } else {
      newExpanded.add(collectionId);
    }
    this.state = { ...this.state, expandedCollectionIds: newExpanded };
  }

  /** Reset to initial state */
  reset(): void {
    this.state = { ...initialState, expandedCollectionIds: new Set() };
  }
}

export const collectionsState = new CollectionsStore();

// ============================================================================
// Tree building & helpers
// ============================================================================

/**
 * Collection with unified name field for UI display
 * Normalizes both CollectionInfo (with content) and CollectionItem (with name)
 */
export interface SelectedCollectionInfo {
  id: string;
  name: string;
  memberCount?: number;
  content?: string;
}

/**
 * Recursively prune collections that have no member nodes visible to the
 * current user. A collection is kept if it has its own members OR any of its
 * descendants does — so a populated collection nested under an empty parent
 * still surfaces. Returns a new array of new items (pure — the input items are
 * not mutated).
 *
 * `memberCount` is sourced from the local per-user store, so it already
 * reflects RBAC visibility: the local DB only holds member edges the signed-in
 * user can see. This filter therefore hides collections that are empty *for
 * this user*, not just globally empty ones.
 */
function pruneEmptyCollections(items: CollectionItem[]): CollectionItem[] {
  return items.reduce<CollectionItem[]>((kept, item) => {
    const keptChildren = item.children ? pruneEmptyCollections(item.children) : [];
    if (item.memberCount > 0 || keptChildren.length > 0) {
      kept.push({ ...item, children: keptChildren });
    }
    return kept;
  }, []);
}

/**
 * Transform flat collections into tree structure for UI display.
 * Uses parentCollectionIds to build proper hierarchy.
 */
function buildCollectionsTree(collections: CollectionInfo[]): CollectionItem[] {
  // Build a map of id -> CollectionItem for quick lookup
  const itemMap = new Map<string, CollectionItem>();
  for (const c of collections) {
    itemMap.set(c.id, {
      id: c.id,
      name: c.content, // Collection name is stored in content field
      memberCount: c.memberCount,
      children: [],
    });
  }

  // Build a set of all IDs that are children of some other collection
  const childIds = new Set<string>();

  // For each collection, add it as a child to its parent(s)
  // Note: A collection can have multiple parents, but for tree display
  // we only show it under the first parent to avoid duplication
  for (const c of collections) {
    const parentIds = c.parentCollectionIds || [];
    if (parentIds.length > 0) {
      // Add to first parent only (to avoid showing same collection multiple times)
      const firstParentId = parentIds[0];
      const parent = itemMap.get(firstParentId);
      const child = itemMap.get(c.id);
      if (parent && child) {
        parent.children = parent.children || [];
        parent.children.push(child);
        childIds.add(c.id);
      }
    }
  }

  // Sort children alphabetically within each parent
  for (const item of itemMap.values()) {
    if (item.children && item.children.length > 0) {
      item.children.sort((a, b) => a.name.localeCompare(b.name));
    }
  }

  // Return only top-level collections (those without parents)
  const topLevel = collections
    .filter((c) => !childIds.has(c.id))
    .map((c) => itemMap.get(c.id)!)
    .sort((a, b) => a.name.localeCompare(b.name));

  // Hide collections with no member nodes visible to the current user. Keep a
  // collection whenever it — or any descendant — has members, so populated
  // sub-collections under an empty parent remain reachable.
  return pruneEmptyCollections(topLevel);
}

/**
 * Find a collection by ID in the tree structure
 */
export function findCollectionById(
  collections: CollectionItem[],
  id: string
): CollectionItem | undefined {
  for (const col of collections) {
    if (col.id === id) return col;
    if (col.children) {
      const found = findCollectionById(col.children, id);
      if (found) return found;
    }
  }
  return undefined;
}
