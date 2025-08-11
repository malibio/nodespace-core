# NodeSpace Component Inheritance Architecture

**Last Updated:** 2025-08-11  
**Status:** Active  
**Related ADRs:** ADR-001, ADR-002, ADR-003

## Overview

NodeSpace uses **Svelte component composition** to implement inheritance between node types. All nodes extend BaseNode through explicit prop overrides, creating a clear hierarchy that's easy to understand and maintain.

## Core Principles

### 1. Component Composition Pattern
```svelte
<!-- Child components extend parent by wrapping and overriding -->
<ParentComponent propA={override} propB={default} on:events>
  <!-- Optional child-specific content -->
</ParentComponent>
```

### 2. Explicit Overrides
- **Inheritance is visible:** You can see exactly what each node type customizes
- **Type safety:** TypeScript validates all prop overrides
- **Event delegation:** Events bubble naturally from parent to child

### 3. Minimal Surface Area
- **BaseNode has minimal, stable API** to reduce coupling
- **Overrides are minimal** - only change what's actually different
- **Sensible defaults** so most node types work with few overrides

## Component Hierarchy

```
BaseNode (foundation)
├── TextNode (multiline + markdown)
├── TaskNode (single-line + task shortcuts)  
├── PersonNode (read-only + computed content)
├── EntityNode (read-only + entity linking)
└── QueryNode (single-line + query syntax)
```

## BaseNode Foundation

BaseNode provides the universal editing foundation for all node types.

### Core Props
```typescript
interface BaseNodeProps {
  // Identity
  nodeId: string;
  nodeType: NodeType;
  
  // Content  
  content: string;
  
  // Editor Configuration
  multiline: boolean;        // false: single-line, true: multiline
  markdown: boolean;         // false: plain text, true: markdown syntax
  contentEditable: boolean;  // false: read-only, true: editable
  
  // Presentation
  placeholder: string;
  className: string;
  
  // Features
  editable: boolean;         // Can this node be edited at all?
  hasChildren: boolean;      // Does this node have child nodes?
  
  // Processing States
  isProcessing: boolean;
  processingAnimation: 'blink' | 'pulse' | 'spin' | 'fade';
  processingIcon: IconName;
  
  // SVG Icon System
  iconName: IconName;
}
```

### Events Dispatched
```typescript
interface BaseNodeEvents {
  click: { nodeId: string; event: MouseEvent };
  contentChanged: { nodeId: string; content: string };  // Debounced
  focus: { nodeId: string };
  blur: { nodeId: string };
}
```

### Default Behavior
- **Always-editing mode:** CodeMirror always visible, no mode switching
- **Single-line editing:** Default for most node types
- **Plain text:** No markdown highlighting by default
- **Editable:** Users can modify content by default
- **Debounced events:** contentChanged fires when user pauses typing

## Node Type Implementations

### TextNode: Multiline Markdown Editor

**Purpose:** Long-form markdown content with hybrid rendering

```svelte
<!-- TextNode.svelte -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let content: string;
  // TextNode-specific props could be added here
</script>

<BaseNode 
  {nodeId}
  {content}
  multiline={true}     <!-- Override: Enable multiline editing -->
  markdown={true}      <!-- Override: Enable markdown syntax highlighting -->
  placeholder="Start writing..."
  on:contentChanged    <!-- Event bubbles up to parent -->
/>
```

**Key Overrides:**
- `multiline={true}` - Allows line breaks and vertical expansion
- `markdown={true}` - Enables syntax highlighting and hybrid rendering

### PersonNode: Read-Only Computed Content

**Purpose:** Display person information composed from structured data

```svelte
<!-- PersonNode.svelte -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let firstName: string;
  export let lastName: string;
  export let email: string;
  
  // Computed content from structured props
  $: content = `${firstName} ${lastName} ${email}`;
</script>

<BaseNode 
  {nodeId}
  {content}
  contentEditable={false}  <!-- Override: Read-only content -->
  placeholder="No person data"
  iconName="user"
  on:click                 <!-- Click might open person editor -->
/>
```

**Key Overrides:**
- `contentEditable={false}` - Makes content read-only
- **Computed content** - Content derived from firstName, lastName, email
- Visual styling through design system (muted colors for read-only)

### TaskNode: Single-Line with Shortcuts

**Purpose:** Task management with keyboard shortcuts

```svelte
<!-- TaskNode.svelte -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let content: string;
  export let completed: boolean = false;
  
  // Task-specific event handling
  function handleKeyDown(event) {
    if (event.key === 'Enter' && event.ctrlKey) {
      toggleCompleted();
    }
  }
</script>

<BaseNode 
  {nodeId}
  {content}
  multiline={false}       <!-- Override: Single-line only -->
  markdown={false}        <!-- Override: Plain text (no markdown) -->
  placeholder="Add a task..."
  iconName={completed ? "check-circle" : "circle"}
  className={completed ? "completed" : ""}
  on:contentChanged
  on:keydown={handleKeyDown}
/>
```

