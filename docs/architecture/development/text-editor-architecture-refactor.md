# Text Editor Architecture Refactor Plan

## Executive Summary

Based on senior architect and frontend expert analysis, our current text editor architecture has fundamental issues that limit scalability, performance, and maintainability. This document outlines a comprehensive refactor plan to implement a modern, event-driven architecture.

## Current Architecture Issues

### 1. **Mixed Component Responsibilities**
- TextNode handles both content transformation AND editing concerns
- MinimalBaseNode mixes UI logic with business logic
- BaseNodeViewer combines state management with view logic

### 2. **Reactivity Anti-Patterns**
- Direct prop mutation breaking Svelte 5 reactivity (partially fixed)
- Complex `nodes = [...nodes]` patterns for forcing updates
- Deep object mutations not triggering UI updates

### 3. **Performance Problems**
- Deep tree traversal on every content update
- No virtualization for large document hierarchies
- Inefficient markdown processing on every keystroke
- Memory leaks from improper event cleanup

### 4. **Scalability Limitations**
- No clear extension points for AI integration
- Difficult to add collaborative editing features
- Plugin system not architected
- Content processing not optimized

## Proposed Architecture

### **Clean Architecture Layers**

```
┌─────────────────────────────────────────┐
│                Services                 │
│  ┌─────────────────────────────────────┐│
│  │           Domain Logic              ││
│  │  - ContentProcessor                 ││
│  │  - NodeManager                     ││
│  │  - ValidationService               ││
│  │  - AIService (future)              ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
                    ↕ Events
┌─────────────────────────────────────────┐
│             State Management            │
│  ┌─────────────────────────────────────┐│
│  │         Stores & Events             ││
│  │  - documentStore                   ││
│  │  - selectionStore                  ││
│  │  - eventBus                        ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
                    ↕ Props/Events
┌─────────────────────────────────────────┐
│              UI Components              │
│  ┌─────────────────────────────────────┐│
│  │          Pure UI Logic              ││
│  │  - NodeEditor                      ││
│  │  - TextEditor                      ││
│  │  - DocumentView                    ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
```

### **Component Restructure**

```
DocumentView (Container)
├── NodeEditor (Per-node logic)
│   ├── TextEditor (ContentEditable)
│   ├── AIAssistant (Future)
│   └── CollaborationMarkers (Future)
├── VirtualList (Performance)
└── CommandPalette (Future)
```

## Implementation Phases

### **Phase 1: Service Extraction (2 weeks)**

#### 1.1 ContentProcessor Service
```typescript
interface ContentProcessor {
  // Markdown processing
  markdownToDisplay(markdown: string): string;
  displayToMarkdown(html: string): string;
  
  // Content validation
  validateContent(content: string): ValidationResult;
  sanitizeContent(content: string): string;
  
  // Header detection
  parseHeaderLevel(content: string): number;
  stripHeaderSyntax(content: string): string;
}
```

#### 1.2 NodeManager Service  
```typescript
interface NodeManager {
  // Node operations
  createNode(afterId: string, content?: string): string;
  updateNodeContent(nodeId: string, content: string): void;
  deleteNode(nodeId: string): void;
  combineNodes(currentId: string, previousId: string): void;
  
  // Hierarchy operations
  indentNode(nodeId: string): boolean;
  outdentNode(nodeId: string): boolean;
  moveNode(nodeId: string, newParentId: string, index: number): void;
  
  // Query operations
  findNode(nodeId: string): Node | null;
  getVisibleNodes(): Node[];
  getNodeAncestors(nodeId: string): Node[];
}
```

#### 1.3 EventBus Implementation
```typescript
interface EventBus {
  // Content events
  emit(event: 'content:changed', data: ContentChangedEvent): void;
  emit(event: 'node:created', data: NodeCreatedEvent): void;
  emit(event: 'node:deleted', data: NodeDeletedEvent): void;
  
  // Selection events
  emit(event: 'selection:changed', data: SelectionChangedEvent): void;
  emit(event: 'cursor:moved', data: CursorMovedEvent): void;
  
  // Collaboration events (future)
  emit(event: 'operation:applied', data: OperationEvent): void;
}
```

### **Phase 2: State Management Refactor (2 weeks)**

