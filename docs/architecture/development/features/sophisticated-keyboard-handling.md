# Sophisticated Keyboard Handling Rules

This document describes the advanced keyboard handling behavior implemented in NodeSpace, inspired by the sophisticated rules from `nodespace-core-ui`.

## Overview

NodeSpace implements intelligent keyboard behavior that considers node hierarchy, collapsed states, formatting preservation, and cursor positioning to provide a seamless editing experience similar to modern note-taking applications.

## Enter Key Behavior

### Basic Rules

The Enter key behavior depends on cursor position and parent node state:

1. **Cursor at Beginning**: Creates new empty node ABOVE current node
2. **Cursor in Middle/End**: Splits content with smart formatting preservation

### Hierarchical Rules

#### Expanded Parent Nodes

```
Before: Parent Content|more content (expanded, has children A, B, C)
After:  Parent Content (still has no children)
        |more content (new node, now has children A, B, C)
```

#### Collapsed Parent Nodes

```
Before: Parent Content|more content (collapsed, has children A, B, C)
After:  Parent Content|more content (collapsed, STILL has children A, B, C)
        New Sibling (empty, no children)
```

#### Cursor at Beginning Special Case

```
Before: |Parent Content (any state)
After:  Empty New Node
        |Parent Content (maintains all original properties)
```

### Smart Text Splitting

When splitting content in the middle, the system preserves markdown formatting:

#### Header Inheritance

```
Before: # Header Te|xt Content
After:  # Header Te
        # |xt Content (inherits header format)
```

#### Inline Formatting Preservation

```
Before: **Bold te|xt here**
After:  **Bold te**
        **|xt here** (formatting properly closed/reopened)
```

#### Complex Formatting

```
Before: # *Header te|xt* content
After:  # *Header te*
        # *|xt* content (both header and italic preserved)
```

## Backspace Key Behavior

### Smart Format Inheritance

When backspacing between different format levels, the receiver's format takes precedence:

#### Header Format Inheritance

```
Before: # Header 1|
        ### Header 3
After:  # Header 1|Header 3 (H3 syntax stripped, H1 format maintained)
```

#### Paragraph to Header

```
Before: Normal text|
        # Header Content
After:  Normal text|Header Content (header syntax stripped)
```

#### Header to Header

```
Before: ## Header 2|
        ### Header 3
After:  ## Header 2|Header 3 (H3 syntax stripped, H2 format maintained)
```

### Empty vs Content Handling

#### Empty Node Removal

```
Before: Some Content|
        (empty node)
After:  Some Content| (cursor at end, empty node removed)
```

#### Content Merging with Format Stripping

```
Before: # Header|
        ### Other Content
After:  # Header|Other Content (### syntax automatically stripped)
```

### Children Transfer Logic

#### Root Source Nodes

- Children go to root ancestor of target node
- Preserves hierarchy depth relationships

#### Non-Root Source Nodes

- Children go directly to target node
- Maintains parent-child relationships

#### Collapsed State Handling

- **Collapsed targets**: New children inserted at beginning and target auto-expands
- **Expanded targets**: New children appended at end

## Collapsed State Management

### Synchronization

- Dual state tracking: `node.expanded` property + `_collapsedNodes` Set
- Both systems stay synchronized through all operations
- UI controls update both representations

### Auto-Expansion Rules

- Nodes receiving children from transfers automatically expand
- Ensures transferred children become visible
- Preserves user intent and hierarchy visibility

## Cursor Positioning

### Coordinate System and Navigation

#### Viewport-Relative Positioning (2024 Enhancement)

The cursor positioning system uses **viewport-relative coordinates** for consistent horizontal position tracking across nodes with different properties:

- **Reference Frame**: Coordinates measured from viewport left edge + horizontal scroll offset
- **Formula**: `cursorRect.left + window.scrollX`
- **Benefits**:
  - Maintains visual cursor position when navigating between headers (H1, H2) and regular text
  - Handles nodes at different depth levels correctly
  - Accounts for horizontal scrolling
  - Provides stable positioning across multi-line content

**Previous Issue**: Element-relative coordinates (`cursorRect.left - elementRect.left`) caused cursor drift because each node had a different left edge reference point.

**Implementation**:
- `getCurrentPixelOffset()`: Returns viewport-relative cursor position
- `setCursorAtPixelOffset()`: Positions cursor using viewport coordinates
- Both methods account for `window.scrollX` to handle horizontal scroll

#### Arrow Key Navigation

When navigating with arrow keys (up/down), the system:

1. **Captures pixel offset** from current cursor position (viewport-relative)
2. **Stores offset** in `lastKnownPixelOffset` for cross-node navigation
3. **Finds target line** in destination node
4. **Positions cursor** at closest character to the stored pixel offset
5. **Uses linear search** through character positions for accuracy

This ensures cursor maintains its horizontal position when moving between:
- Headers with different font sizes (H1, H2, H3)
- Nodes at different depth levels
- Multi-line content blocks

### Smart Positioning After Operations

#### After Inherited Syntax

```
New header node: # |cursor positioned here
Complex format: # **|cursor after all opening markers
```

#### After Content Merging

```
Before merge: Content A| + Content B
After merge:  Content A|Content B (cursor at junction point)
```

#### Shift+Enter Ready-to-Type Positioning (2024 Enhancement)

When using Shift+Enter to create a new line within a node, the cursor is automatically positioned after any syntax markers to enable immediate typing:

**Header Syntax**:
```
Before: ## Header Te|xt
Press Shift+Enter
After:  ## Header Te
        ## |xt (cursor after "## ", ready to type)
```

