# External Node Development Guide

**Document Version:** 1.0  
**Date:** 2025-08-11  
**Context:** Plugin Development for NodeSpace Components

## Overview

This guide provides comprehensive documentation for developing external node types that extend NodeSpace's BaseNode component. External repositories can create custom node types while maintaining full compatibility with NodeSpace's architecture and design system.

**Target Audience:** External plugin developers, third-party integrators, custom node type creators

**Prerequisites:**
- Understanding of Svelte component development
- Familiarity with NodeSpace's component composition patterns
- Basic knowledge of TypeScript for type safety

## 1. Component Composition Architecture

### BaseNode Foundation

The `BaseNode` component provides the foundational architecture for all node types in NodeSpace:

```svelte
<!-- BaseNode core structure -->
<BaseNode
  nodeId={string}           // Unique identifier for the node
  nodeType={NodeType}       // 'text' | 'task' | 'ai-chat' | 'entity' | 'query'
  content={string}          // Primary text content
  hasChildren={boolean}     // Visual indicator for parent nodes
  className={string}        // Additional CSS classes
  editable={boolean}        // Can user interact with node?
  contentEditable={boolean} // Can content be directly edited?
  multiline={boolean}       // Single-line vs multi-line editing
  markdown={boolean}        // Enable markdown syntax support
  placeholder={string}      // Placeholder text for empty content
  iconName={IconName}       // SVG icon identifier
  isProcessing={boolean}    // Show processing animation
  processingAnimation='pulse' | 'blink' | 'spin' | 'fade'
  processingIcon={IconName} // Icon to show during processing
  on:click                  // Node click handler
  on:contentChanged         // Content change handler
>
  <!-- Slot for custom display content -->
  <div slot="display-content">
    <!-- Your custom rendering here -->
  </div>
  
  <!-- Default slot for additional features -->
  <!-- Your additional functionality here -->
</BaseNode>
```

### Component Inheritance Pattern

External nodes extend BaseNode functionality through prop configuration and slot customization:

```svelte
<!-- PdfNode.svelte - External repository example -->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '@nodespace/core/components/BaseNode.svelte';
  
  // Props specific to PDF nodes
  export let nodeId: string;
  export let pdfUrl: string = '';
  export let title: string = '';
  export let pageCount: number = 0;
  export let thumbnailUrl: string = '';
  
  // Computed content for BaseNode
  $: content = `${title} (${pageCount} pages)`;
  
  // Event handling
  const dispatch = createEventDispatcher<{
    openPdf: { nodeId: string; url: string };
    downloadPdf: { nodeId: string; url: string };
  }>();
  
  function handleClick(event: CustomEvent) {
    // Handle node click to open PDF viewer
    dispatch('openPdf', { nodeId, url: pdfUrl });
  }
</script>

<BaseNode 
  {nodeId}
  nodeType="entity"
  {content}
  contentEditable={false}
  iconName="file-pdf"
  className="pdf-node"
  on:click={handleClick}
>
  <!-- Custom PDF display -->
  <div slot="display-content" class="pdf-preview">
    {#if thumbnailUrl}
      <img src={thumbnailUrl} alt={title} class="pdf-thumbnail" />
    {/if}
    <div class="pdf-details">
      <h4 class="pdf-title">{title}</h4>
      <span class="pdf-meta">{pageCount} pages • PDF</span>
      <div class="pdf-actions">
        <button on:click|stopPropagation={() => dispatch('downloadPdf', { nodeId, url: pdfUrl })}>
          Download
        </button>
      </div>
    </div>
  </div>
</BaseNode>

<style>
  .pdf-preview {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  
  .pdf-thumbnail {
    width: 48px;
    height: 60px;
    object-fit: cover;
    border-radius: 4px;
    border: 1px solid hsl(var(--border));
  }
  
  .pdf-details {
    flex: 1;
    min-width: 0;
  }
  
  .pdf-title {
    font-size: 14px;
    font-weight: 600;
    color: hsl(var(--foreground));
    margin: 0 0 4px 0;
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
  }
  
  .pdf-meta {
    font-size: 12px;
    color: hsl(var(--muted-foreground));
  }
  
  .pdf-actions {
    margin-top: 6px;
  }
  
  .pdf-actions button {
    font-size: 12px;
    padding: 2px 8px;
    background: hsl(var(--accent));
    color: hsl(var(--accent-foreground));
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }
</style>
```

