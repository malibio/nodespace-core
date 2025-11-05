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

# Frontend tests (Happy-DOM - fast, recommended for 99% of tests)
bun run test              # Run all unit tests once
bun run test:unit         # Same as above (explicit)
bun run test:watch        # Watch mode for TDD

# Browser tests (Vitest Browser Mode - real Chromium for critical tests)
bun run test:browser      # Run browser integration tests
bun run test:browser:watch # Watch mode for browser tests

# Run all frontend tests (unit + browser)
bun run test:all          # Runs both Happy-DOM and browser tests

# Frontend tests (database mode - full SQLite integration)
bun run test:db           # Full integration tests
bun run test:db:watch     # Watch mode with database

# Frontend tests with coverage
bun run test:coverage

# All tests (backend + frontend + browser)
cargo test && bun run test:all
```

### Test Execution Modes

NodeSpace uses a **hybrid three-tier testing strategy** for optimal speed, reliability, and real browser testing:

**1. Happy-DOM Mode (Default - Recommended for 99% of tests)**
- **Speed**: 100x faster (~100-200ms per test file)
- **Database**: No SQLite persistence
- **DOM**: Happy-DOM simulated environment
- **Use Case**: Unit tests, service tests, component logic, TDD workflow
- **Command**: `bun run test` or `bun run test:watch`
- **Location**: `src/tests/**/*.test.ts` (excluding `browser/`)

**2. Vitest Browser Mode (Real Browser - For Critical Interactions)**
- **Speed**: Slower (~500ms-2s per test file)
- **Browser**: Real Chromium via Playwright
- **DOM**: Actual browser DOM with real focus/blur/event APIs
- **Use Case**: Focus management, real browser events, dropdown interactions, cross-node navigation
- **Command**: `bun run test:browser` or `bun run test:browser:watch`
- **Location**: `src/tests/browser/**/*.test.ts`
- **Setup**: Requires `bunx playwright install chromium` (one-time)

**3. Database Mode (Full Integration)**
- **Speed**: Slowest (~10-15s per test file)
- **Database**: Full SQLite persistence and validation
- **DOM**: Happy-DOM
- **Use Case**: Pre-merge validation, debugging database-specific issues
- **Command**: `bun run test:db` or `bun run test:db:watch`

**When to use which mode:**
- Use **Happy-DOM mode** (default) for 99% of tests - unit tests, service logic, component logic
- Use **Browser Mode** only when you need real focus/blur events or browser-specific DOM APIs that Happy-DOM cannot simulate
- Use **Database Mode** before merging critical changes or when debugging database-specific issues
- CI/CD pipelines run both **Happy-DOM** and **Browser Mode** tests

**Database cleanup behavior:**
- **In-memory mode**: No database files created, no cleanup needed
- **Database mode**: Each test creates a temporary SQLite database file
  - Automatically cleaned up in `afterEach` hooks
  - Test database files use pattern: `/tmp/nodespace-test-{testName}-{random}.db`
  - Cleanup is automatic and handled by `cleanupDatabaseIfNeeded()` utility
- **Mixed test runs**: Running `bun run test` followed by `bun run test:db` is safe
  - Each mode creates separate files (or no files for in-memory)
  - No interference between modes

## Test Execution Behavior

**Conditional Test Skipping:**
- Some integration tests require full database persistence and will automatically skip in in-memory mode
- These tests run when using `bun run test:db`
- Tests use `shouldUseDatabase()` utility to determine execution mode
- This allows fast development feedback while maintaining comprehensive integration testing

**Checking Test Status:**
```bash
# See current test results
bun run test              # Quick feedback (in-memory mode)
cargo test --workspace    # Rust backend tests

