import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import type { OpenAiCompatConfig, AiChatProvider } from '$lib/types/ai-chat-node';

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
      this.appSettings = {
        ...settings,
        openAiConfigs: local.openAiConfigs ?? [],
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

  saveOpenAiConfigs(configs: OpenAiCompatConfig[]): void {
    writeLocalSettings({ openAiConfigs: configs });
    if (this.appSettings) {
      this.appSettings = { ...this.appSettings, openAiConfigs: configs };
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

export function saveOpenAiConfigs(configs: OpenAiCompatConfig[]): void {
  settingsStore.saveOpenAiConfigs(configs);
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
