<!--
  BacklinksPanel - Collapsible panel showing container-level backlinks

  Design matches Schema-Driven Property Forms:
  - Simple collapsible trigger with count and chevron
  - List of node links with icons (like node tree, but without children)
  - Clean, minimal styling

  Uses SharedNodeStore reactivity pattern:
  - mentionedIn is populated during initial root fetch (get_children_tree)
  - Data includes {id, title, nodeType} for efficient display without N+1 queries
  - Updates reactively when domain events trigger node refetch
-->

<script lang="ts">
  import { Collapsible } from 'bits-ui';
  import Icon, { type IconName } from '$lib/design/icons/icon.svelte';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';

  let { nodeId }: { nodeId: string } = $props();

  // Same reactivity pattern as main content area - $derived from store
  let node = $derived(sharedNodeStore.getNode(nodeId));
  let backlinks = $derived(node?.mentionedIn ?? []);

  // No loading state needed - data comes with the node fetch
  // If node isn't loaded yet, backlinks will be empty (handled gracefully)

  let isOpen = $state(false);

  function getNodeIcon(nodeType: string): IconName {
    const iconMap: Record<string, IconName> = {
      date: 'calendar',
      task: 'circle',
      text: 'text',
      'ai-chat': 'aiSquare'
    };
    return iconMap[nodeType] || 'text';
  }
</script>

<!-- Fixed position at bottom, expands upward -->
<div class="backlinks-panel-container">
  <Collapsible.Root bind:open={isOpen}>
    <!-- Trigger stays at bottom -->
    <Collapsible.Trigger aria-label="Toggle backlinks panel" aria-expanded={isOpen}>
      <div class="flex items-center justify-between">
        <span class="text-sm text-muted-foreground">
          Mentioned in: ({backlinks.length}
          {backlinks.length === 1 ? 'node' : 'nodes'})
        </span>

        <svg
          viewBox="0 0 16 16"
          fill="none"
          class="h-4 w-4 text-muted-foreground transition-transform duration-200"
          class:rotate-180={isOpen}
          aria-hidden="true"
        >
          <path
            d="M4 10l4-4 4 4"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>
    </Collapsible.Trigger>

    <!-- Content expands above trigger -->
    <Collapsible.Content class="backlinks-content-wrapper">
      <div class="backlinks-content">
        {#if backlinks.length > 0}
          <ul class="flex flex-col gap-1">
            {#each backlinks as backlink}
              <li>
                <a
                  href="nodespace://{backlink.id}"
                  class="flex items-center gap-2 px-4 py-2 text-sm no-underline hover:bg-muted/50 transition-colors rounded"
                >
                  <Icon name={getNodeIcon(backlink.nodeType)} size={16} />
                  <span class="flex-1 truncate">
                    {backlink.title || backlink.id}
                  </span>
                </a>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </Collapsible.Content>
  </Collapsible.Root>
</div>

<style>
  .backlinks-panel-container {
    /* Push to bottom of flex parent and stay there */
    margin-top: auto;
    flex-shrink: 0;
    background: hsl(var(--background));
    display: flex;
    flex-direction: column-reverse; /* Reverse: trigger renders at bottom, content above */
    padding: 0 var(--viewer-padding-horizontal);
  }

  .backlinks-panel-container :global([data-collapsible-trigger]) {
    width: calc(100% + (var(--viewer-padding-horizontal) * 2)); /* Extend to container edges */
    margin-left: calc(-1 * var(--viewer-padding-horizontal)); /* Break out of container padding */
    padding: 0.75rem var(--viewer-padding-horizontal); /* Match container padding for content alignment */
    border-top: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    cursor: pointer;
    transition: opacity 0.2s;
  }

  .backlinks-panel-container :global([data-collapsible-trigger]:hover) {
    opacity: 0.8;
  }

  .backlinks-content {
    height: var(--backlinks-panel-height);
    overflow-y: auto;
    background: hsl(var(--background));
  }

  .rotate-180 {
    transform: rotate(180deg);
  }
</style>