# For detailed investigation
bun run test:db          # Full integration validation
bun run test:coverage    # With coverage report
```

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

### 5. Browser Mode Testing (For Real Browser Behavior)

**When to Use Browser Mode:**
Use Vitest Browser Mode **only** when testing functionality that requires real browser APIs that Happy-DOM cannot simulate:

- ✅ **Focus/Blur Events**: Real `focus()` and `blur()` events that fire correctly
- ✅ **Edit Mode Transitions**: Click-to-edit workflows with real focus behavior
- ✅ **Dropdown Interactions**: Slash commands, @mentions with real keyboard navigation
- ✅ **Browser-Specific APIs**: `document.activeElement`, `selectionStart/End`, `getBoundingClientRect()`
- ✅ **Textarea Selection API**: Cursor positioning with real selection ranges
- ✅ **Viewport Dimensions**: Real `window.innerWidth/innerHeight` for positioning
- ❌ **NOT for**: Logic tests, service tests, state management (use Happy-DOM)

**Pragmatic Testing Approach:**
Browser tests should focus on **browser-specific APIs**, not full component integration. Testing DOM APIs directly is faster and more maintainable than rendering complex Svelte components with extensive context requirements.

#### Browser Test Examples

**Example 1: Focus Management**
```typescript
// src/tests/browser/focus-management.test.ts
import { describe, it, expect, beforeEach } from 'vitest';

