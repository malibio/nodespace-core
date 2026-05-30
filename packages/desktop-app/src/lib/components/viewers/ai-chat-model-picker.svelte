<!--
  ai-chat-model-picker — model selection for the message-UI provider modes
  (native = built-in GGUF, ollama), composed by AiChatNodeViewer. Internal helper
  (named like ChatMessage/ChatInput), so no *Node/*Viewer/*View suffix.

  - native: lists built-in GGUF models; downloaded ones are selectable, others
    offer a Download action (progress via the model://download-progress event).
    Surfaces a RAM-aware recommendation.
  - ollama: lists models served by a running Ollama (already downloaded there).

  Selecting a ready model writes { provider, model } onto the node; the parent
  then renders the message UI.
-->

<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import {
    chatModelList,
    chatModelRecommended,
    chatModelDownload,
    chatModelCancelDownload,
    getSystemRamGb,
    type ChatModelEntry,
  } from '$lib/services/tauri-commands';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AiChatModelPicker');

  let {
    nodeId,
    provider,
  }: {
    nodeId: string;
    provider: 'native' | 'ollama';
  } = $props();

  const backend = $derived(provider === 'ollama' ? 'ollama' : 'gguf');

  let models = $state<ChatModelEntry[]>([]);
  let recommendedId = $state<string | null>(null);
  let ramGb = $state(0);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // modelId -> {downloaded, total} while a download is in flight.
  let downloads = $state<Record<string, { downloaded: number; total: number }>>({});
  let unlistenProgress: UnlistenFn | null = null;

  const visibleModels = $derived(models.filter((m) => m.backend === backend));

  function isReady(m: ChatModelEntry): boolean {
    return m.status?.status === 'ready' || m.status?.status === 'loaded';
  }
  function isDownloading(m: ChatModelEntry): boolean {
    return m.status?.status === 'downloading' || m.id in downloads;
  }

  async function refresh() {
    try {
      const [list, ram] = await Promise.all([chatModelList(), getSystemRamGb()]);
      models = list;
      ramGb = ram;
      if (provider === 'native') {
        try {
          recommendedId = await chatModelRecommended();
        } catch (e) {
          log.warn('Failed to get recommended model', e);
        }
      }
    } catch (e) {
      log.error('Failed to list models', e);
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await refresh();
    try {
      unlistenProgress = await listen<{
        model_id: string;
        bytes_downloaded: number;
        bytes_total: number;
      }>('model://download-progress', (event) => {
        const { model_id, bytes_downloaded, bytes_total } = event.payload;
        downloads = {
          ...downloads,
          [model_id]: { downloaded: bytes_downloaded, total: bytes_total },
        };
      });
    } catch (e) {
      log.warn('Failed to listen for download progress', e);
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
  });

  async function download(modelId: string) {
    error = null;
    downloads = { ...downloads, [modelId]: { downloaded: 0, total: 0 } };
    try {
      await chatModelDownload(modelId);
      await refresh();
    } catch (e) {
      log.error('Download failed', e);
      error = e instanceof Error ? e.message : String(e);
    } finally {
      const next = { ...downloads };
      delete next[modelId];
      downloads = next;
    }
  }

  async function cancelDownload(modelId: string) {
    try {
      await chatModelCancelDownload(modelId);
    } catch (e) {
      log.warn('Cancel download failed', e);
    } finally {
      const next = { ...downloads };
      delete next[modelId];
      downloads = next;
    }
  }

  /** Select a ready model: persist provider + model so the parent shows the chat UI. */
  function selectModel(modelId: string) {
    const current = sharedNodeStore.getNode(nodeId);
    sharedNodeStore.updateNode(
      nodeId,
      { properties: { ...current?.properties, provider, model: modelId, status: 'active' } },
      { type: 'viewer', viewerId: 'ai-chat-model-picker' }
    );
  }

  function formatGb(bytes: number): string {
    if (!bytes) return '';
    return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  }
  function pct(d: { downloaded: number; total: number }): number {
    return d.total > 0 ? Math.round((d.downloaded / d.total) * 100) : 0;
  }
