/**
 * Regression tests for the pane-content.svelte hydration race (issue #1564).
 *
 * Full-component-mount tests for BaseNodeViewer-adjacent components require complex
 * setup (NodeServiceContext, plugin registry, database mocking - see the note in
 * merge-prevention.test.ts). Instead these tests exercise the exact hydration/race-guard
 * logic extracted from pane-content.svelte's hydrateNode(), driven with real async
 * timing via a controllable mock ensureNode(), to verify:
 *
 * 1. A slow/failed ensureNode() no longer throws an unhandled rejection.
 * 2. Rapid/repeated navigation before an earlier ensureNode() resolves does not let the
 *    stale response overwrite state for a nodeId that is no longer current.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

interface Node {
  id: string;
}

// Mirrors hydrateNode() in pane-content.svelte: tracks the most recently requested
// nodeId so a stale in-flight response can be detected and discarded after await.
function createHydrationHarness(ensureNode: (nodeId: string) => Promise<Node | undefined>) {
  let hydratedNodeIds = new Set<string>();
  let latestRequestedNodeId: string | undefined;
  const closedTabs: string[] = [];
  const errors: unknown[] = [];

  async function hydrateNode(nodeId: string, tabId: string) {
    if (hydratedNodeIds.has(nodeId)) return;

    latestRequestedNodeId = nodeId;

    let node: Node | undefined;
    try {
      node = await ensureNode(nodeId);
    } catch (error) {
      errors.push(error);
      return;
    }

    if (latestRequestedNodeId !== nodeId) return;

    if (!node) {
      closedTabs.push(tabId);
      return;
    }

    hydratedNodeIds = new Set(hydratedNodeIds).add(nodeId);
  }

  return {
    hydrateNode,
    isHydrated: (nodeId: string) => hydratedNodeIds.has(nodeId),
    closedTabs,
    errors
  };
}

describe('pane-content hydration race guard (#1564)', () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it('does not throw or leave an unhandled rejection when ensureNode fails', async () => {
    const ensureNode = vi.fn().mockRejectedValue(new TypeError('Load failed'));
    const harness = createHydrationHarness(ensureNode);

    await expect(harness.hydrateNode('2026-07-07', 'tab-1')).resolves.toBeUndefined();
    expect(harness.errors).toHaveLength(1);
    expect(harness.isHydrated('2026-07-07')).toBe(false);
  });

  it('discards a stale response when a newer navigation supersedes it before resolving', async () => {
    // First request is slow; second (newer) request resolves first.
    const resolvers = new Map<string, (node: Node | undefined) => void>();
    const ensureNode = vi.fn(
      (nodeId: string) =>
        new Promise<Node | undefined>((resolve) => {
          resolvers.set(nodeId, resolve);
        })
    );
    const harness = createHydrationHarness(ensureNode);

    // Simulate rapid prev/next clicks: navigate to "2026-07-08" then immediately to "2026-07-07"
    // before the first ensureNode() call has resolved (the #1564 repro: navigation keeps
    // landing on the wrong date because the stale request wins the race).
    const firstCall = harness.hydrateNode('2026-07-08', 'tab-1');
    const secondCall = harness.hydrateNode('2026-07-07', 'tab-1');

    // Resolve the newer request first, then the stale one - mirrors an out-of-order
    // network response for a slow/failed dev-proxy fetch.
    resolvers.get('2026-07-07')?.({ id: '2026-07-07' });
    await secondCall;
    resolvers.get('2026-07-08')?.({ id: '2026-07-08' });
    await firstCall;

    // Only the current (later-requested) date should be marked hydrated - the stale
    // '2026-07-08' response must not win even though its promise resolves after.
    expect(harness.isHydrated('2026-07-07')).toBe(true);
    expect(harness.isHydrated('2026-07-08')).toBe(false);
  });

  it('closes the tab only if the not-found response is still the latest request', async () => {
    const resolvers = new Map<string, (node: Node | undefined) => void>();
    const ensureNode = vi.fn(
      (nodeId: string) =>
        new Promise<Node | undefined>((resolve) => {
          resolvers.set(nodeId, resolve);
        })
    );
    const harness = createHydrationHarness(ensureNode);

    const staleCall = harness.hydrateNode('deleted-node', 'tab-1');
    const freshCall = harness.hydrateNode('2026-07-07', 'tab-1');

    resolvers.get('2026-07-07')?.({ id: '2026-07-07' });
    await freshCall;
    // The stale request resolves as "not found" after being superseded - should not close the tab.
    resolvers.get('deleted-node')?.(undefined);
    await staleCall;

    expect(harness.closedTabs).toEqual([]);
    expect(harness.isHydrated('2026-07-07')).toBe(true);
  });

  it('repeated rapid navigation across many dates converges on the last requested date', async () => {
    const resolvers = new Map<string, (node: Node | undefined) => void>();
    const ensureNode = vi.fn(
      (nodeId: string) =>
        new Promise<Node | undefined>((resolve) => {
          resolvers.set(nodeId, resolve);
        })
    );
    const harness = createHydrationHarness(ensureNode);

    const dates = ['2026-07-05', '2026-07-06', '2026-07-07', '2026-07-08', '2026-07-09'];
    const calls = dates.map((date) => harness.hydrateNode(date, 'tab-1'));

    // Resolve out of order, simulating an unreliable dev-proxy connection.
    for (const date of [...dates].reverse()) {
      resolvers.get(date)?.({ id: date });
    }
    await Promise.all(calls);

    const hydratedDates = dates.filter((date) => harness.isHydrated(date));
    expect(hydratedDates).toEqual(['2026-07-09']);
  });
});
