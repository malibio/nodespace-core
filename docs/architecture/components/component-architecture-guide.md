# NodeSpace Component Architecture Guide

## Overview

NodeSpace follows a **consistent, hierarchical component architecture** with clear naming conventions that separate concerns and provide intuitive developer experience. This guide documents the established patterns for building new components.

## Architectural Hierarchy

### 1. Foundation Layer

**BaseNode** (`src/lib/design/components/base-node.svelte`)
- **Purpose**: Abstract core component providing fundamental node functionality
- **Usage**: INTERNAL USE ONLY - should never be used directly in application code
- **Responsibilities**:
  - Content editing via **TextareaController** (see [Textarea Architecture](./textarea-editor-architecture.md))
  - Dual-mode rendering (edit: `<textarea>`, view: rendered markdown)
  - Markdown syntax handling
  - Keyboard shortcuts (Cmd+B, Cmd+I for formatting)
  - Event dispatch and management via keyboard command system
  - Focus management integration with FocusManager service
  - Base styling and layout

**BaseNodeViewer** (`src/lib/design/components/base-node-viewer.svelte`)
- **Purpose**: Container that manages collections of nodes
- **Usage**: Main entry point for page-level node management
- **Responsibilities**:
  - Node tree operations (create, delete, indent, outdent)
  - Navigation between nodes (arrow keys, tab)
  - Plugin registry integration
  - Dynamic node component loading
  - Focus management

### 2. Node Components Layer

**Naming Convention**: `*Node` (e.g., TextNode, TaskNode, DateNode)

**Purpose**: Individual node implementations that wrap BaseNode with type-specific functionality

**Pattern**:
```svelte
<!-- TextNode.svelte -->
<script>
  import BaseNode from '$lib/design/components/base-node.svelte';
  // Add node-specific logic here
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {content}
  {autoFocus}
  on:contentChanged
  on:createNewNode
  <!-- Forward all BaseNode events -->
/>
```

**Current Implementations**:
- **TextNode** (`src/lib/components/viewers/text-node.svelte`)
  - Header-aware multiline logic
  - Smart editing based on header level
  - Markdown processing integration

- **TaskNode** (`src/lib/design/components/task-node.svelte`)
  - Task state management (pending/inProgress/completed)
  - Checkbox interaction
  - Task-specific metadata handling

- **DateNode** (`src/lib/design/components/date-node.svelte`)
  - Date formatting and validation
  - Calendar icon integration
  - Date parsing utilities
  - Cannot be created via slash commands (exists implicitly for all dates)

- **CodeBlockNode** (`src/lib/design/components/code-block-node.svelte`)
  - Multiline code editing with language selection
  - Markdown processing bypass via metadata (`disableMarkdown: true`)
  - Three-phase content transformation (storage/edit/view)
  - Language dropdown and copy button UI
  - Leaf node (cannot have children)
  - Auto-completion for fence syntax

### 3. Viewer Components Layer

**Naming Convention**: `*NodeViewer` (e.g., DateNodeViewer, TaskNodeViewer)

**Purpose**: Page-level components that wrap BaseNodeViewer with specialized UI and navigation

**Pattern**:
```svelte
<!-- DateNodeViewer.svelte -->
<script>
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';
  // Add viewer-specific UI (navigation, headers, etc.)
</script>

<div class="specialized-viewer">
  <!-- Custom UI elements (navigation, toolbars, etc.) -->
  <div class="viewer-header">
    <!-- Navigation controls, date picker, etc. -->
  </div>

  <!-- Node content area -->
  <div class="node-content-area">
    <BaseNodeViewer />
  </div>
</div>
```

**Current Implementations**:
- **DateNodeViewer** (`src/lib/components/viewers/date-node-viewer.svelte`)
  - Date navigation header
  - Previous/next day functionality
  - Keyboard navigation support
  - Tab title management

**Future Implementations**:
- **TaskNodeViewer** - Project management interface with Gantt charts, filters
- **EntityNodeViewer** - Structured data management interface
- **QueryNodeViewer** - Live query results with real-time updates