## 2. Extension Patterns and Examples

### Pattern 1: Read-Only Entity Nodes

For nodes that display information without direct text editing:

```svelte
<!-- PersonNode.svelte - Contact entity example -->
<script lang="ts">
  export let nodeId: string;
  export let name: string = '';
  export let email: string = '';
  export let company: string = '';
  export let avatarUrl: string = '';
  
  $: content = `${name} ${company ? `(${company})` : ''}`;
</script>

<BaseNode 
  {nodeId}
  nodeType="entity"
  {content}
  contentEditable={false}
  iconName="user"
  className="person-node"
>
  <div slot="display-content" class="person-card">
    {#if avatarUrl}
      <img src={avatarUrl} alt={name} class="avatar" />
    {/if}
    <div class="person-info">
      <span class="person-name">{name}</span>
      {#if company}
        <span class="person-company">{company}</span>
      {/if}
      {#if email}
        <a href="mailto:{email}" class="person-email">{email}</a>
      {/if}
    </div>
  </div>
</BaseNode>
```

### Pattern 2: Interactive Task Nodes

For nodes that combine content editing with custom functionality:

```svelte
<!-- TaskNode.svelte - Todo item with completion -->
<script lang="ts">
  export let nodeId: string;
  export let content: string = '';
  export let completed: boolean = false;
  export let dueDate: Date | null = null;
  export let priority: 'low' | 'medium' | 'high' = 'medium';
  
  function toggleCompleted() {
    completed = !completed;
    // Emit event for external save handling
    dispatch('taskStatusChanged', { nodeId, completed });
  }
  
  $: isOverdue = dueDate && dueDate < new Date() && !completed;
</script>

<BaseNode 
  {nodeId}
  nodeType="task"
  bind:content
  contentEditable={true}
  multiline={false}
  iconName={completed ? "check-circle" : "circle"}
  className="task-node {completed ? 'completed' : ''} {isOverdue ? 'overdue' : ''}"
  on:contentChanged
>
  <!-- Additional task metadata -->
  <div class="task-metadata">
    <button 
      class="task-toggle"
      on:click|stopPropagation={toggleCompleted}
      aria-label={completed ? 'Mark incomplete' : 'Mark complete'}
    >
      {completed ? '✓' : '○'}
    </button>
    
    {#if dueDate}
      <span class="task-due-date" class:overdue={isOverdue}>
        Due: {dueDate.toLocaleDateString()}
      </span>
    {/if}
    
    <span class="task-priority priority-{priority}">
      {priority.toUpperCase()}
    </span>
  </div>
</BaseNode>
```

### Pattern 3: AI-Powered Nodes

For nodes that integrate AI functionality:

```svelte
<!-- AiChatNode.svelte - AI conversation node -->
<script lang="ts">
  export let nodeId: string;
  export let content: string = '';
  export let aiResponse: string = '';
  export let isGenerating: boolean = false;
  
  async function generateResponse() {
    if (!content.trim()) return;
    
    isGenerating = true;
    try {
      const response = await aiService.generateResponse(content);
      aiResponse = response;
      dispatch('aiResponseGenerated', { nodeId, response });
    } finally {
      isGenerating = false;
    }
  }
</script>

<BaseNode 
  {nodeId}
  nodeType="ai-chat"
  bind:content
  contentEditable={true}
  multiline={true}
  markdown={true}
  {isProcessing}
  processingAnimation="pulse"
  processingIcon="brain"
  placeholder="Ask AI anything..."
  className="ai-chat-node"
  on:contentChanged
>
  <!-- AI response display -->
  {#if aiResponse}
    <div class="ai-response">
      <div class="ai-response-header">
        <Icon name="brain" size={14} />
        <span>AI Response</span>
      </div>
      <div class="ai-response-content">
        <MarkdownRenderer content={aiResponse} />
      </div>
    </div>
  {/if}
  
  <!-- AI actions -->
  <div class="ai-actions">
    <button 
      on:click={generateResponse}
      disabled={isGenerating || !content.trim()}
    >
      {isGenerating ? 'Generating...' : 'Ask AI'}
    </button>
  </div>
</BaseNode>
```

