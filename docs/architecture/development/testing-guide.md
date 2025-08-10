# NodeSpace Testing Guide

**Comprehensive guide for implementing high-quality testing in the NodeSpace project.**

## Quick Start

### Running Tests

```bash
# Rust backend tests
cargo test

# Frontend unit/component tests  
bun run test

# Frontend tests with coverage
bun run test:coverage

# E2E tests
bun run test:e2e

# All tests
cargo test && bun run test && bun run test:e2e
```

### Test Structure

```
nodespace-core/
├── nodespace-app/src-tauri/src/         # Rust tests (inline with code)
├── nodespace-app/src/tests/
│   ├── fixtures/                        # Test data and utilities
│   ├── mocks/                           # API mocks (MSW)
│   ├── utils/                           # Testing utilities
│   └── example/                         # Example test patterns
├── nodespace-app/tests/e2e/             # Playwright E2E tests
└── nodespace-app/tests/performance/     # Performance tests
```

## Rust Backend Testing

### Unit Testing Patterns

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[tokio::test]
    async fn test_create_node_success() {
        // Arrange
        let mock_store = Arc::new(MockDataStore::new());
        let manager = NodeManager::new(mock_store);
        let node = NodeData::new(NodeType::Text, "Test content".to_string());
        
        // Act
        let result = manager.create_node(node).await;
        
        // Assert
        assert!(result.is_ok());
        let created_node = result.unwrap();
        assert_eq!(created_node.content, "Test content");
    }
    
    #[test]
    fn test_validation_error() {
        let mut node = NodeData::new(NodeType::Text, "".to_string());
        
        let result = node.validate();
        
        assert!(result.is_err());
        match result.unwrap_err() {
            NodeError::ValidationError(msg) => {
                assert!(msg.contains("empty"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }
}
```

### Integration Testing Patterns

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_full_node_lifecycle() {
        // Setup
        let storage = Arc::new(MockDataStore::new());
        let manager = NodeManager::new(storage);
        
        // Create
        let node = NodeData::new(NodeType::Text, "Integration test".to_string());
        let created = manager.create_node(node).await.unwrap();
        
        // Read
        let retrieved = manager.get_node(&created.id).await.unwrap();
        assert!(retrieved.is_some());
        
        // Update
        let update_request = NodeUpdateRequest {
            id: created.id.clone(),
            content: Some("Updated content".to_string()),
            metadata: None,
        };
        let updated = manager.update_node(update_request).await.unwrap();
        assert_eq!(updated.content, "Updated content");
        
        // Delete
        let deleted = manager.delete_node(&created.id).await.unwrap();
        assert!(deleted);
        
        // Verify deletion
        let after_delete = manager.get_node(&created.id).await.unwrap();
        assert!(after_delete.is_none());
    }
}
```

### Performance Testing Patterns

```rust
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;
    
    #[tokio::test]
    async fn test_bulk_operations_performance() {
        let storage = Arc::new(MockDataStore::new_fast()); // No delays for perf tests
        let manager = NodeManager::new(storage);
        
        let start = Instant::now();
        
        // Create 1000 nodes
        for i in 0..1000 {
            let node = NodeData::new(NodeType::Text, format!("Node {}", i));
            manager.create_node(node).await.unwrap();
        }
        
        let creation_time = start.elapsed();
        assert!(creation_time.as_millis() < 5000); // Should be under 5 seconds
        
        // Search performance
        let search_start = Instant::now();
        let results = manager.search_nodes("Node").await.unwrap();
        let search_time = search_start.elapsed();
        
        assert_eq!(results.len(), 1000);
        assert!(search_time.as_millis() < 200); // Should be under 200ms
    }
}
```

## Frontend Testing

### Component Testing Patterns

```typescript
// Component test example
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { createTestContext } from '../utils/testUtils';
import TextNode from '$lib/components/TextNode.svelte';

describe('TextNode Component', () => {
  it('renders content correctly', () => {
    render(TextNode, {
      props: {
        content: 'Test content',
        editable: true
      }
    });
    
    expect(screen.getByText('Test content')).toBeInTheDocument();
  });
  
  it('enters edit mode on click', async () => {
    const { user } = renderWithContext(TextNode, {
      content: 'Click to edit',
      editable: true
    });
    
    const content = screen.getByText('Click to edit');
    await user.click(content);
    
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });
  
  it('saves content on blur', async () => {
    const mockSave = vi.fn();
    const { user } = renderWithContext(TextNode, {
      content: 'Original',
      editable: true,
      onSave: mockSave
    });
    
    // Enter edit mode
    await user.click(screen.getByText('Original'));
    
    // Modify content
    const textbox = screen.getByRole('textbox');
    await user.clear(textbox);
    await user.type(textbox, 'Modified');
    
    // Exit edit mode (blur)
    await user.tab();
    
    expect(mockSave).toHaveBeenCalledWith('Modified');
  });
});
```

### API Integration Testing

```typescript
describe('Node API Integration', () => {
  let testContext: ReturnType<typeof createTestContext>;
  
  beforeEach(() => {
    testContext = createTestContext();
  });
  
  it('creates node via API', async () => {
    const testNodes: NodeData[] = [];
    testContext.setupNodeAPI(testNodes);
    
    const newNode = await testContext.mockAPI.invoke('create_node', {
      content: 'New node',
      node_type: NodeType.Text
    });
    
    expect(newNode).toMatchObject({
      content: 'New node',
      node_type: NodeType.Text
    });
    expect(newNode.id).toBeTruthy();
    expect(testNodes).toHaveLength(1);
  });
  
  it('handles API errors gracefully', async () => {
    testContext.mockAPI.invoke.mockRejectedValueOnce(new Error('API Error'));
    
    await expect(
      testContext.mockAPI.invoke('create_node', { content: 'Test' })
    ).rejects.toThrow('API Error');
  });
});
```

### Store/State Testing

```typescript
describe('Node Store', () => {
  let store: NodeStore;
  
  beforeEach(() => {
    store = new NodeStore();
  });
  
  it('adds nodes to store', () => {
    const node = createMockTextNode('Test');
    store.addNode(node);
    
    expect(store.getNodes()).toContain(node);
    expect(store.getNodeCount()).toBe(1);
  });
  
  it('filters nodes by type', () => {
    store.addNode(createMockTextNode('Text'));
    store.addNode(createMockTaskNode('Task'));
    
    const textNodes = store.getNodesByType(NodeType.Text);
    expect(textNodes).toHaveLength(1);
    expect(textNodes[0].content).toBe('Text');
  });
});
```

## E2E Testing

### Basic App Testing

```typescript
// tests/e2e/app-basic.test.ts
import { test, expect } from '@playwright/test';

test.describe('App Launch', () => {
  test('loads successfully', async ({ page }) => {
    await page.goto('/');
    await expect(page).toHaveTitle(/NodeSpace/);
    
    const app = page.locator('#app');
    await expect(app).toBeVisible();
  });
});
```

### User Workflow Testing

```typescript
test.describe('Node Management Workflow', () => {
  test('creates and edits a node', async ({ page }) => {
    await page.goto('/');
    
    // Create new node
    await page.click('[data-testid="new-node"]');
    await page.fill('textarea', 'My new node');
    await page.click('[data-testid="save"]');
    
    // Verify creation
    await expect(page.locator('text=My new node')).toBeVisible();
    
    // Edit node
    await page.click('text=My new node');
    await page.fill('textarea', 'Updated node content');
    await page.press('textarea', 'Escape');
    
    // Verify update
    await expect(page.locator('text=Updated node content')).toBeVisible();
  });
});
```

### Performance Testing

```typescript
test.describe('Performance', () => {
  test('loads within acceptable time', async ({ page }) => {
    const start = Date.now();
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    const loadTime = Date.now() - start;
    
    expect(loadTime).toBeLessThan(3000); // 3 seconds
  });
});
```

## Test Data Management

### Creating Realistic Test Data

```typescript
// fixtures/mockData.ts
export function createRealisticNodes(): NodeData[] {
  return [
    createMockTextNode('Meeting notes from yesterday\'s standup'),
    createMockTaskNode('Fix the node rendering bug', false),
    createMockTextNode('Project requirements document draft'),
    createMockTaskNode('Review pull request #42', true),
    createMockAIChatNode('Help brainstorm marketing ideas'),
  ];
}

export function createTestScenario(name: string): NodeData[] {
  switch (name) {
    case 'empty':
      return [];
    case 'single-node':
      return [createMockTextNode('Single test node')];
    case 'mixed-types':
      return [
        createMockTextNode('Text content'),
        createMockTaskNode('Task content'),
        createMockAIChatNode('Chat content')
      ];
    default:
      return createRealisticNodes();
  }
}
```

### Test Database Setup

```typescript
// tests/fixtures/testSetup.ts
export async function setupTestDatabase() {
  // Create temporary test database
  // Seed with test data
  // Return cleanup function
}

export async function cleanupTestDatabase() {
  // Remove test data
  // Close connections
}
```

## Testing Best Practices

### Test Organization

```typescript
describe('Feature Name', () => {
  describe('Happy Path', () => {
    it('should work correctly with valid input', () => {
      // Test successful scenarios
    });
  });
  
  describe('Edge Cases', () => {
    it('should handle empty input', () => {
      // Test edge cases
    });
    
    it('should handle maximum input length', () => {
      // Test boundaries
    });
  });
  
  describe('Error Handling', () => {
    it('should handle network errors gracefully', () => {
      // Test error scenarios
    });
  });
});
```

### Async Testing

```typescript
it('handles async operations correctly', async () => {
  const promise = asyncOperation();
  
  // Test loading state
  expect(isLoading).toBe(true);
  
  // Wait for completion
  const result = await promise;
  
  // Test final state
  expect(isLoading).toBe(false);
  expect(result).toBeTruthy();
});
```

### Mock Management

```typescript
describe('API Integration', () => {
  let mockAPI: MockedFunction<any>;
  
  beforeEach(() => {
    mockAPI = vi.fn();
    // Setup default behavior
    mockAPI.mockResolvedValue({ success: true });
  });
  
  afterEach(() => {
    mockAPI.mockReset();
  });
  
  it('calls API with correct parameters', async () => {
    await service.createNode({ content: 'test' });
    
    expect(mockAPI).toHaveBeenCalledWith('create_node', {
      content: 'test'
    });
  });
});
```

## Coverage Requirements

### Minimum Coverage Thresholds

- **Rust Backend**: 90% line coverage
- **Frontend Components**: 85% statement coverage
- **Integration Tests**: 80% workflow coverage
- **E2E Tests**: 70% user journey coverage

### Measuring Coverage

```bash
# Rust coverage
cargo tarpaulin --out Html --output-dir coverage/rust

# Frontend coverage
bun run test:coverage

# View coverage reports
open coverage/rust/tarpaulin-report.html
open coverage/index.html
```

## Continuous Integration

### GitHub Actions Integration

```yaml
name: Test Suite
on: [push, pull_request]

jobs:
  rust-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all-features
      
  frontend-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v1
      - run: bun install
      - run: bun run test:coverage
      
  e2e-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v1
      - run: bun install
      - run: bunx playwright install
      - run: bun run test:e2e
```

## Debugging Tests

### Debug Failing Tests

```bash
# Run specific test file
bun run test TextNode.test.ts

# Run with debug output
bun run test --reporter=verbose

# Run single test
bun run test -t "should render correctly"

# Debug mode
bun run test --inspect-brk
```

### Test Debugging Tools

```typescript
// Add debug output
it('debugs component state', () => {
  const { debug } = render(Component);
  debug(); // Prints DOM to console
  
  // Custom debug info
  console.log('Component state:', component.state);
});

// Screenshot on failure (E2E)
test('visual test', async ({ page }) => {
  await page.screenshot({ path: 'debug-screenshot.png' });
});
```

## Common Testing Patterns

### Testing Custom Hooks

```typescript
import { renderHook, act } from '@testing-library/svelte';

it('manages state correctly', () => {
  const { result } = renderHook(() => useNodeManager());
  
  act(() => {
    result.current.addNode(mockNode);
  });
  
  expect(result.current.nodes).toHaveLength(1);
});
```

### Testing Event Handlers

```typescript
it('handles click events', async () => {
  const handleClick = vi.fn();
  const { user } = renderWithContext(Button, {
    onClick: handleClick
  });
  
  await user.click(screen.getByRole('button'));
  
  expect(handleClick).toHaveBeenCalledOnce();
});
```

### Testing Forms

```typescript
it('validates form input', async () => {
  const { user } = renderWithContext(NodeForm);
  
  // Submit empty form
  await user.click(screen.getByRole('button', { name: /submit/i }));
  
  // Check validation errors
  expect(screen.getByText('Content is required')).toBeVisible();
  
  // Fill valid data
  await user.type(screen.getByRole('textbox'), 'Valid content');
  await user.click(screen.getByRole('button', { name: /submit/i }));
  
  // Verify submission
  expect(screen.queryByText('Content is required')).not.toBeInTheDocument();
});
```

This testing guide provides comprehensive patterns and examples for maintaining high-quality testing throughout the NodeSpace project. Follow these patterns for consistent, reliable test coverage across all components and features.