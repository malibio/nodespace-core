# ADR-003: Universal CodeMirror Strategy

**Date:** 2025-08-11  
**Status:** Accepted  
**Context:** Issue #26 Hybrid Markdown Rendering System

## Context and Problem Statement

NodeSpace needs a text editing solution that works consistently across all node types while supporting specialized features like markdown rendering. The decision involves choosing between:

- **Mixed Approach:** TextArea for simple nodes, CodeMirror for complex ones
- **Universal Approach:** CodeMirror for all nodes with different configurations
- **Custom Editor:** Build specialized editor from scratch

**Key Requirements:**
- Professional editing experience across all node types
- Native click-to-cursor positioning without custom implementation
- Foundation for hybrid markdown rendering (syntax + formatting)
- Consistent behavior and styling
- Acceptable bundle size impact

## Decision Drivers

- **Consistency:** All nodes should have similar editing behavior
- **Performance:** Native positioning eliminates custom MockTextElement complexity
- **Future-Proofing:** Foundation for rich text features (#34)
- **Bundle Size:** Must stay within reasonable limits (<200KB addition)
- **Maintenance:** Prefer single editor implementation over multiple systems

## Options Considered

### Option 1: Mixed TextArea/CodeMirror Approach
```svelte
<!-- BaseNode: textarea for simple editing -->
<textarea {content} on:input />

<!-- TextNode: CodeMirror for markdown -->
<CodeMirrorEditor {content} markdown={true} />
```
- **Pros:** Minimal bundle size, familiar textarea behavior
- **Cons:** Inconsistent UX, dual editor implementations, custom positioning still needed

### Option 2: Universal CodeMirror Strategy (Chosen)
```svelte
<!-- All nodes use CodeMirror with different configs -->
<CodeMirrorEditor 
  {content} 
  multiline={nodeType === 'text'} 
  markdown={nodeType === 'text'}
  editable={contentEditable}
/>
```
- **Pros:** Consistent UX, native positioning, foundation for rich features
- **Cons:** Larger bundle size, might be overkill for simple nodes

### Option 3: Custom Editor Implementation
```svelte
<!-- Build NodeSpace-specific editor -->
<NodeSpaceEditor {content} {features} />
```
- **Pros:** Perfect fit for requirements, full control
- **Cons:** Massive implementation effort, reinventing solved problems

### Option 4: ContentEditable-Based Solution
```svelte
<!-- Use contenteditable with custom logic -->
<div contenteditable="true" on:input>
  {content}
</div>
```
- **Pros:** Native browser editing, no external dependencies
- **Cons:** Notoriously complex, browser inconsistencies, poor markdown support

## Decision Outcome

**Chosen:** Universal CodeMirror Strategy (Option 2)

### Rationale
- **Eliminates Complexity:** Removes ~360 lines of MockTextElement positioning code
- **Professional Experience:** CodeMirror provides industry-standard editing UX  
- **Native Positioning:** Click-to-cursor works perfectly out of the box
- **Hybrid Rendering Ready:** Built-in support for syntax highlighting + styling
- **Acceptable Bundle:** ~70-80KB impact (well under 200KB target)
- **Future Foundation:** Enables rich decorations, collaborative editing, etc.

### Implementation Strategy

#### CodeMirrorEditor Component
```typescript
// Minimal, configurable wrapper
interface CodeMirrorEditorProps {
  content: string;
  multiline: boolean;      // true for TextNode, false for others
  markdown: boolean;       // true for TextNode, false for others  
  editable: boolean;       // false for PersonNode, true for others
  on:contentChanged: (content: string) => void;  // Debounced events
}
```

#### Node Type Configurations
```svelte
<!-- BaseNode: Single-line, plain text -->
<CodeMirrorEditor multiline={false} markdown={false} />

<!-- TextNode: Multiline, markdown syntax -->
<CodeMirrorEditor multiline={true} markdown={true} />

<!-- PersonNode: Read-only, computed content -->  
<CodeMirrorEditor editable={false} content={computedName} />

<!-- TaskNode: Single-line with task-specific shortcuts -->
<CodeMirrorEditor multiline={false} markdown={false} />
```

#### Bundle Optimization Strategy
```javascript
// Import only required CodeMirror modules
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';  
import { markdown } from '@codemirror/lang-markdown';

// Conditional extensions based on node type
const extensions = [
  ...(markdown ? [markdown()] : []),
  ...(multiline ? [] : [singleLineExtensions])
];
```

## Consequences

### Positive
- **~360 Lines Removed:** MockTextElement and CursorPositioning.js eliminated
- **Consistent UX:** All nodes have professional editing experience
- **Native Positioning:** Click-to-cursor works perfectly without custom code
- **Performance:** No custom positioning calculations needed  
- **Future-Ready:** Foundation for Issue #34 rich decorations
- **Maintenance:** Single editor system instead of dual implementation

### Negative
- **Bundle Size:** ~70-80KB increase (acceptable, under 200KB target)
- **Potential Overkill:** Simple nodes get full editor features
- **Learning Curve:** CodeMirror API instead of simple textarea

### Risk Mitigation
- **Bundle Size:** Tree-shake unused features, lazy load non-essential extensions
- **Performance:** CodeMirror is highly optimized, profile real usage patterns
- **Complexity:** Wrapper component abstracts CodeMirror complexity from node types

## Implementation Requirements

### Phase 1: Foundation Component
```svelte
<!-- CodeMirrorEditor.svelte -->
<script lang="ts">
  export let content: string = '';
  export let multiline: boolean = false;
  export let markdown: boolean = false;  
  export let editable: boolean = true;
  
  // Debounced contentChanged events only
  const dispatch = createEventDispatcher<{
    contentChanged: { content: string };
  }>();
</script>
```

### Phase 2: BaseNode Integration  
```svelte
<!-- BaseNode.svelte: Always show CodeMirror -->
<CodeMirrorEditor 
  {content}
  {multiline}
  {markdown} 
  editable={contentEditable}
  on:contentChanged={handleContentChanged}
/>
```

### Phase 3: Specialized Node Types
```svelte
<!-- TextNode.svelte: Override for markdown -->
<BaseNode {nodeId} multiline={true} markdown={true} />

<!-- PersonNode.svelte: Override for read-only -->
<BaseNode {nodeId} contentEditable={false} />
```

## Performance Benchmarks

### Bundle Size Analysis
- **Target:** <200KB total addition
- **Actual:** ~70-80KB (CodeMirror core + markdown)
- **Optimization:** Tree-shaking removes unused features

### Response Time Requirements  
- **Keystroke to Update:** <50ms (CodeMirror native performance)
- **Editor Initialization:** <100ms (fast enough for good UX)
- **Click-to-Position:** <25ms (native browser handling)

### Memory Requirements
- **Baseline:** No memory leaks during long editing sessions
- **Multiple Editors:** Linear scaling (no O(n²) issues)
- **Cleanup:** Proper destruction on component unmount

## Related Decisions
- **ADR-001:** Always-Editing Mode (enables universal editor approach)
- **ADR-002:** Component Composition Inheritance (consistent editor across node types)
- **ADR-004:** Debounced Events Architecture (performance optimization)

## Future Implications

### Enables Future Features
- **Issue #34:** Rich decorations and hybrid rendering
- **Collaborative Editing:** CodeMirror has built-in collaborative support
- **Advanced Shortcuts:** Professional editor shortcuts and behaviors
- **Custom Extensions:** CodeMirror's extension system for NodeSpace-specific features

### Technical Foundation
- **Syntax Highlighting:** Built-in for markdown, extensible for other formats
- **Theming:** Professional theme system for NodeSpace branding
- **Accessibility:** CodeMirror handles ARIA and keyboard navigation
- **Mobile Support:** CodeMirror mobile-optimized touch handling

## References
- [CodeMirror 6 Architecture](https://codemirror.net/docs/)
- Issue #26: Hybrid Markdown Rendering System
- Issue #46: CodeMirror Foundation Setup  
- Bundle Size Analysis: webpack-bundle-analyzer results
- Performance Benchmarks: Lighthouse and custom profiling