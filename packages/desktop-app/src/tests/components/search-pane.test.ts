/**
 * search-pane component.
 *
 * The Search view opened from the sidebar's Search nav item. Queries the
 * daemon's semantic root search (`search_roots`) and opens a node tab when a
 * result is clicked. Regression coverage for the dead-Search-nav bug.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockInvoke = vi.fn();
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);

import SearchPane from '$lib/components/search/search-pane.svelte';
import { navigationStore, resetTabState } from '$lib/stores/navigation.svelte';

describe('SearchPane', () => {
  beforeEach(() => {
    resetTabState();
    mockInvoke.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('prompts the user to search before any query is entered', () => {
    mockInvoke.mockResolvedValue([]);
    const { getByPlaceholderText, getByText } = render(SearchPane);
    expect(getByPlaceholderText('Search nodes…')).toBeTruthy();
    expect(getByText('Type to search your nodes.')).toBeTruthy();
  });

  it('queries search_roots on Enter and lists the results', async () => {
    mockInvoke.mockResolvedValue([
      { id: 'n1', nodeType: 'text', content: 'Alpha doc' },
      { id: 'n2', nodeType: 'text', content: 'Beta doc' }
    ]);
    const { getByPlaceholderText, findByText } = render(SearchPane);

    const input = getByPlaceholderText('Search nodes…');
    await fireEvent.input(input, { target: { value: 'doc' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(await findByText('Alpha doc')).toBeTruthy();
    expect(await findByText('Beta doc')).toBeTruthy();
    expect(mockInvoke).toHaveBeenCalledWith('search_roots', {
      params: { query: 'doc', limit: 25 }
    });
  });

  it('opens a node tab when a result is clicked', async () => {
    mockInvoke.mockResolvedValue([{ id: 'n1', nodeType: 'text', content: 'Alpha doc' }]);
    const { getByPlaceholderText, findByText } = render(SearchPane);

    const input = getByPlaceholderText('Search nodes…');
    await fireEvent.input(input, { target: { value: 'alpha' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    await fireEvent.click(await findByText('Alpha doc'));

    const opened = navigationStore.state.tabs.find((t) => t.content?.nodeId === 'n1');
    expect(opened).toBeTruthy();
    expect(opened?.type).toBe('node');
  });

  it('shows an empty state when there are no matches', async () => {
    mockInvoke.mockResolvedValue([]);
    const { getByPlaceholderText, findByText } = render(SearchPane);

    const input = getByPlaceholderText('Search nodes…');
    await fireEvent.input(input, { target: { value: 'zzz' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(await findByText(/No results for/)).toBeTruthy();
  });
});
