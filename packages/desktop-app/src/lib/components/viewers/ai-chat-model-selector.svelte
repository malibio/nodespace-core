<!--
  AiChatModelSelector — unified single-dropdown model picker for AiChatNodeViewer.

  Replaces the two-step provider → AiChatModelPicker full-page flow with a compact
  dropdown in the chat header. Renders section headers (Local, Ollama, OpenAI-compat)
  and "Set up..." action as a native <select>-based custom UI.

  On mount fetches: chatModelList(), getSystemRamGb(), ollamaAvailable(), and
  OpenAI-compat configs from the settings store. Emits a ModelSelection via onSelect.

  For native models that need download, calls onSelect immediately with the selection
  — the parent viewer owns the download modal.
-->

<script lang="ts" module>
  export interface ModelSelection {
    provider: 'native' | 'ollama' | 'openai-compat';
    modelId: string;
    configId?: string; // for openai-compat
  }
</script>

<script lang="ts">
  /* global HTMLSelectElement */
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import {
    chatModelList,
    ollamaAvailable,
    getSystemRamGb,
    type ChatModelEntry,
  } from '$lib/services/tauri-commands';
  import { AGENT_EVENTS } from '$lib/types/agent-types';
  import { getOpenAiConfigs } from '$lib/stores/settings';
  import { settingsInitialCategory } from '$lib/stores/settings';
  import { addTab, tabState, setActiveTab } from '$lib/stores/navigation';
  import { get } from 'svelte/store';
  import { createLogger } from '$lib/utils/logger';
  import type { OpenAiCompatConfig } from '$lib/types/ai-chat-node';

  const log = createLogger('AiChatModelSelector');

  const MIN_RAM_GB = 16;

  // Sentinel values used in the <select> value attribute.
  const SETUP_SENTINEL = '__setup__';
  const HEADER_SENTINEL_PREFIX = '__header__';

  let {
    nodeId,
    disabled = false,
    currentValue = '',
    onSelect,
  }: {
    nodeId: string;
    disabled?: boolean;
    /** Currently selected value, reflected in the <select>. */
    currentValue?: string;
    onSelect?: (_selection: ModelSelection) => void;
  } = $props();

  // --- Async data ---
  let models = $state<ChatModelEntry[]>([]);
  let ramGb = $state(0);
  let ollamaRunning = $state(false);
  let openAiConfigs = $state<OpenAiCompatConfig[]>([]);
  let loading = $state(true);

  // Live download tracking (model_id → bytes)
  let downloads = $state<Record<string, { downloaded: number; total: number }>>({});
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenReady: UnlistenFn | null = null;

  // --- Derived subsets ---
  const nativeModels = $derived(models.filter((m) => m.backend === 'gguf'));
  const ollamaModels = $derived(models.filter((m) => m.backend === 'ollama'));
  const ramTooLow = $derived(ramGb > 0 && ramGb < MIN_RAM_GB);

  function isReady(m: ChatModelEntry): boolean {
    return m.status?.status === 'ready' || m.status?.status === 'loaded';
  }

  function isDownloading(m: ChatModelEntry): boolean {
    return m.status?.status === 'downloading' || m.id in downloads;
  }

  /** Build the value string used in the <select> for a given model entry. */
  function nativeValue(m: ChatModelEntry): string {
    return `native:${m.id}`;
  }

  function ollamaValue(m: ChatModelEntry): string {
    return `ollama:${m.id}`;
  }

  function openAiValue(cfg: OpenAiCompatConfig): string {
    return `openai-compat:${cfg.id}`;
  }

  const isTauri =
    typeof window !== 'undefined' &&
    ('__TAURI__' in window || '__TAURI_INTERNALS__' in window);

  async function refresh(): Promise<void> {
    try {
      const [list, ram, ollama] = await Promise.all([
        chatModelList(),
        getSystemRamGb(),
        ollamaAvailable(),
      ]);
      models = list;
      ramGb = ram;
      ollamaRunning = ollama;
      openAiConfigs = getOpenAiConfigs();
    } catch (err) {
      log.error('Failed to load model list', err);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await refresh();

    if (isTauri) {
      try {
        unlistenProgress = await listen<{
          model_id: string;
          bytes_downloaded: number;
          bytes_total: number;
        }>(AGENT_EVENTS.MODEL_DOWNLOAD_PROGRESS, (event) => {
          const { model_id, bytes_downloaded, bytes_total } = event.payload;
          downloads = {
            ...downloads,
            [model_id]: { downloaded: bytes_downloaded, total: bytes_total },
          };
        });
      } catch (err) {
        log.warn('Failed to listen for download progress', err);
      }

      try {
        unlistenReady = await listen<{ model_id: string }>(
          AGENT_EVENTS.MODEL_DOWNLOAD_READY,
          (event) => {
            const next = { ...downloads };
            delete next[event.payload.model_id];
            downloads = next;
            // Refresh to pick up new status (ready/loaded).
            refresh();
          }
        );
      } catch (err) {
        log.warn('Failed to listen for download ready', err);
      }
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
    unlistenReady?.();
  });

  /** Open the Settings tab pre-focused on the AI Models section. */
  function openAiModelsSettings(): void {
    settingsInitialCategory.set('ai-models');
    const state = get(tabState);
    const existing = state.tabs.find((t) => t.type === 'settings');
    if (existing) {
      setActiveTab(existing.id, existing.paneId);
    } else {
      addTab({
        id: 'settings',
        title: 'Settings',
        type: 'settings',
        closeable: true,
        paneId: state.activePaneId,
      });
    }
  }

  function handleChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value;

    if (!value || value.startsWith(HEADER_SENTINEL_PREFIX)) return;

    if (value === SETUP_SENTINEL) {
      openAiModelsSettings();
      // Reset the <select> back to the current value so it doesn't stay on "Set up…".
      (e.currentTarget as HTMLSelectElement).value = currentValue;
      return;
    }

    if (value.startsWith('native:')) {
      const modelId = value.slice('native:'.length);
      log.debug('Model selected', { provider: 'native', modelId, nodeId });
      onSelect?.({ provider: 'native', modelId });
      return;
    }

    if (value.startsWith('ollama:')) {
      const modelId = value.slice('ollama:'.length);
      log.debug('Model selected', { provider: 'ollama', modelId, nodeId });
      onSelect?.({ provider: 'ollama', modelId });
      return;
    }

    if (value.startsWith('openai-compat:')) {
      const configId = value.slice('openai-compat:'.length);
      const cfg = openAiConfigs.find((c) => c.id === configId);
      if (!cfg) return;
      log.debug('OpenAI-compat config selected', { configId, nodeId });
      onSelect?.({ provider: 'openai-compat', modelId: cfg.name, configId });
      return;
    }
  }

  function downloadBadgeText(m: ChatModelEntry): string {
    const dl = downloads[m.id];
    if (dl && dl.total > 0) {
      const pct = Math.round((dl.downloaded / dl.total) * 100);
      return ` (${pct}%)`;
    }
    return ' (downloading…)';
  }
