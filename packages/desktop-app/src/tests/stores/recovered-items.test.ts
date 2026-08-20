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
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);

const mockWarn = vi.fn();
vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: (...args: unknown[]) => mockWarn(...args),
    error: vi.fn()
  })
}));

import { proSync } from '$lib/stores/pro-sync.svelte';
import { recoveredItems, type RecoveredItem } from '$lib/stores/recovered-items.svelte';

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
    mockWarn.mockReset();
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

describe('recovered-items store — load()', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockWarn.mockReset();
    proSync.tier = 'pro';
    recoveredItems.items = [];
    recoveredItems.loaded = false;
  });

  it('falls back to [] when invoke resolves with null', async () => {
    mockInvoke.mockResolvedValueOnce(null);

    await recoveredItems.load();

    expect(recoveredItems.items).toEqual([]);
    expect(recoveredItems.loaded).toBe(true);
  });

  it('falls back to [] when invoke resolves with undefined', async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    await recoveredItems.load();

    expect(recoveredItems.items).toEqual([]);
    expect(recoveredItems.loaded).toBe(true);
  });

  it('catches a rejected invoke, logs a warning, clears items, and still sets loaded', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('daemon unreachable'));

    await recoveredItems.load();

    expect(mockWarn).toHaveBeenCalledWith(
      'Failed to load recovered items',
      expect.objectContaining({ error: expect.any(Error) })
    );
    expect(recoveredItems.items).toEqual([]);
    expect(recoveredItems.loaded).toBe(true);
  });
});

describe('recovered-items store — hasFor / itemFor', () => {
  const olderMatch: RecoveredItem = { ...sampleItem, recovered_at: '2026-01-01T00:00:00Z' };
  const newerMatch: RecoveredItem = { ...sampleItem, recovered_at: '2026-01-03T00:00:00Z' };
  const otherNode: RecoveredItem = { ...sampleItem, node_id: 'n2' };

  beforeEach(() => {
    recoveredItems.items = [olderMatch, otherNode, newerMatch];
  });

  it('hasFor returns true when a matching entry exists', () => {
    expect(recoveredItems.hasFor('n1')).toBe(true);
    expect(recoveredItems.hasFor('n2')).toBe(true);
  });

  it('hasFor returns false when no entry matches', () => {
    expect(recoveredItems.hasFor('does-not-exist')).toBe(false);
  });

  it('itemFor returns the LAST matching entry (append-ordered log, last wins)', () => {
    expect(recoveredItems.itemFor('n1')).toEqual(newerMatch);
  });

  it('itemFor returns undefined when no entry matches', () => {
    expect(recoveredItems.itemFor('does-not-exist')).toBeUndefined();
  });
});

describe('recovered-items store — dismiss()', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockWarn.mockReset();
    recoveredItems.items = [sampleItem];
  });

  it('optimistically removes matching items and calls the daemon on success', async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    await recoveredItems.dismiss('n1');

    expect(recoveredItems.items).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith('pro_dismiss_recovered_item', { nodeId: 'n1' });
  });

  it('removes the item locally even when the daemon call fails, and logs a warning', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('daemon unreachable'));

    await recoveredItems.dismiss('n1');

    expect(recoveredItems.items).toEqual([]);
    expect(mockWarn).toHaveBeenCalledWith(
      'Failed to dismiss recovered item',
      expect.objectContaining({ error: expect.any(Error), nodeId: 'n1' })
    );
  });
});

describe('recovered-items store — clear()', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockWarn.mockReset();
    recoveredItems.items = [sampleItem];
  });

  it('optimistically clears items and calls the daemon on success', async () => {
    mockInvoke.mockResolvedValueOnce(undefined);

    await recoveredItems.clear();

    expect(recoveredItems.items).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith('pro_clear_recovered_items');
  });

  it('clears items locally even when the daemon call fails, and logs a warning', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('daemon unreachable'));

    await recoveredItems.clear();

    expect(recoveredItems.items).toEqual([]);
    expect(mockWarn).toHaveBeenCalledWith(
      'Failed to clear recovered items',
      expect.objectContaining({ error: expect.any(Error) })
    );
  });
});