#### 2.1 Document Store
```typescript
interface DocumentState {
  nodes: Map<string, Node>;
  hierarchy: HierarchyTree;
  metadata: DocumentMetadata;
  isDirty: boolean;
  lastSaved: Date;
}

export const documentStore = writable<DocumentState>(initialState);
```

#### 2.2 Derived Stores
```typescript
export const visibleNodes = derived(
  [documentStore, selectionStore],
  ([$doc, $selection]) => calculateVisibleNodes($doc.hierarchy, $selection.collapsedNodes)
);

export const selectedNode = derived(
  [documentStore, selectionStore],
  ([$doc, $selection]) => $doc.nodes.get($selection.activeNodeId)
);
```

#### 2.3 Selection Store
```typescript
interface SelectionState {
  activeNodeId: string | null;
  cursorPosition: number;
  selectionRange: [number, number] | null;
  collapsedNodes: Set<string>;
}

export const selectionStore = writable<SelectionState>(initialSelection);
```

### **Phase 3: Component Refactor (2 weeks)**

#### 3.1 Pure UI Components
```svelte
<!-- TextEditor.svelte - Pure UI, no business logic -->
<script lang="ts">
  export let nodeId: string;
  export let content: string;
  export let headerLevel: number = 0;
  export let isActive: boolean = false;
  
  // Only UI concerns - no content transformation
  let editorElement: HTMLDivElement;
  
  function handleInput(event: Event) {
    const newContent = (event.target as HTMLDivElement).innerHTML;
    dispatch('input', { nodeId, content: newContent });
  }
</script>

<div 
  bind:this={editorElement}
  contenteditable="true"
  class="text-editor h{headerLevel}"
  class:active={isActive}
  on:input={handleInput}
  on:keydown
>
  {@html content}
</div>
```

#### 3.2 Container Components
```svelte
<!-- NodeEditor.svelte - Coordinates services and UI -->
<script lang="ts">
  import { documentStore, selectionStore } from '$lib/stores';
  import { contentProcessor, nodeManager } from '$lib/services';
  import TextEditor from './TextEditor.svelte';
  
  export let nodeId: string;
  
  $: node = $documentStore.nodes.get(nodeId);
  $: displayContent = contentProcessor.markdownToDisplay(node?.content ?? '');
  $: isActive = $selectionStore.activeNodeId === nodeId;
  
  function handleContentChange(event) {
    const markdown = contentProcessor.displayToMarkdown(event.detail.content);
    nodeManager.updateNodeContent(nodeId, markdown);
  }
</script>

<TextEditor 
  {nodeId}
  content={displayContent}
  headerLevel={node?.headerLevel ?? 0}
  {isActive}
  on:input={handleContentChange}
  on:keydown={handleKeyDown}
/>
```

### **Phase 4: Selection & Clipboard System (2 weeks)**

#### 4.1 Multi-Node Selection
```typescript
// /src/lib/services/selectionService.ts
interface SelectionService {
  startSelection(nodeId: string, position: number): void;
  extendSelection(nodeId: string, position: number): void;
  selectNodes(nodeIds: string[]): void;
  getSelectedContent(): SelectionContent;
}

export const selectionService = new SelectionServiceImpl();
```

#### 4.2 Visual Highlighting System
```svelte
<!-- SelectionOverlay.svelte -->
<script>
  import { selectionStore } from '$lib/stores';
  
  $: selection = $selectionStore.currentSelection;
  $: highlightBounds = calculateHighlightBounds(selection);
</script>

{#if selection}
  <div class="selection-overlay">
    {#each highlightBounds as bound}
      <div 
        class="highlight-region" 
        style="top: {bound.top}px; left: {bound.left}px; width: {bound.width}px; height: {bound.height}px"
      />
    {/each}
  </div>
{/if}
```

#### 4.3 Content Validation & Paste Control
```typescript
// /src/lib/services/clipboardService.ts
interface ClipboardService {
  validateClipboardContent(content: any): ValidationResult;
  sanitizeContent(content: any): ClipboardContent;
  handleImagePaste(image: File, nodeId: string): Promise<void>;
  preventUnauthorizedContent(content: any): any;
}

// Paste event handling
function handlePaste(event: ClipboardEvent) {
  event.preventDefault();
  const content = event.clipboardData?.getData('text/html') || event.clipboardData?.getData('text');
  const sanitized = clipboardService.sanitizeContent(content);
  nodeManager.insertContent(activeNodeId, sanitized);
}
```