</script>

<div class="model-selector-wrapper">
  {#if loading}
    <span class="model-selector-loading">Loading…</span>
  {:else}
    <select
      class="model-selector"
      aria-label="Select AI model"
      value={currentValue}
      {disabled}
      onchange={handleChange}
    >
      {#if !currentValue}
        <option value="" disabled selected>Choose a model…</option>
      {/if}

      <!-- ── Local section ── -->
      <optgroup label="Local">
        {#each nativeModels as m (m.id)}
          {@const ready = isReady(m)}
          {@const downloading = isDownloading(m)}
          {@const tooLow = ramTooLow}
          <option
            value={nativeValue(m)}
            disabled={tooLow}
            title={tooLow ? `Requires ${MIN_RAM_GB} GB RAM (system has ${ramGb} GB)` : undefined}
          >
            {m.name}{tooLow
              ? ` (requires ${MIN_RAM_GB} GB RAM)`
              : !ready && !downloading
                ? ' (download needed)'
                : downloading
                  ? downloadBadgeText(m)
                  : ''}
          </option>
        {/each}
        {#if nativeModels.length === 0}
          <option value={`${HEADER_SENTINEL_PREFIX}no-local`} disabled>No local models found</option>
        {/if}
      </optgroup>

      <!-- ── Ollama section ── -->
      {#if ollamaRunning}
        <optgroup label="Ollama">
          {#each ollamaModels as m (m.id)}
            <option value={ollamaValue(m)}>{m.name}</option>
          {/each}
          {#if ollamaModels.length === 0}
            <option value={`${HEADER_SENTINEL_PREFIX}no-ollama`} disabled>
              No Ollama models — run ollama pull &lt;name&gt;
            </option>
          {/if}
        </optgroup>
      {:else}
        <optgroup label="Ollama (not running)" disabled></optgroup>
      {/if}

      <!-- ── OpenAI-compat configs ── -->
      {#if openAiConfigs.length > 0}
        <optgroup label="Custom endpoints">
          {#each openAiConfigs as cfg (cfg.id)}
            <option value={openAiValue(cfg)}>{cfg.name}</option>
          {/each}
        </optgroup>
      {/if}

      <!-- ── Action: Set up… ── -->
      <optgroup label="─────────────────" disabled></optgroup>
      <option value={SETUP_SENTINEL}>Set up…</option>
    </select>
  {/if}
</div>

<style>
  .model-selector-wrapper {
    display: flex;
    align-items: center;
  }

  .model-selector {
    font-size: 0.8125rem;
    padding: 0.3125rem 0.5rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    cursor: pointer;
    max-width: 14rem;
  }

  .model-selector:hover:not(:disabled) {
    border-color: hsl(var(--ring));
  }

  .model-selector:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .model-selector-loading {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }
</style>
