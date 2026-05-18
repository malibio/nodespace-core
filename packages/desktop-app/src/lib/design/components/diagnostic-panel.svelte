<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    getLogEntries,
    getDiagnosticStats,
    clearLogEntries,
    exportLogsAsJson,
    type DiagnosticLogEntry,
    type DiagnosticStats
  } from '$lib/services/diagnostic-logger';

  let isOpen = $state(false);
  let logEntries = $state<DiagnosticLogEntry[]>([]);
  let stats = $state<DiagnosticStats | null>(null);
  let autoRefresh = $state(true);
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let dbInitError = $state<string | null>(null);

  // Check for database initialization error
  function checkDbInitError() {
    const win = window as unknown as { __DB_INIT_ERROR__?: string };
    if (win.__DB_INIT_ERROR__) {
      dbInitError = win.__DB_INIT_ERROR__;
    }
  }

  // Keyboard shortcut handler
  function handleKeydown(event: KeyboardEvent) {
    // Ctrl+Shift+D (or Cmd+Shift+D on Mac) to toggle panel
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 'd') {
      event.preventDefault();
      isOpen = !isOpen;
      if (isOpen) {
        refreshLogs();
      }
    }
  }

  function refreshLogs() {
    logEntries = getLogEntries();
    stats = getDiagnosticStats();
  }

  function handleClearLogs() {
    clearLogEntries();
    refreshLogs();
  }

  function handleExportLogs() {
    const json = exportLogsAsJson();
    const blob = new globalThis.Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `nodespace-diagnostics-${new Date().toISOString()}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  function formatDuration(ms: number): string {
    if (ms < 1) return '<1ms';
    if (ms < 1000) return `${ms.toFixed(1)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeydown);

    // Check for database initialization error
    checkDbInitError();

    // Auto-refresh logs every 2 seconds when panel is open
    refreshInterval = setInterval(() => {
      if (isOpen && autoRefresh) {
        refreshLogs();
      }
    }, 2000);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeydown);
    if (refreshInterval) {
      clearInterval(refreshInterval);
    }
  });
</script>