## Naming Convention Rules

### ✅ Correct Naming Patterns

| Component Type | Pattern | Example | Purpose |
|---------------|---------|---------|---------|
| **Foundation** | `Base*` | `BaseNode`, `BaseNodeViewer` | Core abstract components |
| **Node Components** | `*Node` | `TextNode`, `TaskNode`, `DateNode` | Individual node wrappers |
| **Viewer Components** | `*NodeViewer` | `DateNodeViewer`, `TaskNodeViewer` | Page-level viewer wrappers |

### ❌ Incorrect Patterns (Avoid These)

- `TextNodeViewer` - Text nodes don't need viewers (BaseNodeViewer is sufficient)
- `DatePageViewer` - Should be `DateNodeViewer` to follow convention
- `BaseTextNode` - Base components should be generic, not type-specific

## File Structure Conventions

```
src/lib/
├── design/components/           # Foundation layer
│   ├── base-node.svelte        # Abstract core component
│   ├── base-node-viewer.svelte # Container management
│   └── task-node.svelte        # Node components (some)
└── components/viewers/          # Node and viewer components
    ├── text-node.svelte        # Text node implementation
    ├── date-node.svelte        # Date node implementation
    └── date-node-viewer.svelte  # Date viewer implementation
```

**Guidelines**:
- **Foundation components** → `src/lib/design/components/`
- **Node & Viewer components** → `src/lib/components/viewers/`
- Use kebab-case for filenames
- Component names match file names (PascalCase in code)

## Plugin Integration

### Registering New Node Types

```typescript
// In src/lib/plugins/corePlugins.ts
export const customNodePlugin: PluginDefinition = {
  id: 'custom',
  name: 'Custom Node',
  description: 'Custom node type with special functionality',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'custom',
        name: 'Custom',
        description: 'Create a custom node',
        contentTemplate: ''
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },
  node: {
    lazyLoad: () => import('../components/viewers/custom-node.svelte'),
    priority: 1
  },
  // Optional: Add viewer for complex page-level functionality
  viewer: {
    lazyLoad: () => import('../components/viewers/custom-node-viewer.svelte'),
    priority: 1
  }
};
```

### Component Implementation Template

#### Node Component Template

```svelte
<!-- custom-node.svelte -->
<!--
  CustomNode - Brief description of functionality

  Specialized wrapper around BaseNode that provides:
  - Feature 1
  - Feature 2
  - Feature 3
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '$lib/design/components/base-node.svelte';
  import type { NodeViewerProps } from '$lib/types/nodeViewers.js';

  // Props following NodeViewerProps interface
  let {
    nodeId,
    content = '',
    autoFocus = false,
    nodeType = 'custom',
    inheritHeaderLevel = 0,
    children = []
  }: NodeViewerProps = $props();

  const dispatch = createEventDispatcher();

  // Custom node-specific logic here

  // Event forwarding helper
  function forwardEvent<T>(eventName: string) {
    return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
  }
</script>

<!-- Custom UI elements if needed -->
{#if customUINeeded}
  <div class="custom-controls">
    <!-- Custom controls here -->
  </div>
{/if}

<!-- Wrapped BaseNode -->
<BaseNode
  {nodeId}
  {nodeType}
  {autoFocus}
  {content}
  headerLevel={inheritHeaderLevel}
  {children}
  on:createNewNode={forwardEvent('createNewNode')}
  on:contentChanged={forwardEvent('contentChanged')}
  <!-- Forward all other BaseNode events -->
/>

<style>
  .custom-controls {
    /* Custom styling */
  }
</style>
```

#### Viewer Component Template

