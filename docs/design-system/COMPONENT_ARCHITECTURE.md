# NodeSpace Component Architecture Guidelines

## Overview

NodeSpace uses a hybrid component architecture combining **shadcn-svelte** professional UI components with **custom NodeSpace-specific components**. This approach provides both industry-standard patterns and domain-specific functionality for AI-native knowledge management.

## Architecture Strategy

### **Two-Tier Component System**

**Tier 1: shadcn-svelte Foundation Components**
- Professional, battle-tested UI components (Button, Input, Card, Dialog, etc.)
- Industry-standard design patterns and accessibility
- Unified color system using shadcn-svelte variables
- Copy-paste approach for full customization control

**Tier 2: NodeSpace Domain Components**  
- Custom components for knowledge management (BaseNode, TextNode, etc.)
- AI-specific interaction patterns
- Hierarchical node displays and navigation
- Built on shadcn-svelte foundation but with domain expertise

## Core Principles

### 1. Unified Color System (shadcn-svelte)
- All components use **shadcn-svelte color variables** for consistency
- Single source of truth for colors, spacing, and design tokens
- Automatic theme switching support
- NodeSpace colors mapped to shadcn-svelte variables

```svelte
<!-- ✅ Good: Using shadcn-svelte design tokens -->
<style>
  .my-component {
    background-color: hsl(var(--background));
    color: hsl(var(--foreground));
    border: 1px solid hsl(var(--border));
    padding: var(--ns-spacing-4); /* Legacy support during transition */
  }
</style>

<!-- ✅ Better: Use shadcn-svelte components directly -->
<script>
  import { Button } from "$lib/components/ui/button";
  import { Card, CardContent } from "$lib/components/ui/card";
</script>

<Card>
  <CardContent>
    <Button variant="default">Professional UI</Button>
  </CardContent>
</Card>

<!-- ❌ Bad: Hard-coded values -->
<style>
  .my-component {
    background-color: #ffffff;
    padding: 16px;
    color: #1a1a1a;
  }
</style>
```

### 2. TypeScript First
- All components must be written in TypeScript
- Export comprehensive prop interfaces
- Use strict type checking

```typescript
export interface NodeComponentProps {
  nodeType: NodeType;
  nodeId: string;
  title: string;
  content?: string;
  disabled?: boolean;
  loading?: boolean;
  onClick?: (event: MouseEvent) => void;
}
```

### 3. Accessibility by Default
- WCAG 2.1 AA compliance built into every component
- Proper ARIA labels and roles
- Keyboard navigation support
- Focus management

```svelte
<button
  class="ns-button"
  type="button"
  disabled={disabled}
  aria-label={ariaLabel || title}
  on:click={handleClick}
  on:keydown={handleKeydown}
>
  {title}
</button>
```

### 4. Event-Driven Architecture
- Use Svelte's event dispatcher for component communication
- Consistent event naming conventions
- Provide both event and data in dispatched events

```typescript
const dispatch = createEventDispatcher<{
  click: { nodeId: string; event: MouseEvent };
  select: { nodeId: string; selected: boolean };
  change: { value: string; event: Event };
}>();

function handleClick(event: MouseEvent) {
  dispatch('click', { nodeId, event });
}
```

## Component Structure

### File Organization
```
src/lib/components/
├── ui/                          # shadcn-svelte components
│   ├── button/
│   │   ├── button.svelte       # Professional Button component
│   │   └── index.ts
│   ├── input/
│   │   ├── input.svelte        # Professional Input component  
│   │   └── index.ts
│   ├── card/
│   │   ├── card.svelte         # Card container
│   │   ├── card-content.svelte # Card content area
│   │   ├── card-header.svelte  # Card header
│   │   └── index.ts
│   └── textarea/
│       ├── textarea.svelte     # Professional TextArea
│       └── index.ts
├── TextNode.svelte              # Custom NodeSpace component
├── HierarchyDemo.svelte         # Domain-specific component
└── index.ts                     # Component exports

src/lib/design/                  # Design system foundation
├── components/
│   ├── BaseNode.svelte          # Core node architecture
│   └── ThemeProvider.svelte     # Theme context provider
├── theme.ts                     # Theme management
└── tokens.ts                    # Legacy design tokens

src/lib/utils.ts                 # shadcn-svelte utilities (cn, etc.)
```

