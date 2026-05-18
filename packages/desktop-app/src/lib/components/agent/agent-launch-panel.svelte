<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getCaptureSettings,
    ptyCheckAgentAvailability,
    ptyLaunchSession,
    updateCaptureSettings,
    type AgentAvailabilityInfo,
    type CaptureContentLevel,
  } from '$lib/services/tauri-commands';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AgentLaunchPanel');

  const AGENT_OPTIONS = [
    { id: 'claude-code', label: 'Claude Code' },
    { id: 'codex', label: 'Codex' },
    { id: 'gemini-cli', label: 'Gemini CLI' },
    { id: 'pi', label: 'Pi' },
    { id: 'open-code', label: 'Open Code' },
  ];

  const CONTENT_LEVELS: { value: CaptureContentLevel; label: string }[] = [
    { value: 'metadata_only', label: 'Metadata only' },
    { value: 'summary', label: 'Summary' },
    { value: 'full', label: 'Full transcript' },
  ];

  type AgentStatus =
    | 'ready'
    | 'binary_missing'
    | 'auth_missing'
    | 'binary_missing_and_auth_missing'
    | 'unknown';

  let {
    onSessionLaunched,
  }: {
    onSessionLaunched: (_sessionId: string) => void;
  } = $props();

  let selectedAgent = $state('claude-code');
  let prompt = $state('');
  let launching = $state(false);
  let error = $state<string | null>(null);

  let captureEnabled = $state(false);
  let captureSync = $state(false);
  let captureContent = $state<CaptureContentLevel>('metadata_only');

  let availability = $state<Record<string, AgentAvailabilityInfo>>({});
  let availabilityLoading = $state(true);

  onMount(async () => {
    try {
      const [settings, availResult] = await Promise.all([
        getCaptureSettings(),
        ptyCheckAgentAvailability(),
      ]);
      captureEnabled = settings.enabled;
      captureSync = settings.sync;
      captureContent = settings.content;
      const map: Record<string, AgentAvailabilityInfo> = {};
      for (const agent of availResult.agents) {
        map[agent.agentType] = agent;
      }
      availability = map;
    } catch (e) {
      log.warn('Failed to load panel settings', e);
    } finally {
      availabilityLoading = false;
    }
  });

  async function saveCaptureSettings() {
    try {
      await updateCaptureSettings({
        enabled: captureEnabled,
        sync: captureSync,
        content: captureContent,
      });
    } catch (e) {
      log.error('Failed to save capture settings', e);
    }
  }

  function selectedAvailability(): AgentAvailabilityInfo | undefined {
    return availability[selectedAgent];
  }


  function agentStatus(agentId: string): AgentStatus {
    const av = availability[agentId];
    if (!av) return 'unknown';
    if (!av.binaryFound && !av.authFound) return 'binary_missing_and_auth_missing';
    if (!av.binaryFound) return 'binary_missing';
    if (!av.authFound) return 'auth_missing';
    return 'ready';
  }

  async function launch() {
    launching = true;
    error = null;
    try {
      const result = await ptyLaunchSession({
        agentType: selectedAgent,
        prompt: prompt.trim() || null,
        cols: 80,
        rows: 24,
      });
      prompt = '';
      onSessionLaunched(result.sessionId);
    } catch (e) {
      log.error('Failed to launch session', e);
      error = e instanceof Error ? e.message : String(e);
    } finally {
      launching = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      launch();
    }
  }
</script>