```svelte
<!-- custom-node-viewer.svelte -->
<!--
  CustomNodeViewer - Brief description of viewer functionality

  Page-level viewer that wraps BaseNodeViewer with:
  - Navigation elements
  - Specialized UI
  - Custom controls
-->

<script lang="ts">
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';

  // Viewer-specific props
  let { customProp = 'default' }: { customProp?: string } = $props();

  // Viewer-specific logic
</script>

<div class="custom-node-viewer">
  <!-- Custom viewer header/navigation -->
  <div class="viewer-header">
    <!-- Navigation, filters, controls, etc. -->
  </div>

  <!-- Node content area -->
  <div class="node-content-area">
    <BaseNodeViewer />
  </div>
</div>

<style>
  .custom-node-viewer {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .viewer-header {
    flex-shrink: 0;
    /* Header styling */
  }

  .node-content-area {
    flex: 1;
    overflow-y: auto;
    /* Content area styling */
  }
</style>
```

## Advanced Patterns

### Metadata-Driven Extension (CodeBlockNode Example)

CodeBlockNode demonstrates how to extend BaseNode functionality without modifying its core API, using the metadata-driven extension pattern:

**Pattern Overview**:
- BaseNode accepts a `metadata` prop with arbitrary properties
- Node components can add custom metadata flags to control BaseNode behavior
- This maintains BaseNode's generality while enabling specialized behavior

**CodeBlockNode Implementation**:

```svelte
<script lang="ts">
  import BaseNode from '$lib/design/components/base-node.svelte';

  // Parse language from fence syntax
  let language = $state<string>(parseLanguage(content));

  // Create reactive metadata with custom flags
  let codeMetadata = $derived({
    language,
    disableMarkdown: true  // Tell BaseNode to skip markdown processing
  });
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {content}
  metadata={codeMetadata}
  on:contentChanged
  on:createNewNode
/>
```

**BaseNode Metadata Extension Points**:
```typescript
/**
 * Metadata extension points supported by BaseNode:
 *
 * - disableMarkdown: boolean
 *   Skip markdown processing (e.g., for code blocks, raw text)
 *   Implementation: base-node.svelte lines 556-591
 *
 * - language: string
 *   Language identifier (e.g., for code blocks)
 *   Usage: Stored in node metadata, displayed in UI
 *
 * - headerLevel: number
 *   Header level 1-6 (for header nodes)
 *   Usage: Controls markdown heading syntax
 *
 * - taskState: string
 *   Task completion state (for task nodes)
 *   Values: 'pending' | 'inProgress' | 'completed'
 */
```

**Content Transformation Pattern (CodeBlockNode)**:

CodeBlockNode demonstrates a three-phase content transformation approach:

```svelte
<script lang="ts">
  // Phase 1: Storage format (with language in fence)
  // Stored: ```javascript\ncode\n```
  let internalContent = $state(content);

  // Phase 2: Edit format (fences without language)
  // User edits: ```\ncode\n``` (language managed by dropdown)
  function extractCodeForEditing(content: string): string {
    return content.replace(/^```\w+/, '```');
  }
  let editContent = $derived(extractCodeForEditing(internalContent));

  // Phase 3: Display format (no fences, just code)
  // User sees: code (fences become empty lines for spacing)
  function extractCodeForDisplay(content: string): string {
    let result = content.replace(/^```\w*/, '');
    result = result.replace(/```$/, '\n');
    return result;
  }
  let displayContent = $derived(extractCodeForDisplay(internalContent));
</script>

<BaseNode
  content={editContent}
  {displayContent}
  metadata={{ disableMarkdown: true }}
/>
```

**Benefits of This Pattern**:
- ✅ **No BaseNode modification** - Maintains core component stability
- ✅ **Type-specific behavior** - Each node type controls its own rendering
- ✅ **Clear separation** - Storage, editing, and display logic isolated
- ✅ **Reusable pattern** - Other node types can follow the same approach

**Leaf Node Pattern (CodeBlockNode)**:

Some node types should not have children. CodeBlockNode demonstrates proper leaf node implementation:

```typescript
// In plugin configuration (core-plugins.ts)
export const codeBlockNodePlugin: PluginDefinition = {
  config: {
    canHaveChildren: false,  // Leaf node constraint
    canBeChild: true         // Can be nested under other nodes
  }
};

