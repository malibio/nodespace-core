<script lang="ts">
  import { recoveredItems } from '$lib/stores/recovered-items.svelte';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('RecoveredItemsBadge');

  interface Props {
    nodeId: string;
  }

  let { nodeId }: Props = $props();

  // Reactive: the preserved superseded edit for this node, if any. When undefined
  // (the common case — community build, or no conflict on this node) the component
  // renders nothing and is fully inert.
  const item = $derived(recoveredItems.itemFor(nodeId));

  let open = $state(false);

  function toggle(e: MouseEvent): void {
    e.stopPropagation();
    open = !open;
  }

  function close(e?: MouseEvent): void {
    e?.stopPropagation();
    open = false;
  }

  async function restore(e: MouseEvent): Promise<void> {
    e.stopPropagation();
    const current = item;
    if (!current) return;
    // Re-apply the superseded content as a normal local edit: updates the UI
    // reactively, persists immediately, and (in Pro) pushes to the cloud where it
    // wins LWW because it is now the newest write.
    sharedNodeStore.updateNode(
      nodeId,
      { content: current.superseded_content },
      { type: 'database', reason: 'recovered-item-restore' },
      { persist: 'immediate' }
    );
    log.info('Restored superseded content', { nodeId });
    open = false;
    await recoveredItems.dismiss(nodeId);
  }

  async function dismiss(e: MouseEvent): Promise<void> {
    e.stopPropagation();
    open = false;
    await recoveredItems.dismiss(nodeId);
  }
</script>

{#if item}
  <span class="recovered-badge-wrap">
    <button
      type="button"
      class="recovered-badge"
      onclick={toggle}
      aria-haspopup="dialog"
      aria-expanded={open}
      title="A conflicting edit to this node was superseded by sync — click to review or restore"
    >
      <span aria-hidden="true">⟲</span>
      <span class="recovered-badge-label">Recovered</span>
    </button>

    {#if open}
      <div class="recovered-popover" role="dialog" aria-label="Recovered edit">
        <div class="recovered-popover-header">
          <span>Superseded edit recovered</span>
          <button
            type="button"
            class="recovered-popover-x"
            onclick={close}
            aria-label="Close">×</button
          >
        </div>
        <p class="recovered-popover-explain">
          Sync replaced your edit with a newer one from another device. Your version was kept here.
        </p>
        <div class="recovered-section">
          <span class="recovered-section-label">Your edit (superseded)</span>
          <div class="recovered-content recovered-content--mine">{item.superseded_content}</div>
        </div>
        <div class="recovered-section">
          <span class="recovered-section-label">Current (kept by sync)</span>
          <div class="recovered-content">{item.winning_content}</div>
        </div>
        <div class="recovered-popover-actions">
          <button type="button" class="recovered-btn recovered-btn--ghost" onclick={dismiss}>
            Dismiss
          </button>
          <button type="button" class="recovered-btn recovered-btn--primary" onclick={restore}>
            Restore my edit
          </button>
        </div>
      </div>
    {/if}
  </span>
{/if}

<style>
  .recovered-badge-wrap {
    position: relative;
    display: inline-flex;
    margin-left: 8px;
    vertical-align: middle;
  }

  .recovered-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--color-warning, #f59e0b);
    background-color: color-mix(in srgb, var(--color-warning, #f59e0b) 12%, transparent);
    border: 1px solid var(--color-warning, #f59e0b);
    border-radius: 10px;
    cursor: pointer;
    user-select: none;
  }

  .recovered-badge:hover {
    background-color: color-mix(in srgb, var(--color-warning, #f59e0b) 22%, transparent);
  }

  .recovered-badge-label {
    font-weight: 500;
  }

  .recovered-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 950;
    width: 320px;
    max-width: 80vw;
    padding: 12px;
    background-color: var(--color-surface-2, #2a2a2a);
    color: var(--color-text-primary, #e0e0e0);
    border: 1px solid var(--color-warning, #f59e0b);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    cursor: default;
    text-align: left;
    white-space: normal;
  }

  .recovered-popover-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 6px;
  }

  .recovered-popover-x {
    background: none;
    border: none;
    color: var(--color-text-muted, #888);
    cursor: pointer;
    font-size: 18px;
    line-height: 1;
    padding: 0 2px;
  }

  .recovered-popover-x:hover {
    color: var(--color-text-primary, #e0e0e0);
  }

  .recovered-popover-explain {
    font-size: 12px;
    color: var(--color-text-muted, #aaa);
    margin: 0 0 8px;
  }

  .recovered-section {
    margin-bottom: 8px;
  }

  .recovered-section-label {
    display: block;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-text-muted, #888);
    margin-bottom: 2px;
  }

  .recovered-content {
    font-size: 12px;
    padding: 6px 8px;
    background-color: var(--color-surface-1, #1f1f1f);
    border: 1px solid var(--color-border, #3a3a3a);
    border-radius: 4px;
    max-height: 120px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .recovered-content--mine {
    border-color: var(--color-warning, #f59e0b);
  }

  .recovered-popover-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }

  .recovered-btn {
    font-size: 12px;
    padding: 5px 12px;
    border-radius: 5px;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .recovered-btn--ghost {
    background: none;
    border-color: var(--color-border, #3a3a3a);
    color: var(--color-text-muted, #aaa);
  }

  .recovered-btn--ghost:hover {
    color: var(--color-text-primary, #e0e0e0);
    border-color: var(--color-text-muted, #888);
  }

  .recovered-btn--primary {
    background-color: var(--color-warning, #f59e0b);
    color: #1a1a1a;
    font-weight: 500;
  }

  .recovered-btn--primary:hover {
    filter: brightness(1.08);
  }
</style>