<div class="launch-panel">
  <div class="launch-panel-header">
    <h3 class="launch-panel-title">Launch Agent Session</h3>
  </div>

  <div class="launch-panel-body">
    <div class="field">
      <label class="field-label" for="agent-select">Agent</label>
      <select
        id="agent-select"
        class="field-select"
        bind:value={selectedAgent}
        disabled={launching}
      >
        {#each AGENT_OPTIONS as option (option.id)}
          {@const status = agentStatus(option.id)}
          <option value={option.id}>
            {option.label}{status === 'ready' || status === 'unknown' ? '' : ' ⚠'}
          </option>
        {/each}
      </select>
    </div>

    {#if !availabilityLoading}
      {@const av = selectedAvailability()}
      {@const status = agentStatus(selectedAgent)}
      {#if av && status !== 'ready' && status !== 'unknown'}
        <div class="availability-banner availability-banner--warning" role="alert">
          {#if status === 'binary_missing' || status === 'binary_missing_and_auth_missing'}
            <div class="availability-row">
              <span class="availability-icon">⚠</span>
              <span>
                <strong>{av.binary}</strong> not found on PATH.
                {#if av.installHint}
                  <span class="install-hint">{av.installHint}</span>
                {/if}
              </span>
            </div>
          {/if}
          {#if status === 'auth_missing' || status === 'binary_missing_and_auth_missing'}
            <div class="availability-row">
              <span class="availability-icon">⚠</span>
              <span>Auth credential not configured for this agent.</span>
            </div>
          {/if}
        </div>
      {:else if av && status === 'ready'}
        <div class="availability-banner availability-banner--ready" role="status">
          <span class="availability-icon">✓</span> Ready
        </div>
      {/if}
    {/if}

    <div class="field">
      <label class="field-label" for="prompt-input">
        Initial prompt
        <span class="field-hint">(optional)</span>
      </label>
      <textarea
        id="prompt-input"
        class="field-textarea"
        bind:value={prompt}
        onkeydown={handleKeydown}
        placeholder="Enter a task or leave blank for interactive mode…"
        rows={3}
        disabled={launching}
      ></textarea>
      <span class="field-hint-inline">⌘↩ to launch</span>
    </div>

    <details class="capture-section">
      <summary class="capture-summary">Session capture</summary>
      <div class="capture-body">
        <label class="capture-row">
          <input
            type="checkbox"
            class="capture-checkbox"
            bind:checked={captureEnabled}
            onchange={saveCaptureSettings}
          />
          <span class="capture-label">Save session to knowledge graph</span>
        </label>

        {#if captureEnabled}
          <div class="capture-row capture-indent">
            <label class="field-label" for="capture-content">Content</label>
            <select
              id="capture-content"
              class="field-select capture-select"
              bind:value={captureContent}
              onchange={saveCaptureSettings}
            >
              {#each CONTENT_LEVELS as level (level.value)}
                <option value={level.value}>{level.label}</option>
              {/each}
            </select>
          </div>

          <label class="capture-row capture-indent">
            <input
              type="checkbox"
              class="capture-checkbox"
              bind:checked={captureSync}
              onchange={saveCaptureSettings}
            />
            <span class="capture-label">Include in sync</span>
          </label>
        {/if}
      </div>
    </details>

    {#if error}
      <div class="error-banner" role="alert">{error}</div>
    {/if}

    <button class="launch-button" onclick={launch} disabled={launching}>
      {#if launching}
        <span class="spinner" aria-hidden="true"></span>
        Launching…
      {:else}
        Launch
      {/if}
    </button>
  </div>
</div>

<style>
  .launch-panel {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    background: hsl(var(--card));
    overflow: hidden;
  }

  .launch-panel-header {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid hsl(var(--border));
    background: hsl(var(--muted) / 0.4);
  }

  .launch-panel-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .launch-panel-body {
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
    padding: 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .field-label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
  }

  .field-hint {
    font-weight: 400;
    color: hsl(var(--muted-foreground));
    font-size: 0.75rem;
  }

  .field-select,
  .field-textarea {
    padding: 0.5rem 0.625rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    font-size: 0.8125rem;
    font-family: inherit;
    resize: vertical;
    transition: border-color 0.15s;
  }

  .field-select:focus,
  .field-textarea:focus {
    outline: none;
    border-color: hsl(var(--ring));
  }

  .field-select:disabled,
  .field-textarea:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .field-hint-inline {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    align-self: flex-end;
  }

  .availability-banner {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.8125rem;
  }

  .availability-banner--warning {
    background: hsl(38 92% 50% / 0.1);
    border: 1px solid hsl(38 92% 50% / 0.35);
    color: hsl(32 95% 44%);
  }

  .availability-banner--ready {
    background: hsl(142 71% 45% / 0.1);
    border: 1px solid hsl(142 71% 45% / 0.3);
    color: hsl(142 71% 35%);
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }

  .availability-row {
    display: flex;
    align-items: flex-start;
    gap: 0.4rem;
  }

  .availability-icon {
    flex-shrink: 0;
    font-size: 0.75rem;
    margin-top: 0.05rem;
  }

  .install-hint {
    display: block;
    margin-top: 0.2rem;
    font-size: 0.75rem;
    opacity: 0.85;
    font-family: ui-monospace, monospace;
    word-break: break-all;
  }

  .capture-section {
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    overflow: hidden;
  }

  .capture-summary {
    padding: 0.5rem 0.75rem;
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    cursor: pointer;
    user-select: none;
    background: hsl(var(--muted) / 0.3);
  }

  .capture-summary:hover {
    background: hsl(var(--muted) / 0.5);
  }

  .capture-body {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.75rem;
  }

  .capture-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .capture-indent {
    padding-left: 1.25rem;
  }

  .capture-checkbox {
    width: 14px;
    height: 14px;
    cursor: pointer;
    flex-shrink: 0;
  }

  .capture-label {
    font-size: 0.8125rem;
    color: hsl(var(--foreground));
  }

  .capture-select {
    flex: 1;
    margin-top: 0.25rem;
  }

  .error-banner {
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    background: hsl(0 72% 51% / 0.1);
    border: 1px solid hsl(0 72% 51% / 0.3);
    color: hsl(0 72% 51%);
    font-size: 0.8125rem;
  }

  .launch-button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 0.375rem;
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .launch-button:hover:not(:disabled) {
    opacity: 0.9;
  }

  .launch-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid hsl(var(--primary-foreground) / 0.3);
    border-top-color: hsl(var(--primary-foreground));
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
