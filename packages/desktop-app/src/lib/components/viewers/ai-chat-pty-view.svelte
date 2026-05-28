<!--
  AiChatPtyView - Provider mode 2d (pty) sub-view of the ai-chat node viewer.

  Per ADR-034, a PTY agent session IS an ai-chat node (provider: pty). This
  component is the node's *viewer* for that mode: when no session is running it
  shows an inline agent picker + Launch button; once launched it hosts the
  embedded xterm terminal (the "iframe"). The conversation node already exists —
  capture backfills it at session end via the node_id passed to launch.

  This replaces the standalone agent-launch-panel / sessions-panel cluster from
  the old ADR-032 standalone model.
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import PtyTerminal from '$lib/components/agent/pty-terminal.svelte';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import {
    getCaptureSettings,
    updateCaptureSettings,
    ptyCheckAgentAvailability,
    ptyLaunchSession,
    type AgentAvailabilityInfo,
    type CaptureContentLevel,
  } from '$lib/services/tauri-commands';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AiChatPtyView');

  let { nodeId }: { nodeId: string } = $props();

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

  const node = $derived(sharedNodeStore.getNode(nodeId));

  // A previously-launched session id persisted on the node lets the terminal
  // re-attach on reopen (the daemon owns the PTY and supports multi-client
  // streaming per ADR-032).
  const persistedSessionId = $derived(
    (node?.properties?.['capture:session_id'] as string | undefined) ?? null
  );

  let activeSessionId = $state<string | null>(null);
  const sessionId = $derived(activeSessionId ?? persistedSessionId);

  let selectedAgent = $state('claude-code');
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
      log.warn('Failed to load pty view settings', e);
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
        prompt: null,
        cols: 80,
        rows: 24,
        nodeId,
      });
      activeSessionId = result.sessionId;

      // Record the chosen agent + session on the node up front so the node
      // reflects its mode immediately. Capture backfills the rest at session
      // end (transcript/summary/exit code) via the node_id passed above.
      const current = sharedNodeStore.getNode(nodeId);
      sharedNodeStore.updateNode(
        nodeId,
        {
          properties: {
            ...current?.properties,
            'capture:agent_type': selectedAgent,
            'capture:session_id': result.sessionId,
            status: 'active',
          },
        },
        { type: 'viewer', viewerId: 'ai-chat-pty-view' }
      );
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

{#if sessionId}
  <!-- Active session: the embedded terminal IS the node's viewer. -->
  <div class="pty-terminal-host">
    {#key sessionId}
      <PtyTerminal {sessionId} />
    {/key}
  </div>
{:else}
  <!-- Config step: pick an agent and launch. -->
  <div class="pty-config">
    <div class="pty-config-card">
      <h3 class="pty-config-title">Launch agent session</h3>
      <p class="pty-config-subtitle">
        Run an external agent CLI in an embedded terminal. The session is this node.
      </p>

      <div class="field">
        <label class="field-label" for="agent-select">Agent</label>
        <select id="agent-select" class="field-select" bind:value={selectedAgent} disabled={launching}>
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
            <span class="capture-label">Save session back to this node</span>
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

      <button class="launch-button" onclick={launch} onkeydown={handleKeydown} disabled={launching}>
        {#if launching}
          <span class="spinner" aria-hidden="true"></span>
          Launching…
        {:else}
          Launch
        {/if}
      </button>
    </div>
  </div>
{/if}

<style>
  .pty-terminal-host {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: hsl(222 47% 8%);
  }

  .pty-config {
    flex: 1;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    overflow-y: auto;
    padding: 2rem 1rem;
  }

  .pty-config-card {
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
    width: 100%;
    max-width: 26rem;
    padding: 1.25rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    background: hsl(var(--card));
  }

  .pty-config-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .pty-config-subtitle {
    margin: -0.5rem 0 0;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
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

  .field-select {
    padding: 0.5rem 0.625rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    font-size: 0.8125rem;
    font-family: inherit;
    transition: border-color 0.15s;
  }

  .field-select:focus {
    outline: none;
    border-color: hsl(var(--ring));
  }

  .field-select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
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
