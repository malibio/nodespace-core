<!--
  NodeRefInline Component

  Renders a bare `[[node-id]]` wikilink (a valid UUID or date id) that arrived
  from outside the editor — imported docs, agent-written content, hand-edited
  text — as a clickable node reference showing the target node's title.

  Emits the same `<a href="nodespace://{id}" class="ns-noderef ns-noderef-valid">`
  markup the other node references use, so the document-level click handler in
  app-shell.svelte (handleLinkClick) navigates it exactly like every other
  nodespace:// link — no dedicated click path.

  Render-only: it never mutates stored content. Only valid ids ever reach this
  component (the splitter gates on id validity), so an absent node here means the
  target genuinely does not exist; we fall back to the literal `[[id]]` text.
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import { createLogger } from '$lib/utils/logger';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';

  const log = createLogger('NodeRefInline');

  let { id }: { id: string } = $props();

  // Reactive read of the store (SvelteMap): the title updates once the node is
  // present, whether it was already cached or loaded by the on-mount fetch.
  let node = $derived(sharedNodeStore.getNode(id));

  // Set once the on-mount resolution has settled; drives the not-found fallback.
  // Written only from the fetch callback, never from an effect.
  let resolved = $state(false);

  // One-shot load on mount if the node isn't already cached. ensureNode is
  // cache-first and synthesizes virtual date nodes, so valid date ids always
  // resolve; a UUID with no backing row resolves to undefined (not found).
  onMount(() => {
    if (sharedNodeStore.getNode(id)) {
      resolved = true;
      return;
    }
    sharedNodeStore
      .ensureNode(id)
      .catch((error) => {
        log.warn(`Failed to resolve node reference ${id}:`, error);
      })
      .finally(() => {
        resolved = true;
      });
  });

  // Prefer the indexed title, then the first content line, then the raw id.
  let title = $derived(
    node?.title?.trim() || node?.content?.split('\n')[0]?.trim() || id
  );

  // Node genuinely absent after the fetch settled → render the literal token.
  let notFound = $derived(resolved && !node);
</script>

{#if notFound}
  <span class="ns-noderef-missing" title="Unknown node: {id}">[[{id}]]</span>
{:else}
  <a href="nodespace://{id}" class="ns-noderef ns-noderef-valid" data-node-id={id}>{title}</a>
{/if}

<style>
  /* Muted, non-link styling for a wikilink whose target does not exist. */
  .ns-noderef-missing {
    color: hsl(var(--muted-foreground));
    text-decoration: underline dotted;
    text-underline-offset: 2px;
    cursor: default;
  }
</style>
