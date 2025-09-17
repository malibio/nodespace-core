# External Plugin Development Guide

**Status**: Current (December 2024)
**Audience**: External developers wanting to contribute node types
**Last Updated**: December 17, 2024

## Overview

This guide explains how external developers can create custom node types for NodeSpace using the current unified plugin registry system. While we're planning enhanced developer tools for the future, this guide covers the current process for contributing plugins.

## Current Development Process

### Prerequisites

- Node.js 18+ and Bun runtime
- Git and GitHub account
- Basic understanding of Svelte and TypeScript
- Familiarity with NodeSpace's architecture

### Development Environment Setup

```bash
# 1. Fork and clone NodeSpace repository
git clone https://github.com/yourusername/nodespace-core.git
cd nodespace-core

# 2. Install dependencies
bun install

# 3. Start development server
bun run dev
```

## Plugin Architecture Overview

### Core Concepts

NodeSpace plugins are defined using the `PluginDefinition` interface and registered through the unified registry system:

```typescript
export interface PluginDefinition {
  // Plugin metadata
  id: string;                    // Unique identifier
  name: string;                  // Display name
  description: string;           // Description
  version: string;              // Version number

  // Node type configuration
  config: {
    slashCommands: SlashCommandDefinition[];
    canHaveChildren?: boolean;
    canBeChild?: boolean;
  };

  // Component registrations (optional)
  viewer?: {
    component?: NodeViewerComponent;
    lazyLoad?: () => Promise<{ default: NodeViewerComponent }>;
    priority?: number;
  };

  reference?: {
    component: NodeReferenceComponent;
    priority?: number;
  };
}
```

### Component Types

**Viewer Component**: Full-featured component for editing and displaying node content
**Reference Component**: Lightweight component for showing node references

## Step-by-Step Plugin Development

### Step 1: Create Plugin Definition

Create your plugin definition in the core plugins file:

```typescript
// packages/desktop-app/src/lib/plugins/corePlugins.ts

export const whiteboardNodePlugin: PluginDefinition = {
  id: 'whiteboard',
  name: 'Whiteboard Node',
  description: 'Interactive whiteboard with drawing tools',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'whiteboard',
        name: 'Whiteboard',
        description: 'Create an interactive whiteboard',
        contentTemplate: ''
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },
  viewer: {
    lazyLoad: () => import('$lib/components/viewers/WhiteboardViewer.svelte'),
    priority: 1
  },
  reference: {
    component: WhiteboardReference,
    priority: 1
  }
};
```

### Step 2: Create Viewer Component

Create the main viewer component:

```svelte
<!-- packages/desktop-app/src/lib/components/viewers/WhiteboardViewer.svelte -->
<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import type { Node } from '$lib/types/node';

  export let node: Node;
  export let isSelected: boolean = false;
  export let isEditing: boolean = false;

  const dispatch = createEventDispatcher();

  let canvasElement: HTMLCanvasElement;
  let canvasContext: CanvasRenderingContext2D;
  let isDrawing = false;

  onMount(() => {
    if (canvasElement) {
      canvasContext = canvasElement.getContext('2d')!;
      initializeCanvas();
    }
  });

  function initializeCanvas() {
    // Set up canvas for drawing
    canvasContext.lineWidth = 2;
    canvasContext.lineCap = 'round';
    canvasContext.strokeStyle = '#000000';

    // Load existing drawing data if available
    if (node.content) {
      loadDrawingData(node.content);
    }
  }

  function startDrawing(event: MouseEvent) {
    if (!isEditing) return;

    isDrawing = true;
    const rect = canvasElement.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;

    canvasContext.beginPath();
    canvasContext.moveTo(x, y);
  }

  function draw(event: MouseEvent) {
    if (!isDrawing || !isEditing) return;

    const rect = canvasElement.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;

    canvasContext.lineTo(x, y);
    canvasContext.stroke();
  }

  function stopDrawing() {
    if (!isDrawing) return;

    isDrawing = false;
    canvasContext.closePath();

    // Save drawing data
    const drawingData = canvasElement.toDataURL();
    dispatch('content-changed', {
      content: drawingData
    });
  }

  function clearCanvas() {
    if (!isEditing) return;

    canvasContext.clearRect(0, 0, canvasElement.width, canvasElement.height);
    dispatch('content-changed', {
      content: ''
    });
  }

  function loadDrawingData(dataUrl: string) {
    const img = new Image();
    img.onload = () => {
      canvasContext.clearRect(0, 0, canvasElement.width, canvasElement.height);
      canvasContext.drawImage(img, 0, 0);
    };
    img.src = dataUrl;
  }
</script>

<div class="whiteboard-viewer" class:editing={isEditing} class:selected={isSelected}>
  <div class="whiteboard-header">
    <h3>Whiteboard</h3>
    {#if isEditing}
      <div class="whiteboard-tools">
        <button on:click={clearCanvas} class="clear-btn">
          Clear
        </button>
      </div>
    {/if}
  </div>

  <div class="whiteboard-canvas-container">
    <canvas
      bind:this={canvasElement}
      width="600"
      height="400"
      on:mousedown={startDrawing}
      on:mousemove={draw}
      on:mouseup={stopDrawing}
      on:mouseleave={stopDrawing}
      class:editable={isEditing}
    />
  </div>

  {#if !isEditing && !node.content}
    <div class="empty-state">
      <p>Empty whiteboard - click to edit and start drawing</p>
    </div>
  {/if}
</div>

<style>
  .whiteboard-viewer {
    border: 1px solid #e1e5e9;
    border-radius: 8px;
    padding: 16px;
    background: white;
    transition: all 0.2s ease;
  }

  .whiteboard-viewer.selected {
    border-color: #007acc;
    box-shadow: 0 0 0 2px rgba(0, 122, 204, 0.2);
  }

  .whiteboard-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid #f0f0f0;
  }

  .whiteboard-header h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: #1a1a1a;
  }

  .whiteboard-tools {
    display: flex;
    gap: 8px;
  }

  .clear-btn {
    padding: 6px 12px;
    border: 1px solid #dc3545;
    border-radius: 4px;
    background: white;
    color: #dc3545;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.2s ease;
  }

  .clear-btn:hover {
    background: #dc3545;
    color: white;
  }

  .whiteboard-canvas-container {
    display: flex;
    justify-content: center;
    margin: 12px 0;
  }

  canvas {
    border: 1px solid #e1e5e9;
    border-radius: 4px;
    background: white;
  }

  canvas.editable {
    cursor: crosshair;
  }

  .empty-state {
    text-align: center;
    padding: 40px 20px;
    color: #666;
    font-style: italic;
  }
</style>
```

### Step 3: Create Reference Component

Create the lightweight reference component:

```svelte
<!-- packages/desktop-app/src/lib/components/references/WhiteboardReference.svelte -->
<script lang="ts">
  import type { Node } from '$lib/types/node';

  export let node: Node;
  export let className = '';

  // Create a mini preview of the whiteboard
  let canvasElement: HTMLCanvasElement;

  $: if (canvasElement && node.content) {
    drawPreview();
  }

  function drawPreview() {
    const ctx = canvasElement.getContext('2d')!;
    const img = new Image();

    img.onload = () => {
      // Clear and draw scaled down version
      ctx.clearRect(0, 0, canvasElement.width, canvasElement.height);
      ctx.drawImage(img, 0, 0, 120, 80);
    };

    img.src = node.content;
  }
</script>

<span class="whiteboard-reference {className}">
  <span class="whiteboard-icon">🎨</span>
  <span class="whiteboard-label">Whiteboard</span>

  {#if node.content}
    <canvas
      bind:this={canvasElement}
      width="120"
      height="80"
      class="whiteboard-preview"
    />
  {:else}
    <span class="empty-whiteboard">Empty</span>
  {/if}
</span>

<style>
  .whiteboard-reference {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: #f8f9fa;
    border: 1px solid #e1e5e9;
    border-radius: 4px;
    font-size: 14px;
    color: #495057;
    text-decoration: none;
    transition: all 0.2s ease;
  }

  .whiteboard-reference:hover {
    background: #e9ecef;
    border-color: #ced4da;
  }

  .whiteboard-icon {
    font-size: 16px;
  }

  .whiteboard-label {
    font-weight: 500;
  }

  .whiteboard-preview {
    border-radius: 2px;
    border: 1px solid #dee2e6;
  }

  .empty-whiteboard {
    font-size: 12px;
    color: #6c757d;
    font-style: italic;
  }
</style>
```

### Step 4: Register Plugin