</script>

<div class="model-picker">
  <div class="model-picker-head">
    <h3 class="model-picker-title">
      {provider === 'ollama' ? 'Choose an Ollama model' : 'Choose a built-in model'}
    </h3>
    {#if ramGb > 0}
      <span class="model-picker-ram">{ramGb} GB RAM</span>
    {/if}
  </div>

  {#if loading}
    <p class="model-picker-status">Loading models…</p>
  {:else if error}
    <div class="error-banner" role="alert">{error}</div>
  {:else if visibleModels.length === 0}
    <p class="model-picker-status">
      {#if provider === 'ollama'}
        No Ollama models found. Pull a model with <code>ollama pull &lt;name&gt;</code> first.
      {:else}
        No models available.
      {/if}
    </p>
  {:else}
    <ul class="model-list">
      {#each visibleModels as m (m.id)}
        {@const ready = isReady(m)}
        {@const downloading = isDownloading(m)}
        {@const dl = downloads[m.id]}
        {@const tooBig = m.minMemoryGb > 0 && ramGb > 0 && m.minMemoryGb > ramGb}
        <li class="model-row" class:model-row--recommended={m.id === recommendedId}>
          <div class="model-info">
            <span class="model-name">
              {m.name}
              {#if m.id === recommendedId}<span class="model-tag">Recommended</span>{/if}
              {#if tooBig}<span class="model-tag model-tag--warn">Needs {m.minMemoryGb} GB</span>{/if}
            </span>
            <span class="model-meta">
              {#if m.quantization}{m.quantization}{/if}
              {#if m.sizeBytes}· {formatGb(m.sizeBytes)}{/if}
            </span>
          </div>

          <div class="model-action">
            {#if ready}
              <button class="btn btn--primary" onclick={() => selectModel(m.id)}>Use</button>
            {:else if downloading}
              <div class="dl">
                {#if dl && dl.total > 0}
                  <span class="dl-pct">{pct(dl)}%</span>
                {:else}
                  <span class="spinner" aria-hidden="true"></span>
                {/if}
                <button class="btn btn--ghost" onclick={() => cancelDownload(m.id)}>Cancel</button>
              </div>
            {:else}
              <button class="btn" disabled={tooBig} onclick={() => download(m.id)}>Download</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .model-picker {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow-y: auto;
    padding: 1.5rem 1rem;
    max-width: 36rem;
    width: 100%;
    margin: 0 auto;
  }

  .model-picker-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .model-picker-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .model-picker-ram {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
  }

  .model-picker-status {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
  }

  .model-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.625rem 0.875rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    background: hsl(var(--card));
  }

  .model-row--recommended {
    border-color: hsl(var(--ring));
  }

  .model-info {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .model-name {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    display: flex;
    align-items: center;
    gap: 0.375rem;
    flex-wrap: wrap;
  }

  .model-meta {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }

  .model-tag {
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.1);
    border-radius: 0.25rem;
    padding: 0 0.3rem;
  }

  .model-tag--warn {
    color: hsl(32 95% 44%);
    background: hsl(38 92% 50% / 0.12);
  }

  .model-action {
    flex-shrink: 0;
  }

  .dl {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .dl-pct {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }

  .btn {
    padding: 0.375rem 0.75rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    font-size: 0.8125rem;
    cursor: pointer;
    transition:
      opacity 0.15s,
      background 0.15s;
  }

  .btn:hover:not(:disabled) {
    background: hsl(var(--accent));
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn--primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    border-color: hsl(var(--primary));
  }

  .btn--ghost {
    border: none;
    background: none;
    color: hsl(var(--muted-foreground));
    padding: 0.375rem 0.5rem;
  }

  .error-banner {
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    background: hsl(0 72% 51% / 0.1);
    border: 1px solid hsl(0 72% 51% / 0.3);
    color: hsl(0 72% 51%);
    font-size: 0.8125rem;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid hsl(var(--muted-foreground) / 0.3);
    border-top-color: hsl(var(--muted-foreground));
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
