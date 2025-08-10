# NodeSpace Testing Guide

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