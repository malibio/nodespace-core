# Node Viewer Plugin Architecture Specification

## Overview

This document specifies the target architecture for implementing a plugin-style node viewer system with proper BaseNode (abstract) and TextNode (concrete) separation.

## Current State (After Rollback)

- ✅ Working TextNode with proper header/multiline logic
- ✅ All keyboard functionality (Enter, Backspace, Cmd+B/I, header syntax)
- ✅ BaseNodeViewer manages collections of nodes
- ✅ Pre-populated demo content working
- ✅ Date page navigation working

## Target Architecture

### 1. BaseNode (Abstract Foundation)

**Purpose**: Core contenteditable functionality that cannot be instantiated directly.

**Characteristics**:
- Single-line text with auto-wrap when width exceeds container
- Inline formatting support (Cmd+B/I)
- **Cannot be created directly** - only through derived node types
- Provides core editing infrastructure (events, keyboard handling, etc.)

**Implementation**:
```svelte
<!-- base-node.svelte - Abstract component -->
<script lang="ts">
  // Should only be used by concrete node types
  // Never instantiated directly in BaseNodeViewer
</script>
```

### 2. TextNode (Concrete Implementation)

**Purpose**: Concrete text editing with smart line behavior.

**Characteristics**:
- **Headers (H1-H6)**: Single-line only + auto-wrap
- **Regular text**: Multi-line (Shift+Enter for line breaks) + auto-wrap  
- Inherits BaseNode's inline formatting
- Can be created directly by users

**Implementation**:
```svelte
<!-- text-node-viewer.svelte -->
<script lang="ts">
  import BaseNode from '$lib/design/components/base-node.svelte';
  
  // Text-specific logic:
  // - Header level detection
  // - Multi-line vs single-line behavior
  // - Content processing
</script>

<BaseNode {nodeId} {content} headerLevel={calculatedLevel} />
```

### 3. Plugin Viewer System

**Registry Pattern**:
```typescript
// Node types and their viewers
const viewerRegistry = new Map([
  ['text', 'text-node-viewer.svelte'],      // Uses BaseNode internally
  ['date', 'date-node-viewer.svelte'],      // Custom date UI + BaseNode
  ['task', 'task-node-viewer.svelte'],      // Checkboxes + BaseNode
  ['ai-chat', 'ai-chat-node-viewer.svelte'] // Role indicators + BaseNode
]);

// BaseNode is NOT registered - cannot be used directly
```

**BaseNodeViewer Logic**:
```svelte
<!-- base-node-viewer.svelte -->
{#each visibleNodes as node}
  {#await getViewerForNode(node.nodeType) then ViewerComponent}
    <ViewerComponent {nodeId} {content} {nodeType} ... />
  {/await}
{/each}
```

### 4. Node Type Behaviors

| Node Type | Viewer | Line Behavior | Special Features |
|-----------|--------|---------------|------------------|
| `text` | TextNodeViewer | Headers: single-line<br>Regular: multi-line | Header parsing, content processing |
| `date` | DateNodeViewer | Single-line + auto-wrap | Date navigation, calendar UI |
| `task` | TaskNodeViewer | Single-line + auto-wrap | Checkboxes, priority indicators |
| `ai-chat` | AIChatNodeViewer | Multi-line | Role indicators, token counts |

### 5. Key Principles

1. **BaseNode is abstract** - never instantiated directly
2. **All node types inherit BaseNode functionality** through composition
3. **TextNode handles text-specific logic** (headers vs multiline)
4. **Plugin viewers customize UI** while preserving core editing
5. **Graceful fallbacks** for unknown node types

## Implementation Steps

### Phase 1: Make BaseNode Abstract
1. Ensure BaseNode is only used by concrete node viewers
2. Remove any direct BaseNode instantiation from BaseNodeViewer
3. Add validation to prevent direct BaseNode usage

### Phase 2: Create TextNodeViewer
1. Extract text-specific logic from current TextNode
2. Implement header vs multiline behavior
3. Register as 'text' node viewer

### Phase 3: Implement Plugin Registry
1. Create viewer registry system
2. Add lazy loading for custom viewers
3. Update BaseNodeViewer to use registry

### Phase 4: Add Custom Viewers
1. DateNodeViewer (individual date nodes)
2. TaskNodeViewer (checkboxes, priorities)
3. AIChatNodeViewer (role indicators)

## File Structure

```
packages/desktop-app/src/lib/
├── design/components/
│   ├── base-node.svelte                 # Abstract - used by viewers only
│   └── base-node-viewer.svelte          # Collection manager
├── components/viewers/
│   ├── index.ts                         # Viewer registry
│   ├── text-node-viewer.svelte          # Wraps BaseNode
│   ├── date-node-viewer.svelte          # Custom date UI + BaseNode  
│   ├── task-node-viewer.svelte          # Task controls + BaseNode
│   └── ai-chat-node-viewer.svelte       # Chat UI + BaseNode
└── types/
    └── node-viewers.ts                  # TypeScript interfaces
```

## Critical Requirements

1. **Preserve all working functionality** during implementation
2. **TextNode behavior must match current working version**:
   - Headers: single-line only
   - Regular text: multi-line with Shift+Enter
   - All keyboard shortcuts (Enter, Backspace, Cmd+B/I)
   - Header syntax parsing ("# " creates headers)
3. **BaseNode abstraction** - cannot be instantiated directly
4. **Plugin extensibility** - new node types just register viewers

## Success Criteria

- ✅ BaseNode is truly abstract (not directly instantiated)
- ✅ TextNode behavior identical to current working version
- ✅ New node types can be added by creating viewers
- ✅ All existing functionality preserved (keyboard, formatting, navigation)
- ✅ Plugin system supports unlimited node type extensions

---

**Implementation Note**: Start from the rolled-back working state and implement this architecture incrementally, preserving functionality at each step.