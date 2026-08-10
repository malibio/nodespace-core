/**
 * Text-selection helpers shared by the node view layer.
 *
 * A page of nodes is ordinary DOM (only the focused node is a textarea; the rest
 * are plain `<div>`s), so native cross-node text selection is structurally
 * possible — the interaction just has to stop stealing it. These predicates keep
 * that decision in one tested place.
 */

/**
 * True when there is a real, non-empty text selection on the page. A genuine
 * click leaves the selection collapsed at the caret; a drag-select leaves a
 * non-empty range. The view-container click handler uses this to avoid focusing
 * (and thereby collapsing) a selection the user just made.
 */
export function isActiveTextSelection(selection: Selection | null): boolean {
  return !!selection && selection.rangeCount > 0 && !selection.isCollapsed;
}
