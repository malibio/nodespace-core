# Enhanced ContentEditable Architecture

## Overview

The NodeSpace Enhanced ContentEditable system provides sophisticated text editing with Logseq-inspired dual-representation patterns, built around a service-oriented architecture with **BaseNode** (preserved foundation), **ContentProcessor** (dual-representation), **NodeManager** (operations), and **BacklinkService** (knowledge management).

## Architecture Status

### 🔄 **Enhanced ContentEditable Implementation (In Progress)**

Based on comprehensive research including Logseq analysis and ProseMirror evaluation, we've determined that **NodeSpace's existing contenteditable approach with indented node indicators is architecturally superior**. The implementation is being enhanced with proven patterns.

**📋 Current Architecture Plan:** See [Text Editor Architecture Refactor](../decisions/2025-01-text-editor-architecture-refactor.md) for the complete enhanced contenteditable architecture with Logseq-inspired dual-representation patterns.

#### **BaseNode (Preserved Foundation)**
- **Enhanced contenteditable functionality** - preserves unique hierarchical indicators
- **Dual-representation patterns** - source markdown ↔ AST ↔ rendered display (Logseq-inspired)
- **Service integration** - connects to ContentProcessor and NodeManager services
- **Event-driven architecture** - clean separation of concerns
- **Hierarchical visual design** - NodeSpace's key differentiator maintained

#### **TextNode (Specialization Layer)**  
- **Header syntax support** - `# ## ### #### ##### ######` (H1-H6)
- **CSS-based header styling** - each level has proper font size/weight
- **Header level inheritance** - new nodes inherit parent header level
- **Content preservation** - switching header levels preserves existing text
- **Empty reset** - backspacing to empty resets to normal text
- **Multiline text support** - Shift+Enter creates line breaks (except in headers)
- **Single-line headers** - headers enforce single-line behavior

#### **BaseNodeViewer (Management Layer)**
- **Node collection management** - maintains array of node objects
- **Node creation handling** - responds to createNewNode events
- **Header inheritance** - passes header levels to new nodes
- **Auto-focus coordination** - ensures only one node has focus
- **Reactive state management** - Svelte-based reactivity

### 🎯 **Key Architectural Decisions**

#### **Enhanced ContentEditable with Service-Oriented Architecture**
Based on research, we're enhancing the existing BaseNode with proven patterns:
- **Preserve Unique Value** - NodeSpace's hierarchical indicators are superior to ProseMirror
- **Logseq-Inspired Patterns** - dual-representation (source ↔ AST ↔ display)
- **Service-Oriented Design** - ContentProcessor, NodeManager, BacklinkService
- **Performance Optimizations** - migrate proven patterns from `nodespace-core-logic`

#### **CSS-Based Header Formatting**
Headers use CSS styling rather than DOM manipulation:
- **Performance** - no DOM restructuring, just class changes
- **Reliability** - eliminates cursor positioning issues
- **Maintainability** - cleaner code without complex DOM manipulation
- **Flexibility** - easy to modify styling without touching logic

#### **Component Composition Pattern**
- **BaseNode provides editing** - universal text editing capabilities
- **TextNode adds specialization** - header-specific functionality
- **BaseNodeViewer manages collections** - node lifecycle and relationships
- **Event-driven communication** - components communicate via Svelte events

## Component Architecture

### **Data Flow**
```
User types "# " → TextNode detects → Sets headerLevel → Updates className → 
MinimalBaseNode applies CSS → User sees header formatting → Content stored as "# Title"

User presses Enter → MinimalBaseNode emits createNewNode → TextNode adds inheritHeaderLevel → 
BaseNodeViewer creates new node → New TextNode inherits header level → Same formatting continues
```

### **Key Interfaces**

#### **MinimalBaseNode Props**
```typescript
export let nodeId: string;
export let nodeType: string = 'text';
export let autoFocus: boolean = false;
export let allowNewNodeOnEnter: boolean = true;
export let splitContentOnEnter: boolean = false;
export let multiline: boolean = false;
export let content: string = '';
export let className: string = '';
```

#### **TextNode Props**
```typescript
export let nodeId: string;
export let autoFocus: boolean = false;
export let content: string = '';
export let inheritHeaderLevel: number = 0;
```

#### **Event System**
```typescript
// MinimalBaseNode Events
contentChanged: { content: string }
createNewNode: { afterNodeId: string; nodeType: string; currentContent?: string; newContent?: string }

// TextNode Events (extends MinimalBaseNode)
createNewNode: { ...baseEvents; inheritHeaderLevel?: number }
contentChanged: { nodeId: string; content: string }
```

## Markdown Support Matrix

### ✅ **Currently Implemented**
| Syntax | Support | Implementation |
|--------|---------|----------------|
| **Headers** | H1-H6 | `# ## ### #### ##### ######` with CSS styling |
| **Bold** | ✅ | `**text**` via Ctrl+B |
| **Italic** | ✅ | `*text*` via Ctrl+I |
| **Underline** | ✅ | `__text__` via Ctrl+U |
| **Multiline** | ✅ | Shift+Enter for line breaks |
| **Node Creation** | ✅ | Enter creates new nodes |
| **Header Inheritance** | ✅ | New nodes inherit parent header level |

