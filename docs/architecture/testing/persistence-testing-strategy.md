# Persistence Testing Strategy

**Date:** 2025-11-12
**Context:** Addresses test coverage gaps that allowed persistence bugs to reach production

> **Note (Issue #614)**: This document references `beforeSiblingId` field which has been
> removed from the node model. Sibling ordering now uses fractional `order` field on
> `has_child` edges. The testing patterns still apply, but sibling ordering tests should
> use the new edge-based ordering API.

## Problem Statement

Despite ~2000 passing tests, the NodeSpace persistence system experiences repeated regression bugs in production. This document identifies **why tests don't catch these bugs** and proposes a comprehensive testing strategy.

---

## Root Causes of Test Gap

### 1. Tests Use Mocked Services

**Current Pattern:**

```typescript
// Tests mock tauriNodeService
vi.mock('$lib/services/tauri-node-service', () => ({
  tauriNodeService: {
    createNode: vi.fn().mockResolvedValue(undefined),
    updateNode: vi.fn().mockResolvedValue(undefined),
    deleteNode: vi.fn().mockResolvedValue(undefined)
  }
}));
```

**Why This Fails:**

- Mocks return success immediately (no timing, no race conditions)
- Real backend has validation logic tests don't exercise
- Database constraints (UNIQUE, FOREIGN KEY) not enforced
- Network delays and retries not tested

**What This Misses:**

- UNIQUE constraint violations (duplicate CREATE)
- FOREIGN KEY violations (invalid references)
- Race conditions (debounced vs immediate operations)
- Network failure scenarios
- Backend validation errors

---

### 2. Tests Don't Test Timing

**Missing Scenarios:**

```typescript
// This timing bug is NOT caught by tests:

// t=0ms:    User types "> " → content debounced (500ms)
// t=200ms:  Pattern detected → batch started (2000ms)
// t=500ms:  Debounced content fires → persists via old path
// t=2200ms: Batch commits → tries CREATE → UNIQUE violation
```

**Why:** Tests use `vi.advanceTimersByTime()` which skips to exact milliseconds without testing intermediate states

**What's Needed:** Integration tests that exercise real timing with concurrent operations

---

### 3. Tests Don't Test State Transitions

**Missing Coverage:**

```typescript
// Create node
node.persistenceState === 'pending' ✅ Tested

// Persist node
node.persistenceState === 'persisted' ✅ Tested

// Update persisted node
node.persistenceState changes from 'persisted' → 'pending' ❌ NOT TESTED

// Debounced persistence fires
System checks persistenceState === 'pending'
Thinks it's a new node
Attempts CREATE instead of UPDATE → UNIQUE violation ❌ NOT TESTED
```

**Why:** Tests focus on happy path, not state transitions during updates

---

### 4. Tests Don't Test Ephemeral → Pending → Persisted Flow

**Most Complex Path:**

```typescript
// 1. Create ephemeral placeholder
node.persistenceState === 'ephemeral'

// 2. Structural change references placeholder
update.beforeSiblingId = placeholderId
// Should be deferred, waiting for placeholder

// 3. User adds content to placeholder
placeholder transitions: 'ephemeral' → 'pending'

// 4. Deferred updates should process
// But do they? Tests don't verify!
```

**Why:** Tests create placeholders but don't test full lifecycle with dependencies

---

### 5. Tests Don't Test Concurrent Operations

**Missing Scenario:**

```typescript
// User indents node (structural change - immediate)
// While content change is pending (debounced)
// Both operations reference same node
// Order matters!
```

**Why:** Tests run sequentially (by design - singleton SharedNodeStore)

**What's Needed:** Integration tests that exercise concurrent operations with real timing

---

### 6. Happy-DOM Limitations

**What Happy-DOM Doesn't Emulate:**

- Real browser timing (setTimeout/Promise interactions)
- Network request timing and failures
- IndexedDB transactions and locks
- Browser-specific optimizations
- Real focus/blur events (for focus management)

**Current Usage:** 728 tests use Happy-DOM

**When to Use Browser Mode:**
- Focus management tests
- Real DOM event testing
- Timing-sensitive integration tests

---

## Proposed Testing Strategy

### Test Pyramid

```
         ┌─────────────┐
         │   E2E (5%)  │  Playwright, real browser, real backend
         └─────────────┘
              ↑
         ┌─────────────┐
         │ Integration │  Vitest browser mode, real timing, mock backend
         │   (15%)     │  Test state transitions, race conditions
         └─────────────┘
              ↑
         ┌─────────────┐
         │   Unit      │  Happy-DOM, mocked services, fast
         │   (80%)     │  Test business logic, pure functions
         └─────────────┘
```

### Layer 1: Unit Tests (80% - Keep Current Approach)

**What to Test:**

- Pure functions (content processing, validation)
- Business logic (node operations without persistence)
- State management (in-memory updates)
- UI components (rendering, props)

**Tools:**
- Vitest with Happy-DOM
- Mocked services
- Fast (<10ms per test)

**Coverage:**
- Existing 728 tests cover this well
- Keep current approach

### Layer 2: Integration Tests (15% - NEW)

**What to Test:**

- Persistence state transitions
- Timing and race conditions
- Deferred update queue
- Ephemeral → pending → persisted flows
- Concurrent operations
- Backend validation logic

**Tools:**
- Vitest browser mode (Chromium via Playwright)
- Real timing (not mocked)
- Mock backend (but with validation logic)
- Moderate speed (50-200ms per test)

**New Tests Needed:**

#### Persistence State Transitions

```typescript
describe('Persistence State Transitions', () => {
  test('updating persisted node changes state to pending', async () => {
    // Create and persist node
    const nodeId = service.createNode(parentId, 'Initial content');
    await waitForPersistence(nodeId);

    const node = sharedNodeStore.getNode(nodeId);
    expect(node?.persistenceState).toBe('persisted');
    expect(node?.databaseId).toBeDefined();

    // Update node
    service.updateNodeContent(nodeId, 'Updated content');

    // State should change to pending
    const updatedNode = sharedNodeStore.getNode(nodeId);
    expect(updatedNode?.persistenceState).toBe('pending');

    // But databaseId should NOT change
    expect(updatedNode?.databaseId).toBe(node?.databaseId);

    // Wait for persistence
    await waitForPersistence(nodeId);

    // Should use UPDATE, not CREATE
    const finalNode = sharedNodeStore.getNode(nodeId);
    expect(finalNode?.persistenceState).toBe('persisted');
    expect(finalNode?.databaseId).toBe(node?.databaseId);

    // Verify UPDATE was called, not CREATE
    expect(mockBackend.updateNode).toHaveBeenCalledWith(
      nodeId,
      expect.any(Number), // version
      expect.objectContaining({ content: 'Updated content' })
    );
    expect(mockBackend.createNode).not.toHaveBeenCalled();
  });
});
```

#### Timing and Race Conditions

```typescript
describe('Timing and Race Conditions', () => {
  test('pattern conversion race condition', async () => {
    // Real timing bug from Issue #450

    const nodeId = service.createNode(parentId, '');

    // t=0ms: Type "> " (triggers debounce)
    service.updateNodeContent(nodeId, '> ');
    expect(getDebouncedOperations(nodeId)).toBeTruthy();

    // t=200ms: Pattern detected (batch started)
    await waitForMs(200);
    // Pattern detection would normally start batch here
    // (requires integration with pattern detection system)

    // t=500ms: Debounced content fires
    await waitForMs(300); // Total 500ms
    // First persistence should fire here

    // t=2200ms: Batch commits
    await waitForMs(1700); // Total 2200ms
    // Batch persistence should fire here

    // Verify no UNIQUE constraint violation
    expect(mockBackend.createNode).toHaveBeenCalledTimes(1);
    expect(mockBackend.errors).toHaveLength(0);
  });

  test('rapid indent/outdent coalesces to single update', async () => {
    const nodeId = service.createNode(parentId, 'Test');
    await waitForPersistence(nodeId);

    // Rapid operations within 500ms
    service.indentNode(nodeId);  // t=0ms
    await waitForMs(100);
    service.outdentNode(nodeId); // t=100ms

    // Wait for debounce
    await waitForMs(500);

    // Should only fire ONE update with final state
    expect(mockBackend.updateNode).toHaveBeenCalledTimes(1);
  });
});
```

#### Ephemeral Node Lifecycle

```typescript
describe('Ephemeral Node Lifecycle', () => {
  test('deferred update waits for ephemeral node', async () => {
    // Create placeholder between A and B
    const nodeA = service.createNode(parentId, 'A');
    await waitForPersistence(nodeA);

    const placeholder = service.createPlaceholderNode(nodeA);
    // Placeholder is ephemeral, not persisted
    expect(sharedNodeStore.getNode(placeholder)?.persistenceState).toBe('ephemeral');

    const nodeB = service.createNode(placeholder, 'B');
    await waitForPersistence(nodeB);

    // Outdent placeholder while empty
    service.outdentNode(placeholder);

    // B's sibling chain update should be deferred
    const deferredOps = getDeferredUpdatesFor(placeholder);
    expect(deferredOps).toHaveLength(1);
    expect(deferredOps[0].nodeId).toBe(nodeB);

    // Add content to placeholder
    service.updateNodeContent(placeholder, 'C');

    // Wait for placeholder to persist
    await waitForPersistence(placeholder);

    // Deferred updates should process
    await waitForMs(100); // Give time for queue processing
    expect(getDeferredUpdatesFor(placeholder)).toHaveLength(0);

    // B should now have correct sibling reference
    const nodeBFinal = sharedNodeStore.getNode(nodeB);
    expect(nodeBFinal?.beforeSiblingId).not.toBe(placeholder);

    // Verify no errors
    expect(mockBackend.errors).toHaveLength(0);
  });
});
```

#### Concurrent Operations

```typescript
describe('Concurrent Operations', () => {
  test('structural change while content change pending', async () => {
    const nodeId = service.createNode(parentId, 'Initial');
    await waitForPersistence(nodeId);

    // Content change (debounced - 500ms)
    service.updateNodeContent(nodeId, 'Updated content');

    // Structural change (immediate) while content pending
    await waitForMs(100); // Content still pending
    service.indentNode(nodeId);

    // Both operations should complete successfully
    await waitForMs(600); // Wait for both

    // Verify both updates persisted
    const finalNode = sharedNodeStore.getNode(nodeId);
    expect(finalNode?.content).toBe('Updated content');
    expect(finalNode?.parentId).not.toBe(parentId); // Indented

    // Verify correct operation order
    const calls = mockBackend.updateNode.mock.calls;
    expect(calls.length).toBeGreaterThanOrEqual(1);
  });
});
```

### Layer 3: E2E Tests (5% - NEW)

**What to Test:**

- Full user workflows
- Real browser behavior
- Real backend (test database)
- Real timing and network
- Error recovery

**Tools:**
- Playwright
- Real backend with test database
- Slow (1-5 seconds per test)

**New Tests Needed:**

```typescript
// E2E: Full placeholder lifecycle
test('user creates, edits, and outdents placeholder', async ({ page }) => {
  // Navigate to empty viewer
  await page.goto('/date/2025-11-12');

  // Type in placeholder
  await page.locator('[data-testid="node-editor"]').first().fill('Test content');
  await page.keyboard.press('Enter');

  // Create another node
  await page.locator('[data-testid="node-editor"]').last().fill('Second node');

  // Indent second node
  await page.keyboard.press('Tab');

  // Wait for persistence
  await page.waitForTimeout(1000);

  // Reload page
  await page.reload();

  // Verify structure persisted correctly
  const nodes = await page.locator('[data-testid="node-editor"]').all();
  expect(nodes.length).toBe(2);

  // Verify no console errors
  const errors = await page.evaluate(() => window.testErrors || []);
  expect(errors).toHaveLength(0);
});
```

---

## Mock Backend with Validation Logic

**Problem:** Current mocks return success immediately

**Solution:** Mock backend that mimics real validation

```typescript
class MockBackend {
  private nodes = new Map<string, Node>();
  private errors: Error[] = [];

  async createNode(node: Node): Promise<void> {
    // Simulate UNIQUE constraint
    if (this.nodes.has(node.id)) {
      const error = new Error(`UNIQUE constraint failed: node ${node.id} already exists`);
      this.errors.push(error);
      throw error;
    }

    // Simulate FOREIGN KEY constraint
    if (node.parentId && !this.nodes.has(node.parentId)) {
      const error = new Error(`FOREIGN KEY constraint failed: parent ${node.parentId} not found`);
      this.errors.push(error);
      throw error;
    }

    if (node.beforeSiblingId && !this.nodes.has(node.beforeSiblingId)) {
      const error = new Error(`FOREIGN KEY constraint failed: sibling ${node.beforeSiblingId} not found`);
      this.errors.push(error);
      throw error;
    }

    // Add node
    this.nodes.set(node.id, node);

    // Simulate network delay (realistic timing)
    await new Promise(resolve => setTimeout(resolve, 10));
  }

  async updateNode(nodeId: string, version: number, changes: Partial<Node>): Promise<void> {
    const node = this.nodes.get(nodeId);

    if (!node) {
      const error = new Error(`NodeNotFound: node ${nodeId} does not exist`);
      this.errors.push(error);
      throw error;
    }

    // Simulate optimistic concurrency control
    if (node.version !== version) {
      const error = new Error(`Version mismatch: expected ${version}, got ${node.version}`);
      this.errors.push(error);
      throw error;
    }

    // Apply changes
    Object.assign(node, changes, { version: version + 1 });
    this.nodes.set(nodeId, node);

    await new Promise(resolve => setTimeout(resolve, 10));
  }

  getErrors(): Error[] {
    return this.errors;
  }

  reset(): void {
    this.nodes.clear();
    this.errors = [];
  }
}
```

---

## Test Utilities

### Timing Helpers

```typescript
export async function waitForMs(ms: number): Promise<void> {
  await new Promise(resolve => setTimeout(resolve, ms));
}

export async function waitForPersistence(nodeId: string, timeoutMs = 2000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const node = sharedNodeStore.getNode(nodeId);
    if (node?.persistenceState === 'persisted') {
      return;
    }
    await waitForMs(50);
  }
  throw new Error(`Timeout waiting for ${nodeId} to persist`);
}
```

### State Inspection Helpers

```typescript
export function getDebouncedOperations(nodeId: string) {
  // Access internal state for testing
  return (sharedNodeStore as any).activeBatches.get(nodeId);
}

export function getDeferredUpdatesFor(nodeId: string) {
  return (sharedNodeStore as any).deferredUpdates.get(nodeId) || [];
}
```

---

## Vitest Configuration

### Current Config Review

**File:** `packages/desktop-app/vitest.config.ts`

**Current:**
- Environment: Happy-DOM (fast)
- Exclude: `src/tests/browser/**`
- Sequential: Yes (singleton SharedNodeStore)

**Issues:**
- No browser mode tests (need for integration layer)
- No real timing tests
- No concurrent operation tests

### Recommended Updates

```typescript
export default defineConfig({
  test: {
    // Unit tests (fast)
    include: ['src/tests/unit/**/*.{test,spec}.{js,ts}'],
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['src/tests/setup.ts'],
    sequence: { concurrent: false },

    // Coverage
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html'],
      thresholds: {
        lines: 80,
        branches: 75,
        functions: 80
      }
    }
  }
});

// Separate config for integration tests
export const integrationConfig = defineConfig({
  test: {
    include: ['src/tests/integration/**/*.{test,spec}.{js,ts}'],
    environment: 'browser', // Use Vitest browser mode
    browser: {
      enabled: true,
      name: 'chromium',
      provider: 'playwright'
    },
    setupFiles: ['src/tests/integration-setup.ts'],
    sequence: { concurrent: false },
    testTimeout: 10000 // Longer timeout for real timing
  }
});
```

**Run commands:**

```json
{
  "scripts": {
    "test": "vitest",
    "test:unit": "vitest --config vitest.config.ts",
    "test:integration": "vitest --config vitest.integration.config.ts",
    "test:browser": "vitest --config vitest.integration.config.ts",
    "test:all": "npm run test:unit && npm run test:integration"
  }
}
```

---

## Implementation Plan

### Phase 1: Add Mock Backend

**Duration:** 2-3 hours

1. Create `MockBackend` class with validation logic
2. Use in existing tests
3. Verify tests still pass

### Phase 2: Add Integration Tests

**Duration:** 6-8 hours

1. Set up Vitest browser mode config
2. Create integration test utilities
3. Write persistence state transition tests
4. Write timing/race condition tests
5. Write ephemeral lifecycle tests
6. Write concurrent operation tests

### Phase 3: Add E2E Tests

**Duration:** 4-6 hours

1. Set up Playwright
2. Create test database helper
3. Write user workflow tests
4. Write error recovery tests

### Phase 4: Update CI/CD

**Duration:** 2-3 hours

1. Add integration tests to CI pipeline
2. Add E2E tests to nightly builds
3. Set up test result reporting
4. Add coverage thresholds

---

## Success Metrics

### Coverage Targets

- Unit test coverage: 80%+ (current)
- Integration test coverage: 60%+ (NEW)
- E2E test coverage: 40%+ (NEW)

### Bug Prevention

- Zero persistence state bugs after refactoring
- Zero UNIQUE constraint violations
- Zero race condition bugs
- Zero deferred update bugs

### Test Performance

- Unit tests: <30 seconds total
- Integration tests: <2 minutes total
- E2E tests: <5 minutes total

---

## References

- `persistence-system-review.md` - Root cause analysis
- `refactoring-plan.md` - Proposed simplification
- Issue #450 - Example persistence bug
- Vitest documentation: https://vitest.dev/
- Playwright documentation: https://playwright.dev/