### **Phase 5: Performance Optimization (1 week)**

#### 5.1 Virtual Scrolling
```svelte
<!-- DocumentView.svelte -->
<script>
  import { VirtualList } from '@sveltejs/virtual-list';
  import { visibleNodes } from '$lib/stores';
  
  $: items = $visibleNodes;
</script>

<VirtualList {items} let:item>
  <NodeEditor nodeId={item.id} />
</VirtualList>
```

#### 5.2 Debounced Processing
```typescript
const debouncedContentUpdate = debounce((nodeId: string, content: string) => {
  nodeManager.updateContent(nodeId, content);
}, 150);
```

#### 5.3 Memory Management
```typescript
// Cleanup on component destroy
onDestroy(() => {
  eventBus.removeAllListeners(nodeId);
  selectionStore.clearNodeSelection(nodeId);
});
```

## Future Extension Points

### **Multi-Node Selection & Copy/Paste**
```typescript
interface SelectionService {
  // Multi-node selection
  startSelection(nodeId: string, position: number): void;
  extendSelection(nodeId: string, position: number): void;
  selectNodes(nodeIds: string[]): void;
  clearSelection(): void;
  getSelectedContent(): SelectionContent;
  
  // Advanced selection types
  selectEntireNodes(nodeIds: string[]): void;
  selectRange(startNode: string, startPos: number, endNode: string, endPos: number): void;
}

interface ClipboardService {
  // Controlled copy/paste
  copySelection(selection: SelectionContent): Promise<void>;
  pasteContent(content: ClipboardContent, nodeId: string, position: number): Promise<void>;
  
  // Content validation and filtering
  validateClipboardContent(content: any): ValidationResult;
  sanitizeContent(content: any): ClipboardContent;
  stripUnsupportedContent(content: any): ClipboardContent;
  
  // Smart paste handling
  handleImagePaste(image: File, nodeId: string): Promise<void>;
  handleRichTextPaste(html: string, nodeId: string): Promise<void>;
  handlePlainTextPaste(text: string, nodeId: string): Promise<void>;
}

interface ContentValidationService {
  // Prevent unauthorized content
  isContentAllowed(content: any, context: NodeContext): boolean;
  sanitizeHtml(html: string): string;
  extractSupportedContent(content: any): SupportedContent;
  
  // Format enforcement
  enforceTextOnlyNodes(content: string): string;
  preventImageInsertion(content: any): any;
  stripForbiddenTags(html: string): string;
}
```

### **Visual Selection & Highlighting**
```typescript
interface HighlightService {
  // Cross-node visual selection
  highlightRange(startNode: string, startPos: number, endNode: string, endPos: number): void;
  highlightNodes(nodeIds: string[]): void;
  clearHighlights(): void;
  
  // Selection visualization
  renderSelectionOverlay(selection: SelectionRange): void;
  updateSelectionBounds(selection: SelectionRange): void;
  handleSelectionInteraction(event: MouseEvent | KeyboardEvent): void;
}

interface SelectionRange {
  startNodeId: string;
  startPosition: number;
  endNodeId: string;
  endPosition: number;
  selectedNodes: string[];
  visualBounds: DOMRect[];
}
```

### **AI Integration**
```typescript
interface AIService {
  enhanceContent(content: string): Promise<string>;
  suggestCompletions(partial: string, context: Node[]): Promise<string[]>;
  summarizeDocument(nodeIds: string[]): Promise<string>;
  chatWithDocument(query: string, context: Node[]): Promise<string>;
  
  // Multi-node AI operations
  enhanceSelection(selection: SelectionContent): Promise<string>;
  summarizeSelection(selection: SelectionContent): Promise<string>;
}
```

### **Collaborative Editing**
```typescript
interface CollaborationService {
  broadcastOperation(op: Operation): void;
  applyRemoteOperation(op: Operation): void;
  resolveConflicts(ops: Operation[]): Operation[];
  showCursors(users: CollaboratorCursor[]): void;
  
  // Multi-user selection
  broadcastSelection(selection: SelectionRange): void;
  showUserSelections(selections: Map<string, SelectionRange>): void;
}
```

### **Plugin System**
```typescript
interface EditorPlugin {
  name: string;
  nodeTypes: string[];
  component: SvelteComponent;
  processor: ContentProcessor;
  commands: Command[];
  
  // Selection and clipboard hooks
  onSelectionChanged?(selection: SelectionRange): void;
  onContentPasted?(content: ClipboardContent, context: NodeContext): ClipboardContent;
  validateContent?(content: any): ValidationResult;
}
```

