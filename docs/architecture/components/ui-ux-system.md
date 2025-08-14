# UI/UX System Architecture

## Overview

NodeSpace's user interface system provides modern, accessible, and performant interaction patterns that rival contemporary IDEs and knowledge management tools. The system is built on Svelte's reactive framework with careful attention to accessibility, keyboard navigation, and visual design consistency.

## Multi-Node Selection System

### Architecture Overview

The multi-node selection system enables users to select and operate on multiple blocks simultaneously across hierarchical structures, similar to modern file managers and IDEs.

#### Core Components

```typescript
// Selection State Management
interface SelectionState {
  selectedBlocks: string[];           // Array of selected block IDs
  mode: 'single' | 'range' | 'multi'; // Current selection mode
  rangeStart: string | null;          // Start block for range operations
  rangeEnd: string | null;            // End block for range operations
  multiSelectAnchor: string | null;   // First block in multi-select session
  direction: 'up' | 'down' | null;    // Selection direction
  lastActionType: SelectionActionType; // Last performed action
}

type SelectionActionType = 'single' | 'range' | 'multi-add' | 'multi-remove' | 'clear';
```

#### Implementation Layers

**1. Foundation Layer (Issue #35)**
- Single-block selection with CSS-based visual highlighting
- Reactive Svelte store for selection state management
- Click-based selection with immediate visual feedback
- Theme-integrated highlighting using design system tokens

**2. Range Selection Layer (Issue #36)**
- Shift+click range selection across hierarchy levels
- DOM traversal algorithm for finding blocks between two points
- Direction-aware selection (upward/downward)
- Hierarchical range calculation respecting parent-child relationships

**3. Multi-Select Layer (Issue #37)**
- Ctrl+Click (Cmd+Click on Mac) individual block toggle
- Platform-aware modifier key detection
- Non-contiguous block selection support
- Visual indicators for multi-select mode

**4. Keyboard Navigation Layer (Issue #38)**
- Arrow key navigation between blocks
- Keyboard selection shortcuts (Shift+Arrow, Ctrl+A, Space)
- Focus management with auto-scroll
- WCAG 2.1 compliant accessibility features

### Selection Interaction Patterns

#### Mouse Interactions
```typescript
// BaseNode click handling
function handleClick(event: MouseEvent) {
  const action = ModifierKeyDetector.getSelectionAction(event);
  
  switch (action) {
    case 'single-select':
      selectionStore.selectBlock(nodeId);
      break;
    case 'range-extend':
      selectionStore.extendRange(nodeId);
      break;
    case 'multi-toggle':
      selectionStore.toggleBlock(nodeId);
      break;
  }
}
```

#### Keyboard Interactions
```typescript
// Keyboard navigation controller
class KeyboardNavigationController {
  handleKeydown(event: KeyboardEvent): boolean {
    switch (event.key) {
      case 'ArrowUp':
        return this.handleArrowUp(event.shiftKey, event.ctrlKey);
      case 'ArrowDown':
        return this.handleArrowDown(event.shiftKey, event.ctrlKey);
      case ' ':
        return this.handleSpace(event.shiftKey, event.ctrlKey);
      case 'a':
        if (event.ctrlKey) return this.handleSelectAll(event.shiftKey);
        break;
    }
    return false;
  }
}
```

### Visual Design System

#### Selection Highlighting
```css
/* CSS-based selection highlighting */
.base-node.selected {
  background-color: var(--selection-background);
  transition: background-color 200ms ease;
}

.base-node.selected:hover {
  background-color: var(--selection-background-hover);
}

/* Multi-select indicators */
.base-node.multi-select-mode.selected {
  border-left: 2px solid var(--selection-border);
}
```

#### Design Tokens
```css
/* Selection design tokens */
:root {
  --selection-background: var(--color-accent-subtle);
  --selection-background-hover: var(--color-accent-muted);
  --selection-border: var(--color-accent-border);
  --selection-transition: background-color 200ms ease;
}
```

## Hierarchical Visual System

### Modern Chevron Controls (Issue #31)

NodeSpace uses contemporary chevron/caret controls inspired by VS Code and modern IDEs:

#### Visual Design
- **Collapsed State**: Right-pointing chevron (`>`)
- **Expanded State**: Down-pointing chevron (`v`)
- **Hover Behavior**: Chevrons appear (opacity: 0 → 1) on node hover
- **Animation**: Smooth 90-degree rotation with 150ms transition

#### Implementation
```css
.node-chevron {
  opacity: 0;
  transition: opacity 150ms ease-in-out, transform 150ms ease-in-out;
  transform-origin: center;
}

.node-container:hover .node-chevron {
  opacity: 1;
}

.node-chevron.expanded {
  transform: rotate(90deg);
}
```

### Visual Connecting Lines

#### Clean Line System
- **Visual-Only**: Lines are purely decorative with no click interactions
- **Sibling Connection**: Vertical lines connect sibling nodes when parent has children
- **Consistent Styling**: 1px solid lines using design system border colors
- **Collapsed State**: Lines disappear when parent nodes are collapsed

#### CSS Implementation
```css
.node-children-container {
  position: relative;
  margin-left: var(--ns-spacing-6); /* 24px indentation */
}

.node-children {
  border-left: 1px solid var(--color-border-subtle);
}
```

### Hierarchy Indentation System

- **Progressive Indentation**: Consistent 24px per nesting level
- **Mobile-Friendly**: Optimized spacing for touch devices
- **Theme Integration**: Border colors adapt to light/dark themes
- **Performance**: Efficient CSS-based implementation

## Advanced Text Rendering

### Hybrid Syntax Rendering (Issue #34)

NodeSpace provides sophisticated text rendering that combines live markdown display with inline editing capabilities:

#### Architecture
- **Live Rendering**: Markdown syntax renders immediately as decorations
- **Inline Editing**: Click-to-edit without mode switching
- **Syntax Preservation**: Raw markdown remains editable while showing visual formatting
- **Performance**: Optimized for large documents with incremental updates

### Precise Cursor Positioning (Issue #33)

Advanced cursor positioning system for accurate text interaction:

#### Mock Element System
```typescript
// Mock element for precise cursor calculation
interface MockElementConfig {
  content: string;
  fontFamily: string;
  fontSize: string;
  fontWeight: string;
  lineHeight: string;
  width: number;
}

class CursorPositioning {
  static findCharacterFromClick(
    mockElement: HTMLElement,
    clickX: number,
    clickY: number,
    textareaRect: DOMRect
  ): number {
    // Algorithm for precise cursor positioning
    // using DOM measurement and character mapping
  }
}
```

#### Benefits
- **Pixel-Perfect Positioning**: Accurate cursor placement on complex text
- **Unicode Support**: Proper handling of emoji and multi-byte characters
- **Performance**: < 50ms positioning calculation
- **Cross-Browser**: Consistent behavior across modern browsers

### Interactive Node References

Rich preview system for node references and cross-links:

#### Features
- **Hover Previews**: Content preview on node reference hover
- **Click Navigation**: Quick navigation to referenced nodes
- **Contextual Information**: Metadata and relationship display
- **Performance**: Lazy-loaded previews to avoid performance impact

## Accessibility Architecture

### WCAG 2.1 Compliance

#### Keyboard Navigation
- **Full Keyboard Support**: All functionality accessible via keyboard
- **Logical Tab Order**: Predictable navigation sequence
- **Focus Management**: Clear focus indicators and state restoration
- **Keyboard Shortcuts**: Standard shortcuts with platform awareness

#### Screen Reader Support
```html
<!-- ARIA implementation for hierarchical selection -->
<div
  role="tree"
  aria-label="Hierarchical node tree"
  aria-multiselectable="true"
>
  <div
    role="treeitem"
    aria-selected={isSelected}
    aria-expanded={!collapsed}
    aria-level={depth}
    aria-setsize={siblingCount}
    aria-posinset={positionInSet}
  >
    <!-- Node content -->
  </div>
</div>
```

#### Visual Accessibility
- **High Contrast Support**: Colors meet WCAG AA standards
- **Focus Indicators**: 2px focus rings with sufficient contrast
- **Motion Respect**: Reduced motion support for transitions
- **Color Independence**: Information not conveyed by color alone

### Platform Integration

#### Operating System Conventions
- **Modifier Keys**: Ctrl (Windows/Linux) vs Cmd (Mac) detection
- **Selection Patterns**: OS-consistent multi-select behavior
- **Keyboard Shortcuts**: Platform-appropriate key combinations
- **Context Menus**: Native context menu integration

## Performance Optimization

### Rendering Efficiency

#### Selective Updates
- **Reactive Rendering**: Only update changed components
- **Virtual Scrolling**: Efficient handling of large hierarchies
- **Batch Operations**: Group DOM updates to prevent thrashing
- **CSS-Based Animations**: Hardware-accelerated transitions

#### Memory Management
- **Event Delegation**: Efficient event handling for large trees
- **State Cleanup**: Automatic cleanup of selection state
- **Component Lifecycle**: Proper mounting/unmounting patterns
- **Memory Profiling**: Regular memory leak detection

### Performance Targets

| Operation | Target | Measurement |
|-----------|---------|-------------|
| Selection Response | < 16ms | Click to visual feedback |
| Keyboard Navigation | < 16ms | Key press to focus change |
| Range Calculation | < 50ms | Large hierarchy traversal |
| Text Positioning | < 50ms | Click to cursor placement |
| Animation Smoothness | 60fps | Expand/collapse transitions |

## Node Hierarchy Interaction System

NodeSpace provides sophisticated hierarchical node interaction capabilities including collapse/expand controls, keyboard-driven indentation, and intelligent child node management.

### Current Implementation Status

#### Collapse/Expand System (Implemented)

**Visual Controls:**
- **Chevron Icons**: Modern right-pointing chevrons (16x16px) that rotate 90° to point down when expanded
- **Hover Behavior**: Chevrons appear on node hover with opacity transition (0 → 1)
- **Color Coordination**: Chevrons inherit node type colors from design system (`var(--node-text)`, `var(--node-task)`, etc.)
- **Positioning**: Centered between parent circle indicator and current node circle, with precise spacing

**Implementation Architecture:**
```typescript
// Node data structure with expanded state
interface NodeData {
  id: string;
  type: string;
  content: string;
  children: NodeData[];
  expanded: boolean; // Controls visibility of children
}

// Component integration
// BaseNodeViewer.svelte - Manages hierarchy display
// MinimalBaseNode.svelte - Individual node rendering
// Icon.svelte - Chevron icon system with rotation
```

**Spacing Architecture:**
- **Circle to Text Gap**: 24px (`1.5rem`) to accommodate chevron space
- **Chevron Positioning**: `right: -0.35rem` for optimal centering between indicators
- **Child Indentation**: 40px (`2.5rem`) to align with parent circle + gap spacing
- **Chevron Alignment**: `margin-top: 0.4rem` for vertical alignment with text

#### Basic Keyboard Navigation (Implemented)

**Current Tab/Shift+Tab Behavior:**
- **Tab**: Indent node (make child of previous sibling)
- **Shift+Tab**: Outdent node (make sibling of parent)
- **Integration**: Handled in BaseNodeViewer with cursor position preservation

**Current Limitations:**
- No node type compatibility checking
- No advanced child transfer logic
- No collapsed state awareness in hierarchy operations

### Advanced Indentation System (Planned - Issue #TBD)

Based on analysis of NodeSpace core-ui implementation, the following advanced features are planned:

#### Node Type Compatibility System

**`canHaveChildren` Property:**
```typescript
// MinimalBaseNode enhancement
export let canHaveChildren: boolean = true; // Svelte property pattern

// Node type specific overrides
// ai-chat nodes: canHaveChildren = false
// text nodes: canHaveChildren = true (default)
// task nodes: canHaveChildren = true (default)
```

#### Enhanced Indentation Rules

**Tab (Indent) Rules:**
1. Cannot indent if no previous sibling exists
2. Cannot indent if previous sibling has `canHaveChildren = false`
3. Node becomes child of previous sibling when valid

**Shift+Tab (Outdent) Rules:**
1. Cannot outdent root nodes
2. Next immediate sibling becomes child of outdented node (hierarchy preservation)
3. Maintains relative positioning in tree structure

#### Sophisticated Child Transfer Logic

**When Deleting Nodes with Children:**
- **Depth-aware transfer**: Root node children vs nested node children handled differently
- **Collapsed state awareness**: Insert position depends on target's collapsed state
- **Auto-expansion**: Nodes receiving children automatically expand

**Backspace/Delete Scenarios:**
```typescript
// Child transfer rules based on node depth and target state
transferChildrenWithDepthPreservation(
  sourceNode: BaseNode,
  targetNode: BaseNode, 
  collapsedNodes: Set<string>
): void {
  // Insert at beginning if target was collapsed
  // Insert at end if target was expanded
  // Auto-expand target after receiving children
}
```

#### Enter Key Enhancement

**Content Splitting with Children:**
- **Collapsed parent**: Children stay with original (left) node
- **Expanded parent**: Children move to new (right) node
- **Preserves hierarchy intent** based on visual state

### Implementation Integration Points

#### Component Architecture
```
BaseNodeViewer.svelte
├── Node hierarchy management
├── Collapse/expand state tracking
├── Keyboard event handling
├── Child transfer orchestration
└── Visual layout coordination

MinimalBaseNode.svelte
├── Individual node rendering
├── canHaveChildren property
├── Icon display logic
├── Keyboard shortcuts
└── Content editing

TextNode.svelte
├── Text-specific behaviors
├── Header level inheritance
├── Markdown formatting
└── Content persistence
```

#### State Management
```typescript
// Enhanced node structure
interface EnhancedNodeData extends NodeData {
  canHaveChildren: boolean;
  nodeType: string; // For type-specific rules
  depth: number; // For depth-aware operations
}

// Collapsed state tracking
const collapsedNodes = new Set<string>();
```

### Future Extensions

#### Planned Enhancements
- **Bulk Operations**: Context menus for selected blocks
- **Copy/Paste Integration**: Multi-block clipboard operations
- **Drag and Drop**: Visual drag-drop for hierarchy reorganization
- **Touch Gestures**: Mobile-optimized selection patterns
- **Advanced Keyboard**: Vim-style navigation modes
- **Smart Hierarchy Operations**: AI-assisted node organization

#### Extension Points
- **Selection Plugins**: Custom selection behavior for node types
- **Interaction Modes**: Alternative interaction patterns
- **Visual Themes**: Customizable selection and hierarchy styling
- **Accessibility Extensions**: Enhanced screen reader support
- **Node Type Rules**: Extensible compatibility system for new node types

### Performance Considerations

**Current Optimizations:**
- CSS-based chevron animations (hardware accelerated)
- Efficient DOM updates with Svelte reactivity
- Minimal re-renders during hierarchy operations

**Planned Optimizations:**
- Batch hierarchy operations for large trees
- Virtual scrolling for deep hierarchies
- Debounced state persistence
- Memory-efficient collapsed state tracking

This comprehensive hierarchy interaction system provides the foundation for sophisticated knowledge management workflows while maintaining performance and accessibility standards.