<!--
  DateNodeViewer - Page-level viewer that wraps BaseNodeViewer with date navigation

  Features:
  - Date navigation header with professional styling
  - Previous/next day functionality
  - Keyboard support for arrow key navigation
  - Database integration for persistent daily journal entries
  - Lazy date node creation when first child is added
  - Debounced saving (500ms) for user input
  - Seamless integration with existing BaseNodeViewer
  - Clean date formatting (e.g., "September 7, 2025")

  Follows the *NodeViewer pattern (like TaskNodeViewer) for page-level viewers.
-->

<script lang="ts">
  import BaseNodeViewer from '$lib/design/components/base-node-viewer.svelte';
  import Icon from '$lib/design/icons/icon.svelte';
  import { updateTabTitle, getDateTabTitle } from '$lib/stores/navigation.js';
  import NodeServiceContext, { getNodeServices } from '$lib/contexts/node-service-context.svelte';
  import { v4 as uuidv4 } from 'uuid';

  // Props using Svelte 5 runes mode
  let { tabId = 'today' }: { tabId?: string } = $props();

  // Get services from context
  const services = getNodeServices();
  if (!services) {
    throw new Error('NodeServices not available in DateNodeViewer');
  }

  const { nodeManager, databaseService } = services;

  // Normalize date to midnight local time (ignore time components)
  function normalizeDate(d: Date): Date {
    return new Date(d.getFullYear(), d.getMonth(), d.getDate());
  }

  // Current date state - defaults to today, normalized to midnight
  let currentDate = $state(normalizeDate(new Date()));

  // Format date for display (e.g., "September 7, 2025") using Svelte 5 $derived
  const formattedDate = $derived(
    currentDate.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'long',
      day: 'numeric'
    })
  );

  // Derive current date ID from currentDate
  const currentDateId = $derived(currentDate.toISOString().split('T')[0]);

  // Map to track debounce timeouts for each node
  const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();

  // Update tab title when date changes using Svelte 5 $effect
  $effect(() => {
    if (tabId) {
      const newTitle = getDateTabTitle(currentDate);
      updateTabTitle(tabId, newTitle);
    }
  });

  // Load nodes from database when date changes
  $effect(() => {
    loadNodesForDate(currentDateId);
  });

  // Watch nodeManager for content changes and persist to database with debounce
  $effect(() => {
    // Access visibleNodes to trigger reactivity
    const nodes = nodeManager.visibleNodes;

    // Schedule persistence for each non-placeholder node
    for (const node of nodes) {
      if (node.content.trim() && !node.isPlaceholder) {
        scheduleNodePersistence(node.id, node.content);
      }
    }
  });

  /**
   * Load nodes for the current date from database
   */
  async function loadNodesForDate(dateId: string) {
    try {
      // Load children from database (already in unified Node format)
      const children = await databaseService.getChildren(dateId);

      // Initialize with loaded nodes directly (no conversion needed)
      nodeManager.initializeNodes(children, {
        expanded: true,
        autoFocus: children.length === 0,  // Auto-focus first node if empty
        inheritHeaderLevel: 0
      });

      // If no children, create an empty node as placeholder for immediate typing
      // This creates a real node in memory but won't save to DB until user types content
      if (children.length === 0) {
        // Create node: afterNodeId, content, nodeType, headerLevel, insertAtBeginning, originalContent, focusNewNode
        nodeManager.createNode(currentDateId, '', 'text', 0, true, '', true);
      }
    } catch (error) {
      console.error('[DateNodeViewer] Failed to load nodes for date:', dateId, error);
    }
  }

  /**
   * Schedule node persistence with debouncing (500ms)
   */
  function scheduleNodePersistence(nodeId: string, content: string) {
    // Clear existing timeout for this node
    const existingTimeout = saveTimeouts.get(nodeId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
    }

    // Set new debounced save
    const saveTimeout = setTimeout(async () => {
      if (content.trim()) {
        try {
          // Ensure date node exists before saving child
          const dateNode = await databaseService.getNode(currentDateId);
          if (!dateNode) {
            // Create date node lazily
            await databaseService.createNode({
              id: currentDateId,
              node_type: 'date',
              content: currentDateId,
              parent_id: null,
              root_id: null,
              before_sibling_id: null,
              properties: {}
            });
          }

          // Save or update the child node
          const existingNode = await databaseService.getNode(nodeId);
          if (existingNode) {
            await databaseService.updateNode(nodeId, { content });
          } else {
            await databaseService.createNode({
              id: nodeId,
              node_type: 'text',
              content,
              parent_id: currentDateId,
              root_id: currentDateId,
              before_sibling_id: null,
              properties: {}
            });
          }

          console.log('[DateNodeViewer] Persisted node:', nodeId, 'to date:', currentDateId);
        } catch (error) {
          console.error('[DateNodeViewer] Failed to save node:', nodeId, error);
        }
      }

      // Clean up timeout reference
      saveTimeouts.delete(nodeId);
    }, 500); // 500ms debounce

    saveTimeouts.set(nodeId, saveTimeout);
  }

  /**
   * Navigate to previous or next day
   */
  function navigateDate(direction: 'prev' | 'next') {
    const newDate = new Date(currentDate);
    if (direction === 'prev') {
      newDate.setDate(newDate.getDate() - 1);
    } else {
      newDate.setDate(newDate.getDate() + 1);
    }
    currentDate = normalizeDate(newDate);
  }

  /**
   * Handle keyboard navigation for date switching
   */
  function handleKeydown(event: KeyboardEvent) {
    // Only handle if focus is not on contenteditable elements
    const activeElement = document.activeElement as HTMLElement;
    const isTextEditor = activeElement && activeElement.contentEditable === 'true';

    if (isTextEditor) return;

    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      navigateDate('prev');
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      navigateDate('next');
    }
  }
</script>

<!-- Keyboard event listener -->
<svelte:window on:keydown={handleKeydown} />

<NodeServiceContext>
  <BaseNodeViewer>
    {#snippet header()}
      <!-- Date Navigation Header - inherits base styling from BaseNodeViewer -->
      <div class="date-nav-container">
        <div class="date-display">
          <Icon
            name="calendar"
            size={24}
            color="hsl(var(--muted-foreground))"
            className="calendar-icon"
          />
          <h1>{formattedDate}</h1>
        </div>

        <div class="date-nav-buttons">
          <button
            class="date-nav-btn"
            onclick={() => navigateDate('prev')}
            aria-label="Previous day"
          >
            <Icon name="chevronRight" size={16} color="currentColor" className="rotate-left" />
          </button>
          <button class="date-nav-btn" onclick={() => navigateDate('next')} aria-label="Next day">
            <Icon name="chevronRight" size={16} color="currentColor" />
          </button>
        </div>
      </div>
    {/snippet}
  </BaseNodeViewer>
</NodeServiceContext>

<style>
  /* Date-specific navigation styles - base header styling comes from BaseNodeViewer */
  .date-nav-container {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .date-display {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .date-nav-buttons {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .date-nav-btn {
    width: 32px;
    height: 32px;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--muted-foreground));
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .date-nav-btn:hover {
    background: hsl(var(--muted));
    color: hsl(var(--foreground));
  }

  /* Calendar icon styling */
  :global(.calendar-icon) {
    opacity: 0.8;
  }

  /* Left arrow rotation for previous button */
  :global(.rotate-left) {
    transform: rotate(180deg);
  }
</style>
