/**
 * Node Hierarchy Utilities
 *
 * Helper functions for managing parent-child relationships in the node graph.
 * Addresses code review feedback from Issue #528 PR review.
 */

import { sharedNodeStore } from '$lib/services/shared-node-store';

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
export function registerChildWithParent(parentId: string, _childId: string): void {
  // DEPRECATED: Cache updates removed - hierarchy is now computed on-demand from parentId field
  // This function is kept for backward compatibility but does nothing
  const _parent = sharedNodeStore.getNode(parentId); // Validate parent exists (no-op validation)
}
