# ADR-001: Always-Editing Mode Architecture

**Date:** 2025-08-11  
**Status:** Accepted  
**Context:** Issue #26 Hybrid Markdown Rendering System

## Context and Problem Statement

NodeSpace nodes need to provide seamless text editing experiences. The original BaseNode implementation used focused/unfocused states with separate edit and display modes, creating complexity around:

- Mode switching logic (~100+ lines of code)
- Custom cursor positioning system (MockTextElement + CursorPositioning.js, ~300+ lines)
- State synchronization between edit and display modes
- User experience friction when switching between modes

**Key Question:** Should NodeSpace nodes maintain separate edit/display modes, or use a unified always-editing approach?

## Decision Drivers

- **User Experience:** Seamless editing without mode switching friction
- **Code Complexity:** Minimize state management and positioning logic
- **Performance:** Reduce rendering complexity and state synchronization
- **Consistency:** Unified behavior across all node types (TextNode, TaskNode, PersonNode)
- **Maintainability:** Simpler architecture with fewer edge cases

## Options Considered

### Option 1: Enhanced Edit/Display Mode Toggle (Status Quo)
- **Approach:** Improve existing focused/unfocused state system
- **Pros:** Familiar pattern, read-only nodes have clear visual distinction
- **Cons:** Complex state management, custom positioning logic required, mode switching friction

### Option 2: Always-Editing Mode (Chosen)
- **Approach:** Nodes always show interactive editor, use `editable` prop for read-only
- **Pros:** Eliminates mode switching, native editor positioning, consistent behavior
- **Cons:** Less visual distinction between editable/read-only, potential performance concerns

### Option 3: Hybrid Approach
- **Approach:** Always-editing for TextNode, traditional modes for others
- **Pros:** Best of both approaches for different use cases
- **Cons:** Inconsistent behavior, still maintains dual complexity

## Decision Outcome

**Chosen:** Always-Editing Mode (Option 2)

### Implementation Strategy
1. **Remove focused/unfocused states** from BaseNode
2. **Always render editor** (CodeMirror) with different configurations
3. **Use `editable` prop** to control read-only behavior via CodeMirror's `editable: false`
4. **Eliminate MockTextElement system** - rely on native CodeMirror click-to-cursor
5. **Visual distinction** via design system colors (muted foreground for read-only)

### Architecture Changes
- **BaseNode:** Always shows CodeMirror editor, no mode switching logic
- **TextNode:** Extends BaseNode with `multiline={true}`, `markdown={true}`
- **PersonNode:** Extends BaseNode with `editable={false}`, composed content
- **TaskNode:** Extends BaseNode with task-specific configurations

## Consequences

### Positive
- **~300+ lines removed:** MockTextElement and CursorPositioning complexity eliminated
- **Native positioning:** CodeMirror handles click-to-cursor automatically
- **Consistent UX:** All nodes behave predictably with same interaction patterns
- **Simpler state:** No focused/unfocused state management needed
- **Better performance:** No mode switching renders, single component lifecycle

### Negative
- **Less visual distinction:** Read-only nodes look similar to editable (mitigated by color)
- **Bundle size:** CodeMirror for all nodes vs textarea (acceptable: <200KB impact)
- **Learning curve:** Different from traditional edit/display patterns

### Risks and Mitigation
- **Risk:** Users confused by always-visible cursor
- **Mitigation:** Use design system colors to distinguish editable vs read-only
- **Risk:** Performance with many nodes
- **Mitigation:** CodeMirror is lightweight, profile actual usage patterns

## Implementation Requirements

### Component Composition Pattern
```svelte
<!-- BaseNode: Always-editing foundation -->
<CodeMirrorEditor 
  {content} 
  multiline={false} 
  markdown={false}
  editable={contentEditable}
  on:contentChanged={handleContentChanged}
/>

<!-- TextNode: Extends BaseNode -->
<BaseNode {nodeId} multiline={true} markdown={true}>
  <!-- Inherits always-editing behavior -->
</BaseNode>

<!-- PersonNode: Extends BaseNode -->  
<BaseNode {nodeId} contentEditable={false}>
  <!-- Read-only always-editing -->
</BaseNode>
```

### Visual Design System Integration
```css
/* Editable state */
.ns-editor--editable {
  color: hsl(var(--foreground));
  cursor: text;
}

/* Read-only state */
.ns-editor--readonly {
  color: hsl(var(--muted-foreground));
  cursor: default;
}
```

## Related Decisions
- **ADR-002:** Component Composition Inheritance
- **ADR-003:** Universal CodeMirror Strategy  
- **ADR-004:** Debounced Events Architecture

## References
- Issue #26: Hybrid Markdown Rendering System
- Issue #46: CodeMirror Foundation Setup
- MockTextElement Analysis: ~360 lines of positioning complexity
- Design System: Color tokens for state distinction