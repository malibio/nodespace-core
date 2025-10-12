# NodeSpace Testing Guide - OFFICIAL TESTING APPROACH

> **✅ AUTHORITATIVE DOCUMENT**: This is the single source of truth for NodeSpace testing practices. All agents and developers should follow this guide.
>
> **Status**: Active (Updated 2025-01-16)
> **Scope**: Covers actual implementation approach with pragmatic mocks for early-stage development

**Practical testing for early-stage development with gradual complexity growth.**

## Quick Start

```bash
# Rust backend tests
cargo test

# Frontend tests  
bun run test

# Frontend tests with coverage
bun run test:coverage

# All tests
cargo test && bun run test
```

## Current Test Status (Updated 2025-01-16)

**Frontend Tests**: 377 passing, 2 skipped, 3 intermittent failures
**Backend Tests**: Basic coverage with example tests
**Overall Health**: ✅ Good (98%+ pass rate)

**Known Issues**:
- 3 ContentProcessor tests have intermittent failures due to test isolation issues (pass individually, fail in full suite)
- 2 Svelte reactivity tests intentionally skipped (require Svelte runtime)

**Recently Fixed**:
- ContentEditableController nesting logic for mixed formatting markers
- marked.js performance test threshold adjustment

## 4 Core Testing Types

### 1. Unit Testing
Test individual functions and methods in isolation.

**Rust Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_validation() {
        let node = NodeData::new(NodeType::Text, "".to_string());
        let result = node.validate();
        assert!(result.is_err());
    }
}
```

**TypeScript Example:**
```typescript
import { describe, it, expect } from 'vitest';
import { validateNodeContent } from './nodeUtils';

describe('Node Validation', () => {
  it('rejects empty content', () => {
    expect(validateNodeContent('')).toBe(false);
  });
});
```

### 2. Component Testing
Test UI components with realistic user interactions.

```typescript
import { render, screen, fireEvent } from '@testing-library/svelte';
import TextNode from '$lib/components/TextNode.svelte';

describe('TextNode', () => {
  it('renders and edits content', async () => {
    render(TextNode, { props: { content: 'Hello' } });
    
    const element = screen.getByText('Hello');
    await fireEvent.click(element);
    
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });
});
```

### 3. Integration Testing  
Test component workflows and API integration.

```typescript
import { createMockAPI } from './fixtures/mockAPI';

describe('Node Management', () => {
  it('saves node via API', async () => {
    const mockAPI = createMockAPI();
    const result = await mockAPI.saveNode({ content: 'Test' });
    
    expect(result.id).toBeTruthy();
    expect(mockAPI.getLastSaved().content).toBe('Test');
  });
});
```

### 4. Basic E2E Testing
Test critical user workflows when UI exists.

```typescript
import { test, expect } from '@playwright/test';

test('creates and saves a note', async ({ page }) => {
  await page.goto('/');
  await page.click('[data-testid="new-note"]');
  await page.fill('textarea', 'My note content');
  await page.click('[data-testid="save"]');
  
  await expect(page.locator('text=My note content')).toBeVisible();
});
```

## Simple Mock Strategy

### Mock Data Store
```typescript
export class SimpleMockStore {
  private nodes = new Map<string, NodeData>();

  async save(node: NodeData): Promise<string> {
    const id = node.id || `node-${Date.now()}`;
    this.nodes.set(id, { ...node, id });
    return id;
  }

  async load(id: string): Promise<NodeData | null> {
    return this.nodes.get(id) || null;
  }
}
```

### Test Utilities
```typescript
export function createTestNode(content = 'Test'): NodeData {
  return {
    id: `test-${Date.now()}`,
    type: 'text',
    content,
    createdAt: new Date()
  };
}

export function renderWithStore(component: any, props = {}) {
  const mockStore = new SimpleMockStore();
  return render(component, {
    props: { ...props, dataStore: mockStore },
    context: new Map([['dataStore', mockStore]])
  });
}
```

## Coverage Requirements

- **Rust Backend**: >80% line coverage
- **Frontend Components**: >75% statement coverage  
- **Critical Workflows**: 100% tested

## Testing Commands

```bash
# Run tests with coverage
cargo test --coverage        # Rust (requires cargo-llvm-cov)
bun run test:coverage       # Frontend