Add your plugin to the core plugins array:

```typescript
// packages/desktop-app/src/lib/plugins/corePlugins.ts

import WhiteboardReference from '$lib/components/references/WhiteboardReference.svelte';

// ... existing imports and plugins ...

export const corePlugins = [
  textNodePlugin,
  taskNodePlugin,
  aiChatNodePlugin,
  dateNodePlugin,
  userNodePlugin,
  documentNodePlugin,
  whiteboardNodePlugin  // Add your plugin here
];
```

### Step 5: Add Component Imports

Update the references index to include your reference component:

```typescript
// packages/desktop-app/src/lib/components/references/index.ts

import WhiteboardReference from './WhiteboardReference.svelte';

// ... existing exports ...

export { WhiteboardReference };
```

### Step 6: Create Tests

Create comprehensive tests for your plugin:

```typescript
// packages/desktop-app/src/tests/plugins/whiteboard-plugin.test.ts

import { describe, it, expect, beforeEach } from 'vitest';
import { PluginRegistry } from '$lib/plugins/pluginRegistry';
import { whiteboardNodePlugin } from '$lib/plugins/corePlugins';

describe('Whiteboard Plugin', () => {
  let registry: PluginRegistry;

  beforeEach(() => {
    registry = new PluginRegistry();
    registry.register(whiteboardNodePlugin);
  });

  it('should register whiteboard plugin successfully', () => {
    expect(registry.hasPlugin('whiteboard')).toBe(true);
    expect(registry.isEnabled('whiteboard')).toBe(true);
  });

  it('should provide whiteboard slash command', () => {
    const command = registry.findSlashCommand('whiteboard');
    expect(command).toBeDefined();
    expect(command?.name).toBe('Whiteboard');
    expect(command?.description).toBe('Create an interactive whiteboard');
  });

  it('should have viewer component', () => {
    expect(registry.hasViewer('whiteboard')).toBe(true);
  });

  it('should have reference component', () => {
    expect(registry.hasReferenceComponent('whiteboard')).toBe(true);
    const reference = registry.getReferenceComponent('whiteboard');
    expect(reference).toBeDefined();
  });

  it('should lazy load viewer component', async () => {
    const viewer = await registry.getViewer('whiteboard');
    expect(viewer).toBeDefined();
  });

  it('should be included in all slash commands', () => {
    const allCommands = registry.getAllSlashCommands();
    const whiteboardCommand = allCommands.find(cmd => cmd.id === 'whiteboard');
    expect(whiteboardCommand).toBeDefined();
  });

  it('should allow children and be a child', () => {
    const plugin = registry.getPlugin('whiteboard');
    expect(plugin?.config.canHaveChildren).toBe(true);
    expect(plugin?.config.canBeChild).toBe(true);
  });
});
```

### Step 7: Test Your Plugin

Run the test suite to ensure your plugin works correctly:

```bash
# Run all tests
bun run test

# Run specific plugin tests
bun run test whiteboard-plugin.test.ts

# Run development server to test manually
bun run dev
```

## Development Best Practices

### Code Quality

1. **Follow TypeScript Standards**
   - Use proper typing throughout
   - Avoid `any` types
   - Export all necessary interfaces

2. **Component Best Practices**
   - Follow Svelte conventions
   - Use proper event handling
   - Implement accessibility features

3. **Testing Requirements**
   - Unit tests for plugin registration
   - Component tests for UI behavior
   - Integration tests with the registry

### Plugin Design Principles

1. **Performance**
   - Use lazy loading for viewer components
   - Minimize bundle size impact
   - Implement efficient rendering

2. **User Experience**
   - Consistent UI patterns with core plugins
   - Clear error handling and states
   - Responsive design

3. **Compatibility**
   - Work with existing node hierarchy
   - Support copy/paste operations
   - Handle edge cases gracefully

## Advanced Features

### Custom Slash Command Shortcuts

```typescript
export const whiteboardNodePlugin: PluginDefinition = {
  // ... other config
  config: {
    slashCommands: [
      {
        id: 'whiteboard',
        name: 'Whiteboard',
        description: 'Create an interactive whiteboard',
        contentTemplate: '',
        shortcut: 'wb'  // Type /wb to trigger
      }
    ]
  }
};
```

### Component Props and Events

