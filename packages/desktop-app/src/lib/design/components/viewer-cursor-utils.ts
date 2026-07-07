/**
 * Viewer cursor utilities
 *
 * Offset-based caret save/restore for the contenteditable elements the viewer
 * renders (`contenteditable-<nodeId>`). Used to preserve the caret across DOM
 * mutations such as indent/outdent and expand/collapse.
 *
 * Distinct from cursor-positioning.ts, which maps click coordinates to a
 * character index. These helpers work purely from a character offset.
 */

/**
 * Read the current caret position within a node's contenteditable element,
 * as a character offset from the start of its content.
 * Returns 0 when the element or a selection is unavailable.
 */
export function saveCursorPosition(nodeId: string): number {
  const element = document.getElementById(`contenteditable-${nodeId}`);
  if (!element) return 0;

  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return 0;

  const range = selection.getRangeAt(0);
  const preCaretRange = range.cloneRange();
  preCaretRange.selectNodeContents(element);
  preCaretRange.setEnd(range.startContainer, range.startOffset);

  return preCaretRange.toString().length;
}

/**
 * Place the caret at the given character offset within a node's contenteditable
 * element and focus it. Falls back to the end of the content when the offset is
 * beyond the available text. Uses `preventScroll` so restoring the caret does not
 * disturb scroll state during tab switching.
 */
export function restoreCursorPosition(nodeId: string, position: number): void {
  const element = document.getElementById(`contenteditable-${nodeId}`);
  if (!element) return;

  try {
    const selection = window.getSelection();
    if (!selection) return;

    const range = document.createRange();
    const textNodes = getTextNodes(element);

    let currentOffset = 0;
    for (const textNode of textNodes) {
      const nodeLength = textNode.textContent?.length || 0;
      if (currentOffset + nodeLength >= position) {
        range.setStart(textNode, Math.max(0, position - currentOffset));
        range.collapse(true);
        selection.removeAllRanges();
        selection.addRange(range);
        // Use preventScroll to avoid browser auto-scrolling when focusing
        // This preserves scroll state during tab switching and cursor restoration
        element.focus({ preventScroll: true });
        return;
      }
      currentOffset += nodeLength;
    }

    // If we couldn't find the exact position, place cursor at end
    if (textNodes.length > 0) {
      const lastNode = textNodes[textNodes.length - 1];
      range.setStart(lastNode, lastNode.textContent?.length || 0);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      // Use preventScroll to avoid browser auto-scrolling when focusing
      // This preserves scroll state during tab switching and cursor restoration
      element.focus({ preventScroll: true });
    }
  } catch {
    // Silently handle cursor restoration errors
  }
}

/** Collect all text nodes under an element in document order. */
export function getTextNodes(element: Element): Text[] {
  const textNodes: Text[] = [];
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT, null);

  let node = walker.nextNode();
  while (node) {
    textNodes.push(node as Text);
    node = walker.nextNode();
  }

  return textNodes;
}
