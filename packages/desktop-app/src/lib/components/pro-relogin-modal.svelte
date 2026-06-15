<!--
  Pro-tier re-login modal (T18, #1304).

  Surfaced by the app shell when the Pro daemon reports `AUTH_REQUIRED`
  (`SyncStatusEvent.state` = 4) — i.e. the persisted refresh token could
  not be renewed, so CloudSyncService needs a fresh interactive sign-in.

  Purely presentational: the shell owns the visibility + the actions.
  - "Sign In Again" kicks off the PKCE flow (`pro_initiate_oauth`); the
    daemon opens the browser and the modal closes as `sync:status`
    transitions away from `auth-required`.
  - "Work Offline" dismisses the modal for this auth-required episode.
    Local edits keep saving and replay once a session is restored, so
    nothing is lost by staying offline.
-->

<script lang="ts">
  import { focusTrap } from '$lib/actions/focus-trap';

  interface Props {
    /** Whether the modal is visible. */
    open?: boolean;
    /** Free-form daemon detail (e.g. the refresh error); shown if present. */
    detail?: string;
    /** True while an InitiateOAuth call is in flight — disables the buttons. */
    pending?: boolean;
    /** Start a fresh interactive sign-in. */
    onSignIn: () => void;
    /** Keep working without syncing for now. */
    onWorkOffline: () => void;
  }

  let { open = false, detail = '', pending = false, onSignIn, onWorkOffline }: Props = $props();
</script>

{#if open}
  <div class="relogin-overlay" onclick={onWorkOffline} role="presentation" tabindex="-1">
    <div
      class="relogin-content"
      use:focusTrap={{ onEscape: onWorkOffline }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      aria-labelledby="relogin-title"
      aria-describedby="relogin-body"
      aria-modal="true"
      tabindex="0"
    >
      <h2 id="relogin-title">Sign-in required</h2>
      <p id="relogin-body" class="relogin-body">
        Your NodeSpace Pro session expired and couldn't be renewed automatically. Sign in again to
        keep syncing across your devices — or keep working offline. Your changes are saved locally
        and will sync once you're signed back in.
      </p>

      {#if detail}
        <p class="relogin-detail">{detail}</p>
      {/if}

      <div class="relogin-actions">
        <button class="btn btn-secondary" type="button" onclick={onWorkOffline} disabled={pending}>
          Work Offline
        </button>
        <button class="btn btn-primary" type="button" onclick={onSignIn} disabled={pending}>
          {pending ? 'Opening…' : 'Sign In Again'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .relogin-overlay {
    position: fixed;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .relogin-content {
    background: var(--surface-1, #ffffff);
    color: var(--text-primary, #1f2937);
    border-radius: 8px;
    padding: 24px;
    width: min(440px, calc(100vw - 48px));
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .relogin-content h2 {
    margin: 0 0 12px 0;
    font-size: 20px;
  }

  .relogin-body {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    color: var(--text-secondary, #4b5563);
  }

  .relogin-detail {
    margin: 12px 0 0 0;
    padding: 8px 10px;
    background: var(--surface-2, #f3f4f6);
    border-radius: 4px;
    font-size: 12px;
    color: var(--text-secondary, #6b7280);
    word-break: break-word;
  }

  .relogin-actions {
    display: flex;
    gap: 12px;
    justify-content: flex-end;
    margin-top: 24px;
  }

  .btn {
    padding: 10px 18px;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }

  .btn:disabled {
    cursor: default;
    opacity: 0.7;
  }

  .btn-primary {
    background-color: #2563eb;
    color: #ffffff;
  }

  .btn-primary:hover:not(:disabled) {
    background-color: #1d4ed8;
  }

  .btn-secondary {
    background-color: var(--surface-2, #e5e7eb);
    color: var(--text-primary, #1f2937);
  }

  .btn-secondary:hover:not(:disabled) {
    background-color: var(--surface-3, #d1d5db);
  }

  @media (prefers-color-scheme: dark) {
    .relogin-content {
      background: var(--surface-1, #1f2937);
      color: var(--text-primary, #e5e7eb);
    }
    .btn-secondary {
      background-color: var(--surface-2, #374151);
      color: var(--text-primary, #e5e7eb);
    }
    .btn-secondary:hover:not(:disabled) {
      background-color: var(--surface-3, #4b5563);
    }
  }
</style>