**Task Checkbox Syntax**:
```
Before: [ ] Task ite|m
Press Shift+Enter
After:  [ ] Task ite
        [ ] |m (cursor after "[ ] ", ready to type)
```

**Quote Syntax**:
```
Before: > Quote te|xt
Press Shift+Enter
After:  > Quote te
        > |xt (cursor after "> ", ready to type)
```

**Plain Text** (no syntax):
```
Before: Plain te|xt
Press Shift+Enter
After:  Plain te
        |xt (cursor at line start, no syntax to skip)
```

**Implementation**: The `positionCursorAfterSyntax()` method:
1. Detects syntax patterns using regex (headers, tasks, quotes)
2. Calculates syntax length (e.g., "## " = 3 characters)
3. Uses TreeWalker to navigate text nodes
4. Positions cursor after the syntax markers
5. Falls back gracefully for plain text or edge cases

### Positioning Algorithm

1. **Skip header syntax** (`#`, `##`, etc.)
2. **Skip opening formatting markers** (`**`, `__`, `*` in precedence order)
3. **Validate marker pairs** (ensure closing counterparts exist)
4. **Position optimally** for continued typing

## Technical Implementation

### Core Components

#### NodeManager

- **`createNode()`**: Handles sophisticated Enter logic with collapsed state awareness
- **`combineNodes()`**: Implements smart format inheritance on backspace
- **`transferChildrenWithDepthPreservation()`**: Advanced hierarchy management

#### ContentEditableController

- **`smartTextSplit()`**: Formatting-aware content splitting
- **`handleKeyDown()`**: Cursor position detection and behavior routing
- **`getCurrentPixelOffset()`**: Returns viewport-relative cursor position (+ scroll offset)
- **`positionCursorAfterSyntax()`**: Positions cursor after markdown syntax on Shift+Enter

#### BaseNodeViewer

- **`setCursorAtPixelOffset()`**: Positions cursor in node using viewport-relative coordinates
- **Arrow navigation handling**: Maintains pixel offset across node transitions
- **Defensive fallback**: Handles empty text nodes gracefully

#### ReactiveNodeManager

- **State synchronization** between expanded/collapsed representations
- **UI reactivity** for complex state changes

### State Management

#### Collapsed State Tracking

```typescript
private _collapsedNodes: Set<string>; // Collapsed nodes set
node.expanded: boolean;               // Individual node property
```

#### Synchronization Methods

- `toggleExpanded()`: Updates both systems
- `toggleCollapsed()`: Maintains consistency
- Initialization syncs state from node properties

## Advanced Scenarios

### Complex Hierarchy Operations

#### Multi-Level Children Transfer

```
Before: Root Node A (depth 0, children X, Y)
        Target Node B (depth 2)

After backspace merge:
        Root Node A children → Target's root ancestor
        Preserves depth relationships
        Auto-expands receiving nodes
```

#### Nested Formatting Preservation

```
Before: # **__Bold underline te|xt__** more
After:  # **__Bold underline te__**
        # **__Bold underline |xt__** more
```

### Edge Cases Handled

1. **Empty formatting markers**: Validated before cursor positioning
2. **Malformed hierarchy**: Graceful fallback to direct parent
3. **Missing nodes**: Null checks prevent crashes
4. **Circular references**: Prevented through depth calculations

## Performance Considerations

### Optimizations Implemented

- **Efficient state synchronization**: Incremental updates where possible
- **Cached depth calculations**: Avoid repeated tree traversals
- **Minimal reactivity triggers**: Only update UI when necessary
- **Pattern reuse**: Compiled regex patterns for performance

### Memory Management

- **Auto-cleanup**: Removed nodes cleared from all state structures
- **Reference management**: Proper parent/child reference updates
- **Set operations**: Efficient add/remove for collapsed state tracking

## Future Enhancements

### Potential Improvements

1. **Undo/Redo Integration**: Capture sophisticated operations for reversal
2. **Animation Support**: Smooth transitions for hierarchy changes
3. **Accessibility**: Screen reader support for complex operations
4. **Performance**: Further optimizations for large documents
5. **Customization**: User-configurable keyboard behavior rules

## Testing Guidelines

### Key Scenarios to Test

1. **Enter on collapsed parents**: Verify children stay with original
2. **Backspace between formats**: Confirm format inheritance
3. **Complex hierarchy transfers**: Validate depth preservation
4. **Cursor positioning**: Ensure optimal placement after operations
5. **State synchronization**: Check expanded/collapsed consistency
6. **Arrow navigation**: Verify horizontal position maintained across nodes (2024)
7. **Shift+Enter syntax positioning**: Confirm cursor placement after syntax markers (2024)

### Test Cases

```javascript
// Enter key scenarios
testEnterAtBeginning();
testEnterOnCollapsedParent();
testEnterWithFormatting();
testEnterWithComplexHierarchy();

// Backspace scenarios
testBackspaceFormatInheritance();
testBackspaceChildrenTransfer();
testBackspaceEmptyNodeRemoval();
testBackspaceDepthPreservation();

// State management
testCollapsedStateSynchronization();
testAutoExpansion();
testReactivityTriggers();

// Cursor positioning (2024)
testViewportRelativeCoordinates();
testScrollOffsetHandling();
testArrowNavigationPixelOffset();
testShiftEnterHeaderSyntax();
testShiftEnterTaskCheckboxSyntax();
testShiftEnterQuoteSyntax();
testShiftEnterPlainText();
testEmptyLineHandling();
```

**Test Coverage**: See `src/tests/integration/cursor-positioning.test.ts` for comprehensive cursor positioning tests (added December 2024).

---

_This sophisticated keyboard handling provides a professional editing experience that intelligently manages hierarchy, preserves formatting, and maintains user intent throughout complex document operations._