### Component Development Patterns

#### **Pattern 1: Use shadcn-svelte Components (Preferred)**
For standard UI elements, use professional shadcn-svelte components:

```svelte
<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/components/ui/card";
</script>

<!-- Professional, accessible, consistent -->
<Card>
  <CardHeader>
    <CardTitle>Settings</CardTitle>
  </CardHeader>
  <CardContent class="space-y-4">
    <Input placeholder="Enter your name..." />
    <Button variant="default">Save Changes</Button>
  </CardContent>
</Card>
```

#### **Pattern 2: Custom NodeSpace Components**
For domain-specific functionality, build custom components using shadcn-svelte foundation:

```svelte
<!--
  TextNode.svelte - Custom NodeSpace Component
  
  Domain-specific text node with AI integration and hierarchical display.
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { Button } from "$lib/components/ui/button";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { cn } from "$lib/utils.js";
  
  // NodeSpace-specific props
  export let nodeId: string = '';
  export let title: string = '';
  export let content: string = '';
  export let hasChildren: boolean = false;
  export let compact: boolean = false;
  
  // Event dispatcher for node interactions
  const dispatch = createEventDispatcher<{
    edit: { nodeId: string };
    expand: { nodeId: string };
    aiAssist: { nodeId: string; content: string };
  }>();
  
  // Use shadcn-svelte styling with custom logic
  $: containerClasses = cn(
    "text-node transition-all duration-200",
    compact && "text-node--compact",
    hasChildren && "text-node--parent"
  );
</script>

<!-- Build on shadcn-svelte Card foundation -->
<Card class={containerClasses}>
  <CardContent class="p-4">
    <div class="flex items-start gap-3">
      <!-- Custom circle indicator for NodeSpace hierarchy -->
      <div class="circle-indicator {hasChildren ? 'has-children' : 'childless'}"></div>
      
      <div class="flex-1">
        <h3 class="font-semibold text-foreground">{title}</h3>
        <p class="text-muted-foreground mt-1">{content}</p>
      </div>
      
      <!-- Use shadcn-svelte Button for actions -->
      <Button 
        variant="ghost" 
        size="sm" 
        onclick={() => dispatch('aiAssist', { nodeId, content })}
      >
        AI Assist
      </Button>
    </div>
  </CardContent>
</Card>

<style>
  /* Custom styles for NodeSpace-specific features */
  .circle-indicator {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 6px;
  }
  
  .circle-indicator.childless {
    background-color: hsl(var(--muted-foreground));
  }
  
  .circle-indicator.has-children {
    background-color: hsl(var(--primary));
    box-shadow: 0 0 8px hsl(var(--primary) / 0.3);
  }
  
  .text-node--compact {
    /* Custom compact styling using shadcn variables */
  }
</style>
```

## Naming Conventions

### CSS Classes
- Use `ns-` prefix for all design system classes
- Follow BEM methodology: `ns-component__element--modifier`
- Use semantic names: `ns-button--primary`, `ns-node--selected`

### Props
- Use camelCase for prop names
- Boolean props should be descriptive: `disabled`, `loading`, `selected`
- Enum props should use string unions: `variant: 'primary' | 'secondary'`

### Events
- Use standard DOM event names when possible: `click`, `focus`, `change`
- Custom events should be descriptive: `select`, `expand`, `validate`

## Theme Integration

