/**
 * FocusManagerService - Single Source of Truth for Editor Focus
 *
 * ARCHITECTURAL IMPROVEMENT (Issue #274):
 * Replaces the three-way conflict between autoFocus, focusedNodeId, and pendingCursorPositions
 * with a single reactive state that works naturally with Svelte 5.
 *
 * Problem Solved:
 * - Multiple conflicting state sources causing race conditions
 * - autoFocus checks blocking new node focus
 * - Manual _updateTrigger++ calls fighting reactivity
 *
 * Solution:
 * - Module-level reactive state (works naturally with Svelte 5 runes)
 * - Optional cursor position tracking
 * - Components derive isEditing from this single source
 *
 * FUTURE: Multi-Viewer Support (Multi-Tab/Pane)
 * When implementing multi-viewer support, extend this to per-viewer focus tracking:
 * - Track focus state per viewer: Map<viewerId, FocusState>
 * - Track active viewer for global focus determination
 * - Migration path: Add viewerId context, default to 'default' for backwards compat
 *
 * Usage:
 * ```typescript
 * import { focusManager } from '$lib/services/focus-manager.svelte';
 *
 * // Set editing state with optional cursor position
 * focusManager.setEditingNode(nodeId, cursorPosition);
 *
 * // Derive editing state in components
 * const isEditing = $derived(node.id === focusManager.editingNodeId);
 * ```
 */

export interface FocusState {
  nodeId: string | null;
  cursorPosition: number | null;
}

/**
 * Module-level reactive state for focus management
 * Using functional pattern (not class) to work naturally with Svelte 5 runes
 */

// ARCHITECTURE NOTE: These are currently global (single viewer)
// For multi-viewer: Convert to Map<viewerId, FocusState> and add activeViewerId tracking

// Single source of truth for which node is being edited
let _editingNodeId = $state<string | null>(null);

// Optional cursor position for precise positioning after node type changes
let _pendingCursorPosition = $state<number | null>(null);

export const focusManager = {
  /**
   * Public reactive getter for editing node ID
   */
  get editingNodeId(): string | null {
    return _editingNodeId;
  },

  /**
   * Public reactive getter for pending cursor position
   */
  get pendingCursorPosition(): number | null {
    return _pendingCursorPosition;
  },

  /**
   * Set which node is being edited
   * @param nodeId - The node to edit, or null to clear editing state
   * @param cursorPosition - Optional cursor position for precise positioning
   */
  setEditingNode(nodeId: string | null, cursorPosition?: number): void {
    _editingNodeId = nodeId;
    _pendingCursorPosition = cursorPosition ?? null;
  },

  /**
   * Clear editing state (no node is being edited)
   */
  clearEditing(): void {
    _editingNodeId = null;
    _pendingCursorPosition = null;
  },

  /**
   * Clear cursor position after it's been consumed
   */
  clearCursorPosition(): void {
    _pendingCursorPosition = null;
  },

  /**
   * Check if a specific node is being edited
   */
  isNodeEditing(nodeId: string): boolean {
    return _editingNodeId === nodeId;
  },

  /**
   * Get current focus state (for debugging/logging)
   */
  getCurrentState(): FocusState {
    return {
      nodeId: _editingNodeId,
      cursorPosition: _pendingCursorPosition
    };
  }
};

// Legacy export for backwards compatibility
export function getFocusManager() {
  return focusManager;
}
