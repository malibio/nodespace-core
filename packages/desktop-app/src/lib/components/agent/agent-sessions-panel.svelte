<script lang="ts">
  import AgentLaunchPanel from './agent-launch-panel.svelte';
  import SessionsPanel from './sessions-panel.svelte';
  import PtyTerminal from './pty-terminal.svelte';

  let activeSessionId = $state<string | null>(null);

  function handleSessionLaunched(sessionId: string) {
    activeSessionId = sessionId;
  }

  function handleSelectSession(sessionId: string) {
    activeSessionId = sessionId;
  }

  function handleSessionTerminated(sessionId: string) {
    if (activeSessionId === sessionId) {
      activeSessionId = null;
    }
  }
</script>

<div class="agent-sessions-panel">
  <div class="sidebar">
    <AgentLaunchPanel onSessionLaunched={handleSessionLaunched} />
    <div class="sidebar-divider"></div>
    <SessionsPanel
      {activeSessionId}
      onSelectSession={handleSelectSession}
      onSessionTerminated={handleSessionTerminated}
    />
  </div>

  <div class="terminal-area">
    {#if activeSessionId}
      {#key activeSessionId}
        <PtyTerminal sessionId={activeSessionId} />
      {/key}
    {:else}
      <div class="terminal-empty">
        <p class="terminal-empty-text">Launch an agent session to get started</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .agent-sessions-panel {
    display: flex;
    height: 100%;
    overflow: hidden;
    background: hsl(var(--background));
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: 18rem;
    flex-shrink: 0;
    padding: 0.875rem;
    border-right: 1px solid hsl(var(--border));
    overflow-y: auto;
  }

  .sidebar-divider {
    height: 1px;
    background: hsl(var(--border));
    margin: 0 -0.875rem;
  }

  .terminal-area {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    padding: 0.5rem;
    background: hsl(222 47% 8%);
  }

  .terminal-empty {
    display: flex;
    height: 100%;
    align-items: center;
    justify-content: center;
  }

  .terminal-empty-text {
    font-size: 0.875rem;
    color: hsl(0 0% 50%);
  }
</style>