## Migration Strategy

### **Incremental Migration**
1. **Week 1-2**: Extract services while keeping existing components
2. **Week 3-4**: Migrate to new state management
3. **Week 5-6**: Refactor components to use new architecture
4. **Week 7**: Performance optimization and cleanup

### **Compatibility Layer**
```typescript
// Temporary bridge to maintain compatibility
class LegacyAdapter {
  static adaptOldEvents(oldEvent: LegacyEvent): NewEvent {
    // Transform old event format to new
  }
  
  static adaptOldComponents(component: LegacyComponent): NewComponent {
    // Wrap old components for gradual migration
  }
}
```

## Success Metrics

### **Performance**
- [ ] Document with 1000+ nodes loads in < 2 seconds
- [ ] Keystroke latency < 16ms (60fps)
- [ ] Memory usage < 50MB for large documents
- [ ] No memory leaks after 1 hour of editing

### **Reliability**
- [ ] Zero reactivity bugs in text editing
- [ ] 100% test coverage for core services
- [ ] No data loss during operations
- [ ] Proper error recovery and user feedback

### **Developer Experience**
- [ ] Clear component responsibilities
- [ ] Easy to add new node types
- [ ] Comprehensive TypeScript types
- [ ] Excellent debugging capabilities

## Risk Mitigation

### **Technical Risks**
- **Risk**: Breaking existing functionality during migration
- **Mitigation**: Incremental migration with feature flags

- **Risk**: Performance regression during transition
- **Mitigation**: Benchmark each phase against baseline

- **Risk**: Complex event system becoming unmaintainable
- **Mitigation**: Clear event schemas and comprehensive testing

### **Timeline Risks**
- **Risk**: Underestimating migration complexity
- **Mitigation**: Start with service extraction (lowest risk, high value)

- **Risk**: User-facing bugs during development
- **Mitigation**: Use feature flags and gradual rollout

---

## ✅ IMPLEMENTED: Advanced Nested Formatting System

**Status**: **COMPLETED** - Revolutionary keyboard shortcut formatting with advanced nested markdown support

### Overview

A comprehensive formatting system that provides intelligent, context-aware keyboard shortcuts (Cmd+B, Cmd+I) with support for complex nested markdown scenarios. This implementation solves the fundamental issues with previous toggle detection and establishes a robust foundation for advanced text editing.

### Key Features

#### **🧠 Context-Aware Toggle Detection**
- Analyzes surrounding text context instead of just selected content
- Properly handles nested formatting scenarios like `*__bold__*` → select "bold" → Cmd+B → `*bold*`
- Uses actual DOM selection positions for precise formatting application

#### **🔄 Cross-Marker Compatibility** 
- Unified handling of equivalent markdown syntax: `**bold**` and `__bold__` both work with Cmd+B
- Italic support for both `*italic*` and `_italic_` with Cmd+I
- Consistent behavior regardless of which syntax variant is used

#### **🎯 Advanced Nested Scenarios**
Perfect handling of complex formatting combinations:
- `*__bold__*` → select "bold" → Cmd+B → `*bold*` (removes inner `__`)
- `**_italic_**` → select "italic" → Cmd+I → `**italic**` (removes inner `_`)
- `***text***` → Cmd+I → `**text**` (removes italic, keeps bold)
- `**text**` → Cmd+I → `***text***` (adds italic to existing bold)

### Technical Implementation

#### **Core Files**
- **Main Controller**: `/src/lib/design/components/ContentEditableController.ts`
- **Library Integration**: `/src/lib/utils/markedConfig.ts` (marked.js configuration)
- **Test Suite**: `/src/tests/components/ContentEditableController-toggle.test.ts`

#### **Revolutionary Algorithm**

