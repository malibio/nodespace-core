<script lang="ts">
  import DateNodeViewer from '$lib/components/viewers/date-node-viewer.svelte';
  import TaskNodeViewer from '$lib/components/viewers/task-node-viewer.svelte';
  import TextNodeViewer from '$lib/components/viewers/text-node-viewer.svelte';
  import AIChatNodeViewer from '$lib/components/viewers/ai-chat-node-viewer.svelte';
  import { toggleTheme } from '$lib/design/theme.js';
  import { tabState } from '$lib/stores/navigation.js';
  import type { Tab } from '$lib/stores/navigation.js';

  // Subscribe to tab state from store
  $: ({ tabs, activeTabId } = $tabState);
  $: activeTab = tabs.find((t) => t.id === activeTabId);

  // Global keyboard shortcuts
  function handleKeydown(event: KeyboardEvent) {
    // Toggle theme - Cmd+\ (Mac) or Ctrl+\ (Windows/Linux) per design system
    if ((event.metaKey || event.ctrlKey) && event.key === '\\') {
      event.preventDefault();
      toggleTheme();
    }
  }

  // Helper to get tab content safely
  function getTabContent(tab: Tab | undefined): { nodeId?: string; nodeType?: string } {
    if (!tab?.content) return {};
    return tab.content;
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if activeTab}
  {#if activeTab.type === 'date'}
    <DateNodeViewer tabId={activeTab.id} />
  {:else if activeTab.type === 'node'}
    {@const content = getTabContent(activeTab)}
    {#if content.nodeType === 'task'}
      <TaskNodeViewer nodeId={content.nodeId || ''} />
    {:else if content.nodeType === 'text'}
      <TextNodeViewer nodeId={content.nodeId || ''} />
    {:else if content.nodeType === 'ai-chat'}
      <AIChatNodeViewer nodeId={content.nodeId || ''} />
    {:else}
      <div class="error-message">
        <h2>Unknown Node Type</h2>
        <p>Node type "{content.nodeType}" is not supported.</p>
      </div>
    {/if}
  {:else if activeTab.type === 'placeholder'}
    <div class="placeholder-content">
      <h2>{activeTab.title}</h2>
      <p>This is a placeholder tab. Content will be implemented later.</p>
    </div>
  {:else}
    <div class="error-message">
      <h2>Unknown Tab Type</h2>
      <p>Tab type "{activeTab.type}" is not supported.</p>
    </div>
  {/if}
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
    margin: 0;
    color: hsl(var(--muted-foreground));
  }

  /* Error message styling */
  .error-message {
    padding: 2rem;
    text-align: center;
    color: hsl(var(--destructive));
  }

  .error-message h2 {
    margin: 0 0 1rem 0;
  }

  .error-message p {
    margin: 0;
    color: hsl(var(--destructive) / 0.8);
  }
</style>