# Run specific tests
cargo test node_validation  # Rust specific test
bun run test TextNode       # Frontend specific test
```

## Common Test Patterns & Scenarios

### 1. Keyboard Interaction Testing
Test keyboard shortcuts and navigation extensively:

```typescript
// Example: Testing Cmd+B for bold formatting
const boldEvent = new KeyboardEvent('keydown', {
  key: 'b',
  metaKey: true,  // Cmd on Mac
  ctrlKey: true,  // Ctrl on PC
  bubbles: true
});
element.dispatchEvent(boldEvent);
expect(element.textContent).toBe('**selected text**');
```

**Recommended Keyboard Tests:**
- ✅ Formatting shortcuts (Cmd+B, Cmd+I)
- ✅ Navigation (Tab, Shift+Tab for indentation)
- ✅ Node operations (Enter for new nodes, Backspace for merging)
- ✅ Arrow keys for cursor movement
- ✅ Home/End keys for line navigation

### 2. Node Lifecycle Testing
Test complete node creation, update, and deletion workflows:

```typescript
// Test node creation → content editing → deletion chain
it('should handle complete node lifecycle', () => {
  const nodeId = nodeManager.createNode('text', 'Initial content');
  nodeManager.updateNodeContent(nodeId, 'Updated content');
  expect(nodeManager.getNode(nodeId).content).toBe('Updated content');

  nodeManager.deleteNode(nodeId);
  expect(nodeManager.getNode(nodeId)).toBeNull();
});
```

### 3. Event Bus Integration Testing
Test coordination between services through EventBus:

```typescript
// Test that node operations emit proper events
it('should emit events for node operations', () => {
  const eventSpy = vi.fn();
  eventBus.subscribe('node:created', eventSpy);

  nodeManager.createNode('text', 'Test content');

  expect(eventSpy).toHaveBeenCalledWith({
    type: 'node:created',
    nodeId: expect.any(String),
    data: expect.objectContaining({ content: 'Test content' })
  });
});
```

### 4. Content Processing Chain Testing
Test markdown processing with proper formatting:

```typescript
// Test source → AST → HTML round-trip
it('should maintain content integrity through processing chain', () => {
  const source = '# Header\n\nThis is **bold** text.';
  const ast = processor.parseMarkdown(source);
  const html = processor.renderAST(ast);
  const backToSource = processor.astToMarkdown(ast);

  expect(html).toContain('<h1 class="ns-markdown-heading">');
  expect(html).toContain('<strong class="ns-markdown-bold">');
  expect(backToSource).toBe(source);
});
```

### 5. Tauri IPC Testing (Frontend-Backend)
Test communication between frontend and Rust backend:

```typescript
// Mock Tauri commands for testing
const mockTauri = { invoke: vi.fn() };
global.__TAURI__ = mockTauri;

it('should call Rust commands correctly', async () => {
  mockTauri.invoke.mockResolvedValue('Hello, Test!');

  const result = await window.__TAURI__.invoke('greet', { name: 'Test' });

  expect(mockTauri.invoke).toHaveBeenCalledWith('greet', { name: 'Test' });
  expect(result).toBe('Hello, Test!');
});
```

### 6. Error Handling & Edge Cases
Always test error conditions and edge cases:

```typescript
// Test empty content, null values, invalid types
const errorCases = [
  { content: '', expectedError: 'Content cannot be empty' },
  { content: '   ', expectedError: 'Content cannot be empty' },
  { nodeType: 'invalid', expectedError: 'Invalid node type' }
];

