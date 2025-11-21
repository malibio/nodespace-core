/**
 * ReactiveStructureTree - Reactive Store for Node Hierarchy
 *
 * Maintains a reactive map of parent→children relationships using Svelte 5 $state.
 * Subscribes to LIVE SELECT edge events (edge:created, edge:updated, edge:deleted)
 * and automatically keeps the tree synchronized and sorted.
 *
 * Features:
 * - Svelte 5 $state for automatic UI reactivity
 * - Binary search insertion to maintain sorted children
 * - Snapshot/restore for optimistic rollback
 * - Real-time synchronization via Tauri events
 */

import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type { EdgeEventData } from '$lib/services/event-types';

interface ChildInfo {
  nodeId: string;
  order: number;
}

class ReactiveStructureTree {
  // Reactive map using Svelte 5 $state - automatically triggers reactivity
  children = $state(new Map<string, ChildInfo[]>());

  // Reverse parent map for O(1) parent lookups
  // Maps child ID -> parent ID
  private parentMap = $state(new Map<string, string>());

  private unlisteners: UnlistenFn[] = [];
  private initialized = false;

  /**
   * Initialize the tree with LIVE SELECT event subscriptions
   * This should be called once during app startup
   *
   * TODO (#XXXX): Add bulk initialization from backend on startup
   * Currently the tree starts empty and gets populated via LIVE SELECT events.
   * This works functionally but means existing hierarchy isn't visible until
   * external operations trigger events. Need backend command to fetch all edges.
   */
  async initialize() {
    if (this.initialized) return;

    console.log('[ReactiveStructureTree] Initializing...');

    try {
      // TODO: Add initial bulk load before subscribing to events
      // const initialEdges = await tauriNodeService.getAllEdges();
      // this.buildTree(initialEdges);

      // Subscribe to LIVE SELECT structure events
      await this.subscribeToEvents();

      this.initialized = true;
      console.log('[ReactiveStructureTree] Initialization complete');
    } catch (error) {
      console.error('[ReactiveStructureTree] Initialization failed:', error);
      throw error;
    }
  }

  /**
   * Build initial tree from bulk edge data
   * Used for initialization when getAllEdges() backend method is available
   * @private
   */
  private buildTree(edges: EdgeEventData[]) {
    const tree = new Map<string, ChildInfo[]>();
    const parents = new Map<string, string>();

    for (const edge of edges) {
      if (!tree.has(edge.in)) {
        tree.set(edge.in, []);
      }
      tree.get(edge.in)!.push({
        nodeId: edge.out,
        order: edge.order
      });

      // Build reverse parent map
      parents.set(edge.out, edge.in);
    }

    // Sort all children arrays by order
    for (const [_parentId, children] of tree) {
      children.sort((a, b) => a.order - b.order);
    }

    this.children = tree;
    this.parentMap = parents;
  }

  /**
   * PUBLIC API: Populate tree from nodes with parentId
   * Temporary method until backend provides getAllEdges()
   * Creates synthetic edge data from node.parentId and assigns default ordering
   *
   * @param nodes - Array of nodes with parentId field
   */
  syncFromNodes(nodes: Array<{ id: string; parentId?: string | null }>): void {
    // Build synthetic edges from parentId
    // Use array index as default order (insertion order)
    const edges: EdgeEventData[] = [];
    const nodesByParent = new Map<string, Array<{ id: string; index: number }>>();

    nodes.forEach((node, index) => {
      const parentId = node.parentId ?? null;
      const parentKey = parentId || '__root__';

      if (!nodesByParent.has(parentKey)) {
        nodesByParent.set(parentKey, []);
      }
      nodesByParent.get(parentKey)!.push({ id: node.id, index });
    });

    // Convert to edge data with fractional ordering
    for (const [parentKey, children] of nodesByParent) {
      const parentId = parentKey === '__root__' ? null : parentKey;
      children.forEach((child, idx) => {
        edges.push({
          id: `${parentId || 'root'}-${child.id}`,
          in: parentId || '__root__',
          out: child.id,
          order: idx // Simple sequential ordering for now
        });
      });
    }

    this.buildTree(edges);
  }

  /**
   * Subscribe to edge CRUD events from LIVE SELECT
   * @throws Error if critical event subscriptions fail
   */
  private async subscribeToEvents() {
    try {
      // Edge created event
      const unlistenCreated = await listen<EdgeEventData>('edge:created', (event) => {
        console.log('[ReactiveStructureTree] Edge created:', event.payload);
        this.addChild(event.payload);
      });
      this.unlisteners.push(unlistenCreated);
    } catch (error) {
      console.error('[ReactiveStructureTree] Failed to subscribe to edge:created', error);
      throw new Error('Failed to subscribe to critical structure events: edge:created');
    }

    try {
      // Edge deleted event
      const unlistenDeleted = await listen<EdgeEventData>('edge:deleted', (event) => {
        console.log('[ReactiveStructureTree] Edge deleted:', event.payload);
        this.removeChild(event.payload);
      });
      this.unlisteners.push(unlistenDeleted);
    } catch (error) {
      console.error('[ReactiveStructureTree] Failed to subscribe to edge:deleted', error);
      throw new Error('Failed to subscribe to critical structure events: edge:deleted');
    }

    try {
      // Edge updated event (for order changes during rebalancing)
      const unlistenUpdated = await listen<EdgeEventData>('edge:updated', (event) => {
        console.log('[ReactiveStructureTree] Edge updated:', event.payload);
        this.updateChildOrder(event.payload);
      });
      this.unlisteners.push(unlistenUpdated);
    } catch (error) {
      console.error('[ReactiveStructureTree] Failed to subscribe to edge:updated', error);
      throw new Error('Failed to subscribe to critical structure events: edge:updated');
    }
  }