## 3. Integration with Save Systems

### Auto-Save Pattern Implementation

External nodes can implement auto-save functionality following NodeSpace patterns:

```svelte
<!-- ExternalNode.svelte - Auto-save implementation -->
<script lang="ts">
  import { onMount } from 'svelte';
  
  export let nodeId: string;
  export let content: string = '';
  export let autoSave: boolean = true;
  
  // Save status tracking
  let saveStatus: 'saved' | 'saving' | 'unsaved' | 'error' = 'saved';
  let saveError = '';
  let lastSavedContent = content;
  
  // Auto-save with debouncing
  let debounceTimeout: NodeJS.Timeout;
  const DEBOUNCE_DELAY = 2000;
  
  function debounceAutoSave() {
    if (!autoSave) return;
    
    clearTimeout(debounceTimeout);
    saveStatus = 'unsaved';
    
    debounceTimeout = setTimeout(async () => {
      if (content !== lastSavedContent) {
        await handleAutoSave();
      }
    }, DEBOUNCE_DELAY);
  }
  
  async function handleAutoSave() {
    if (!nodeId || nodeId === 'new') return;
    
    saveStatus = 'saving';
    try {
      await externalSaveService.saveNode(nodeId, {
        content,
        // Additional node-specific data
        customData: getCustomData()
      });
      
      lastSavedContent = content;
      saveStatus = 'saved';
      dispatch('save', { nodeId, content });
    } catch (error) {
      saveError = error.message;
      saveStatus = 'error';
      dispatch('error', { nodeId, error: error.message });
    }
  }
  
  // Handle content changes from BaseNode
  function handleContentChanged(event: CustomEvent) {
    content = event.detail.content;
    debounceAutoSave();
  }
</script>

<BaseNode
  {nodeId}
  bind:content
  isProcessing={saveStatus === 'saving'}
  on:contentChanged={handleContentChanged}
>
  <!-- Save status indicator -->
  {#if saveStatus !== 'saved'}
    <div class="save-status">
      {#if saveStatus === 'saving'}
        <Icon name="loader" className="animate-spin" />
        Saving...
      {:else if saveStatus === 'unsaved'}
        <Icon name="circle" />
        Unsaved changes
      {:else if saveStatus === 'error'}
        <Icon name="alert-triangle" />
        {saveError}
      {/if}
    </div>
  {/if}
</BaseNode>
```

## 4. TypeScript Integration

### Type Definitions for External Nodes

```typescript
// types/external-nodes.ts
export interface ExternalNodeProps {
  nodeId: string;
  content?: string;
  editable?: boolean;
  autoSave?: boolean;
}

export interface PdfNodeData extends ExternalNodeProps {
  pdfUrl: string;
  title: string;
  pageCount: number;
  thumbnailUrl?: string;
}

export interface PersonNodeData extends ExternalNodeProps {
  name: string;
  email?: string;
  company?: string;
  avatarUrl?: string;
}

export interface TaskNodeData extends ExternalNodeProps {
  completed: boolean;
  dueDate?: Date;
  priority: 'low' | 'medium' | 'high';
}

// Event types for external nodes
export interface ExternalNodeEvents {
  save: { nodeId: string; content: string };
  error: { nodeId: string; error: string };
  customAction: { nodeId: string; action: string; data?: any };
}
```

### Custom Node Type Registration

