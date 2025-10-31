<!--
  PaneManager Component

  Manages multiple side-by-side panes, each with its own tab system.

  Features:
  - Maximum 2 panes supported (side-by-side vertical split)
  - Each pane has its own TabSystem with separate tab bar
  - Draggable resize handle between panes
  - Active pane tracking (clicking in pane makes it active)
  - Auto-close empty panes when last tab closed

  Resize Handle:
  - 4px wide hit area for dragging
  - Minimum 200px per pane enforced
  - Smooth drag experience with cursor feedback
  - Maintains 100% total width

  Integration:
  - Uses navigation store (tabState, setActivePane, resizePane)
  - Slot pattern for content with activeTab prop per pane
  - Passes through tab events to store

  Usage:
  <PaneManager>
    Snippet: children({ activeTab })
      Content based on activeTab
  </PaneManager>
-->
<script lang="ts">
  import { tabState, setActivePane, resizePane } from '$lib/stores/navigation.js';
  import TabSystem from './tab-system.svelte';
  import { cn } from '$lib/utils.js';
  import type { Tab } from '$lib/stores/navigation.js';
  import type { Snippet } from 'svelte';

  // Props
  // eslint-disable-next-line no-unused-vars
  let { children }: { children?: Snippet<[{ activeTab: Tab | undefined }]> } = $props();

  // Resize state
  let resizing = $state(false);
  let startX = $state(0);
  let startWidth = $state(0);
  let containerWidth = $state(0);
  let containerElement: HTMLElement | null = $state(null);

  // Start resize operation
  function handleResizeStart(event: MouseEvent): void {
    if (!containerElement) return;

    resizing = true;
    startX = event.clientX;
    containerWidth = containerElement.clientWidth;

    // Get first pane's current width in pixels
    const firstPane = $tabState.panes[0];
    if (firstPane) {
      startWidth = (firstPane.width / 100) * containerWidth;
    }

    // Prevent text selection during drag
    event.preventDefault();
  }

  // Handle resize drag movement
  function handleResizeMove(event: MouseEvent): void {
    if (!resizing || !containerElement) return;

    const delta = event.clientX - startX;
    const newWidth = startWidth + delta;

    // Calculate minimum width in pixels (200px)
    const minWidth = 200;
    const maxWidth = containerWidth - minWidth;

    // Enforce minimum widths
    if (newWidth < minWidth || newWidth > maxWidth) {
      return;
    }

    // Convert to percentage and update first pane
    const newWidthPercent = (newWidth / containerWidth) * 100;
    const firstPaneId = $tabState.panes[0].id;
    resizePane(firstPaneId, newWidthPercent);
  }

  // End resize operation
  function handleResizeEnd(): void {
    resizing = false;
  }

  // Get tabs for a specific pane
  function getTabsForPane(paneId: string) {
    return $tabState.tabs.filter((tab) => tab.paneId === paneId);
  }

  // Get active tab ID for a specific pane
  function getActiveTabId(paneId: string): string {
    return $tabState.activeTabIds[paneId] || '';
  }
</script>

<!-- Window-level mouse event handlers for resize -->
<svelte:window on:mousemove={handleResizeMove} on:mouseup={handleResizeEnd} />

<div class="pane-manager" bind:this={containerElement}>
  {#each $tabState.panes as pane, index (pane.id)}
    {@const activeTabId = getActiveTabId(pane.id)}

    <!-- Pane wrapper -->
    <div
      class={cn('pane', pane.id === $tabState.activePaneId && 'pane--active')}
      style="width: {pane.width}%"
      data-pane-id={pane.id}
    >
      <!-- Clickable overlay to activate pane -->
      <button
        class="pane-activator"
        onclick={() => setActivePane(pane.id)}
        aria-label="Activate Pane {index + 1}"
        type="button"
      ></button>

      <!-- Each pane gets its own TabSystem -->
      <TabSystem tabs={getTabsForPane(pane.id)} {activeTabId} {pane}>
        {#snippet children({ activeTab: tabForPane })}
          <!-- Render content for this pane -->
          {#if children}
            {@render children({ activeTab: tabForPane })}
          {/if}
        {/snippet}
      </TabSystem>
    </div>

    <!-- Resize handle between panes -->
    {#if index < $tabState.panes.length - 1}
      <button
        class={cn('resize-handle', resizing && 'resize-handle--active')}
        aria-label="Resize panes - drag to adjust width"
        onmousedown={handleResizeStart}
        type="button"
      ></button>
    {/if}
  {/each}
</div>

<style>
  .pane-manager {
    display: flex;
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    position: relative;
    background-color: hsl(var(--content-background));
    /* Subtle border to indicate pane boundary */
    border-right: 1px solid hsl(var(--border));
  }

  /* Remove border from last pane */
  .pane:last-child {
    border-right: none;
  }

  /* Active pane visual indicator */
  .pane--active {
    /* Subtle highlight to show which pane is active */
    box-shadow: inset 0 0 0 1px hsl(var(--ring) / 0.2);
  }

  /* Invisible button overlay to activate pane when clicked */
  .pane-activator {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
    z-index: 0;
    /* Allow clicks to pass through to child elements */
    pointer-events: none;
  }

  /* Resize handle */
  .resize-handle {
    width: 4px;
    background-color: hsl(var(--border));
    cursor: col-resize;
    position: relative;
    flex-shrink: 0;
    transition: background-color 0.15s ease;
    /* Ensure handle is above pane content */
    z-index: 10;
    /* Button reset */
    border: none;
    padding: 0;
    height: 100%;
    outline: none;
  }

  .resize-handle:hover,
  .resize-handle--active {
    background-color: hsl(var(--ring));
  }

  /* Increase hit area for easier grabbing */
  .resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    left: -4px;
    right: -4px;
    bottom: 0;
    /* Invisible but extends hit area */
  }

  /* Prevent text selection during resize */
  .pane-manager:has(.resize-handle--active) {
    user-select: none;
  }

  /* Apply col-resize cursor to body when resizing */
  .pane-manager:has(.resize-handle--active) * {
    cursor: col-resize !important;
  }
</style>