  /**
   * Get ordered child IDs for a parent (reactive)
   * This is the main public API - returns nodes in sorted order by edge.order
   */
  getChildren(parentId: string): string[] {
    const childInfos = this.children.get(parentId) || [];
    // Children are already sorted on insertion
    return childInfos.map((c) => c.nodeId);
  }

  /**
   * Get child info with order (for internal use, debugging)
   */
  getChildrenWithOrder(parentId: string): ChildInfo[] {
    return this.children.get(parentId) || [];
  }

  /**
   * Check if a node has children (reactive)
   */
  hasChildren(parentId: string): boolean {
    const children = this.children.get(parentId);
    return children ? children.length > 0 : false;
  }

  /**
   * Get parent of a node (O(1) lookup using reverse map)
   */
  getParent(nodeId: string): string | null {
    return this.parentMap.get(nodeId) || null;
  }

  /**
   * Get parent IDs for a node (array for API consistency with SharedNodeStore)
   */
  getParentsForNode(nodeId: string): string[] {
    const parentId = this.parentMap.get(nodeId);
    return parentId ? [parentId] : [];
  }

  /**
   * Add a child with binary search insertion to maintain sort by order
   */
  private addChild(edge: EdgeEventData) {
    const parentId = edge.in;
    const childId = edge.out;
    const order = edge.order;

    let children = this.children.get(parentId);
    if (!children) {
      children = [];
      this.children.set(parentId, children);
    }

    // Check if child already exists in current parent
    const existingIndex = children.findIndex((c) => c.nodeId === childId);
    if (existingIndex >= 0) {
      console.warn(
        `[ReactiveStructureTree] Duplicate edge detected: ${childId} already child of ${parentId}`,
        edge
      );
      // Update order if different
      if (children[existingIndex].order !== order) {
        children[existingIndex].order = order;
        // Re-sort array
        children.sort((a, b) => a.order - b.order);
        // Notify Svelte of the change
        this.children.set(parentId, children);
      }
      return;
    }

    // Check if child already has a DIFFERENT parent (tree invariant violation)
    const currentParent = this.getParent(childId);
    if (currentParent && currentParent !== parentId) {
      console.error(
        `[ReactiveStructureTree] Tree invariant violation: ${childId} already has parent ${currentParent}, cannot add to ${parentId}`,
        edge
      );
      // In a tree structure, a node can only have one parent
      // This indicates either a database inconsistency or a bug in event emission
      // For now, we'll ignore the duplicate edge to preserve tree structure
      return;
    }

    // Binary search to find insertion position
    const insertIndex = this.findInsertPosition(children, order);

    // Insert at correct position to maintain sort
    const newChild: ChildInfo = {
      nodeId: childId,
      order
    };

    children.splice(insertIndex, 0, newChild);
    // Notify Svelte of the change
    this.children.set(parentId, children);

    // Update reverse parent map for O(1) parent lookups
    this.parentMap.set(childId, parentId);
  }

  /**
   * Remove a child from parent's children
   */
  private removeChild(edge: EdgeEventData) {
    const parentId = edge.in;
    const childId = edge.out;

    const children = this.children.get(parentId) || [];
    const filtered = children.filter((c) => c.nodeId !== childId);

    if (filtered.length === 0) {
      this.children.delete(parentId);
    } else {
      this.children.set(parentId, filtered);
    }

    // Remove from parent map
    this.parentMap.delete(childId);
  }

  /**
   * Update child order (rare - only during rebalancing)
   */
  private updateChildOrder(edge: EdgeEventData) {
    // Remove and re-add to update order
    this.removeChild(edge);
    this.addChild(edge);
  }

  /**
   * Binary search to find insertion position to maintain sort by order
   * @param children - sorted array of ChildInfo
   * @param order - order value to insert
   * @returns index where item should be inserted
   */
  private findInsertPosition(children: ChildInfo[], order: number): number {
    let left = 0;
    let right = children.length;

    while (left < right) {
      const mid = Math.floor((left + right) / 2);
      if (children[mid].order < order) {
        left = mid + 1;
      } else {
        right = mid;
      }
    }

    return left;
  }

  /**
   * Take snapshot of tree for optimistic rollback
   */
  snapshot(): Map<string, ChildInfo[]> {
    // Deep copy the map
    const snapshot = new Map<string, ChildInfo[]>();
    for (const [parentId, childrenList] of this.children) {
      snapshot.set(parentId, [...childrenList]);
    }
    return snapshot;
  }

  /**
   * Restore tree from snapshot (rollback on error)
   */
  restore(snapshot: Map<string, ChildInfo[]>) {
    this.children = snapshot;
  }

  /**
   * Cleanup event listeners
   */
  async destroy() {
    for (const unlisten of this.unlisteners) {
      unlisten();
    }
    this.unlisteners = [];
    this.initialized = false;
  }

  /**
   * TEST ONLY: Direct access to addChild for testing binary search algorithm
   * @internal
   */
  __testOnly_addChild(edge: EdgeEventData) {
    this.addChild(edge);
  }
}

// Export singleton instance
export const structureTree = new ReactiveStructureTree();
