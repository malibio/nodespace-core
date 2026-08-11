<script lang="ts">
  import { getSubtreeAccessDeniedState } from '$lib/services/subtree-access-denied.svelte';
  import { focusTrap } from '$lib/actions/focus-trap';

  const refusal = getSubtreeAccessDeniedState();
</script>

{#if refusal.pending}
  <div class="overlay" role="none" onclick={refusal.dismiss} tabindex="-1">
    <div
      class="modal"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="subtree-access-denied-title"
      aria-describedby="subtree-access-denied-desc"
      use:focusTrap={{ onEscape: refusal.dismiss }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="0"
    >
      <h2 id="subtree-access-denied-title">Delete aborted</h2>
      <p id="subtree-access-denied-desc">
        {refusal.pending.inaccessibleCount}
        {refusal.pending.inaccessibleCount === 1 ? 'item' : 'items'}
        in this delete
        {refusal.pending.inaccessibleCount === 1 ? 'is' : 'are'}
        not visible to you — nothing was deleted.
      </p>
      <div class="actions">
        <button class="btn-ok" onclick={refusal.dismiss}>OK</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--color-surface, #1e1e1e);
    border: 1px solid var(--color-border, #333);
    border-radius: 8px;
    padding: 24px;
    max-width: 360px;
    width: 100%;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary, #e0e0e0);
  }

  p {
    margin: 0 0 20px;
    font-size: 13px;
    color: var(--color-text-secondary, #888);
  }

  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  button {
    padding: 7px 16px;
    border-radius: 6px;
    border: none;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-ok {
    background: var(--color-surface-elevated, #2a2a2a);
    color: var(--color-text-primary, #e0e0e0);
  }

  .btn-ok:hover {
    background: var(--color-surface-hover, #333);
  }
</style>
