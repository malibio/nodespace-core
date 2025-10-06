# NodeSpace Test Helpers Guide

**Complete API reference and best practices for unified test utilities**

## Overview

All test helpers have been consolidated into a single, well-documented module to eliminate duplication and ensure consistency across the codebase.

**Single import point:**
```typescript
import { createTestNode, waitForEffects, MOCK_TEXT_NODE } from '@tests/helpers';
```

## Table of Contents

- [Quick Start](#quick-start)
- [Node Creation Utilities](#node-creation-utilities)
- [Async/Timing Utilities](#asynctiming-utilities)
- [EventBus Testing Utilities](#eventbus-testing-utilities)
- [Mock Event Handler Utilities](#mock-event-handler-utilities)
- [Component Testing Utilities](#component-testing-utilities)
- [Standard Test Fixtures](#standard-test-fixtures)
- [Migration Guide](#migration-guide)
- [Common Patterns](#common-patterns)

---

## Quick Start

### Import Path Alias

All helpers are imported from the `@tests/helpers` alias:

```typescript
import { createTestNode, waitForEffects, MOCK_TEXT_NODE } from '@tests/helpers';
```

This alias is configured in `tsconfig.json` and maps to:
- `src/tests/helpers/test-helpers.ts` - Core helper functions
- `src/tests/fixtures/test-fixtures.ts` - Standard test fixtures

Both files are re-exported through `src/tests/helpers/index.ts` for convenient single-import access.

### Basic Test Setup

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import {
  createTestNode,
  waitForEffects,
  createMockNodeManagerEvents,
  MOCK_TEXT_NODE
} from '@tests/helpers';

describe('MyFeature', () => {
  it('should work correctly', async () => {
    const node = createTestNode({ content: 'Test content' });

    // ... perform operations ...

    await waitForEffects();
    expect(node.content).toBe('Test content');
  });
});
```

---

## Node Creation Utilities

### `createTestNode(options?)`

Create a test node with sensible defaults. Supports all use cases from previous `createNode` variants.

**Signature:**
```typescript
function createTestNode(options?: Partial<Node>): Node
```

**Parameters:**
- `options` (optional): Partial node properties to override defaults

**Returns:** Complete `Node` object ready for testing

**Examples:**

```typescript
// Simple text node with auto-generated ID
const node = createTestNode();

// Node with specific content
const node = createTestNode({ content: 'My custom content' });

// Complete node specification
const node = createTestNode({
  id: 'node-123',
  content: 'Parent node',
  nodeType: 'document',
  parentId: null,
  properties: { priority: 'high' }
});

// Child node
const parent = createTestNode({ id: 'parent-1' });
const child = createTestNode({
  parentId: 'parent-1',
  originNodeId: 'parent-1',
  content: 'Child content'
});
```

### Convenience Functions

#### `createTextNode(content)`
```typescript
const textNode = createTextNode('This is a text node');
// Equivalent to: createTestNode({ nodeType: 'text', content: '...' })
```

#### `createTaskNode(content)`
```typescript
const taskNode = createTaskNode('Complete testing setup');
// Equivalent to: createTestNode({ nodeType: 'task', content: '...' })
```

#### `createDocumentNode(content)`
```typescript
const docNode = createDocumentNode('Project documentation');
```

#### `createNodeWithParent(parentId, content, overrides?)`
```typescript
const child = createNodeWithParent('parent-1', 'Child content');
// Automatically sets parentId and originNodeId
```

#### `createTestNodeTree(depth, breadth)`
```typescript
// Create tree: 1 root with 3 children, each with 2 children
const nodes = createTestNodeTree(3, 2);
// Returns array of 1 + 3 + 6 = 10 nodes in tree structure
```

### `createTestUIState(nodeId, overrides?)`
```typescript
const uiState = createTestUIState('node-1', {
  expanded: true,
  depth: 2,
  autoFocus: false
});
```

---

## Async/Timing Utilities

### `waitForEffects(additionalMs?)`

**⭐ USE THIS INSTEAD OF `setTimeout`**

Waits for all pending Svelte 5 effects and reactive updates to complete. Essential for testing async component behavior.

**Why use this:**
- ✅ Waits for Svelte's `tick()` automatically
- ✅ Clear semantic meaning
- ✅ Consistent across all tests
- ✅ Can evolve with smarter waiting logic

**Signature:**
```typescript
async function waitForEffects(additionalMs: number = 0): Promise<void>
```

**Examples:**

```typescript
// Wait for Svelte effects only
await waitForEffects();

// Wait for effects + animations (100ms)
await waitForEffects(100);

// BEFORE (BAD):
await new Promise(resolve => setTimeout(resolve, 100));

// AFTER (GOOD):
await waitForEffects(100);
```

### `waitForEventBusBatch(batchMs?)`

Wait for EventBus batch processing to complete.

```typescript
eventBus.emit({ type: 'node:updated', nodeId: 'test-1' });
await waitForEventBusBatch(); // Default: 100ms
// Now batched handlers have processed
```

### `waitForAsync(ms)`

Generic async wait utility for specific timing needs.

```typescript
await waitForAsync(50); // Wait exactly 50ms
```

---

## EventBus Testing Utilities

### `createEventLogger()`

Creates an event logger that collects all events for verification.

**Returns:** `EventLogger` instance

**Methods:**
- `events: NodeSpaceEvent[]` - All captured events
- `subscribe()` - Subscribe to eventBus, returns unsubscribe function
- `clear()` - Clear all captured events
- `getEventTypes()` - Get array of event types in order
- `findEvent(type)` - Find first event of specific type
- `findAllEvents(type)` - Find all events of specific type
- `hasEvent(type)` - Check if event type was emitted

**Example:**

```typescript
const logger = createEventLogger();
const unsubscribe = logger.subscribe();

// ... trigger operations ...

expect(logger.events).toHaveLength(3);
expect(logger.hasEvent('node:created')).toBe(true);
expect(logger.getEventTypes()).toEqual([
  'node:created',
  'node:updated',
  'hierarchy:changed'
]);

unsubscribe();
```

### `subscribeAndCollect(eventTypes)`

Subscribe and collect specific event types.

**Parameters:**
- `eventTypes`: Event type(s) to collect (string or array)

**Returns:** `EventCollector` instance

**Methods:**
- `events: T[]` - Collected events
- `unsubscribe()` - Unsubscribe from eventBus
- `clear()` - Clear collected events
- `waitForCount(count, timeout?)` - Wait for N events to be collected

**Example:**

```typescript
const collector = subscribeAndCollect<NodeCreatedEvent>('node:created');

// ... trigger operations ...

await collector.waitForCount(2); // Wait for 2 events
expect(collector.events).toHaveLength(2);

collector.unsubscribe();
```

### `waitForEvent(eventType, timeoutMs?)`

Wait for specific event to be emitted.

```typescript
const event = await waitForEvent<NodeCreatedEvent>('node:created', 2000);
expect(event.nodeId).toBe('test-node-1');
```

### `expectEventSequence(events, expectedTypes)`

Verify events fired in expected order.

```typescript
const logger = createEventLogger();
// ... operations ...
expectEventSequence(logger.events, [
  'node:created',
  'node:updated',
  'hierarchy:changed'
]);
```

---

## Mock Event Handler Utilities

### `createMockEventHandlers()`

Creates typed mock functions for component event handlers.

**Example:**

```typescript
const handlers = createMockEventHandlers<{
  select: { id: string; title: string };
  close: void;
}>();

// Attach to component
component.$on('select', handlers.select);
component.$on('close', handlers.close);

// Verify
expect(handlers.select).toHaveBeenCalledWith(
  expect.objectContaining({ detail: { id: 'node-1' } })
);
```

### `createMockNodeManagerEvents()`

Standard mock handlers for NodeManager event interface.

```typescript
const events = createMockNodeManagerEvents();
const service = createReactiveNodeService(events);

// ... operations ...

expect(events.nodeCreated).toHaveBeenCalledWith('node-1');
expect(events.hierarchyChanged).toHaveBeenCalled();
```

### `expectHandlerCalled(handler, times?)`

Verify mock handler was called.

```typescript
expectHandlerCalled(handlers.select, 1); // Called exactly once
expectHandlerCalled(handlers.close); // Called at least once
```

---

## Component Testing Utilities

### `createKeyboardEvent(key, options?)`

Create KeyboardEvent with proper configuration.

```typescript
const enterEvent = createKeyboardEvent('Enter', { shiftKey: true });
element.dispatchEvent(enterEvent);
```

### `simulateKeyPress(element, key, modifiers?)`

Simulate key press on element.

```typescript
simulateKeyPress(input, 'Enter');
simulateKeyPress(input, 's', ['ctrl']); // Ctrl+S
simulateKeyPress(input, 'c', ['shift', 'meta']); // Shift+Cmd+C
```

### `expectEventStopped(event)`

Verify event propagation was stopped.

```typescript
const event = new KeyboardEvent('keydown', { key: 'Escape' });
event.stopPropagation = vi.fn();
element.dispatchEvent(event);
expect(expectEventStopped(event)).toBe(true);
```

### `getAriaAttributes(element)`

Get ARIA attributes from element.

```typescript
const aria = getAriaAttributes(listbox);
expect(aria.role).toBe('listbox');
expect(aria.selected).toBe('true');
expect(aria.activedescendant).toBe('option-2');
```

---

## Standard Test Fixtures

### Mock Nodes

```typescript
import {
  MOCK_TEXT_NODE,
  MOCK_TASK_NODE,
  MOCK_DOCUMENT_NODE,
  MOCK_AI_CHAT_NODE,
  MOCK_DATE_NODE,
  MOCK_NODES // Array of all above
} from '@tests/helpers';
```

### Mock Node Hierarchies

```typescript
import {
  MOCK_PARENT_NODE,
  MOCK_CHILD_NODE_1,
  MOCK_CHILD_NODE_2,
  MOCK_GRANDCHILD_NODE,
  MOCK_NODE_HIERARCHY // Array of all above
} from '@tests/helpers';
```

### Mock Autocomplete Results

```typescript
import {
  MOCK_AUTOCOMPLETE_RESULTS,
  MOCK_FILTERED_RESULTS,
  MOCK_EMPTY_RESULTS
} from '@tests/helpers';
```

### Mock Slash Commands

```typescript
import {
  MOCK_SLASH_COMMANDS,
  MOCK_HEADER_COMMANDS,
  MOCK_NON_HEADER_COMMANDS
} from '@tests/helpers';
```

### Mock Data Utilities

```typescript
// Create variation of fixture
const customNode = withOverrides(MOCK_TEXT_NODE, {
  content: 'Custom content'
});

// Create multiple nodes from template
const nodes = createNodesFromTemplate(MOCK_TEXT_NODE, 5, (node, i) => ({
  ...node,
  id: `node-${i}`,
  content: `Node ${i}`
}));
```

---

## Migration Guide

### From Old `createNode()` Variants

**BEFORE:**
```typescript
// Local helper function
function createNode(id: string, content: string, nodeType = 'text') {
  return {
    id,
    content,
    nodeType,
    parentId: null,
    // ... manual field setup
  };
}

const node = createNode('node-1', 'Test content');
```

**AFTER:**
```typescript
import { createTestNode } from '@tests/helpers';

const node = createTestNode({
  id: 'node-1',
  content: 'Test content'
});
```

### From Raw `setTimeout` Patterns

**BEFORE:**
```typescript
await new Promise(resolve => setTimeout(resolve, 100));
```

**AFTER:**
```typescript
import { waitForEffects } from '@tests/helpers';

await waitForEffects(100);
```

### From Inline Mock Data

**BEFORE:**
```typescript
const mockResults = [
  { id: 'node-1', title: 'First Node', type: 'text' },
  { id: 'node-2', title: 'Second Node', type: 'task' }
];
```

**AFTER:**
```typescript
import { MOCK_AUTOCOMPLETE_RESULTS } from '@tests/helpers';

const mockResults = MOCK_AUTOCOMPLETE_RESULTS.slice(0, 2);
```

### From Manual Event Mocking

**BEFORE:**
```typescript
const events = {
  focusRequested: vi.fn(),
  hierarchyChanged: vi.fn(),
  nodeCreated: vi.fn(),
  nodeDeleted: vi.fn()
};
```

**AFTER:**
```typescript
import { createMockNodeManagerEvents } from '@tests/helpers';

const events = createMockNodeManagerEvents();
```

---

## Common Patterns

### Service Test Pattern

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import {
  createTestNode,
  createMockNodeManagerEvents,
  waitForEffects
} from '@tests/helpers';
import { createReactiveNodeService } from '$lib/services/reactiveNodeService.svelte';

describe('MyService', () => {
  let service;
  let events;

  beforeEach(() => {
    events = createMockNodeManagerEvents();
    service = createReactiveNodeService(events);
  });

  it('should create nodes correctly', async () => {
    const nodes = [
      createTestNode({ id: 'root-1', content: 'Root' }),
      createTestNode({ id: 'child-1', content: 'Child', parentId: 'root-1' })
    ];

    service.initializeNodes(nodes, {
      expanded: true,
      autoFocus: false,
      inheritHeaderLevel: 0
    });

    await waitForEffects();

    expect(service.visibleNodes).toHaveLength(2);
    expect(events.hierarchyChanged).toHaveBeenCalled();
  });
});
```

### Component Test Pattern

```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import {
  createKeyboardEvent,
  waitForEffects,
  getAriaAttributes,
  MOCK_AUTOCOMPLETE_RESULTS
} from '@tests/helpers';
import MyComponent from '$lib/components/my-component.svelte';

describe('MyComponent', () => {
  it('should handle keyboard navigation', async () => {
    render(MyComponent, {
      props: {
        results: MOCK_AUTOCOMPLETE_RESULTS
      }
    });

    const listbox = screen.getByRole('listbox');
    const aria = getAriaAttributes(listbox);
    expect(aria.role).toBe('listbox');

    document.dispatchEvent(createKeyboardEvent('ArrowDown'));
    await waitForEffects();

    const options = screen.getAllByRole('option');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');
  });
});
```

### Integration Test Pattern

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import {
  createTestNode,
  createEventLogger,
  expectEventSequence,
  waitForEventBusBatch
} from '@tests/helpers';
import { eventBus } from '$lib/services/eventBus';

describe('Feature Integration', () => {
  let logger;

  beforeEach(() => {
    logger = createEventLogger();
    logger.subscribe();
    eventBus.reset();
  });

  it('should emit events in correct order', async () => {
    const node = createTestNode({ id: 'test-1' });

    // ... trigger operations that emit events ...

    await waitForEventBusBatch();

    expectEventSequence(logger.events, [
      'node:created',
      'hierarchy:changed',
      'cache:invalidate'
    ]);
  });
});
```

---

## When to Create New Helpers

**DO create new helpers when:**
- ✅ Pattern is used in 3+ test files
- ✅ Logic is complex and error-prone
- ✅ Abstraction improves test readability
- ✅ Helper enforces best practices

**DON'T create helpers for:**
- ❌ One-off test scenarios
- ❌ Simple operations (single function call)
- ❌ Test-specific business logic
- ❌ Variations that obscure test intent

**How to add new helpers:**
1. Add function to `src/tests/helpers/test-helpers.ts`
2. Add JSDoc comments with examples
3. Export from `src/tests/helpers/index.ts`
4. Update this guide with usage examples
5. Migrate existing tests to use new helper

---

## Best Practices

### ✅ DO

- **Use semantic helpers:** `waitForEffects()` instead of raw `setTimeout`
- **Import from single location:** `@tests/helpers`
- **Use fixtures for common data:** `MOCK_TEXT_NODE` instead of inline objects
- **Document complex test setup:** Add comments explaining why
- **Test one thing per test:** Keep tests focused and clear

### ❌ DON'T

- **Don't create local helper functions:** Use shared helpers instead
- **Don't duplicate mock data:** Use unified fixtures
- **Don't skip async waits:** Always `await waitForEffects()`
- **Don't suppress lint warnings:** Fix issues properly
- **Don't use `any` types:** Use proper TypeScript types

---

## Troubleshooting

### Tests fail after migration

**Symptom:** Tests that worked with old helpers now fail

**Solutions:**
1. Check that you're using `createTestNode()` with object syntax, not positional parameters
2. Ensure all `await waitForEffects()` calls are in place
3. Verify fixture data matches your test expectations
4. Check imports are from `@tests/helpers`, not old paths

### TypeScript errors with new helpers

**Symptom:** Type errors when using helpers

**Solutions:**
1. Ensure you're importing the correct type: `Node` from `$lib/types/node`
2. Check that overrides match the `Node` interface
3. Use `Partial<Node>` for optional fields
4. Don't mix old `MockNodeData` type with new `Node` type

### EventBus events not captured

**Symptom:** Event logger shows no events

**Solutions:**
1. Ensure `logger.subscribe()` is called **before** operations
2. Add `await waitForEffects()` or `await waitForEventBusBatch()`
3. Check that EventBus is not reset after subscribe
4. Verify events are actually being emitted (check implementation)

---

## Additional Resources

- [Test Strategy Documentation](../architecture/testing-strategy.md)
- [Component Testing Guide](./component-testing.md)
- [Integration Testing Guide](./integration-testing.md)
- [Node Type System](../architecture/core/unified-node-schema.md)

---

**Questions or suggestions?**

If you find a pattern that should be a helper or notice duplication, please:
1. Open a GitHub issue with the `testing` label
2. Propose the helper in the issue
3. Submit a PR with the implementation and this guide updated