### Using Theme Context
```svelte
<script lang="ts">
  import { getContext } from 'svelte';
  import type { ThemeContext } from '../theme.js';
  
  const theme = getContext<ThemeContext>('theme');
  
  $: isDark = theme.theme === 'dark';
  $: tokens = theme.tokens;
</script>
```

### Theme-Aware Styling
```svelte
<style>
  .component {
    /* Always use CSS custom properties */
    background-color: var(--ns-color-surface-background);
    
    /* Theme-specific adjustments via CSS custom properties */
    box-shadow: var(--ns-shadow-sm);
  }
  
  /* Avoid theme-specific selectors */
  /* Use design tokens instead */
</style>
```

## State Management

### Local State
- Use Svelte's reactive statements for derived state
- Keep component state minimal and focused
- Prefer props for external state control

### Event Communication
- Use event dispatching for parent communication
- Include relevant data with events
- Avoid tight coupling between components

## Testing Guidelines

### Component Testing
```typescript
import { render, fireEvent } from '@testing-library/svelte';
import Component from './Component.svelte';

test('renders with correct attributes', () => {
  const { getByRole } = render(Component, {
    props: { title: 'Test Button' }
  });
  
  const button = getByRole('button');
  expect(button).toHaveTextContent('Test Button');
  expect(button).not.toBeDisabled();
});

test('dispatches click event', async () => {
  const { getByRole, component } = render(Component);
  const clickHandler = jest.fn();
  
  component.$on('click', clickHandler);
  
  await fireEvent.click(getByRole('button'));
  
  expect(clickHandler).toHaveBeenCalledWith(
    expect.objectContaining({
      detail: expect.objectContaining({
        event: expect.any(MouseEvent)
      })
    })
  );
});
```

### Accessibility Testing
```typescript
test('supports keyboard navigation', async () => {
  const { getByRole } = render(Component);
  const button = getByRole('button');
  
  button.focus();
  expect(button).toHaveFocus();
  
  await fireEvent.keyDown(button, { key: 'Enter' });
  // Assert expected behavior
});

test('provides proper ARIA attributes', () => {
  const { getByRole } = render(Component, {
    props: { disabled: true, ariaLabel: 'Custom label' }
  });
  
  const button = getByRole('button');
  expect(button).toHaveAttribute('aria-disabled', 'true');
  expect(button).toHaveAttribute('aria-label', 'Custom label');
});
```

## Performance Guidelines

### Bundle Size
- Use code splitting for large components
- Prefer CSS custom properties over JavaScript theme switching
- Minimize runtime theme calculations

### Rendering Performance
- Use CSS transforms for animations
- Avoid frequent DOM updates
- Leverage Svelte's reactive optimizations

```svelte
<!-- Good: CSS-based animation -->
<style>
  .component {
    transform: translateY(0);
    transition: transform var(--ns-duration-fast) var(--ns-easing-easeOut);
  }
  
  .component--active {
    transform: translateY(-2px);
  }
</style>

<!-- Avoid: JavaScript-based position updates -->
<script>
  // Don't manually update styles in JavaScript
  // Let CSS handle transitions
</script>
```

## Common Patterns

### Compound Components
```svelte
<!-- Card.svelte -->
<article class="ns-card">
  <slot />
</article>

<!-- CardHeader.svelte -->
<header class="ns-card__header">
  <slot />
</header>

<!-- CardContent.svelte -->
<div class="ns-card__content">
  <slot />
</div>

<!-- Usage -->
<Card>
  <CardHeader>
    <h2>Title</h2>
  </CardHeader>
  <CardContent>
    <p>Content</p>
  </CardContent>
</Card>
```

### Render Props Pattern
```svelte
<DataProvider let:data let:loading let:error>
  {#if loading}
    <LoadingSpinner />
  {:else if error}
    <ErrorMessage {error} />
  {:else}
    <DataDisplay {data} />
  {/if}
</DataProvider>
```

This architecture ensures consistency, maintainability, and excellent developer experience across all NodeSpace components.