### ❌ **Planned Markdown Features**
| Syntax | Priority | Notes |
|--------|----------|-------|
| **Lists** | High | `- item`, `1. item` with nested support |
| **Blockquotes** | High | `> quoted text` |
| **Code Spans** | Medium | `` `code` `` inline formatting |
| **Code Blocks** | Medium | ````code```` block formatting |
| **Links** | Medium | `[text](url)` |
| **Images** | Medium | `![alt](url)` |
| **Strikethrough** | Low | `~~text~~` |
| **Tables** | Low | `| col1 | col2 |` |
| **Task Lists** | Low | `- [ ]` (may use dedicated TaskNode) |

## Behavior Specifications

### **Keyboard Interactions**
| Key Combination | Normal TextNode | Header TextNode | Behavior |
|----------------|-----------------|-----------------|----------|
| **Enter** | Creates new TextNode | Creates new header node (same level) | Node creation |
| **Shift+Enter** | Line break within node | Prevented (no action) | Line break |
| **Ctrl+B** | Bold formatting | Bold formatting | Inline formatting |
| **Ctrl+I** | Italic formatting | Italic formatting | Inline formatting |
| **Ctrl+U** | Underline formatting | Underline formatting | Inline formatting |

### **Header Syntax Detection**
- **Pattern**: `#{1,6} ` (1-6 hash symbols followed by space)
- **Trigger**: Space key after hash pattern
- **Behavior**: 
  1. Remove syntax symbols
  2. Apply CSS header class
  3. Preserve existing content when switching levels
  4. Position cursor at beginning for continued typing

### **Content Storage Format**
- **Normal text**: Stored as entered (multiline preserved)
- **Headers**: Stored with markdown syntax (`# Title`, `## Subtitle`, etc.)
- **Inline formatting**: Stored as markdown (`**bold**`, `*italic*`, `__underline__`)
- **Mixed content**: Combines markdown syntax appropriately

## Technical Implementation Details

### **CSS Classes Structure**
```css
/* Base contenteditable */
.markdown-editor {
  min-height: 20px;
}

/* Single-line vs multiline modes */
.markdown-editor--singleline {
  white-space: nowrap;
  overflow-x: auto;
  overflow-y: hidden;
}

.markdown-editor--multiline {
  white-space: pre-wrap;
  overflow-y: auto;
  word-wrap: break-word;
}

/* Header styling (TextNode specific) */
.text-node--h1 .markdown-editor {
  font-size: 2rem;
  font-weight: bold;
  line-height: 1.2;
  white-space: nowrap !important; /* Headers always single-line */
}
/* ... h2-h6 variations ... */
```

### **Reactive State Management**
```typescript
// TextNode reactive className computation
$: nodeClassName = `text-node ${headerLevel ? `text-node--h${headerLevel}` : ''}`.trim();

// Header level inheritance
let headerLevel: number = inheritHeaderLevel;

// Content processing with header syntax
const finalContent = headerLevel > 0 ? `${'#'.repeat(headerLevel)} ${newContent}` : newContent;
```

## Testing Strategy

### **Manual Testing Workflows**
1. **Basic Editing**:
   - Type text → should appear normally
   - Backspace → should delete correctly
   - Enter → should create new node

2. **Header Creation**:
   - Type "# Title" → should become large header
   - Type "## Title" → should become medium header
   - Continue through H1-H6

3. **Header Switching**:
   - In H1, place cursor at start
   - Type "## " → should preserve "Title" and change to H2

4. **Header Inheritance**:
   - In H2 node, press Enter → new node should also be H2
   - Backspace to empty → should reset to normal

5. **Multiline Behavior**:
   - Normal text + Shift+Enter → should create line break
   - Header text + Shift+Enter → should be prevented

## Performance Considerations

### **Optimizations Implemented**
- **CSS-based styling** - no DOM manipulation overhead
- **Event debouncing** - prevents excessive reactive updates
- **Minimal re-renders** - targeted updates via Svelte reactivity
- **Efficient markdown conversion** - optimized HTML ↔ Markdown transforms

### **Memory Management**
- **Component cleanup** - proper event listener removal
- **Reactive statement efficiency** - minimal reactive dependencies
- **DOM reference management** - proper binding and cleanup

## Future Enhancements

### **Short Term (Next Session)**
1. **Parent/child node relationships** - hierarchical structure
2. **List support** - unordered and ordered lists
3. **Blockquote support** - `> quoted text`

### **Medium Term**
1. **Code span support** - `` `inline code` ``
2. **Link editing** - `[text](url)` with UI helpers
3. **Image support** - `![alt](url)` with drag-drop

### **Long Term**
1. **Table editing** - visual table editor
2. **Task list integration** - may use dedicated TaskNode
3. **Advanced formatting** - additional markdown extensions

## Migration Path

### **From BaseNode to MinimalBaseNode**
The transition involves:
1. **Replace BaseNode imports** with MinimalBaseNode
2. **Update prop interfaces** to match MinimalBaseNode API
3. **Migrate CSS classes** to new naming convention
4. **Test multiline behavior** where applicable
5. **Verify event handling** matches new event signatures

### **Component Update Process**
1. Update import statements
2. Adjust prop passing
3. Update CSS selectors
4. Test functionality
5. Remove old BaseNode files once migration complete

---

**Status**: ✅ Implementation Complete - Ready for next markdown features and parent/child relationships
**Next Priority**: Parent/child node hierarchical structure implementation