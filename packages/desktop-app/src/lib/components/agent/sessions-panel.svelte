<script lang="ts">
  import { ptyListSessions, ptyTerminateSession, type PtySessionInfo } from '$lib/services/tauri-commands';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('SessionsPanel');

  const AGENT_LABELS: Record<string, string> = {
    'claude-code': 'Claude Code',
    codex: 'Codex',
    'gemini-cli': 'Gemini CLI',
    pi: 'Pi',
    'open-code': 'Open Code',
  };

  let {
    activeSessionId = null,
    onSelectSession,
    onSessionTerminated,
  }: {
    activeSessionId?: string | null;
    onSelectSession: (_sessionId: string) => void;
    onSessionTerminated: (_sessionId: string) => void;
  } = $props();

  let sessions = $state<PtySessionInfo[]>([]);
  let loading = $state(false);
  let terminatingIds = $state(new Set<string>());

  async function refresh() {
    loading = true;
    try {
      const result = await ptyListSessions();
      sessions = result.sessions;
    } catch (e) {
      log.warn('Failed to list sessions', e);
    } finally {
      loading = false;
    }
  }

  async function terminate(sessionId: string) {
    terminatingIds = new Set([...terminatingIds, sessionId]);
    try {
      await ptyTerminateSession(sessionId);
      onSessionTerminated(sessionId);
      sessions = sessions.filter((s) => s.sessionId !== sessionId);
    } catch (e) {
      log.error('Failed to terminate session', e);
    } finally {
      terminatingIds = new Set([...terminatingIds].filter((id) => id !== sessionId));
    }
  }

  function formatAge(startedAt: number): string {
    const seconds = Math.floor(Date.now() / 1000) - startedAt;
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    return `${Math.floor(seconds / 3600)}h`;
  }

  $effect(() => {
    refresh();
  });
</script>

<div class="sessions-panel">
  <div class="sessions-header">
    <h3 class="sessions-title">Active Sessions</h3>
    <button
      class="refresh-button"
      onclick={refresh}
      disabled={loading}
      aria-label="Refresh sessions"
      title="Refresh"
    >
      <svg
        class="refresh-icon"
        class:spinning={loading}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        width="14"
        height="14"
        aria-hidden="true"
      >
        <polyline points="23 4 23 10 17 10" />
        <polyline points="1 20 1 14 7 14" />
        <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15" />
      </svg>
    </button>
  </div>

  <div class="sessions-body">
    {#if sessions.length === 0}
      <div class="sessions-empty">No active sessions</div>
    {:else}
      {#each sessions as session (session.sessionId)}
        {@const agentLabel = AGENT_LABELS[session.agentType] ?? session.agentType}
        {@const isActive = session.sessionId === activeSessionId}
        {@const isTerminating = terminatingIds.has(session.sessionId)}
        <div class="session-row" class:active={isActive}>
          <button
            class="session-select"
            onclick={() => onSelectSession(session.sessionId)}
            aria-pressed={isActive}
            title="Open terminal"
          >
            <span class="session-agent">{agentLabel}</span>
            <span class="session-meta">
              <span class="session-id">{session.sessionId.slice(0, 8)}</span>
              <span class="session-age">{formatAge(session.startedAt)}</span>
            </span>
          </button>
          <button
            class="terminate-button"
            onclick={() => terminate(session.sessionId)}
            disabled={isTerminating}
            aria-label="Terminate {agentLabel} session"
            title="Terminate"
          >
            {#if isTerminating}
              <span class="spinner" aria-hidden="true"></span>
            {:else}
              <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                width="12"
                height="12"
                aria-hidden="true"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            {/if}
          </button>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .sessions-panel {
    display: flex;
    flex-direction: column;
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    background: hsl(var(--card));
    overflow: hidden;
  }

  .sessions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid hsl(var(--border));
    background: hsl(var(--muted) / 0.4);
  }

  .sessions-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .refresh-button {
    display: flex;
    align-items: center;
    padding: 0.25rem;
    border: none;
    background: none;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    border-radius: 0.25rem;
    transition: color 0.15s;
  }

  .refresh-button:hover:not(:disabled) {
    color: hsl(var(--foreground));
  }

  .refresh-button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .refresh-icon {
    transition: transform 0.2s;
  }

  .spinning {
    animation: spin 0.7s linear infinite;
  }

  .sessions-body {
    display: flex;
    flex-direction: column;
  }

  .sessions-empty {
    padding: 1rem;
    text-align: center;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }

  .session-row {
    display: flex;
    align-items: center;
    border-bottom: 1px solid hsl(var(--border) / 0.5);
    transition: background 0.1s;
  }

  .session-row:last-child {
    border-bottom: none;
  }

  .session-row.active {
    background: hsl(var(--accent));
  }

  .session-select {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.625rem 0.875rem;
    border: none;
    background: none;
    color: hsl(var(--foreground));
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .session-select:hover {
    background: hsl(var(--accent));
  }

  .session-agent {
    font-size: 0.8125rem;
    font-weight: 500;
  }

  .session-meta {
    display: flex;
    gap: 0.5rem;
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }

  .session-id {
    font-family: monospace;
  }

  .terminate-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    margin-right: 0.25rem;
    border: none;
    background: none;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    border-radius: 0.25rem;
    flex-shrink: 0;
    transition: color 0.15s, background 0.15s;
  }

  .terminate-button:hover:not(:disabled) {
    color: hsl(0 72% 51%);
    background: hsl(0 72% 51% / 0.1);
  }

  .terminate-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 1.5px solid hsl(var(--muted-foreground) / 0.3);
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
