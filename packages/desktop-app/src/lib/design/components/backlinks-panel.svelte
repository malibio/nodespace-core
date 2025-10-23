<!--
  BacklinksPanel - Collapsible panel showing container-level backlinks

  Features:
  - Displays containers that mention the current node
  - Collapsible accordion-style design (matches Schema-Driven Property Forms)
  - Automatic deduplication (multiple mentions from same container shown once)
  - Navigation via nodespace:// links (handled by global app-shell handler)
  - Loading, empty, and error states

  Design:
  - Uses bits-ui Collapsible component
  - Chevron on right (rotates 180° when open)
  - Shows count badge in trigger
  - Card-style list of backlink containers
-->

<script lang="ts">
  import { Collapsible } from 'bits-ui';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { sharedNodeStore } from '$lib/services/shared-node-store';
  import { onMount } from 'svelte';
  import type { Node } from '$lib/types/node';

  let { nodeId }: { nodeId: string } = $props();

  let backlinks = $state<Node[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let isOpen = $state(true);

  onMount(async () => {
    try {
      // Get container IDs that mention this node
      const containerIds = await backendAdapter.getMentioningContainers(nodeId);

      // Fetch full node data for each container
      backlinks = containerIds
        .map((id: string) => sharedNodeStore.getNode(id))
        .filter((node: Node | undefined): node is Node => node !== undefined);
    } catch (err) {
      console.error('Failed to load backlinks:', err);
      error = err instanceof Error ? err.message : 'Failed to load backlinks';
    } finally {
      loading = false;
    }
  });

  function getPreviewText(content: string): string {
    // Extract first 100 chars, strip markdown formatting
    return content
      .replace(/\[@[^\]]+\]\([^)]+\)/g, '') // Remove mention links
      .replace(/[#*_`]/g, '') // Remove basic markdown
      .trim()
      .substring(0, 100);
  }
</script>

<div class="backlinks-panel">
  <Collapsible.Root bind:open={isOpen}>
    <Collapsible.Trigger>
      <div class="backlinks-trigger">
        <div class="backlinks-header">
          <span class="backlinks-title">Mentioned by</span>
          {#if !loading && backlinks.length > 0}
            <span class="backlinks-count">{backlinks.length}</span>
          {/if}
        </div>

        <!-- Chevron on right -->
        <svg class="chevron-icon" class:rotate-180={isOpen} viewBox="0 0 16 16" fill="none">
          <path
            d="M4 6l4 4 4-4"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>
    </Collapsible.Trigger>

    <Collapsible.Content>
      <div class="backlinks-content">
        {#if loading}
          <div class="backlinks-loading">Loading backlinks...</div>
        {:else if error}
          <div class="backlinks-error">{error}</div>
        {:else if backlinks.length === 0}
          <div class="backlinks-empty">No pages mention this one yet.</div>
        {:else}
          <ul class="backlinks-list">
            {#each backlinks as backlink}
              <li class="backlink-item">
                <!-- Uses nodespace:// protocol, handled by app-shell.svelte global handler -->
                <a href="nodespace://{backlink.id}" class="backlink-link">
                  <div class="backlink-type">{backlink.nodeType}</div>
                  <div class="backlink-preview">
                    {getPreviewText(backlink.content)}
                  </div>
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
  .backlinks-panel {
    border-top: 1px solid hsl(var(--border));
    padding-top: var(--spacing-md);
    margin-top: var(--spacing-lg);
  }

  .backlinks-trigger {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-sm) 0;
    font-weight: 500;
    transition: opacity 0.15s ease;
    background: none;
    border: none;
    cursor: pointer;
    color: hsl(var(--foreground));
  }

  .backlinks-trigger:hover {
    opacity: 0.8;
  }

  .backlinks-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
  }

  .backlinks-title {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .backlinks-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.5rem;
    height: 1.5rem;
    padding: 0 var(--spacing-xs);
    font-size: var(--font-size-xs);
    font-weight: 500;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted));
    border: 1px solid hsl(var(--border));
    border-radius: 9999px;
  }

  .chevron-icon {
    width: 1rem;
    height: 1rem;
    color: hsl(var(--muted-foreground));
    transition: transform 0.2s ease;
  }

  .chevron-icon.rotate-180 {
    transform: rotate(180deg);
  }

  .backlinks-content {
    padding-bottom: var(--spacing-md);
  }

  .backlinks-loading,
  .backlinks-empty,
  .backlinks-error {
    padding: var(--spacing-sm);
    color: hsl(var(--muted-foreground));
    font-size: var(--font-size-sm);
  }

  .backlinks-error {
    color: hsl(var(--destructive));
  }

  .backlinks-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
  }

  .backlink-item {
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .backlink-link {
    display: block;
    padding: var(--spacing-sm);
    background: hsl(var(--background));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-sm);
    text-decoration: none;
    color: hsl(var(--foreground));
    transition: all 0.15s ease;
  }

  .backlink-link:hover {
    background: hsl(var(--accent));
    border-color: hsl(var(--accent-foreground) / 0.2);
  }

  .backlink-type {
    font-size: var(--font-size-xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: hsl(var(--muted-foreground));
    margin-bottom: var(--spacing-xs);
  }

  .backlink-preview {
    font-size: var(--font-size-sm);
    color: hsl(var(--foreground));
    line-height: 1.4;
  }
</style>