{#if isOpen}
  <div class="diagnostic-panel">
    <div class="panel-header">
      <h2>Diagnostic Panel</h2>
      <div class="header-actions">
        <span class="shortcut-hint">Ctrl+Shift+D to toggle</span>
        <button class="close-button" onclick={() => (isOpen = false)}>X</button>
      </div>
    </div>

    {#if dbInitError}
      <div class="init-error-banner">
        <strong>DATABASE INITIALIZATION FAILED:</strong> {dbInitError}
        <p class="error-hint">This is why all operations are failing. Check console/terminal for more details.</p>
      </div>
    {/if}

    <div class="panel-content">
      <div class="logs-tab">
        <div class="toolbar">
          <label class="auto-refresh">
            <input type="checkbox" bind:checked={autoRefresh} />
            Auto-refresh
          </label>
          <button onclick={refreshLogs}>Refresh</button>
          <button onclick={handleClearLogs}>Clear</button>
          <button onclick={handleExportLogs}>Export JSON</button>
        </div>

        {#if stats}
          <div class="stats-bar">
            <span>Total: {stats.totalCalls}</span>
            <span class="success">Success: {stats.successCalls}</span>
            <span class="error">Errors: {stats.errorCalls}</span>
            <span>Avg: {formatDuration(stats.avgDurationMs)}</span>
          </div>
        {/if}

        <div class="log-list">
          {#each [...logEntries].reverse() as entry (entry.id)}
            <div class="log-entry" class:error={entry.status === 'error'} class:pending={entry.status === 'pending'}>
              <div class="entry-header">
                <span class="method">{entry.method}</span>
                <span class="status" class:success={entry.status === 'success'} class:error={entry.status === 'error'}>
                  {entry.status}
                </span>
                <span class="duration">{formatDuration(entry.durationMs)}</span>
                <span class="timestamp">{new Date(entry.timestamp).toLocaleTimeString()}</span>
              </div>
              <div class="entry-details">
                <div class="args">
                  <strong>Args:</strong>
                  <code>{JSON.stringify(entry.args, null, 2)}</code>
                </div>
                {#if entry.result !== undefined}
                  <div class="result">
                    <strong>Result:</strong>
                    <code>{JSON.stringify(entry.result, null, 2)}</code>
                  </div>
                {/if}
                {#if entry.error}
                  <div class="error-msg">
                    <strong>Error:</strong>
                    <code>{entry.error}</code>
                  </div>
                {/if}
              </div>
            </div>
          {/each}
          {#if logEntries.length === 0}
            <div class="empty-message">No backend calls logged yet. Make some operations in the app.</div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .diagnostic-panel {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 50vh;
    background: var(--color-bg-secondary, #1e1e1e);
    border-top: 2px solid var(--color-border, #444);
    z-index: 10000;
    display: flex;
    flex-direction: column;
    font-family: monospace;
    font-size: 12px;
    color: var(--color-text, #e0e0e0);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    background: var(--color-bg-tertiary, #252525);
    border-bottom: 1px solid var(--color-border, #444);
  }

  .panel-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .shortcut-hint {
    color: var(--color-text-muted, #888);
    font-size: 11px;
  }

  .close-button {
    background: transparent;
    border: 1px solid var(--color-border, #444);
    color: var(--color-text, #e0e0e0);
    padding: 4px 8px;
    cursor: pointer;
    border-radius: 4px;
  }

  .close-button:hover {
    background: var(--color-bg-hover, #333);
  }

  .init-error-banner {
    background: #da3633;
    color: white;
    padding: 12px 16px;
    margin: 0;
    font-weight: 500;
  }

  .init-error-banner strong {
    display: block;
    margin-bottom: 4px;
  }

  .init-error-banner .error-hint {
    margin: 8px 0 0 0;
    font-size: 11px;
    opacity: 0.9;
  }

  .panel-content {
    flex: 1;
    overflow: auto;
    padding: 12px;
  }

  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
    align-items: center;
  }

  .toolbar button {
    padding: 4px 12px;
    background: var(--color-bg-tertiary, #252525);
    border: 1px solid var(--color-border, #444);
    color: var(--color-text, #e0e0e0);
    border-radius: 4px;
    cursor: pointer;
  }

  .toolbar button:hover:not(:disabled) {
    background: var(--color-bg-hover, #333);
  }

  .toolbar button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .auto-refresh {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--color-text-muted, #888);
  }

  .stats-bar {
    display: flex;
    gap: 16px;
    padding: 8px 12px;
    background: var(--color-bg-tertiary, #252525);
    border-radius: 4px;
    margin-bottom: 12px;
  }

  .stats-bar .success {
    color: #3fb950;
  }

  .stats-bar .error {
    color: #f85149;
  }

  .log-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .log-entry {
    background: var(--color-bg-tertiary, #252525);
    border: 1px solid var(--color-border, #444);
    border-radius: 4px;
    padding: 8px;
  }

  .log-entry.error {
    border-color: #f85149;
  }

  .log-entry.pending {
    border-color: #d29922;
  }

  .entry-header {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 8px;
  }

  .method {
    font-weight: 600;
    color: var(--color-primary, #58a6ff);
  }

  .status {
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 10px;
    text-transform: uppercase;
  }

  .status.success {
    background: #238636;
    color: white;
  }

  .status.error {
    background: #da3633;
    color: white;
  }

  .duration {
    color: var(--color-text-muted, #888);
  }

  .timestamp {
    color: var(--color-text-muted, #888);
    margin-left: auto;
  }

  .entry-details {
    font-size: 11px;
  }

  .entry-details code {
    display: block;
    background: var(--color-bg-secondary, #1e1e1e);
    padding: 4px 8px;
    border-radius: 3px;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 100px;
    overflow-y: auto;
  }

  .entry-details .args,
  .entry-details .result,
  .entry-details .error-msg {
    margin-top: 4px;
  }

  .error-msg {
    color: #f85149;
  }

  .empty-message {
    color: var(--color-text-muted, #888);
    text-align: center;
    padding: 20px;
  }
</style>
