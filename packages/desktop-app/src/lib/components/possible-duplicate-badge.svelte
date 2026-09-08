<!--
  PossibleDuplicateBadge - Inline node-view indicator for the convergence
  "possible duplicate" marker (ADR-065 §4).

  Renders nothing unless the node this badge is attached to is a `person`
  node currently carrying `properties.person._possible_duplicate === true`
  (stamped by `NodeService::mark_possible_duplicates` or nodespace-sync's
  equivalent pulled-write handler, once an offline write or sync convergence
  produces two colliding active people). Purely informational and non-modal —
  it never blocks editing the node it's attached to, mirroring
  RecoveredItemsBadge's inline-pill-with-popover pattern.

  Clicking it re-runs the same adopt-existing lookup the creation-time
  suggestion in person-schema-form.svelte uses
  (`backendAdapter.findDuplicateFor('person', 'email', …)`) and offers the
  same "Use existing" (navigate, non-destructive) / "Dismiss" choice — no
  separate merge/resolution machinery. Dismissing only closes this popover;
  the underlying marker is not cleared (no dismiss-persistence mechanism is
  in scope — see the parent ADR's non-goals), so the badge can reappear on a
  later render if the node is still flagged.
-->

<script lang="ts">
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { getNavigationService } from '$lib/services/navigation-service';
  import { isPossibleDuplicate } from '$lib/utils/possible-duplicate';
  import { createLogger } from '$lib/utils/logger';
  import type { Node } from '$lib/types';

  const log = createLogger('PossibleDuplicateBadge');

  interface Props {
    nodeId: string;
  }

  let { nodeId }: Props = $props();

  const node = $derived(sharedNodeStore.getNode(nodeId));
  // Re-running the lookup needs to know which unique field to query — only
  // person/email wires that up on the desktop today, even though
  // the marker itself (mark_possible_duplicates) is generic across node
  // types. Scoped to person here for that reason, not a schema limitation.
  const flagged = $derived(node?.nodeType === 'person' && isPossibleDuplicate(node));

  let open = $state(false);
  let checking = $state(false);
  let match = $state<Node | null>(null);
  // Rapid open→close→reopen can start a second lookup before the first
  // resolves; a monotonic generation ensures only the MOST RECENTLY STARTED
  // check may ever write `match`/`checking`, regardless of resolve order
  // (mirrors the same fix applied to person-schema-form.svelte's checkForDuplicate).
  let checkGeneration = 0;

  function toggle(e: MouseEvent): void {
    e.stopPropagation();
    if (open) {
      close();
      return;
    }
    open = true;
    void runCheck();
  }

  function close(e?: MouseEvent): void {
    e?.stopPropagation();
    open = false;
  }

  async function runCheck(): Promise<void> {
    const generation = ++checkGeneration;
    const email = (node?.properties?.['person'] as Record<string, unknown> | undefined)?.[
      'email'
    ] as string | undefined;
    if (!email?.trim()) {
      // No await happened yet — this generation is still current by
      // construction, so no staleness check is needed here.
      match = null;
      return;
    }
    checking = true;
    try {
      const found = await backendAdapter.findDuplicateFor('person', 'email', email, nodeId);
      if (generation !== checkGeneration) return;
      match = found && found.id !== nodeId ? found : null;
    } catch (err) {
      if (generation !== checkGeneration) return;
      log.error('Duplicate re-check failed (non-blocking)', { err });
      match = null;
    } finally {
      // A superseded call's own `checking = false` must not flip the flag
      // while a NEWER check (which already set it true) is still in flight.
      if (generation === checkGeneration) checking = false;
    }
  }

  function adoptExisting(e: MouseEvent): void {
    e.stopPropagation();
    if (!match) return;
    // Deliberately non-destructive — same navigate-only action as the
    // creation-time suggestion's "Use existing"; no delete/merge.
    getNavigationService().navigateToNodeInOtherPane(match.id);
    open = false;
    match = null;
  }

  function dismiss(e: MouseEvent): void {
    e.stopPropagation();
    open = false;
    match = null;
  }

  const matchDisplayName = $derived(
    (match?.properties?.['person'] as Record<string, unknown> | undefined)?.['name'] as
      | string
      | undefined
  );
</script>

{#if flagged}
  <span class="possible-duplicate-badge-wrap">
    <button
      type="button"
      class="possible-duplicate-badge"
      onclick={toggle}
      aria-haspopup="dialog"
      aria-expanded={open}
      title="This person may be a duplicate — click to review"
    >
      <span aria-hidden="true">⚠</span>
      <span class="possible-duplicate-badge-label">Possible duplicate</span>
    </button>

    {#if open}
      <div class="possible-duplicate-popover" role="dialog" aria-label="Possible duplicate">
        {#if checking}
          <p class="possible-duplicate-status">Checking…</p>
        {:else if match}
          <p class="possible-duplicate-explain">
            A person with this email already exists{matchDisplayName
              ? `: ${matchDisplayName}`
              : ''} — use them instead?
          </p>
          <div class="possible-duplicate-actions">
            <button
              type="button"
              class="possible-duplicate-btn possible-duplicate-btn--ghost"
              onclick={dismiss}
            >
              Dismiss
            </button>
            <button
              type="button"
              class="possible-duplicate-btn possible-duplicate-btn--primary"
              onclick={adoptExisting}
            >
              Use existing
            </button>
          </div>
        {:else}
          <p class="possible-duplicate-status">No conflicting person found right now.</p>
          <div class="possible-duplicate-actions">
            <button
              type="button"
              class="possible-duplicate-btn possible-duplicate-btn--ghost"
              onclick={close}
            >
              Close
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </span>
{/if}

<style>
  .possible-duplicate-badge-wrap {
    position: relative;
    display: inline-flex;
    margin-left: 8px;
    vertical-align: middle;
  }

  .possible-duplicate-badge {
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

  .possible-duplicate-badge:hover {
    background-color: color-mix(in srgb, var(--color-warning, #f59e0b) 22%, transparent);
  }

  .possible-duplicate-badge-label {
    font-weight: 500;
  }

  .possible-duplicate-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 950;
    width: 300px;
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

  .possible-duplicate-explain,
  .possible-duplicate-status {
    font-size: 12px;
    color: var(--color-text-muted, #aaa);
    margin: 0 0 8px;
  }

  .possible-duplicate-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .possible-duplicate-btn {
    font-size: 12px;
    padding: 5px 12px;
    border-radius: 5px;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .possible-duplicate-btn--ghost {
    background: none;
    border-color: var(--color-border, #3a3a3a);
    color: var(--color-text-muted, #aaa);
  }

  .possible-duplicate-btn--ghost:hover {
    color: var(--color-text-primary, #e0e0e0);
    border-color: var(--color-text-muted, #888);
  }

  .possible-duplicate-btn--primary {
    background-color: var(--color-warning, #f59e0b);
    color: #1a1a1a;
    font-weight: 500;
  }

  .possible-duplicate-btn--primary:hover {
    filter: brightness(1.08);
  }
</style>