```typescript
// external-node-registry.ts
import type { ComponentType } from 'svelte';

export interface NodeTypeDefinition {
  name: string;
  component: ComponentType;
  defaultProps?: Record<string, any>;
  category: 'content' | 'media' | 'entity' | 'system';
}

export class ExternalNodeRegistry {
  private static nodeTypes = new Map<string, NodeTypeDefinition>();
  
  static register(definition: NodeTypeDefinition) {
    this.nodeTypes.set(definition.name, definition);
  }
  
  static get(nodeType: string): NodeTypeDefinition | undefined {
    return this.nodeTypes.get(nodeType);
  }
  
  static getAll(): NodeTypeDefinition[] {
    return Array.from(this.nodeTypes.values());
  }
}

// Usage in external repository
import PdfNode from './PdfNode.svelte';
import PersonNode from './PersonNode.svelte';

ExternalNodeRegistry.register({
  name: 'pdf',
  component: PdfNode,
  defaultProps: { pageCount: 0 },
  category: 'media'
});

ExternalNodeRegistry.register({
  name: 'person',
  component: PersonNode,
  category: 'entity'
});
```

## 5. Testing Patterns

### Component Testing with BaseNode

```javascript
// PdfNode.test.ts
import { render, fireEvent } from '@testing-library/svelte';
import PdfNode from './PdfNode.svelte';

describe('PdfNode', () => {
  it('renders PDF information correctly', () => {
    const { getByText, getByAltText } = render(PdfNode, {
      props: {
        nodeId: 'test-pdf',
        title: 'Test Document',
        pageCount: 5,
        pdfUrl: 'test.pdf',
        thumbnailUrl: 'thumbnail.jpg'
      }
    });
    
    expect(getByText('Test Document')).toBeInTheDocument();
    expect(getByText('5 pages • PDF')).toBeInTheDocument();
    expect(getByAltText('Test Document')).toBeInTheDocument();
  });
  
  it('emits openPdf event when clicked', async () => {
    const { component } = render(PdfNode, {
      props: {
        nodeId: 'test-pdf',
        pdfUrl: 'test.pdf'
      }
    });
    
    const eventHandler = vi.fn();
    component.$on('openPdf', eventHandler);
    
    const node = document.querySelector('.pdf-node');
    await fireEvent.click(node);
    
    expect(eventHandler).toHaveBeenCalledWith(
      expect.objectContaining({
        detail: { nodeId: 'test-pdf', url: 'test.pdf' }
      })
    );
  });
  
  it('integrates correctly with BaseNode props', () => {
    const { container } = render(PdfNode, {
      props: {
        nodeId: 'test-pdf',
        title: 'Test PDF'
      }
    });
    
    const baseNode = container.querySelector('.ns-node');
    expect(baseNode).toHaveAttribute('data-node-id', 'test-pdf');
    expect(baseNode).toHaveAttribute('data-node-type', 'entity');
    expect(baseNode).toHaveClass('pdf-node');
  });
});
```

### Integration Testing

```javascript
// external-integration.test.ts
import { render } from '@testing-library/svelte';
import { ExternalNodeRegistry } from './external-node-registry';
import NodeRenderer from './NodeRenderer.svelte';

describe('External Node Integration', () => {
  beforeEach(() => {
    // Register test nodes
    ExternalNodeRegistry.register({
      name: 'test-external',
      component: TestExternalNode,
      category: 'content'
    });
  });
  
  it('renders external nodes through NodeRenderer', () => {
    const { getByText } = render(NodeRenderer, {
      props: {
        nodeType: 'test-external',
        nodeData: { nodeId: 'test', content: 'Test content' }
      }
    });
    
    expect(getByText('Test content')).toBeInTheDocument();
  });
});
```

## 6. Build and Integration

### Package Configuration

```json
// package.json for external node package
{
  "name": "@yourorg/nodespace-pdf-nodes",
  "version": "1.0.0",
  "type": "module",
  "svelte": "./src/index.js",
  "exports": {
    ".": {
      "svelte": "./src/index.js"
    }
  },
  "peerDependencies": {
    "@nodespace/core": "^1.0.0",
    "svelte": "^4.0.0"
  },
  "devDependencies": {
    "@testing-library/svelte": "^4.0.0",
    "vite": "^5.0.0",
    "vitest": "^1.0.0"
  }
}
```

