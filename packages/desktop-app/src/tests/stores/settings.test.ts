import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import { settingsStore, loadSettings, updateDisplaySetting } from '$lib/stores/settings.svelte';
import type { AppSettings } from '$lib/stores/settings.svelte';

describe('Settings Store', () => {
  const mockSettings: AppSettings = {
    activeDatabasePath: '/tmp/test.db',
    display: {
      renderMarkdown: true,
      theme: 'light'
    },
    openAiConfigs: [],
    defaultModelSelection: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    settingsStore.appSettings = null;
  });

  describe('appSettings store', () => {
    it('should start as null', () => {
      expect(settingsStore.appSettings).toBeNull();
    });
  });

  describe('loadSettings', () => {
    it('should call invoke and set store', async () => {
      // Backend only returns the persisted (non-localStorage) subset.
      const backendSettings = {
        activeDatabasePath: mockSettings.activeDatabasePath,
        display: mockSettings.display,
      };
      mockInvoke.mockResolvedValueOnce(backendSettings);

      await loadSettings();

      expect(mockInvoke).toHaveBeenCalledWith('get_settings');
      // Store merges backend fields with localStorage defaults.
      expect(settingsStore.appSettings).toEqual(mockSettings);
    });

    it('should handle errors gracefully', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('invoke failed'));

      await loadSettings();

      expect(settingsStore.appSettings).toBeNull();
    });
  });

  describe('updateDisplaySetting', () => {
    it('should update renderMarkdown setting', async () => {
      settingsStore.appSettings = mockSettings;
      mockInvoke.mockResolvedValueOnce(undefined);

      await updateDisplaySetting('renderMarkdown', false);

      expect(mockInvoke).toHaveBeenCalledWith('update_display_settings', {
        render_markdown: false
      });
      expect(settingsStore.appSettings?.display.renderMarkdown).toBe(false);
    });

    it('should update theme setting', async () => {
      settingsStore.appSettings = mockSettings;
      mockInvoke.mockResolvedValueOnce(undefined);

      await updateDisplaySetting('theme', 'dark');

      expect(mockInvoke).toHaveBeenCalledWith('update_display_settings', {
        theme: 'dark'
      });
      expect(settingsStore.appSettings?.display.theme).toBe('dark');
    });

    it('should handle errors gracefully', async () => {
      settingsStore.appSettings = mockSettings;
      mockInvoke.mockRejectedValueOnce(new Error('update failed'));

      await updateDisplaySetting('renderMarkdown', false);

      // Optimistic update is after the await, so it's skipped when invoke rejects
      expect(settingsStore.appSettings?.display.renderMarkdown).toBe(true);
    });

    it('should return null when store is null', async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      await updateDisplaySetting('theme', 'dark');

      // Optimistic update on null store should keep it null
      expect(settingsStore.appSettings).toBeNull();
    });
  });
});
