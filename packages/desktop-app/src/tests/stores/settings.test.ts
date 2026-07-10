import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

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

import {
  settingsStore,
  loadSettings,
  updateDisplaySetting,
  saveOpenAiConfigs,
} from '$lib/stores/settings.svelte';
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

  function enableTauri(): void {
    (globalThis.window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
  }

  function disableTauri(): void {
    delete (globalThis.window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }

  beforeEach(() => {
    vi.clearAllMocks();
    settingsStore.appSettings = null;
    localStorage.clear();
    disableTauri();
  });

  afterEach(() => {
    // singleFork test execution shares one `window` across every test file
    // in the run — an enabled flag left set here would leak into unrelated
    // suites (e.g. tauri-commands.test.ts's "outside Tauri" fallback tests).
    disableTauri();
    localStorage.clear();
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

  describe('OpenAI-compat config daemon persistence', () => {
    it('loadSettings refreshes openAiConfigs from the daemon when running under Tauri', async () => {
      enableTauri();
      const backendSettings = {
        activeDatabasePath: mockSettings.activeDatabasePath,
        display: mockSettings.display,
      };
      const daemonConfigs = [
        { id: 'abc', name: 'My Endpoint', baseUrl: 'https://api.example.com/v1', apiKey: 'sk-test' },
      ];
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(backendSettings);
        if (cmd === 'get_openai_compat_configs') return Promise.resolve(daemonConfigs);
        return Promise.resolve(undefined);
      });

      await loadSettings();

      expect(mockInvoke).toHaveBeenCalledWith('get_openai_compat_configs');
      expect(settingsStore.appSettings?.openAiConfigs).toEqual(daemonConfigs);
    });

    it('loadSettings falls back to the local cache if the daemon call fails', async () => {
      enableTauri();
      const backendSettings = {
        activeDatabasePath: mockSettings.activeDatabasePath,
        display: mockSettings.display,
      };
      localStorage.setItem(
        'nodespace-settings',
        JSON.stringify({
          openAiConfigs: [
            { id: 'cached', name: 'Cached', baseUrl: 'https://cached.example.com', apiKey: '' },
          ],
        })
      );
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(backendSettings);
        if (cmd === 'get_openai_compat_configs') return Promise.reject(new Error('daemon unreachable'));
        return Promise.resolve(undefined);
      });

      await loadSettings();

      expect(settingsStore.appSettings?.openAiConfigs).toEqual([
        { id: 'cached', name: 'Cached', baseUrl: 'https://cached.example.com', apiKey: '' },
      ]);
    });

    it('saveOpenAiConfigs writes to localStorage immediately and pushes to the daemon', async () => {
      enableTauri();
      mockInvoke.mockResolvedValue([]);
      const configs = [
        { id: 'new-id', name: 'New Endpoint', baseUrl: 'https://new.example.com', apiKey: 'sk-new' },
      ];

      await saveOpenAiConfigs(configs);

      expect(mockInvoke).toHaveBeenCalledWith('set_openai_compat_configs', { configs });
      const cached = JSON.parse(localStorage.getItem('nodespace-settings') ?? '{}');
      expect(cached.openAiConfigs).toEqual(configs);
    });

    it('saveOpenAiConfigs keeps the local write even if the daemon push fails', async () => {
      enableTauri();
      mockInvoke.mockRejectedValue(new Error('daemon unreachable'));
      const configs = [
        { id: 'x', name: 'X', baseUrl: 'https://x.example.com', apiKey: '' },
      ];

      await saveOpenAiConfigs(configs);

      const cached = JSON.parse(localStorage.getItem('nodespace-settings') ?? '{}');
      expect(cached.openAiConfigs).toEqual(configs);
    });
  });
});
