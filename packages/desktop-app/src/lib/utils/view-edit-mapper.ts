/**
 * View-to-Edit Position Mapper
 *
 * Maps character positions from view content (markdown syntax stripped)
 * to edit content (with markdown syntax visible).
 *
 * This is essential for click-to-cursor positioning where:
 * - View mode shows rendered text: "Hello world"
 * - Edit mode shows raw markdown: "**Hello** world"
 *
 * When user clicks on 'w' in view, we need to find corresponding position in edit.
 */

/**
 * Map character position from view content to edit content
 *
 * View content has markdown syntax stripped (e.g., "Hello world")
 * Edit content has markdown syntax visible (e.g., "**Hello** world")
 *
 * This function accounts for syntax markers when mapping positions.
 *
 * @param viewPosition - Character position in view content (0-based)
 * @param viewContent - The view content (syntax stripped, from displayContent)
 * @param editContent - The edit content (with syntax, from content prop)
 * @returns Character position in edit content (0-based)
 *
 * @example
 * // View: "Hello world" (click at position 6 = 'w')
 * // Edit: "**Hello** world" (should position at 10 = 'w')
 * mapViewPositionToEditPosition(6, "Hello world", "**Hello** world") // → 10
 */
export function mapViewPositionToEditPosition(
  viewPosition: number,
  viewContent: string,
  editContent: string
): number {
  // Fast path: If no displayContent (view === edit), no mapping needed
  if (viewContent === editContent) {
    return viewPosition;
  }

  // Edge case: position 0 - skip leading syntax markers
  if (viewPosition === 0) {
    let editIndex = 0;
    while (editIndex < editContent.length) {
      const remaining = editContent.substring(editIndex);

      // Skip markdown syntax markers
      if (remaining.startsWith('**') || remaining.startsWith('__')) {
        editIndex += 2;
        continue;
      }
      if (remaining.startsWith('*') || remaining.startsWith('_')) {
        editIndex += 1;
        continue;
      }
      if (remaining.startsWith('~~')) {
        editIndex += 2;
        continue;
      }
      if (remaining.startsWith('`')) {
        editIndex += 1;
        continue;
      }

      // Found first non-marker character
      break;
    }
    return editIndex;
  }

  // Walk both strings character-by-character for positions > 0
  // Skip syntax markers in editContent while advancing viewContent
  let viewIndex = 0;
  let editIndex = 0;

  while (viewIndex < viewPosition && editIndex < editContent.length) {
    // Check if we're at a syntax marker in edit content
    const remaining = editContent.substring(editIndex);

    // Skip markdown syntax markers (don't advance viewIndex)
    if (remaining.startsWith('**') || remaining.startsWith('__')) {
      editIndex += 2; // Skip bold markers (**)
      continue;
    }
    if (remaining.startsWith('*') || remaining.startsWith('_')) {
      editIndex += 1; // Skip italic markers (*)
      continue;
    }
    if (remaining.startsWith('~~')) {
      editIndex += 2; // Skip strikethrough markers (~~)
      continue;
    }
    if (remaining.startsWith('`')) {
      editIndex += 1; // Skip code markers (`)
      continue;
    }

    // Regular character - advance both indices
    viewIndex++;
    editIndex++;
  }

  return editIndex;
}