```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  // Standard props every viewer should accept
  export let node: Node;
  export let isSelected: boolean = false;
  export let isEditing: boolean = false;

  const dispatch = createEventDispatcher();

  // Standard events every viewer should dispatch
  function updateContent(newContent: string) {
    dispatch('content-changed', {
      content: newContent
    });
  }

  function requestAction(actionType: string, data: any) {
    dispatch('node-action', {
      type: actionType,
      nodeId: node.id,
      data
    });
  }
</script>
```

### Plugin Priorities and Ordering

```typescript
// Higher priority plugins appear first in slash command lists
export const whiteboardNodePlugin: PluginDefinition = {
  // ... other config
  viewer: {
    lazyLoad: () => import('./WhiteboardViewer.svelte'),
    priority: 10  // Higher number = higher priority
  },
  reference: {
    component: WhiteboardReference,
    priority: 10
  }
};
```

## Contribution Process

### 1. Development Workflow

```bash
# Create feature branch
git checkout -b feature/whiteboard-plugin

# Develop and test plugin
bun run dev
bun run test

# Ensure code quality
bun run lint
bun run typecheck

# Commit changes
git add .
git commit -m "Add whiteboard plugin with drawing capabilities"
```

### 2. Pull Request Process

1. **Create PR** with detailed description
2. **Include tests** for all plugin functionality
3. **Demonstrate functionality** with screenshots/videos
4. **Document any new dependencies** or requirements
5. **Follow code review feedback** from maintainers

### 3. Review Criteria

- **Code Quality**: TypeScript compliance, lint passing
- **Test Coverage**: Comprehensive test suite
- **User Experience**: Intuitive interface, consistent patterns
- **Performance**: No significant impact on app performance
- **Documentation**: Clear component documentation

## Common Patterns

### File/Media Plugins

```typescript
// For plugins that handle files or media
export const imageNodePlugin: PluginDefinition = {
  id: 'image',
  name: 'Image Node',
  config: {
    slashCommands: [
      {
        id: 'image',
        name: 'Image',
        description: 'Insert an image',
        contentTemplate: ''
      }
    ]
  },
  viewer: {
    lazyLoad: () => import('./ImageViewer.svelte'),
    priority: 1
  },
  reference: {
    component: ImageReference,
    priority: 1
  }
};
```

### Data Visualization Plugins

```typescript
// For plugins that display charts, graphs, etc.
export const chartNodePlugin: PluginDefinition = {
  id: 'chart',
  name: 'Chart Node',
  config: {
    slashCommands: [
      {
        id: 'chart',
        name: 'Chart',
        description: 'Create a data chart',
        contentTemplate: 'chart:'
      }
    ],
    canHaveChildren: false,  // Charts typically don't have children
    canBeChild: true
  }
};
```

### Reference-Only Plugins

```typescript
// For plugins that only provide references (no viewer)
export const linkNodePlugin: PluginDefinition = {
  id: 'link',
  name: 'Link Reference',
  config: {
    slashCommands: [],  // No slash commands
    canHaveChildren: false,
    canBeChild: true
  },
  // No viewer component
  reference: {
    component: LinkReference,
    priority: 1
  }
};
```

## Troubleshooting

### Common Issues

1. **Plugin Not Appearing in Slash Commands**
   - Check plugin registration in corePlugins array
   - Verify slash command definition
   - Ensure plugin is enabled

2. **Viewer Component Not Loading**
   - Check lazy load import path
   - Verify component exports default
   - Check browser console for errors

3. **TypeScript Errors**
   - Ensure proper typing for Node interface
   - Check component prop types
   - Verify import paths

4. **Tests Failing**
   - Check plugin registration in test setup
   - Verify mock implementations
   - Ensure async operations are properly awaited

### Getting Help

- **Documentation**: Check existing plugin implementations for patterns
- **Community**: Join NodeSpace Discord for developer support
- **Issues**: Create GitHub issues for bugs or feature requests
- **Code Review**: Request feedback during PR process

## Future Improvements

The current development process will be enhanced with upcoming features:

- **Plugin CLI**: Automated scaffolding and validation tools
- **Hot Reloading**: Live plugin development without app restart
- **Plugin Templates**: Pre-built templates for common plugin types
- **Marketplace**: Distribution platform for external plugins

Until these tools are available, the current process provides a solid foundation for creating high-quality NodeSpace plugins that integrate seamlessly with the unified registry system.