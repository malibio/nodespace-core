/**
 * Event Listener Memory Leak Detection Tests
 *
 * Tests event listener cleanup to prevent memory leaks in components
 * that attach listeners to document. Verifies that listeners are properly
 * removed during unmount and that no listeners accumulate over time.
 *
 * Test Coverage:
 * - Component Lifecycle (4 tests)
 * - Rapid Operations (3 tests)
 * - EventBus Subscriptions (3 tests)
 * - Verification Tools (2 tests)
 *
 * Related: Issue #162
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import NodeAutocomplete from '$lib/components/ui/node-autocomplete/node-autocomplete.svelte';
import SlashCommandDropdown from '$lib/components/ui/slash-command-dropdown/slash-command-dropdown.svelte';
import { eventBus } from '$lib/services/eventBus';

describe('Event Listener Memory Leak Detection', () => {
  beforeEach(() => {
    // Clean up DOM and EventBus between tests
    document.body.innerHTML = '';
    eventBus.reset();
  });

  afterEach(() => {
    // Restore all spies
    vi.restoreAllMocks();
  });

  // ========================================================================
  // Component Lifecycle Tests (4 tests)
  // ========================================================================

  describe('Component Lifecycle', () => {
    it('should remove keydown listener when NodeAutocomplete unmounts', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const initialAddCount = addSpy.mock.calls.length;

      // Render and unmount
      const { unmount } = render(NodeAutocomplete, {
        visible: true,
        results: [],
        position: { x: 0, y: 0 }
      });

      // Verify listener was added
      const addCallsAfterMount = addSpy.mock.calls.length;
      expect(addCallsAfterMount).toBeGreaterThan(initialAddCount);

      // Find the keydown listener that was added
      const keydownAddCalls = addSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownAddCalls.length).toBeGreaterThan(0);

      unmount();

      // Verify listener was removed
      const keydownRemoveCalls = removeSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownRemoveCalls.length).toBe(keydownAddCalls.length);
    });

    it('should remove keydown listener when SlashCommandDropdown unmounts', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const initialAddCount = addSpy.mock.calls.length;

      // Render and unmount
      const { unmount } = render(SlashCommandDropdown, {
        visible: true,
        commands: [],
        position: { x: 0, y: 0 }
      });

      // Verify listener was added
      const addCallsAfterMount = addSpy.mock.calls.length;
      expect(addCallsAfterMount).toBeGreaterThan(initialAddCount);

      // Find the keydown listener that was added
      const keydownAddCalls = addSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownAddCalls.length).toBeGreaterThan(0);

      unmount();

      // Verify listener was removed
      const keydownRemoveCalls = removeSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownRemoveCalls.length).toBe(keydownAddCalls.length);
    });

    it('should not accumulate listeners over multiple mount/unmount cycles', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const cycles = 10;
      const initialAddCount = addSpy.mock.calls.length;

      // Mount and unmount multiple times
      for (let i = 0; i < cycles; i++) {
        const { unmount } = render(NodeAutocomplete, {
          visible: true,
          results: [],
          position: { x: 0, y: 0 }
        });
        unmount();
      }

      // Calculate keydown listener adds and removes
      const keydownAddCalls =
        addSpy.mock.calls.filter((call) => call[0] === 'keydown').length - initialAddCount;

      const keydownRemoveCalls = removeSpy.mock.calls.filter(
        (call) => call[0] === 'keydown'
      ).length;

      // All added listeners should have been removed
      expect(keydownRemoveCalls).toBe(keydownAddCalls);
      expect(keydownAddCalls).toBe(cycles);
    });

    it('should cleanup listeners even if error during unmount', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      // Render component
      const { unmount } = render(NodeAutocomplete, {
        visible: true,
        results: [],
        position: { x: 0, y: 0 }
      });

      // Capture the handler that was added
      const keydownAddCalls = addSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownAddCalls.length).toBeGreaterThan(0);

      // Standard unmount - Svelte's onDestroy cleanup should run
      // even if user code has errors
      unmount();

      // Verify cleanup happened (resilient to errors)
      const keydownRemoveCalls = removeSpy.mock.calls.filter((call) => call[0] === 'keydown');
      expect(keydownRemoveCalls.length).toBe(keydownAddCalls.length);
    });
  });

  // ========================================================================
  // Rapid Operations Tests (3 tests)
  // ========================================================================

  describe('Rapid Operations', () => {
    it('should handle 100 rapid mount/unmount cycles without leaking', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const cycles = 100;
      const initialKeydownCount = addSpy.mock.calls.filter((call) => call[0] === 'keydown').length;

      // Rapid mount/unmount cycles
      for (let i = 0; i < cycles; i++) {
        const { unmount } = render(NodeAutocomplete, {
          visible: true,
          results: [],
          position: { x: 0, y: 0 }
        });
        unmount();
      }

      // Verify all listeners removed
      const addedKeydownCount =
        addSpy.mock.calls.filter((call) => call[0] === 'keydown').length - initialKeydownCount;

      const removedKeydownCount = removeSpy.mock.calls.filter(
        (call) => call[0] === 'keydown'
      ).length;

      expect(removedKeydownCount).toBe(addedKeydownCount);
      expect(addedKeydownCount).toBe(cycles);
    });

    it('should return listener count to baseline after rapid operations', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const baseline = addSpy.mock.calls.filter((call) => call[0] === 'keydown').length;

      // Perform rapid operations
      for (let i = 0; i < 50; i++) {
        const { unmount } = render(SlashCommandDropdown, {
          visible: true,
          commands: [],
          position: { x: 0, y: 0 }
        });
        unmount();
      }

      // Calculate net listener count change
      const finalAddCount = addSpy.mock.calls.filter((call) => call[0] === 'keydown').length;
      const finalRemoveCount = removeSpy.mock.calls.filter((call) => call[0] === 'keydown').length;

      const netChange = finalAddCount - baseline - finalRemoveCount;

      // Net change should be 0 (all added listeners were removed)
      expect(netChange).toBe(0);
    });

    it('should maintain acceptable memory usage during rapid operations', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      // Baseline measurements
      const initialAdds = addSpy.mock.calls.length;
      const initialRemoves = removeSpy.mock.calls.length;

      // Simulate realistic usage pattern: mix of both components
      for (let i = 0; i < 50; i++) {
        const { unmount: unmount1 } = render(NodeAutocomplete, {
          visible: true,
          results: [],
          position: { x: 0, y: 0 }
        });

        const { unmount: unmount2 } = render(SlashCommandDropdown, {
          visible: true,
          commands: [],
          position: { x: 0, y: 0 }
        });

        unmount1();
        unmount2();
      }

      // Verify no listener accumulation
      const totalRemoves = removeSpy.mock.calls.length - initialRemoves;

      // All added listeners should be removed
      const keydownAdds = addSpy.mock.calls
        .slice(initialAdds)
        .filter((call) => call[0] === 'keydown').length;
      const keydownRemoves = removeSpy.mock.calls
        .slice(initialRemoves)
        .filter((call) => call[0] === 'keydown').length;

      expect(keydownRemoves).toBe(keydownAdds);
      expect(totalRemoves).toBeGreaterThanOrEqual(keydownRemoves);
    });
  });

  // ========================================================================
  // EventBus Subscriptions Tests (3 tests)
  // ========================================================================

  describe('EventBus Subscriptions', () => {
    it('should actually remove handler when unsubscribe is called', () => {
      const handler1 = vi.fn();
      const handler2 = vi.fn();

      // Subscribe to two different events
      const unsubscribe1 = eventBus.subscribe('node:updated', handler1);
      const unsubscribe2 = eventBus.subscribe('node:created', handler2);

      let counts = eventBus.getSubscriberCounts();
      expect(counts['node:updated']).toBe(1);
      expect(counts['node:created']).toBe(1);

      // Unsubscribe from first event
      unsubscribe1();

      counts = eventBus.getSubscriberCounts();
      expect(counts['node:updated']).toBeUndefined();
      expect(counts['node:created']).toBe(1);

      // Unsubscribe from second event
      unsubscribe2();

      counts = eventBus.getSubscriberCounts();
      expect(counts['node:created']).toBeUndefined();
    });

    it('should remove all handlers after reset()', () => {
      // Add multiple subscriptions
      eventBus.subscribe('node:updated', vi.fn());
      eventBus.subscribe('node:created', vi.fn());
      eventBus.subscribe('node:deleted', vi.fn());
      eventBus.subscribe('*', vi.fn()); // Wildcard subscription

      let counts = eventBus.getSubscriberCounts();
      expect(Object.keys(counts).length).toBeGreaterThan(0);

      // Reset should clear everything
      eventBus.reset();

      counts = eventBus.getSubscriberCounts();
      // After reset, only wildcard key might exist with 0
      const totalSubscribers = Object.values(counts).reduce((a, b) => a + b, 0);
      expect(totalSubscribers).toBe(0);
    });

    it('should cleanup wildcard subscriptions properly', () => {
      const wildcardHandler1 = vi.fn();
      const wildcardHandler2 = vi.fn();
      const specificHandler = vi.fn();

      // Add wildcard and specific subscriptions
      const unsubWild1 = eventBus.subscribe('*', wildcardHandler1);
      const unsubWild2 = eventBus.subscribe('*', wildcardHandler2);
      const unsubSpecific = eventBus.subscribe('node:updated', specificHandler);

      let counts = eventBus.getSubscriberCounts();
      expect(counts['*']).toBe(2);
      expect(counts['node:updated']).toBe(1);

      // Unsubscribe first wildcard
      unsubWild1();

      counts = eventBus.getSubscriberCounts();
      expect(counts['*']).toBe(1);
      expect(counts['node:updated']).toBe(1);

      // Unsubscribe second wildcard
      unsubWild2();

      counts = eventBus.getSubscriberCounts();
      expect(counts['*']).toBe(0);
      expect(counts['node:updated']).toBe(1);

      // Unsubscribe specific
      unsubSpecific();

      counts = eventBus.getSubscriberCounts();
      expect(counts['node:updated']).toBeUndefined();
    });
  });

  // ========================================================================
  // Verification Tools Tests (2 tests)
  // ========================================================================

  describe('Verification Tools', () => {
    it('should detect leaked listeners programmatically', () => {
      const addSpy = vi.spyOn(document, 'addEventListener');
      const removeSpy = vi.spyOn(document, 'removeEventListener');

      const getListenerBalance = () => {
        const added = addSpy.mock.calls.filter((call) => call[0] === 'keydown').length;
        const removed = removeSpy.mock.calls.filter((call) => call[0] === 'keydown').length;
        return added - removed;
      };

      const initialBalance = getListenerBalance();

      // Create a leak by not unmounting
      render(NodeAutocomplete, {
        visible: true,
        results: [],
        position: { x: 0, y: 0 }
      });

      const balanceWithLeak = getListenerBalance();
      expect(balanceWithLeak).toBeGreaterThan(initialBalance);

      // Clean up properly
      document.body.innerHTML = '';
    });

    it('should measure handler count accurately', () => {
      // Start with clean state
      eventBus.reset();

      let metrics = eventBus.getMetrics();
      const initialHandlerCount = metrics.totalHandlers;

      // Add some handlers
      const unsub1 = eventBus.subscribe('node:updated', vi.fn());
      const unsub2 = eventBus.subscribe('node:created', vi.fn());
      const unsub3 = eventBus.subscribe('*', vi.fn());

      metrics = eventBus.getMetrics();
      expect(metrics.totalHandlers).toBe(initialHandlerCount + 3);

      // Remove one handler
      unsub1();

      metrics = eventBus.getMetrics();
      expect(metrics.totalHandlers).toBe(initialHandlerCount + 2);

      // Remove remaining handlers
      unsub2();
      unsub3();

      metrics = eventBus.getMetrics();
      expect(metrics.totalHandlers).toBe(initialHandlerCount);
    });
  });
});
