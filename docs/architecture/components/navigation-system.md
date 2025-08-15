# Node Navigation System

## Overview

The NodeSpace navigation system implements sophisticated arrow key navigation between nodes at different hierarchy levels with proper cursor positioning that accounts for visual properties like indentation and font scaling.

## Architecture

### Entry/Exit Methods Pattern (GitHub Issue #28)

The navigation system uses a pluggable architecture where each node type implements the `NodeNavigationMethods` interface:

```typescript
export interface NodeNavigationMethods {
  canAcceptNavigation(): boolean;
  enterFromTop(columnHint?: number): boolean;
  enterFromBottom(columnHint?: number): boolean;
  exitToTop(): { canExit: boolean; columnPosition: number };
  exitToBottom(): { canExit: boolean; columnPosition: number };
  getCurrentColumn(): number;
}
```

### Component Responsibilities

1. **MinimalBaseNode**: Implements core navigation logic with fixed assumptions approach
2. **TextNode**: Wraps MinimalBaseNode and forwards navigation methods
3. **BaseNodeViewer**: Coordinates navigation between nodes and handles target node entry

### Fixed Assumptions Approach

To ensure reliable cursor positioning across different node types and hierarchy levels:

- **Indentation Factor**: 4 characters per hierarchy level
- **Font Scaling Factors**:
  - H1: 2.0x scaling
  - H2: 1.5x scaling  
  - H3: 1.25x scaling
  - H4: 1.125x scaling
  - H5: 1.0x scaling (normal)
  - H6: 0.875x scaling

## Implementation Details

### Visual Column Calculation

```typescript
function calculateVisualColumnPosition(): number {
  const hierarchyLevel = getHierarchyLevel();
  const fontScaling = getFontScaling();
  const columnInLine = getCursorPositionInLine();
  
  // Visual column = logical column + indentation + font scaling adjustment
  const indentationOffset = hierarchyLevel * 4;
  const fontAdjustment = Math.round((fontScaling - 1.0) * columnInLine);
  
  return columnInLine + indentationOffset + fontAdjustment;
}
```

### Cursor Position Conversion

When entering a node, the system converts the visual column hint back to logical position:

```typescript
function calculateVisualCursorPosition(columnHint: number, lineTarget: string): number {
  const hierarchyLevel = getHierarchyLevel();
  const fontScaling = getFontScaling();
  
  // Remove indentation offset from columnHint to get logical column
  const indentationOffset = hierarchyLevel * 4;
  let logicalColumn = Math.max(0, columnHint - indentationOffset);
  
  // Adjust for font scaling (reverse the scaling)
  if (fontScaling !== 1.0) {
    logicalColumn = Math.round(logicalColumn / fontScaling);
  }
  
  return targetLineStart + Math.min(logicalColumn, targetLine.length);
}
```

### Text Content Separation

The system maintains separate representations for different purposes:

- **Visual Text Navigation**: Uses `getVisibleTextPosition()` which ignores HTML markup
- **Display Content**: Rendered HTML with formatting spans and headers
- **Storage Content**: Markdown format via `htmlToMarkdown()` conversion

## Navigation Flow

1. **Exit Detection**: Node detects arrow key at boundary using `isAtNodeBoundary()`
2. **Column Calculation**: Current node calculates visual column position
3. **Target Discovery**: BaseNodeViewer finds next/previous navigable node  
4. **Entry Positioning**: Target node converts visual hint to logical cursor position
5. **Focus Management**: Target node focuses and positions cursor appropriately

## Key Features

### Hierarchy-Aware Positioning
- Accounts for visual indentation when moving between parent/child nodes
- Maintains consistent column alignment across different hierarchy levels

### Font Scaling Support  
- Adjusts cursor positioning for different header sizes
- Ensures visual alignment between H1, H2, H3 headers and normal text

### Boundary Detection
- Respects node content boundaries (first/last line)
- Only exits nodes when cursor is at appropriate edge

### Visual Consistency
- Fixed assumptions prevent positioning drift
- Predictable behavior across different content types

## Testing Scenarios

The system has been tested with:

- Multi-level hierarchy navigation (root → child → grandchild)
- Mixed font sizes (H1, H2, H3 headers with normal text)
- Collapsed/expanded node states
- Complex content with inline formatting (bold, italic, underline)
- Edge cases (empty nodes, single character nodes)

## Implementation Notes

### Performance Considerations
- Calculations use integer math and simple assumptions for speed
- DOM queries are minimized and cached where possible
- Visual positioning completes within ~1ms for typical content

### Browser Compatibility  
- Uses standard Selection and Range APIs
- Tested across Chrome, Firefox, Safari
- Handles ContentEditable quirks consistently

### Future Enhancements
- Support for mixed-width fonts and custom font families
- Variable indentation based on node type
- Enhanced boundary detection for complex node types
- Integration with hybrid markdown and decoration systems

## Related Files

- `/src/lib/types/navigation.ts` - Navigation interface definitions
- `/src/lib/design/components/MinimalBaseNode.svelte` - Core navigation implementation
- `/src/lib/components/TextNode.svelte` - TextNode navigation wrapper
- `/src/lib/design/components/BaseNodeViewer.svelte` - Navigation coordination
- `/src/lib/utils/navigationUtils.ts` - Navigation utility functions

## References

- GitHub Issue #28: Pluggable Node Navigation System
- Architecture Decision Record: Entry/Exit Methods Pattern
- ContentEditable Implementation Guide