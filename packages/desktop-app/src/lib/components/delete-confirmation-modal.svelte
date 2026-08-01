<script lang="ts">
  import { getDeleteConfirmationState } from '$lib/services/delete-confirmation.svelte';
  import { focusTrap } from '$lib/actions/focus-trap';

  const confirmation = getDeleteConfirmationState();
  // Keyboard handling lives elsewhere: focusTrap owns Escape + Tab and lands
  // initial focus on Cancel, and each button activates on its own Enter
  // natively. Deliberately NO global Enter→confirm handler — with focus
  // defaulting to Cancel, that would make Enter delete the node while the
  // highlighted control says Cancel, on a dialog that warns "cannot be undone".
</script>

{#if confirmation.pending}
  <div
    class="overlay"
    role="none"
    onclick={confirmation.cancel}
    tabindex="-1"
  >
    <div
      class="modal"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="delete-modal-title"
      aria-describedby="delete-modal-desc"
      use:focusTrap={{ onEscape: confirmation.cancel }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="0"
    >
      <h2 id="delete-modal-title">Delete node and {confirmation.pending.descendantCount}
        {confirmation.pending.descendantCount === 1 ? 'descendant' : 'descendants'}?</h2>
      <p id="delete-modal-desc">This cannot be undone.</p>
      <div class="actions">
        <button class="btn-cancel" onclick={confirmation.cancel}>Cancel</button>
        <button class="btn-delete" onclick={confirmation.confirm}>Delete</button>
      </div>
    </div>
  </div>
{:else if confirmation.pendingRefusal}
  <div
    class="overlay"
    role="none"
    onclick={confirmation.acknowledge}
    tabindex="-1"
  >
    <div
      class="modal"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="refusal-modal-title"
      aria-describedby="refusal-modal-desc"
      use:focusTrap={{ onEscape: confirmation.acknowledge }}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="0"
    >
      <h2 id="refusal-modal-title">Can't delete this node</h2>
      <p id="refusal-modal-desc">
        This contains {confirmation.pendingRefusal.inaccessibleCount}
        {confirmation.pendingRefusal.inaccessibleCount === 1 ? 'item' : 'items'} you don't have access to.
        Nothing was deleted.
      </p>
      <div class="actions">
        <button class="btn-cancel" onclick={confirmation.acknowledge}>OK</button>
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

  .btn-cancel {
    background: var(--color-surface-elevated, #2a2a2a);
    color: var(--color-text-primary, #e0e0e0);
  }

  .btn-cancel:hover {
    background: var(--color-surface-hover, #333);
  }

  .btn-delete {
    background: #c0392b;
    color: #fff;
  }

  .btn-delete:hover {
    background: #a93226;
  }
</style>
