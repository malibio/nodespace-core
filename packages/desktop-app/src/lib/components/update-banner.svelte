<script lang="ts">
  /**
   * Non-blocking "an update is available" banner. Renders only when the update
   * store says a newer version exists and the user hasn't dismissed it. "Download"
   * opens the release page (the app ships no in-app installer — see
   * `update-status.svelte.ts`); dismissal is per-version.
   */
  import { updateStatus } from '$lib/stores/update-status.svelte';

  let downloading = $state(false);

  async function onDownload() {
    downloading = true;
    try {
      await updateStatus.download();
    } finally {
      downloading = false;
    }
  }
</script>

{#if updateStatus.showBanner}
  <div class="update-banner" role="status" aria-live="polite">
    <span class="msg">
      NodeSpace <strong>{updateStatus.latest}</strong> is available
      <span class="cur">(you have {updateStatus.current})</span>
    </span>
    <span class="actions">
      <button class="download" onclick={onDownload} disabled={downloading}>
        {downloading ? 'Opening…' : 'Download'}
      </button>
      <button class="dismiss" onclick={() => updateStatus.dismiss()} aria-label="Dismiss update notice">
        Later
      </button>
    </span>
  </div>
{/if}

<style>
  .update-banner {
    position: fixed;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1000;
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: calc(100vw - 32px);
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--color-surface-raised, #1f242c);
    color: var(--color-text, #e6e9ef);
    border: 1px solid var(--color-border, #333b47);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.28);
    font-size: 13px;
  }
  .msg {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cur {
    color: var(--color-text-muted, #9aa4b2);
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  button {
    font: inherit;
    padding: 4px 12px;
    border-radius: 6px;
    cursor: pointer;
    border: 1px solid transparent;
  }
  .download {
    background: var(--color-accent, #3b82f6);
    color: #fff;
  }
  .download:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .download:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .dismiss {
    background: transparent;
    color: var(--color-text-muted, #9aa4b2);
    border-color: var(--color-border, #333b47);
  }
  .dismiss:hover {
    color: var(--color-text, #e6e9ef);
  }
</style>
