import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import type { OpenAiCompatConfig, AiChatProvider } from '$lib/types/ai-chat-node';
import {
  getOpenAiCompatConfigsFromDaemon,
  setOpenAiCompatConfigsOnDaemon,
} from '$lib/services/tauri-commands';

const log = createLogger('SettingsStore');

export type { OpenAiCompatConfig };

const LOCAL_STORAGE_KEY = 'nodespace-settings';

export interface ModelSelection {
  provider: AiChatProvider;
  modelId: string;
  configId?: string;
}

export interface AppSettings {
  activeDatabasePath: string;
  display: {
    renderMarkdown: boolean;
    theme: string;
  };
  openAiConfigs: OpenAiCompatConfig[];
  defaultModelSelection: ModelSelection | null;
}

// ---------------------------------------------------------------------------
// localStorage helpers for client-side settings
//
// OpenAI-compat configs are persisted on the daemon (~/.nodespace/daemon.toml,
// via SettingsService) — that is the source of truth the backend reads by
// UUID when loading an "openai-compat:<uuid>" model. localStorage is kept as
// a synchronous read cache only, refreshed from the daemon on loadSettings().
// ---------------------------------------------------------------------------

interface LocalPersistedSettings {
  openAiConfigs?: OpenAiCompatConfig[];
  defaultModelSelection?: ModelSelection | null;
}

function readLocalSettings(): LocalPersistedSettings {
  if (typeof localStorage === 'undefined') return {};
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY);
    if (!raw) return {};
    return JSON.parse(raw) as LocalPersistedSettings;
  } catch {
    return {};
  }
}

function writeLocalSettings(patch: LocalPersistedSettings): void {
  if (typeof localStorage === 'undefined') return;
  try {
    const existing = readLocalSettings();
    localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify({ ...existing, ...patch }));
  } catch (err) {
    log.warn('Failed to persist settings to localStorage', err);
  }
}

class SettingsStore {
  appSettings = $state<AppSettings | null>(null);

  /** Set before opening the settings tab to pre-select a category (e.g. 'integrations'). */
  initialCategory = $state<string | null>(null);

  async loadSettings(): Promise<void> {
    try {
      const settings =
        await invoke<Omit<AppSettings, 'openAiConfigs' | 'defaultModelSelection'>>('get_settings');
      const local = readLocalSettings();

      // Refresh the OpenAI-compat cache from the daemon (source of truth).
      // Falls back to the local cache if the daemon call fails (e.g. offline
      // dev-proxy mode) so the UI still has something to show.
      let openAiConfigs = local.openAiConfigs ?? [];
      try {
        const daemonConfigs = await getOpenAiCompatConfigsFromDaemon();
        openAiConfigs = daemonConfigs.map((c) => ({
          id: c.id,
          name: c.name,
          baseUrl: c.baseUrl,
          apiKey: c.apiKey,
          model: c.model,
        }));
        writeLocalSettings({ openAiConfigs });
      } catch (err) {
        log.warn('Failed to load OpenAI-compat configs from daemon, using local cache', err);
      }

      this.appSettings = {
        ...settings,
        openAiConfigs,
        defaultModelSelection: local.defaultModelSelection ?? null,
      };
    } catch (err) {
      log.error('Failed to load settings:', err);
    }
  }

  async updateDisplaySetting(
    key: 'renderMarkdown' | 'theme',
    value: boolean | string
  ): Promise<void> {
    try {
      const params: Record<string, unknown> = {};
      if (key === 'renderMarkdown') params.render_markdown = value;
      if (key === 'theme') params.theme = value;

      await invoke('update_display_settings', params);

      // Optimistic update
      if (this.appSettings) {
        this.appSettings = {
          ...this.appSettings,
          display: { ...this.appSettings.display, [key]: value },
        };
      }
    } catch (err) {
      log.error('Failed to update display setting:', err);
    }
  }

  /**
   * Persist the full set of OpenAI-compat configs. Writes to the local cache
   * immediately (so synchronous getOpenAiConfigs() readers see the change),
   * then pushes to the daemon — the backend-accessible source of truth used
   * to resolve "openai-compat:<uuid>" models.
   */
  async saveOpenAiConfigs(configs: OpenAiCompatConfig[]): Promise<void> {
    writeLocalSettings({ openAiConfigs: configs });
    if (this.appSettings) {
      this.appSettings = { ...this.appSettings, openAiConfigs: configs };
    }
    try {
      await setOpenAiCompatConfigsOnDaemon(
        configs.map((c) => ({
          id: c.id,
          name: c.name,
          baseUrl: c.baseUrl,
          apiKey: c.apiKey,
          model: c.model,
        }))
      );
    } catch (err) {
      log.error('Failed to persist OpenAI-compat configs to daemon:', err);
    }
  }

  saveDefaultModelSelection(selection: ModelSelection | null): void {
    writeLocalSettings({ defaultModelSelection: selection });
    if (this.appSettings) {
      this.appSettings = { ...this.appSettings, defaultModelSelection: selection };
    }
  }
}

export const settingsStore = new SettingsStore();

// ---------------------------------------------------------------------------
// Free-function delegators / helpers (keep existing callers working unchanged)
// ---------------------------------------------------------------------------

export function getOpenAiConfigs(): OpenAiCompatConfig[] {
  return readLocalSettings().openAiConfigs ?? [];
}

export function saveOpenAiConfigs(configs: OpenAiCompatConfig[]): Promise<void> {
  return settingsStore.saveOpenAiConfigs(configs);
}

export function getDefaultModelSelection(): ModelSelection | null {
  return readLocalSettings().defaultModelSelection ?? null;
}

export function saveDefaultModelSelection(selection: ModelSelection | null): void {
  settingsStore.saveDefaultModelSelection(selection);
}

export const loadSettings = (): Promise<void> => settingsStore.loadSettings();

export const updateDisplaySetting = (
  key: 'renderMarkdown' | 'theme',
  value: boolean | string
): Promise<void> => settingsStore.updateDisplaySetting(key, value);