**Context-Aware Selection Analysis:**
```typescript
private shouldRemoveInnerFormatting(selectedText: string, marker: string, 
    textContent: string, selectionStartOffset: number, selectionEndOffset: number): boolean {
  
  if (marker === '**') {
    // Cmd+B: Check if selection is inside *__text__* pattern (bold inside italic)
    const beforeSelection = textContent.substring(0, selectionStartOffset);
    const afterSelection = textContent.substring(selectionEndOffset);
    
    let isInsideStarUnderscore = false;
    
    if (selectedText.startsWith('__') && selectedText.endsWith('__')) {
      // Case 1: Selection includes the __ markers, look for * around
      isInsideStarUnderscore = beforeSelection.endsWith('*') && afterSelection.startsWith('*');
    } else {
      // Case 2: Selection is just text, look for *__ before and __* after
      isInsideStarUnderscore = !!beforeSelection.match(/\*__[^_]*$/) && !!afterSelection.match(/^[^_]*__\*/);
    }
    
    return isInsideStarUnderscore;
  }
  // Similar logic for italic...
}
```

**Cross-Marker Compatibility:**
```typescript
private isTextAlreadyFormatted(text: string, marker: string): boolean {
  if (marker === '**') {
    // Bold: Recognize both ** and __ variants
    return (text.startsWith('**') && text.endsWith('**')) || 
           (text.startsWith('__') && text.endsWith('__'));
  } else if (marker === '*') {
    // Italic: Recognize both * and _ variants  
    return (text.startsWith('*') && text.endsWith('*')) || 
           (text.startsWith('_') && text.endsWith('_'));
  }
  return false;
}
```

### marked.js Integration

#### **Purpose & Benefits**
- **Robust Parsing**: Leverages battle-tested markdown library to eliminate edge cases
- **Consistency**: Provides predictable behavior across all formatting scenarios
- **Maintainability**: Reduces custom parsing logic and associated bugs

#### **Custom Configuration** (`markedConfig.ts`)
```typescript
// Custom renderer for NodeSpace-specific CSS classes
const renderer = new marked.Renderer();
renderer.strong = (text) => `<span class="markdown-bold">${text}</span>`;
renderer.em = (text) => `<span class="markdown-italic">${text}</span>`;

// Headers preserved as plain text (NodeSpace handles separately)
renderer.heading = (text, level) => `${'#'.repeat(level)} ${text}`;
```

### Quality Assurance

#### **Comprehensive Test Coverage**
- **19+ Test Scenarios**: Covering nested, sequential, cross-marker, and edge cases
- **Integration Tests**: marked.js library integration validation
- **Round-trip Testing**: Markdown → HTML → Markdown consistency
- **Performance Testing**: Efficient handling of large text blocks

#### **Supported Scenarios Matrix**

| Input State | User Selection | Keyboard Action | Expected Result | Status |
|-------------|----------------|-----------------|-----------------|---------|
| `*__bold__*` | `__bold__` or `bold` | Cmd+B | `*bold*` | ✅ |
| `**_italic_**` | `_italic_` or `italic` | Cmd+I | `**italic**` | ✅ |
| `__standalone__` | `standalone` | Cmd+B | `standalone` | ✅ |
| `*italic*` | `italic` | Cmd+I | `italic` | ✅ |
| `**existing**` | `existing` | Cmd+I | `***existing***` | ✅ |
| `***both***` | `both` | Cmd+I | `**both**` | ✅ |
| `***both***` | `both` | Cmd+B | `*both*` | ✅ |

### Developer Experience Improvements

#### **Enhanced Debugging**
- Comprehensive inline documentation explaining complex logic
- Clear method naming and parameter descriptions  
- Detailed comments for edge case handling

#### **Maintainable Architecture**
- Separation of concerns: formatting logic vs DOM manipulation
- Testable methods with clear input/output contracts
- Integration with existing NodeSpace component lifecycle

### Performance & Reliability

#### **Optimized Implementation**
- **O(n) Complexity**: Linear time relative to text length (same as previous)
- **Minimal DOM Queries**: Uses existing selection API efficiently
- **Memory Efficient**: No additional storage overhead

#### **Robust Error Handling**
- Graceful handling of malformed markdown input
- Protection against infinite loops in complex nested scenarios
- Comprehensive input validation and sanitization

---

## Next Steps

1. **Create GitHub Issues** for each phase (see next section)
2. **Set up feature flags** for incremental rollout
3. **Establish benchmarks** for performance comparison
4. **Create test suites** for regression prevention
5. **Begin Phase 1** with ContentProcessor service extraction

---

*This refactor will establish a modern, scalable foundation for NodeSpace's text editing capabilities, enabling AI integration, collaborative editing, and plugin extensibility while solving current reactivity and performance issues.*