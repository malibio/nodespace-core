/**
 * Unit tests for computeTabTitle.
 *
 * Type-specific title logic is covered separately in plugin-registry.test.ts
 * (getNodeTitle/getTitle hook) — these tests isolate computeTabTitle's own branching:
 * tab-type gating, the getNode lookup, and the fallback chain to tab.title.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { computeTabTitle } from '$lib/utils/tab-title';
import { pluginRegistry } from '$lib/plugins/plugin-registry';
import type { Tab } from '$lib/stores/navigation.svelte';
import { createTestNode } from '../helpers/test-helpers';

function createTestTab(overrides: Partial<Tab> = {}): Tab {
  return {
    id: 'tab-1',
    title: 'Fallback Title',
    type: 'node',
    closeable: true,
    paneId: 'pane-1',
    ...overrides
  };
}

describe('computeTabTitle', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns tab.title for settings tabs without looking up a node', () => {
    const tab = createTestTab({ type: 'settings', title: 'Settings' });
    const getNode = vi.fn();

    expect(computeTabTitle(tab, getNode)).toBe('Settings');
    expect(getNode).not.toHaveBeenCalled();
  });

  it('returns tab.title for placeholder tabs without looking up a node', () => {
    const tab = createTestTab({ type: 'placeholder', title: 'Placeholder' });
    const getNode = vi.fn();

    expect(computeTabTitle(tab, getNode)).toBe('Placeholder');
    expect(getNode).not.toHaveBeenCalled();
  });

  it('returns tab.title for a node tab with no content', () => {
    const tab = createTestTab({ type: 'node', content: undefined, title: 'No Content Yet' });
    const getNode = vi.fn();

    expect(computeTabTitle(tab, getNode)).toBe('No Content Yet');
    expect(getNode).not.toHaveBeenCalled();
  });

  it('returns tab.title when getNode finds no node for the tab content', () => {
    const tab = createTestTab({
      content: { nodeId: 'missing-node', nodeType: 'text' },
      title: 'Stale Title'
    });
    const getNode = vi.fn().mockReturnValue(undefined);

    expect(computeTabTitle(tab, getNode)).toBe('Stale Title');
    expect(getNode).toHaveBeenCalledWith('missing-node');
  });

  it('derives the title from the node via pluginRegistry.getNodeTitle', () => {
    const node = createTestNode({ nodeType: 'text', content: 'Live content' });
    const tab = createTestTab({
      content: { nodeId: node.id, nodeType: 'text' },
      title: 'Stale Title'
    });
    const getNode = vi.fn().mockReturnValue(node);
    vi.spyOn(pluginRegistry, 'getNodeTitle').mockReturnValue('Live content');

    expect(computeTabTitle(tab, getNode)).toBe('Live content');
    expect(pluginRegistry.getNodeTitle).toHaveBeenCalledWith(node);
  });

  it('falls back to tab.title when pluginRegistry.getNodeTitle returns undefined', () => {
    const node = createTestNode({ nodeType: 'date', content: '' });
    const tab = createTestTab({
      content: { nodeId: node.id, nodeType: 'date' },
      title: 'Fallback Title'
    });
    const getNode = vi.fn().mockReturnValue(node);
    vi.spyOn(pluginRegistry, 'getNodeTitle').mockReturnValue(undefined);

    expect(computeTabTitle(tab, getNode)).toBe('Fallback Title');
  });

  it('truncates and strips markdown header syntax via formatTabTitle', () => {
    const node = createTestNode({ nodeType: 'header', content: '# A Header' });
    const tab = createTestTab({ content: { nodeId: node.id, nodeType: 'header' } });
    const getNode = vi.fn().mockReturnValue(node);
    vi.spyOn(pluginRegistry, 'getNodeTitle').mockReturnValue('# A Header');

    expect(computeTabTitle(tab, getNode)).toBe('A Header');
  });
});