errorCases.forEach(({ content, nodeType, expectedError }) => {
  it(`should handle error: ${expectedError}`, () => {
    expect(() => {
      createNode(content || 'valid', nodeType || 'text');
    }).toThrow(expectedError);
  });
});
```

### 7. Performance Testing
Test performance with realistic data sizes:

```typescript
// Test with large datasets
it('should handle 1000 nodes efficiently', () => {
  const start = performance.now();

  for (let i = 0; i < 1000; i++) {
    nodeManager.createNode('text', `Node ${i} content`);
  }

  const duration = performance.now() - start;
  expect(duration).toBeLessThan(1000); // Should complete in <1s
  expect(nodeManager.getAllNodes()).toHaveLength(1000);
});
```

### 8. Concurrency & Race Conditions
Test thread safety and concurrent operations:

```rust
// Rust example: Test atomic operations
#[test]
fn test_concurrent_id_generation() {
    let handles: Vec<_> = (0..10).map(|_| {
        thread::spawn(|| {
            (0..100).map(|_| generate_unique_id()).collect::<Vec<_>>()
        })
    }).collect();

    let mut all_ids = HashSet::new();
    for handle in handles {
        for id in handle.join().unwrap() {
            assert!(all_ids.insert(id), "Duplicate ID generated");
        }
    }
}
```

### 9. Test Isolation Best Practices
Prevent test contamination:

```typescript
// Always clean up shared state
beforeEach(() => {
  // Reset singletons
  ContentProcessor.getInstance().clearReferenceCache();

  // Clear DOM
  document.body.innerHTML = '';

  // Reset mocks
  vi.clearAllMocks();

  // Reset global state
  nodeManager.reset();
});
```

**CRITICAL: Vitest Setup File Pattern**

When registering singletons for tests (plugin registries, service containers), **always use setup files** (`setupFiles` in vitest.config.ts), **never global setup** (`globalSetup`).

**Why?** Vitest creates separate module graphs for:
- Global Setup: Node.js context
- Test Files & Components: Happy-DOM browser context

This causes module duplication even with `globalThis` singleton patterns.

**Correct Pattern:**
```typescript
// ✅ src/tests/setup.ts (setupFiles - runs in Happy-DOM context)
import { pluginRegistry } from '$lib/plugins/index';
import { registerCorePlugins } from '$lib/plugins/core-plugins';

if (!pluginRegistry.hasPlugin('text')) {
  registerCorePlugins(pluginRegistry);
}
```

**Wrong Pattern:**
```typescript
// ❌ src/tests/global-setup.ts (globalSetup - runs in Node context)
// This creates a SEPARATE registry instance that tests can't access
export default async function setup() {
  const { pluginRegistry } = await import('$lib/plugins/index');
  registerCorePlugins(pluginRegistry); // Wrong context!
}
```

**See Also:** [Vitest Module Duplication Lesson Learned](./lessons/vitest-module-duplication-fix.md) for detailed explanation and troubleshooting.

### 10. Mock Service Patterns
Create realistic but controlled test environments:

```typescript
// Good mock pattern - realistic but predictable
class MockDataStore {
  private data = new Map();

  async save(id: string, data: any): Promise<void> {
    // Simulate async delay
    await new Promise(resolve => setTimeout(resolve, 1));
    this.data.set(id, data);
  }

  async load(id: string): Promise<any> {
    await new Promise(resolve => setTimeout(resolve, 1));
    return this.data.get(id) || null;
  }

  // Helper for tests
  getStoredData() { return Array.from(this.data.entries()); }
}
```

## Test Quality Checklist

Before committing new tests, verify:

- ✅ **Descriptive names**: Test names clearly describe what they test
- ✅ **Single responsibility**: Each test verifies one specific behavior
- ✅ **Proper setup/cleanup**: No test pollution or shared state
- ✅ **Edge cases covered**: Empty inputs, null values, boundary conditions
- ✅ **Error conditions tested**: Invalid inputs and error handling
- ✅ **Performance considerations**: Large datasets and time limits where relevant
- ✅ **Realistic scenarios**: Tests reflect actual user workflows
- ✅ **Maintainable**: Tests are easy to understand and modify

## Debugging Failed Tests

1. **Run tests individually** first to check for isolation issues
2. **Check for shared state** between tests (singletons, global variables)
3. **Verify mock cleanup** - ensure mocks are reset between tests
4. **Look for async issues** - missing awaits or unresolved promises
5. **Check DOM cleanup** - leftover DOM elements affecting subsequent tests
6. **Review recent changes** - what code changes might have affected this test?

## Quality Gates

Before creating PR, verify:
- [ ] All tests pass
- [ ] Coverage thresholds met
- [ ] No linting errors (`bun run quality:fix`)
- [ ] Component works in isolation

## Growth Strategy

**Start Simple** (Current):
- Basic unit/component tests
- Simple mocks
- Essential workflows

**Add Complexity** (As Needed):
- Performance testing
- Security validation  
- Cross-platform E2E
- Advanced mocking

This approach provides immediate practical value while allowing natural growth as the project matures.