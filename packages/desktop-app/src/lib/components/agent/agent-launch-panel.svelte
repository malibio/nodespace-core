<script lang="ts">
  import { ptyLaunchSession } from '$lib/services/tauri-commands';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AgentLaunchPanel');

  const AGENT_OPTIONS = [
    { id: 'claude-code', label: 'Claude Code' },
    { id: 'codex', label: 'Codex' },
    { id: 'gemini-cli', label: 'Gemini CLI' },
    { id: 'pi', label: 'Pi' },
    { id: 'open-code', label: 'Open Code' },
  ];

  let {
    onSessionLaunched,
  }: {
    onSessionLaunched: (_sessionId: string) => void;
  } = $props();

  let selectedAgent = $state('claude-code');
  let prompt = $state('');
  let launching = $state(false);
  let error = $state<string | null>(null);

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
          <option value={option.id}>{option.label}</option>
        {/each}
      </select>
    </div>

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
