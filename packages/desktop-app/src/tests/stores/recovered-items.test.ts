/**
 * Recovered-items store — Pro-path gate regression.
 *
 * Locks the gate/trigger contract: `load()` gates on the same axis-1 `proSync.isPro`
 * signal that fires its one-shot `onProConfirmed` trigger. Narrowing the gate to the
 * two-axis `isProSyncActive()` (which additionally requires the active database's
 * `DatabaseSettingsNode` to be hydrated) regressed the feature: that node hydrates on
 * a separate, strictly-longer async chain than tier resolution, so the one-shot
 * trigger fired while the gate was still unsatisfied and the log never loaded for the
 * session. These tests assert the gate keys off Pro tier alone.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import { proSync } from '$lib/stores/pro-sync.svelte';
import { recoveredItems } from '$lib/stores/recovered-items.svelte';

const sampleItem = {
  node_id: 'n1',
  superseded_content: 'mine',
  superseded_modified_at: '2026-01-01T00:00:00Z',
  winning_content: 'theirs',
  winning_modified_at: '2026-01-02T00:00:00Z',
  recovered_at: '2026-01-02T00:00:01Z'
};

describe('recovered-items store gates on Pro tier alone', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    proSync.tier = 'community';
    recoveredItems.items = [];
    recoveredItems.loaded = false;
  });

  it('loads when the daemon is Pro even before the DatabaseSettingsNode hydrates', async () => {
    // Pro tier resolved; the per-database settings node is deliberately NOT hydrated
    // — the exact window in which the one-shot onProConfirmed trigger fires at boot.
    proSync.tier = 'pro';
    mockInvoke.mockResolvedValueOnce([sampleItem]);

    await recoveredItems.load();

    expect(mockInvoke).toHaveBeenCalledWith('pro_list_recovered_items');
    expect(recoveredItems.items).toEqual([sampleItem]);
    expect(recoveredItems.hasFor('n1')).toBe(true);
  });

  it('is a no-op outside Pro and never calls the daemon', async () => {
    proSync.tier = 'community';

    await recoveredItems.load();

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(recoveredItems.items).toEqual([]);
    expect(recoveredItems.loaded).toBe(true);
  });
});