**Key Overrides:**
- `multiline={false}` - Ensures single-line behavior
- `markdown={false}` - Disables markdown (unnecessary for tasks)
- **Task-specific shortcuts** - Ctrl+Enter to toggle completion

### EntityNode: Read-Only with Linking

**Purpose:** Entity references with linking to entity details

```svelte
<!-- EntityNode.svelte -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let entityId: string;
  export let entityType: string;
  export let displayName: string;
  
  $: content = displayName;
  
  function handleEntityClick() {
    // Navigate to entity details
    goto(`/entities/${entityType}/${entityId}`);
  }
</script>

<BaseNode 
  {nodeId}
  {content}
  contentEditable={false}  <!-- Override: Read-only entity reference -->
  iconName="link"
  className="entity-link"
  on:click={handleEntityClick}
/>
```

**Key Overrides:**
- `contentEditable={false}` - Entity names aren't directly editable
- **Click handling** - Navigate to entity details
- **Entity-specific styling** - Link appearance

## Implementation Guidelines

### Creating New Node Types

1. **Start with BaseNode:** Always extend BaseNode, never start from scratch
2. **Minimal Overrides:** Only change props that are actually different
3. **Document Overrides:** Comment why specific props are overridden
4. **Test Inheritance:** Ensure BaseNode events and behavior work correctly

```svelte
<!-- NewNodeType.svelte -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  // 1. Define node-specific props
  export let nodeId: string;
  export let content: string;
  export let customProp: string;
  
  // 2. Node-specific logic if needed
  function handleCustomLogic() {
    // ...
  }
</script>

<!-- 3. Extend BaseNode with minimal overrides -->
<BaseNode 
  {nodeId}
  {content}
  propToOverride={customValue}  <!-- Only override what's different -->
  on:contentChanged             <!-- Always bubble events -->
  on:customEvent={handleCustomLogic}
/>
```

### Event Handling Pattern

Events flow naturally through the component hierarchy:

```
CodeMirror → BaseNode → SpecificNode → Parent Component
    ↓           ↓           ↓              ↓
  (native)  (dispatch)  (bubble)      (handle)
```

Example event flow for content changes:
```svelte
<!-- TextNode.svelte -->
<BaseNode on:contentChanged={handleContentChanged} />

<script>
  function handleContentChanged(event) {
    // TextNode-specific handling
    content = event.detail.content;
    
    // Auto-save logic for TextNode
    if (autoSave) {
      saveContent(content);
    }
    
    // Bubble to parent
    dispatch('contentChanged', event.detail);
  }
</script>
```

### Styling and Theming

Node types inherit BaseNode styling and can add specific overrides:

```svelte
<!-- PersonNode.svelte -->
<BaseNode 
  className="person-node"
  contentEditable={false}
/>

<style>
  /* Node-specific styling */
  :global(.person-node) {
    /* Inherit BaseNode styles */
    color: hsl(var(--muted-foreground)); /* Read-only styling */
    cursor: default;
  }
  
  :global(.person-node:hover) {
    /* Person-specific hover state */
    background: hsl(var(--accent) / 0.1);
  }
</style>
```

## Testing Strategy

### Inheritance Testing
```typescript
// Ensure all node types properly extend BaseNode
test('TextNode inherits BaseNode events', () => {
  const { component } = render(TextNode, {
    nodeId: 'test',
    content: 'Hello'
  });
  
  // Should inherit BaseNode event interface
  expect(component.$on('contentChanged')).toBeDefined();
  expect(component.$on('focus')).toBeDefined();
  expect(component.$on('blur')).toBeDefined();
  
  // Should have TextNode-specific overrides
  expect(component.multiline).toBe(true);
  expect(component.markdown).toBe(true);
});
```

### Override Testing
```typescript  
// Ensure overrides work correctly
test('PersonNode is read-only', () => {
  const { getByRole } = render(PersonNode, {
    nodeId: 'person-1',
    firstName: 'John',
    lastName: 'Doe',
    email: 'john@example.com'
  });
  
  const editor = getByRole('textbox');
  expect(editor).toHaveAttribute('contenteditable', 'false');
});
```

## Related Documentation

- **ADR-001:** Always-Editing Mode Architecture
- **ADR-002:** Component Composition Inheritance Pattern
- **ADR-003:** Universal CodeMirror Strategy
- **Design System:** Component styling guidelines
- **Testing Guide:** Component testing patterns