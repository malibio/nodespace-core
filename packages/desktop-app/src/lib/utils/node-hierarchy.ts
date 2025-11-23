/**
 * Node Hierarchy Utilities
 *
 * Helper functions for managing parent-child relationships in the node graph.
 * Addresses code review feedback from Issue #528 PR review.
 */

import { sharedNodeStore } from '$lib/services/shared-node-store';
import { structureTree as reactiveStructureTree } from '$lib/stores/reactive-structure-tree.svelte';

/**
 * Registers a child node with its parent in the SharedNodeStore children cache.
 *
 * This ensures the promoted node appears in the parent's children array,
 * preventing orphaning and UI disappearance issues.
 *
 * **Critical for Issue #528 fix**: After promoting a placeholder, the children
 * cache must be updated to establish the parent-child relationship in the
 * frontend (backend edges are created separately via NodeOperations).
 *
 * @param parentId - The ID of the parent node
 * @param childId - The ID of the child node to register
 *
 * @example
 * ```typescript
 * // After promoting a placeholder:
 * const promotedNode = { ...placeholder, content: newContent };
 * await backend.createNode(promotedNode);
 * registerChildWithParent(parentNodeId, promotedNode.id); // ✅ Update cache
 * ```
 */
export function registerChildWithParent(
  parentId: string,
  childId: string,
  insertAfterNodeId?: string
): void {
  // Validate parent exists in store
  const parent = sharedNodeStore.getNode(parentId);
  if (!parent) {
    console.warn(
      `[registerChildWithParent] Parent ${parentId} not found in store, skipping cache update`
    );
    return;
  }

  // Get existing children (if any)
  const existingChildren = reactiveStructureTree.getChildren(parentId);

  // Don't add duplicate
  if (existingChildren.includes(childId)) {
    return;
  }

  // Calculate order based on positioning
  let order: number;

  if (insertAfterNodeId) {
    // Insert after specific sibling - find its position and insert after it
    const afterNodeIndex = existingChildren.indexOf(insertAfterNodeId);
    if (afterNodeIndex >= 0) {
      // Get the order of the node we're inserting after
      const childrenInfo = reactiveStructureTree.getChildrenWithOrder(parentId);
      const afterNodeOrder = childrenInfo.find(c => c.nodeId === insertAfterNodeId)?.order || 1.0;

      // If there's a node after it, calculate midpoint
      if (afterNodeIndex < existingChildren.length - 1) {
        const nextNodeId = existingChildren[afterNodeIndex + 1];
        const nextNodeOrder = childrenInfo.find(c => c.nodeId === nextNodeId)?.order || afterNodeOrder + 1.0;
        order = (afterNodeOrder + nextNodeOrder) / 2;
      } else {
        // Inserting at end - add 1 to last order
        order = afterNodeOrder + 1.0;
      }
    } else {
      // afterNodeId not found, append at end
      order = existingChildren.length + 1.0;
    }
  } else {
    // No position specified - append at end (default behavior)
    order = existingChildren.length > 0 ? existingChildren.length + 1.0 : 1.0;
  }

  reactiveStructureTree.addInMemoryRelationship(parentId, childId, order);
}
