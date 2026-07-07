import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const { mockLoadCollections, mockLoadMembers, mockCollectionsState } = vi.hoisted(() => {
  const mockLoadCollections = vi.fn().mockResolvedValue(undefined);
  const mockLoadMembers = vi.fn().mockResolvedValue(undefined);

  // Minimal rune-store-like mock: exposes a reactive-style `state` field.
  // `set` is a test helper to configure that field.
  const mockCollectionsState = {
    state: { selectedCollectionId: null as string | null },
    set(newValue: { selectedCollectionId: string | null }) {
      this.state = newValue;
    }
  };

  return { mockLoadCollections, mockLoadMembers, mockCollectionsState };
});

vi.mock('$lib/stores/collections.svelte', () => ({
  collectionsData: {
    loadCollections: (...args: unknown[]) => mockLoadCollections(...args),
    loadMembers: (...args: unknown[]) => mockLoadMembers(...args)
  },
  collectionsState: mockCollectionsState
}));

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

import { scheduleCollectionRefresh, clearCollectionRefreshTimer } from '$lib/utils/collection-refresh';

describe('Collection Refresh', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    clearCollectionRefreshTimer();
    mockCollectionsState.set({ selectedCollectionId: null });
  });

  afterEach(() => {
    clearCollectionRefreshTimer();
    vi.useRealTimers();
  });

  describe('scheduleCollectionRefresh', () => {
    it('should refresh collections after debounce delay', async () => {
      scheduleCollectionRefresh();

      expect(mockLoadCollections).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).toHaveBeenCalledTimes(1);
    });

    it('should debounce multiple calls', async () => {
      scheduleCollectionRefresh();
      scheduleCollectionRefresh();
      scheduleCollectionRefresh();

      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).toHaveBeenCalledTimes(1);
    });

    it('should refresh members if affected collection is selected', async () => {
      mockCollectionsState.set({ selectedCollectionId: 'col-1' });

      scheduleCollectionRefresh('col-1');
      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).toHaveBeenCalledTimes(1);
      expect(mockLoadMembers).toHaveBeenCalledWith('col-1');
    });

    it('should not refresh members if different collection is selected', async () => {
      mockCollectionsState.set({ selectedCollectionId: 'col-2' });

      scheduleCollectionRefresh('col-1');
      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).toHaveBeenCalledTimes(1);
      expect(mockLoadMembers).not.toHaveBeenCalled();
    });

    it('should not refresh members if no collection is selected', async () => {
      scheduleCollectionRefresh('col-1');
      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).toHaveBeenCalledTimes(1);
      expect(mockLoadMembers).not.toHaveBeenCalled();
    });
  });

  describe('clearCollectionRefreshTimer', () => {
    it('should cancel pending refresh', async () => {
      scheduleCollectionRefresh();
      clearCollectionRefreshTimer();

      await vi.advanceTimersByTimeAsync(300);

      expect(mockLoadCollections).not.toHaveBeenCalled();
    });

    it('should be safe to call when no timer is pending', () => {
      expect(() => clearCollectionRefreshTimer()).not.toThrow();
    });
  });
});
