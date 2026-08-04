import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockSetActiveTab, mockAddTab, mockNavigationStore, mockSettingsStore } = vi.hoisted(() => {
  const mockSetActiveTab = vi.fn();
  const mockAddTab = vi.fn();
  const mockNavigationStore = {
    state: {
      tabs: [] as Array<{ id: string; type: string; paneId: string }>,
      activePaneId: 'pane-1'
    }
  };
  const mockSettingsStore = {
    initialCategory: null as string | null
  };
  return { mockSetActiveTab, mockAddTab, mockNavigationStore, mockSettingsStore };
});

vi.mock('$lib/stores/navigation.svelte', () => ({
  navigationStore: mockNavigationStore,
  setActiveTab: (...args: unknown[]) => mockSetActiveTab(...args),
  addTab: (...args: unknown[]) => mockAddTab(...args)
}));

vi.mock('$lib/stores/settings.svelte', () => ({
  settingsStore: mockSettingsStore
}));

import { openSettings } from '$lib/utils/open-settings';

describe('openSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockNavigationStore.state = { tabs: [], activePaneId: 'pane-1' };
    mockSettingsStore.initialCategory = null;
  });

  describe('no existing settings tab', () => {
    it('creates a new settings tab and does not call setActiveTab', () => {
      openSettings();

      expect(mockAddTab).toHaveBeenCalledWith({
        id: 'settings',
        title: 'Settings',
        type: 'settings',
        closeable: true,
        paneId: 'pane-1'
      });
      expect(mockSetActiveTab).not.toHaveBeenCalled();
    });

    it('sets settingsStore.initialCategory when a category is passed', () => {
      openSettings('database');

      expect(mockSettingsStore.initialCategory).toBe('database');
      expect(mockAddTab).toHaveBeenCalled();
    });

    it('leaves initialCategory untouched when category is omitted', () => {
      mockSettingsStore.initialCategory = null;

      openSettings();

      expect(mockSettingsStore.initialCategory).toBeNull();
    });
  });

  describe('existing settings tab', () => {
    beforeEach(() => {
      mockNavigationStore.state = {
        tabs: [{ id: 'settings', type: 'settings', paneId: 'pane-2' }],
        activePaneId: 'pane-1'
      };
    });

    it('focuses the existing tab and does not create a new one', () => {
      openSettings();

      expect(mockSetActiveTab).toHaveBeenCalledWith('settings', 'pane-2');
      expect(mockAddTab).not.toHaveBeenCalled();
    });

    it('sets settingsStore.initialCategory when a category is passed', () => {
      openSettings('appearance');

      expect(mockSettingsStore.initialCategory).toBe('appearance');
      expect(mockSetActiveTab).toHaveBeenCalledWith('settings', 'pane-2');
    });

    it('leaves initialCategory untouched when category is omitted', () => {
      mockSettingsStore.initialCategory = null;

      openSettings();

      expect(mockSettingsStore.initialCategory).toBeNull();
    });
  });
});
