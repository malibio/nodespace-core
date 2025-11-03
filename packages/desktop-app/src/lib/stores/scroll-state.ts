/**
 * Scroll State Store
 *
 * Manages scroll positions for viewers across different tabs and panes.
 * Each viewer is identified by a composite key: `${tabId}-${paneId}`
 *
 * This ensures:
 * - Independent scroll positions per viewer instance
 * - Scroll state preserved when switching tabs
 * - Scroll state preserved when switching panes
 * - Same node in different tabs/panes maintains separate scroll positions
 */

// Map from viewer ID to scroll position
const scrollPositions = new Map<string, number>();

/**
 * Generate viewer ID from tab and pane IDs
 * This creates a unique identifier for each viewer instance
 */
export function getViewerId(tabId: string, paneId: string): string {
  return `${tabId}-${paneId}`;
}

/**
 * Save scroll position for a viewer
 * @param viewerId - Composite viewer ID (from getViewerId)
 * @param position - Scroll position in pixels
 */
export function saveScrollPosition(viewerId: string, position: number): void {
  scrollPositions.set(viewerId, position);
}

/**
 * Get saved scroll position for a viewer
 * @param viewerId - Composite viewer ID (from getViewerId)
 * @returns Saved scroll position or 0 if none exists
 */
export function getScrollPosition(viewerId: string): number {
  return scrollPositions.get(viewerId) ?? 0;
}

/**
 * Clear scroll position for a viewer (e.g., when tab is closed)
 * @param viewerId - Composite viewer ID (from getViewerId)
 */
export function clearScrollPosition(viewerId: string): void {
  scrollPositions.delete(viewerId);
}

/**
 * Clear all scroll positions for a pane (when pane is closed)
 * @param paneId - Pane ID
 */
export function clearPaneScrollPositions(paneId: string): void {
  // Find all viewer IDs that belong to this pane
  const viewerIds = Array.from(scrollPositions.keys()).filter((id) => id.endsWith(`-${paneId}`));

  // Delete each one
  viewerIds.forEach((id) => scrollPositions.delete(id));
}
