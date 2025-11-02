<script lang="ts">
  import { setContext } from 'svelte';
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';
  import { tabState, updateTabTitle, updateTabContent } from '$lib/stores/navigation.js';
  import { pluginRegistry } from '$lib/plugins/plugin-registry';
  import type { Pane } from '$lib/stores/navigation.js';

  // ✅ Receive the PANE as a prop - each pane instance gets its own pane object
  let { pane }: { pane: Pane } = $props();

  // Set paneId in context so all descendant components can access it
  // This avoids prop threading through all component layers
  setContext('paneId', pane.id);

  // Derive tab state using Svelte 5 $derived
  // KEY FIX: Use pane.id instead of global $tabState.activePaneId
  const tabs = $derived($tabState.tabs);
  const activeTabId = $derived($tabState.activeTabIds[pane.id]); // ✅ Use THIS pane's ID
  const activeTab = $derived(tabs.find((t) => t.id === activeTabId));

  // Track loaded viewer components by nodeType
  let viewerComponents = $state<Map<string, unknown>>(new Map());

  // Load viewer for active tab's node type
  $effect(() => {
    const nodeType = activeTab?.content?.nodeType;
    if (nodeType && !viewerComponents.has(nodeType)) {
      (async () => {
        const viewer = await pluginRegistry.getViewer(nodeType);
        if (viewer) {
          viewerComponents.set(nodeType, viewer);
          viewerComponents = new Map(viewerComponents); // Trigger reactivity
        }
      })();
    }
  });
</script>

{#if activeTab?.content}
  <!-- Dynamic viewer routing via plugin registry -->
  <!-- Falls back to BaseNodeViewer if no custom viewer registered -->
  {@const content = activeTab.content}
  {@const ViewerComponent = (viewerComponents.get(content.nodeType ?? 'text') ??
    BaseNodeViewer) as typeof BaseNodeViewer}

  <!-- KEY FIX: Use {#key} to force separate component instances per pane+nodeId -->
  <!-- This ensures each pane gets its own BaseNodeViewer instance with isolated state -->
  {#key `${pane.id}-${content.nodeId}`}
    <ViewerComponent
      nodeId={content.nodeId}
      onTitleChange={(title: string) => updateTabTitle(activeTabId, title)}
      onNodeIdChange={(newNodeId: string) =>
        updateTabContent(activeTabId, { nodeId: newNodeId, nodeType: content.nodeType })}
    />
  {/key}
{:else if activeTab}
  <!-- Placeholder content for tabs without node content -->
  <div class="placeholder-content">
    <h2>{activeTab.title}</h2>
    <p>This is a placeholder tab. Content will be implemented later.</p>
  </div>
{:else}
  <!-- No active tab -->
  <div class="empty-state">
    <p>No tab selected</p>
  </div>
{/if}

<style>
  /* Placeholder content */
  .placeholder-content {
    padding: 2rem;
    text-align: center;
  }

  .placeholder-content h2 {
    margin: 0 0 1rem 0;
    color: hsl(var(--foreground));
  }

  .placeholder-content p {
    margin: 0.5rem 0;
    color: hsl(var(--muted-foreground));
  }

  /* Empty state */
  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: hsl(var(--muted-foreground));
  }
</style>