// Backend validation (mod.rs - Rust)
pub struct CodeBlockNodeBehavior;

impl NodeBehavior for CodeBlockNodeBehavior {
    fn can_have_children(&self) -> bool {
        false  // Enforced at backend level
    }
}
```

**Icon Registry Configuration**:
```typescript
// Icon registry (registry.ts)
iconRegistry.register('code-block', {
  component: CodeBlockIcon,
  semanticClass: 'node-icon',
  colorVar: 'hsl(var(--node-text, 200 40% 45%))',
  hasState: false,
  hasRingEffect: false  // No ring because leaf nodes have no children
});
```

## Development Guidelines

### When to Create Each Component Type

**Create a Node Component When**:
- You need specialized behavior for individual nodes
- The node type has unique data or interaction patterns
- You need custom rendering or editing logic
- Example: TaskNode (checkbox, state), DateNode (formatting)

**Create a Viewer Component When**:
- You need page-level navigation or controls
- The viewing experience requires specialized UI
- You need custom layouts or organizational features
- Example: DateNodeViewer (date navigation), TaskNodeViewer (project views)

**Don't Create When**:
- BaseNodeViewer already handles the use case adequately
- The functionality could be added to an existing component
- Example: TextNode doesn't need a TextNodeViewer

### Event Handling Patterns

**Node Components** should forward all BaseNode events:
```typescript
// Standard event forwarding pattern
function forwardEvent<T>(eventName: string) {
  return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
}

// Usage
on:contentChanged={forwardEvent('contentChanged')}
on:createNewNode={forwardEvent('createNewNode')}
```

**Viewer Components** handle their own UI events and delegate node management to BaseNodeViewer:
```typescript
// Viewer-specific event handlers
function handleNavigationClick() {
  // Custom navigation logic
}

function handleFilterChange(filter: string) {
  // Custom filtering logic
}
```

### Styling Conventions

**CSS Class Naming**:
```css
/* Component root */
.component-name-node { }      /* Node components */
.component-name-node-viewer { } /* Viewer components */

/* Sub-elements */
.component-name-node .controls { }
.component-name-node-viewer .header { }
.component-name-node-viewer .content-area { }
```

**Design System Integration**:
```css
/* Use CSS custom properties */
color: hsl(var(--foreground));
background: hsl(var(--background));
border: 1px solid hsl(var(--border));

/* Use semantic spacing */
padding: var(--spacing-4);
margin: var(--spacing-2);
```

## Testing Patterns

### Component Testing Strategy

```typescript
// Node component tests
describe('CustomNode', () => {
  test('forwards all BaseNode events', () => {
    // Test event forwarding
  });

  test('applies custom logic correctly', () => {
    // Test node-specific functionality
  });
});

// Viewer component tests
describe('CustomNodeViewer', () => {
  test('renders navigation correctly', () => {
    // Test viewer UI
  });

  test('integrates with BaseNodeViewer', () => {
    // Test BaseNodeViewer integration
  });
});
```

## Migration Guide

### Converting Old Components

If you have components that don't follow the new naming convention:

1. **Identify the component type** (Node or Viewer)
2. **Rename following the pattern**: `*Node` or `*NodeViewer`
3. **Update all imports** throughout the codebase
4. **Update plugin registrations**
5. **Update CSS class names** to match
6. **Test thoroughly** with type checking

### Example Migration

```typescript
// Before (incorrect naming)
import DatePageViewer from './date-page-viewer.svelte';
import TextNodeViewer from './text-node-viewer.svelte';

// After (correct naming)
import DateNodeViewer from './date-node-viewer.svelte';
import TextNode from './text-node.svelte';
```

## Summary

This architecture provides:
- **Consistent naming** that indicates component purpose
- **Clear separation** between node-level and page-level concerns
- **Extensible patterns** for new node types
- **Maintainable code** with predictable structure
- **Type safety** with proper interfaces

Follow these patterns for all new component development to maintain architectural consistency and developer productivity.