### Vite Configuration

```javascript
// vite.config.js
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  build: {
    lib: {
      entry: 'src/index.js',
      formats: ['es']
    },
    rollupOptions: {
      external: ['@nodespace/core', 'svelte']
    }
  }
});
```

### Entry Point

```javascript
// src/index.js - Package entry point
export { default as PdfNode } from './PdfNode.svelte';
export { default as PersonNode } from './PersonNode.svelte';
export { default as TaskNode } from './TaskNode.svelte';
export { ExternalNodeRegistry } from './external-node-registry.js';
export type * from './types/external-nodes.ts';
```

## 7. Plugin Development Template

### Starter Template Structure

```
external-nodespace-plugin/
├── src/
│   ├── nodes/
│   │   ├── CustomNode.svelte
│   │   └── AnotherNode.svelte
│   ├── types/
│   │   └── node-types.ts
│   ├── services/
│   │   └── external-service.ts
│   ├── tests/
│   │   └── CustomNode.test.ts
│   └── index.js
├── package.json
├── vite.config.js
├── vitest.config.ts
└── README.md
```

### Template CustomNode.svelte

```svelte
<!-- src/nodes/CustomNode.svelte -->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '@nodespace/core/components/BaseNode.svelte';
  
  // Required props
  export let nodeId: string;
  export let content: string = '';
  
  // Custom props
  export let customProperty: string = '';
  
  // Event dispatcher
  const dispatch = createEventDispatcher<{
    customEvent: { nodeId: string; data: any };
  }>();
  
  // Custom logic
  function handleCustomAction() {
    dispatch('customEvent', { 
      nodeId, 
      data: { customProperty } 
    });
  }
  
  // Reactive statements
  $: displayContent = content || 'Empty custom node';
</script>

<BaseNode 
  {nodeId}
  nodeType="entity"
  {content}
  contentEditable={true}
  iconName="custom-icon"
  className="custom-node"
  on:contentChanged
>
  <!-- Custom display content -->
  <div slot="display-content" class="custom-display">
    <span class="custom-content">{displayContent}</span>
    {#if customProperty}
      <span class="custom-property">{customProperty}</span>
    {/if}
  </div>
  
  <!-- Additional features -->
  <div class="custom-actions">
    <button on:click={handleCustomAction}>
      Custom Action
    </button>
  </div>
</BaseNode>

<style>
  .custom-display {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  
  .custom-content {
    font-size: 14px;
    color: hsl(var(--foreground));
  }
  
  .custom-property {
    font-size: 12px;
    color: hsl(var(--muted-foreground));
  }
  
  .custom-actions {
    margin-top: 8px;
  }
  
  .custom-actions button {
    padding: 4px 8px;
    border: 1px solid hsl(var(--border));
    border-radius: 4px;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    cursor: pointer;
  }
</style>
```

## Best Practices Summary

### DO:
- ✅ Extend BaseNode for all custom node types
- ✅ Use the slot system for custom display content
- ✅ Follow NodeSpace naming conventions (ns-prefix)
- ✅ Implement proper TypeScript types
- ✅ Handle events through createEventDispatcher
- ✅ Use design system tokens for styling
- ✅ Implement comprehensive testing
- ✅ Follow auto-save patterns for data persistence

### DON'T:
- ❌ Bypass BaseNode and create from scratch
- ❌ Use inline styles instead of design system tokens
- ❌ Ignore TypeScript type safety
- ❌ Implement custom cursor positioning
- ❌ Create focused/unfocused state management
- ❌ Break component composition patterns
- ❌ Ignore accessibility requirements

### Testing Requirements:
- Unit tests for component rendering
- Integration tests with BaseNode
- Event handling verification
- Auto-save functionality testing
- TypeScript type checking

---

*This guide provides the complete foundation for developing external NodeSpace node types. Follow these patterns to ensure compatibility, maintainability, and excellent user experience.*