<!--
  Pro-tier sync-status pill. Renders only when the daemon answers
  the `nodespace.pro.v1.CloudSyncService` probe; community mode
  hides it entirely.

  The visual contract: pill color follows the orchestrator's state
  (`SyncStatusEvent.state`):
    grey       — DISCONNECTED / UNSPECIFIED
    amber      — CONNECTING / AUTHENTICATING / SYNCING
    blue       — AUTH_REQUIRED (action needed)
    green      — CONNECTED
    red        — ERROR

  Click: when the daemon is signed out (DISCONNECTED, AUTH_REQUIRED,
  UNSPECIFIED, ERROR), clicking triggers the PKCE flow via
  `pro_initiate_oauth`. The daemon opens the browser; this UI just
  watches `sync:status` for the resulting transitions.
-->

<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { proSync, type SyncState } from '$lib/stores/pro-sync.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('ProSyncPill');

  const labels: Record<SyncState, string> = {
    'unspecified': 'Sign in',
    'disconnected': 'Sign in',
    'connecting': 'Connecting…',
    'authenticating': 'Signing in…',
    'auth-required': 'Sign in required',
    'syncing': 'Syncing…',
    'connected': 'Synced',
    'error': 'Retry sign-in',
  };

  const tones: Record<SyncState, string> = {
    'unspecified': 'grey',
    'disconnected': 'grey',
    'connecting': 'amber',
    'authenticating': 'amber',
    'auth-required': 'blue',
    'syncing': 'amber',
    'connected': 'green',
    'error': 'red',
  };

  // States where clicking should kick off a fresh sign-in attempt.
  const SIGN_IN_STATES: SyncState[] = [
    'unspecified',
    'disconnected',
    'auth-required',
    'error',
  ];

  // While an InitiateOAuth call is in flight, disable the pill so a
  // double-click doesn't spawn two browser windows.
  let pending = $state(false);

  async function onClick() {
    if (pending) return;
    if (!SIGN_IN_STATES.includes(proSync.state)) return;
    pending = true;
    try {
      const attemptId = await invoke<string>('pro_initiate_oauth', {
        // workerUrl + userHint omitted — backend defaults apply.
      });
      log.info('PKCE attempt started', { attemptId });
    } catch (e) {
      log.warn('pro_initiate_oauth failed', { error: e });
    } finally {
      pending = false;
    }
  }

  let clickable = $derived(SIGN_IN_STATES.includes(proSync.state));
</script>

{#if proSync.isPro}
  <button
    class="pro-sync-pill"
    class:clickable
    data-tone={tones[proSync.state]}
    title={proSync.detail || labels[proSync.state]}
    type="button"
    aria-label="NodeSpace Pro sync status: {labels[proSync.state]}"
    disabled={pending || !clickable}
    onclick={onClick}
  >
    <span class="dot" aria-hidden="true"></span>
    <span class="label">{labels[proSync.state]}</span>
  </button>
{/if}

<style>
  .pro-sync-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid var(--border-color, #d1d5db);
    background: var(--surface-1, #f9fafb);
    color: var(--text-primary, #1f2937);
    font-size: 12px;
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
  }

  .pro-sync-pill:hover:not(:disabled) {
    background: var(--surface-2, #f3f4f6);
  }

  .pro-sync-pill:disabled {
    cursor: default;
    opacity: 0.85;
  }

  .pro-sync-pill:not(.clickable) {
    cursor: default;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #9ca3af;
  }

  .pro-sync-pill[data-tone='amber'] .dot {
    background: #f59e0b;
    box-shadow: 0 0 0 2px rgba(245, 158, 11, 0.18);
  }
  .pro-sync-pill[data-tone='blue'] .dot {
    background: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.18);
  }
  .pro-sync-pill[data-tone='green'] .dot {
    background: #16a34a;
    box-shadow: 0 0 0 2px rgba(22, 163, 74, 0.18);
  }
  .pro-sync-pill[data-tone='red'] .dot {
    background: #dc2626;
    box-shadow: 0 0 0 2px rgba(220, 38, 38, 0.18);
  }

  @media (prefers-color-scheme: dark) {
    .pro-sync-pill {
      background: var(--surface-1, #1f2937);
      border-color: var(--border-color, #374151);
      color: var(--text-primary, #e5e7eb);
    }
    .pro-sync-pill:hover {
      background: var(--surface-2, #374151);
    }
  }
</style>
