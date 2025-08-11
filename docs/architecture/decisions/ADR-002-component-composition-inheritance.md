# ADR-002: Component Composition Inheritance Pattern

**Date:** 2025-08-11  
**Status:** Accepted  
**Context:** Issue #26 Hybrid Markdown Rendering System

## Context and Problem Statement

NodeSpace has multiple node types (TextNode, TaskNode, PersonNode, EntityNode) that need to extend BaseNode functionality while customizing specific behaviors. The architecture needs to support:

- **Code reuse:** Common functionality in BaseNode
- **Clear inheritance:** Explicit overrides for specialized behavior  
- **Maintainability:** Easy to understand and extend
- **Type safety:** Proper TypeScript integration

**Key Question:** How should NodeSpace implement component inheritance in Svelte to maximize reuse while enabling customization?

## Decision Drivers

- **Svelte Best Practices:** Follow established Svelte component patterns
- **Clear Relationships:** Obvious inheritance chain (TextNode extends BaseNode)
- **Explicit Overrides:** Easy to see what each node type customizes
- **Event Delegation:** Natural event bubbling from base to specialized components
- **Maintainability:** Easy to add new node types without modifying BaseNode

## Options Considered

### Option 1: Component Composition (Chosen)
```svelte
<!-- TextNode.svelte -->
<BaseNode {nodeId} multiline={true} markdown={true} on:contentChanged>
  <!-- TextNode-specific content if needed -->
</BaseNode>
```
- **Pros:** Clear inheritance, explicit overrides, event delegation works naturally
- **Cons:** Requires prop drilling for complex customizations

### Option 2: Context-Based Configuration
```svelte
<!-- TextNode.svelte -->
<script>
  setContext('nodeConfig', { multiline: true, markdown: true });
</script>
<BaseNode {nodeId} on:contentChanged />
```
- **Pros:** Flexible configuration, no prop drilling
- **Cons:** Hidden configuration, harder to understand inheritance

### Option 3: Factory Pattern
```svelte
<!-- TextNode.svelte -->
<script>
  const nodeConfig = createTextNodeConfig();
</script>
<BaseNode {nodeConfig} {nodeId} on:contentChanged />
```
- **Pros:** Centralized configuration logic
- **Cons:** Less clear inheritance, additional abstraction layer

### Option 4: Slot-Based Extension
```svelte
<!-- BaseNode.svelte -->
<slot name="editor" {content} {editable}>
  <DefaultEditor {content} {editable} />
</slot>

<!-- TextNode.svelte -->
<BaseNode {nodeId}>
  <MarkdownEditor slot="editor" {content} multiline={true} />
</BaseNode>
```
- **Pros:** Maximum flexibility
- **Cons:** More complex, breaks consistency

## Decision Outcome

**Chosen:** Component Composition (Option 1)

### Rationale
- **Industry Standard:** Most common Svelte inheritance pattern
- **Explicit and Clear:** Easy to see what each node type overrides
- **Natural Event Flow:** Events bubble up from BaseNode automatically
- **TypeScript Friendly:** Props are explicitly typed and validated
- **Easy Extension:** Adding new node types requires minimal BaseNode changes

### Implementation Pattern
```svelte
<!-- BaseNode.svelte: Foundation with defaults -->
<script lang="ts">
  export let nodeId: string = '';
  export let content: string = '';
  export let multiline: boolean = false;  // Default: single-line
  export let markdown: boolean = false;   // Default: plain text
  export let contentEditable: boolean = true;
  // ... other BaseNode logic
</script>

<div class="ns-node">
  <CodeMirrorEditor 
    {content} 
    {multiline}
    {markdown}
    editable={contentEditable}
    on:contentChanged={handleContentChanged}
  />
</div>

<!-- TextNode.svelte: Extends BaseNode -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let content: string;
  // Could add TextNode-specific props here
</script>

<BaseNode 
  {nodeId}
  {content}
  multiline={true}     <!-- Override: TextNode is multiline -->
  markdown={true}      <!-- Override: TextNode has markdown -->  
  on:contentChanged    <!-- Event bubbles up -->
/>

<!-- PersonNode.svelte: Extends BaseNode -->
<script lang="ts">
  import BaseNode from './BaseNode.svelte';
  
  export let nodeId: string;
  export let firstName: string;
  export let lastName: string; 
  export let email: string;
  
  $: content = `${firstName} ${lastName} ${email}`;
</script>

<BaseNode 
  {nodeId}
  {content}
  contentEditable={false}  <!-- Override: PersonNode is read-only -->
  on:contentChanged        <!-- Event bubbles up -->
/>
```

## Consequences

### Positive
- **Clear Inheritance Chain:** `TextNode extends BaseNode` is obvious from code
- **Explicit Overrides:** Can see exactly what each node type customizes  
- **Event Delegation:** Events naturally flow: CodeMirror → BaseNode → TextNode → Parent
- **Type Safety:** TypeScript validates all props and relationships
- **Easy Extension:** New node types just compose BaseNode with different props
- **Maintainable:** BaseNode changes automatically propagate to all node types

### Negative  
- **Prop Drilling:** Complex configurations require passing many props
- **Coupling:** Specialized nodes are tightly coupled to BaseNode prop interface
- **Limited Flexibility:** Hard to completely override BaseNode behavior

### Mitigation Strategies
- **Keep BaseNode Simple:** Minimal, stable prop interface reduces coupling
- **Use Context When Needed:** For complex configurations that don't fit props
- **Composition Over Inheritance:** Add specialized behavior via slots when needed

## Implementation Guidelines

### BaseNode Design Principles
1. **Stable Interface:** Minimize breaking changes to props
2. **Sensible Defaults:** Most node types should work with minimal overrides
3. **Event Consistency:** All node types fire same events (`contentChanged`, etc.)
4. **Prop Validation:** Use TypeScript to catch inheritance issues early

### Node Type Implementation Pattern
1. **Import BaseNode:** Always import from relative path for consistency
2. **Re-export Props:** Define props that parent components need to pass
3. **Override Minimally:** Only override what's actually different
4. **Document Overrides:** Comment why specific props are overridden

### Testing Strategy
```typescript
// Ensure inheritance works correctly
test('TextNode inherits BaseNode behavior', () => {
  const { component } = render(TextNode, { nodeId: 'test', content: 'Hello' });
  
  // Should fire BaseNode events
  expect(component.$on('contentChanged')).toBeDefined();
  
  // Should have TextNode-specific behavior  
  expect(component.multiline).toBe(true);
  expect(component.markdown).toBe(true);
});
```

## Related Decisions
- **ADR-001:** Always-Editing Mode Architecture (enables consistent inheritance)
- **ADR-003:** Universal CodeMirror Strategy (foundation for all node types)
- **ADR-004:** Debounced Events Architecture (consistent event behavior)

## References
- [Svelte Component Composition Patterns](https://svelte.dev/docs#component-format)
- Issue #26: Hybrid Markdown Rendering System
- BaseNode.svelte: Foundation component implementation
- Design System: Component hierarchy documentation