describe('Focus Management - Browser Mode', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('handles focus transitions between textareas', () => {
    const textarea1 = document.createElement('textarea');
    const textarea2 = document.createElement('textarea');
    document.body.appendChild(textarea1);
    document.body.appendChild(textarea2);

    textarea1.focus();
    expect(document.activeElement).toBe(textarea1);

    textarea2.focus();
    expect(document.activeElement).toBe(textarea2);
  });

  it('fires focus/blur events correctly', () => {
    const textarea = document.createElement('textarea');
    document.body.appendChild(textarea);

    let focusCount = 0;
    let blurCount = 0;

    textarea.addEventListener('focus', () => focusCount++);
    textarea.addEventListener('blur', () => blurCount++);

    textarea.focus();
    expect(focusCount).toBe(1);

    const other = document.createElement('textarea');
    document.body.appendChild(other);
    other.focus();

    expect(blurCount).toBe(1);
  });
});
```

**Example 2: Dropdown Positioning with getBoundingClientRect**
```typescript
// src/tests/browser/dropdown-positioning.test.ts
describe('Dropdown Positioning - Browser Mode', () => {
  it('verifies getBoundingClientRect returns real dimensions', () => {
    const textarea = document.createElement('textarea');
    textarea.style.position = 'absolute';
    textarea.style.left = '100px';
    textarea.style.top = '100px';
    textarea.style.width = '300px';
    textarea.style.height = '100px';
    document.body.appendChild(textarea);

    const rect = textarea.getBoundingClientRect();

    // Happy-DOM returns all zeros, real browser returns actual values
    expect(rect.width).toBeGreaterThan(0);
    expect(rect.height).toBeGreaterThan(0);
    expect(rect.left).toBeGreaterThan(0);
    expect(rect.top).toBeGreaterThan(0);
  });

  it('detects viewport edges for smart positioning', () => {
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    expect(viewportWidth).toBeGreaterThan(0);
    expect(viewportHeight).toBeGreaterThan(0);

    // Simulate dropdown near right edge
    const cursorX = viewportWidth - 100;
    const dropdownWidth = 300;
    const wouldOverflow = cursorX + dropdownWidth > viewportWidth;

    expect(wouldOverflow).toBe(true);

    // Smart positioning would flip to left
    const adjustedX = cursorX - dropdownWidth;
    expect(adjustedX).toBeGreaterThan(0);
  });
});
```

**Example 3: Textarea Selection API**
```typescript
// src/tests/browser/cursor-positioning.test.ts
describe('Cursor Positioning - Browser Mode', () => {
  it('can detect cursor position for text splitting', () => {
    const textarea = document.createElement('textarea');
    textarea.value = 'Hello World';
    document.body.appendChild(textarea);

    textarea.focus();
    textarea.setSelectionRange(6, 6); // After "Hello "

    expect(textarea.selectionStart).toBe(6);
    expect(textarea.selectionEnd).toBe(6);

    const beforeCursor = textarea.value.substring(0, 6);
    const afterCursor = textarea.value.substring(6);

    expect(beforeCursor).toBe('Hello ');
    expect(afterCursor).toBe('World');
  });

  it('preserves cursor position after focus/blur cycle', () => {
    const textarea1 = document.createElement('textarea');
    textarea1.value = 'Test content';
    const textarea2 = document.createElement('textarea');
    document.body.appendChild(textarea1);
    document.body.appendChild(textarea2);

    textarea1.focus();
    textarea1.setSelectionRange(5, 5);

    // Blur and refocus
    textarea2.focus();
    textarea1.focus();

    // Chromium restores cursor position automatically
    expect(textarea1.selectionStart).toBe(5);
  });

  it('can insert content at cursor position', () => {
    const textarea = document.createElement('textarea');
    textarea.value = 'Before  After';
    document.body.appendChild(textarea);

    textarea.focus();
    textarea.setSelectionRange(7, 7);

    const cursorPos = textarea.selectionStart;
    const reference = '@[[Node Title]]';
    const before = textarea.value.substring(0, cursorPos);
    const after = textarea.value.substring(cursorPos);
    textarea.value = before + reference + after;

    expect(textarea.value).toBe('Before @[[Node Title]] After');

    // Move cursor after insertion
    const newCursorPos = cursorPos + reference.length;
    textarea.setSelectionRange(newCursorPos, newCursorPos);
    expect(textarea.selectionStart).toBe(newCursorPos);
  });
});
```

**Example 4: Keyboard Event Propagation**
```typescript
// src/tests/browser/keyboard-events.test.ts
describe('Keyboard Events - Browser Mode', () => {
  it('detects Enter key with preventDefault', () => {
    const textarea = document.createElement('textarea');
    document.body.appendChild(textarea);
    textarea.focus();

    let enterPressed = false;

    textarea.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        enterPressed = true;
        e.preventDefault();
      }
    });

    textarea.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        bubbles: true,
        cancelable: true
      })
    );

    expect(enterPressed).toBe(true);
  });

  it('detects arrow key navigation', () => {
    const textarea = document.createElement('textarea');
    document.body.appendChild(textarea);
    textarea.focus();

    let arrowDownCount = 0;

    textarea.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown') {
        arrowDownCount++;
        e.preventDefault();
      }
    });

    textarea.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })
    );

    expect(arrowDownCount).toBe(1);
  });
});
```

**Key Differences from Happy-DOM Tests:**

| Feature | Happy-DOM | Browser Mode |
|---------|-----------|--------------|
| Speed | ~100-200ms per file | ~500ms-2s per file |
| Setup | None | `bunx playwright install chromium` |
| DOM API | Simulated | Real browser (Chromium) |
| Focus/Blur | ❌ Doesn't work | ✅ Works correctly |
| getBoundingClientRect | ❌ Returns zeros | ✅ Returns real dimensions |
| Textarea Selection | ❌ Unreliable | ✅ Works correctly |
| Test Location | `src/tests/**/*.test.ts` | `src/tests/browser/**/*.test.ts` |
| Use Case | 99% of tests | Critical browser interactions |
| Run Command | `bun run test` | `bun run test:browser` |

**Best Practices for Browser Tests:**

1. **Keep them minimal**: Only test what Happy-DOM cannot
2. **Test APIs, not components**: Focus on browser APIs directly rather than complex component integration
3. **Clean up DOM**: Always use `beforeEach(() => document.body.innerHTML = '')`
4. **Direct DOM manipulation**: Create elements with `document.createElement()`, not component rendering
5. **Avoid framework complexity**: Don't render Svelte components unless absolutely necessary (requires extensive context setup)
6. **Synchronous when possible**: Prefer direct DOM manipulation over async interactions
7. **Focus on browser-specific behavior**: Real focus events, cursor positioning, viewport calculations
8. **Complement, don't duplicate**: Business logic stays in Happy-DOM unit tests

**When NOT to use Browser Mode:**
- ❌ Testing service methods (use Happy-DOM unit tests)
- ❌ Testing business logic (use Happy-DOM unit tests)
- ❌ Testing data transformations (use Happy-DOM unit tests)
- ❌ Testing event emissions (use Happy-DOM with spies)
- ❌ Testing full component rendering (too complex for browser mode)

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