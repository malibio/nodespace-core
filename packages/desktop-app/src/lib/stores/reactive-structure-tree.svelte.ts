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
  private unlisteners: UnlistenFn[] = [];
  private initialized = false;

  /**
   * Initialize the tree with LIVE SELECT event subscriptions
   * This should be called once during app startup
   */
  async initialize() {
    if (this.initialized) return;

    console.log('[ReactiveStructureTree] Initializing...');

    try {
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
   * Subscribe to edge CRUD events from LIVE SELECT
   */
  private async subscribeToEvents() {
    // Edge created event
    const unlistenCreated = await listen<EdgeEventData>('edge:created', (event) => {
      console.log('[ReactiveStructureTree] Edge created:', event.payload);
      this.addChild(event.payload);
    });
    this.unlisteners.push(unlistenCreated);

    // Edge deleted event
    const unlistenDeleted = await listen<EdgeEventData>('edge:deleted', (event) => {
      console.log('[ReactiveStructureTree] Edge deleted:', event.payload);
      this.removeChild(event.payload);
    });
    this.unlisteners.push(unlistenDeleted);

    // Edge updated event (for order changes during rebalancing)
    const unlistenUpdated = await listen<EdgeEventData>('edge:updated', (event) => {
      console.log('[ReactiveStructureTree] Edge updated:', event.payload);
      this.updateChildOrder(event.payload);
    });
    this.unlisteners.push(unlistenUpdated);
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
   * Get parent of a node by searching all edges
   * Note: This is O(n) - consider adding reverse map if performance issue
   */
  getParent(nodeId: string): string | null {
    for (const [parentId, childrenList] of this.children) {
      if (childrenList.some((c) => c.nodeId === nodeId)) {
        return parentId;
      }
    }
    return null;
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

    // Check if child already exists (shouldn't happen, but handle gracefully)
    const existingIndex = children.findIndex((c) => c.nodeId === childId);
    if (existingIndex >= 0) {
      // Update order if different
      if (children[existingIndex].order !== order) {
        children[existingIndex].order = order;
        // Re-sort array
        children.sort((a, b) => a.order - b.order);
      }
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
}

// Export singleton instance
export const structureTree = new ReactiveStructureTree();
