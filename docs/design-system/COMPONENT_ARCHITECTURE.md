# NodeSpace Component Architecture Guidelines

## Overview

NodeSpace components follow consistent patterns for maintainability, accessibility, and developer experience. This document outlines the architectural standards for building components within the design system.

## Core Principles

### 1. Design Token First
- All styling must use CSS custom properties from the design system
- No hard-coded colors, spacing, or typography values
- Support automatic theme switching

```svelte
<!-- ✅ Good: Using design tokens -->
<style>
  .my-component {
    background-color: var(--ns-color-surface-background);
    padding: var(--ns-spacing-4);
    color: var(--ns-color-text-primary);
  }
</style>

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
src/lib/design/components/
├── BaseNode.svelte              # Core node component
├── ThemeProvider.svelte         # Theme context provider
├── Button/
│   ├── Button.svelte           # Main button component
│   ├── ButtonGroup.svelte      # Button grouping
│   └── index.ts                # Exports
├── Input/
│   ├── Input.svelte
│   ├── TextArea.svelte
│   └── index.ts
└── index.ts                    # All component exports
```

### Component Template
```svelte
<!--
  ComponentName.svelte
  
  Brief description of what this component does and when to use it.
-->

<script lang="ts">
  import { createEventDispatcher, getContext } from 'svelte';
  import type { ThemeContext } from '../theme.js';
  
  // Props with defaults
  export let variant: 'primary' | 'secondary' = 'primary';
  export let size: 'small' | 'medium' | 'large' = 'medium';
  export let disabled: boolean = false;
  export let loading: boolean = false;
  export let className: string = '';
  
  // Internal state
  let isActive = false;
  
  // Theme context
  const themeContext = getContext<ThemeContext>('theme');
  
  // Event dispatcher
  const dispatch = createEventDispatcher<{
    click: { event: MouseEvent };
    focus: { event: FocusEvent };
    blur: { event: FocusEvent };
  }>();
  
  // Event handlers
  function handleClick(event: MouseEvent) {
    if (disabled || loading) return;
    dispatch('click', { event });
  }
  
  // CSS classes
  $: componentClasses = [
    'ns-component',
    `ns-component--${variant}`,
    `ns-component--${size}`,
    disabled && 'ns-component--disabled',
    loading && 'ns-component--loading',
    className
  ].filter(Boolean).join(' ');
</script>

<button
  class={componentClasses}
  type="button"
  {disabled}
  aria-disabled={disabled}
  on:click={handleClick}
  on:focus={(e) => dispatch('focus', { event: e })}
  on:blur={(e) => dispatch('blur', { event: e })}
>
  <slot />
</button>

<style>
  .ns-component {
    /* Base component styles using design tokens */
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--ns-spacing-2);
    padding: var(--ns-spacing-3) var(--ns-spacing-4);
    font-family: var(--ns-font-family-ui);
    font-size: var(--ns-font-size-sm);
    font-weight: var(--ns-font-weight-medium);
    line-height: var(--ns-line-height-tight);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-base);
    background-color: var(--ns-color-surface-background);
    color: var(--ns-color-text-primary);
    cursor: pointer;
    transition: all var(--ns-duration-fast) var(--ns-easing-easeInOut);
    outline: none;
  }
  
  .ns-component:hover:not(.ns-component--disabled) {
    border-color: var(--ns-color-border-strong);
    background-color: var(--ns-color-surface-panel);
  }
  
  .ns-component:focus-visible {
    outline: 2px solid var(--ns-color-primary-500);
    outline-offset: 2px;
  }
  
  .ns-component--disabled {
    opacity: 0.6;
    cursor: not-allowed;
    pointer-events: none;
  }
  
  .ns-component--loading {
    pointer-events: none;
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