/**
 * HierarchyStore - Reactive Structure Binding (Svelte 5 Runes)
 *
 * Manages parent-child relationships independently of node content.
 * Uses Svelte 5 runes for fine-grained reactivity.
 *
 * Architecture:
 * - Separates structure (parent-child) from data (content/properties)
 * - Uses reactive Maps for O(1) lookups
 * - Provides public API for hierarchy queries
 * - Receives updates from SharedNodeStore when hierarchy changes
 *
 * Key Design:
 * - childrenMap: parentId -> Set<childId> (O(1) lookup for children)
 * - parentMap: childId -> parentId (O(1) lookup for parent)
 * - updateTrigger: Counter to force derived recalculation in Svelte
 *
 * PUBLIC API:
 * - getNodesForParent(parentId): Get all children for a parent
 * - getParentsForNode(nodeId): Get parent(s) for a node
 * - updateParentChild(nodeId, newParentId): Update relationship
 * - removeNode(nodeId): Remove node from hierarchy
 * - syncFromBackend(nodes): Initialize from backend data
 */

/**
 * Reactive parent-child mapping
 */
class HierarchyStore {
  // Reactive state - parent-to-children mapping
  private childrenMap = $state(new Map<string | null, Set<string>>());

  // Reactive state - child-to-parent mapping
  private parentMap = $state(new Map<string, string | null>());

  // Update counter for forcing derived recalculation
  private updateTrigger = $state(0);

  /**
   * Get children for a parent
   * @param parentId - Parent node ID or null for root
   * @returns Array of child node IDs
   */
  getNodesForParent(parentId: string | null): string[] {
    void this.updateTrigger; // Access to track for reactivity
    const children = this.childrenMap.get(parentId);
    return children ? Array.from(children) : [];
  }

  /**
   * Get parent for a node (typically one parent, but returns array for API consistency)
   * @param nodeId - Node ID
   * @returns Array with parent ID (empty if root)
   */
  getParentsForNode(nodeId: string): string[] {
    void this.updateTrigger; // Access to track for reactivity
    const parentId = this.parentMap.get(nodeId);
    return parentId !== undefined && parentId !== null ? [parentId] : [];
  }

  /**
   * Update parent-child relationship
   * Removes from old parent and adds to new parent
   * @param nodeId - Node being moved/added
   * @param newParentId - New parent ID or null for root
   * @param oldParentId - Old parent ID (if known, for optimization)
   */
  updateParentChild(nodeId: string, newParentId: string | null, oldParentId?: string | null): void {
    // Determine old parent from map if not provided
    const actualOldParentId = oldParentId !== undefined ? oldParentId : this.parentMap.get(nodeId);

    // Remove from old parent
    if (actualOldParentId !== undefined) {
      const oldChildren = this.childrenMap.get(actualOldParentId);
      if (oldChildren) {
        oldChildren.delete(nodeId);
        // Clean up empty sets
        if (oldChildren.size === 0) {
          this.childrenMap.delete(actualOldParentId);
        }
      }
    }

    // Add to new parent
    if (!this.childrenMap.has(newParentId)) {
      this.childrenMap.set(newParentId, new Set());
    }
    this.childrenMap.get(newParentId)!.add(nodeId);

    // Update parent mapping
    if (newParentId === null) {
      this.parentMap.delete(nodeId);
    } else {
      this.parentMap.set(nodeId, newParentId);
    }

    // Trigger reactivity
    this.updateTrigger++;
  }

  /**
   * Remove node from structure (on delete)
   * @param nodeId - Node to remove
   */
  removeNode(nodeId: string): void {
    const parentId = this.parentMap.get(nodeId);

    // Remove from parent's children
    if (parentId !== undefined) {
      const children = this.childrenMap.get(parentId);
      if (children) {
        children.delete(nodeId);
        // Clean up empty sets
        if (children.size === 0) {
          this.childrenMap.delete(parentId);
        }
      }
    }

    // Remove parent mapping
    this.parentMap.delete(nodeId);

    // Remove its children (if node had children)
    this.childrenMap.delete(nodeId);

    // Trigger reactivity
    this.updateTrigger++;
  }

  /**
   * Sync from backend query results (initial load, refresh)
   * Replaces entire hierarchy with data from backend
   * @param nodes - Array of nodes with parentId
   */
  syncFromBackend(nodes: Array<{ id: string; parentId?: string | null }>): void {
    this.childrenMap.clear();
    this.parentMap.clear();

    for (const node of nodes) {
      const parentId = node.parentId ?? null;

      // Add to parent's children
      if (!this.childrenMap.has(parentId)) {
        this.childrenMap.set(parentId, new Set());
      }
      this.childrenMap.get(parentId)!.add(node.id);

      // Track parent relationship
      if (parentId !== null) {
        this.parentMap.set(node.id, parentId);
      }
    }

    // Trigger reactivity
    this.updateTrigger++;
  }
}

// Singleton instance
export const hierarchyStore = new HierarchyStore();
