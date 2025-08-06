<!--
  NodeTree Component
  
  Hierarchical display component for parent-child node relationships.
  Supports tree indentation, expand/collapse functionality, and integrates with existing TextNode components.
-->

<script context="module" lang="ts">
  // TypeScript interfaces for hierarchical node data
  export interface HierarchicalNode {
    id: string;
    content: string;
    title?: string;
    parentId?: string;
    children: string[];
    depth: number;
    isCollapsed?: boolean;
  }

  export interface TreeState {
    expandedNodes: Set<string>;
    selectedNode: string | null;
  }
</script>

<script lang="ts">
  import { writable, type Writable } from 'svelte/store';
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '$lib/design/components/BaseNode.svelte';

  // Props
  export let nodes: HierarchicalNode[] = [];
  export let maxDepth: number = 5;
  export let autoExpand: boolean = false;
  export let className: string = '';

  // Component state
  const treeState: Writable<TreeState> = writable({
    expandedNodes: new Set(),
    selectedNode: null
  });

  let visibleNodes: HierarchicalNode[] = [];
  let nodeMap: Map<string, HierarchicalNode> = new Map();

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    nodeToggle: { nodeId: string; expanded: boolean };
    nodeSelect: { nodeId: string; node: HierarchicalNode };
    nodeUpdate: { nodeId: string; content: string; title: string };
    treeChange: { visibleNodes: HierarchicalNode[] };
  }>();

  // Initialize node map and auto-expand if needed
  $: {
    nodeMap.clear();
    nodes.forEach(node => nodeMap.set(node.id, node));
    
    if (autoExpand) {
      const rootNodes = nodes.filter(n => !n.parentId);
      rootNodes.forEach(node => expandNodeRecursively(node.id));
    }
    
    updateVisibleNodes();
  }

  // Update visible nodes based on expansion state
  function updateVisibleNodes() {
    const roots = nodes.filter(n => !n.parentId).sort((a, b) => a.title?.localeCompare(b.title || '') || a.content.localeCompare(b.content));
    const visible: HierarchicalNode[] = [];
    
    function processNode(node: HierarchicalNode, depth: number = 0) {
      if (depth > maxDepth) return;
      
      const nodeWithDepth = { ...node, depth };
      visible.push(nodeWithDepth);
      
      if ($treeState.expandedNodes.has(node.id) && node.children.length > 0) {
        const childNodes = node.children
          .map(childId => nodeMap.get(childId))
          .filter((child): child is HierarchicalNode => child !== undefined)
          .sort((a, b) => a.title?.localeCompare(b.title || '') || a.content.localeCompare(b.content));
        
        childNodes.forEach(child => processNode(child, depth + 1));
      }
    }
    
    roots.forEach(root => processNode(root));
    visibleNodes = visible;
    
    dispatch('treeChange', { visibleNodes });
  }

  // Toggle node expansion
  function toggleNode(nodeId: string) {
    treeState.update(state => {
      const newExpandedNodes = new Set(state.expandedNodes);
      const isExpanded = newExpandedNodes.has(nodeId);
      
      if (isExpanded) {
        newExpandedNodes.delete(nodeId);
      } else {
        newExpandedNodes.add(nodeId);
      }
      
      dispatch('nodeToggle', { nodeId, expanded: !isExpanded });
      
      return {
        ...state,
        expandedNodes: newExpandedNodes
      };
    });
    
    updateVisibleNodes();
  }

  // Expand node and all its ancestors
  function expandNodeRecursively(nodeId: string) {
    const node = nodeMap.get(nodeId);
    if (!node) return;
    
    treeState.update(state => {
      const newExpandedNodes = new Set(state.expandedNodes);
      newExpandedNodes.add(nodeId);
      
      // Expand parent nodes too
      let currentNode = node;
      while (currentNode.parentId) {
        const parentNode = nodeMap.get(currentNode.parentId);
        if (parentNode) {
          newExpandedNodes.add(parentNode.id);
          currentNode = parentNode;
        } else {
          break;
        }
      }
      
      return {
        ...state,
        expandedNodes: newExpandedNodes
      };
    });
  }

  // Handle node selection
  function selectNode(nodeId: string) {
    const node = nodeMap.get(nodeId);
    if (!node) return;
    
    treeState.update(state => ({
      ...state,
      selectedNode: nodeId
    }));
    
    dispatch('nodeSelect', { nodeId, node });
  }

  // Handle BaseNode click to toggle children
  function handleNodeClick(event: CustomEvent<{ nodeId: string; event: MouseEvent }>) {
    const { nodeId } = event.detail;
    const node = nodeMap.get(nodeId);
    
    if (node && node.children.length > 0) {
      toggleNode(nodeId);
    }
    
    selectNode(nodeId);
  }

  // Check if node is expanded
  function isNodeExpanded(nodeId: string): boolean {
    return $treeState.expandedNodes.has(nodeId);
  }

  // Check if node is selected
  function isNodeSelected(nodeId: string): boolean {
    return $treeState.selectedNode === nodeId;
  }

  // Get indentation style for depth
  function getIndentStyle(depth: number): string {
    return `margin-left: calc(var(--ns-spacing-4) * ${depth})`;
  }
</script>

<div class="ns-tree {className}">
  {#if visibleNodes.length === 0}
    <div class="ns-tree__empty">
      <p class="ns-tree__empty-message">No nodes to display</p>
    </div>
  {:else}
    <div class="ns-tree__container" role="tree" aria-label="Node hierarchy">
      {#each visibleNodes as node (node.id)}
        <div
          class="ns-tree__node-wrapper"
          style={getIndentStyle(node.depth)}
          role="treeitem"
          aria-level={node.depth + 1}
          aria-expanded={node.children.length > 0 ? isNodeExpanded(node.id) : undefined}
          aria-selected={isNodeSelected(node.id)}
        >
          <BaseNode
            nodeId={node.id}
            content={node.content}
            title={node.title || ''}
            nodeType="text"
            hasChildren={node.children.length > 0}
            selected={isNodeSelected(node.id)}
            on:click={handleNodeClick}
          />
          
          {#if node.children.length > 0}
            <button
              type="button"
              class="ns-tree__toggle-btn"
              class:ns-tree__toggle-btn--expanded={isNodeExpanded(node.id)}
              aria-label="{isNodeExpanded(node.id) ? 'Collapse' : 'Expand'} {node.title || node.content.slice(0, 30)}"
              on:click|stopPropagation={() => toggleNode(node.id)}
            >
              <svg
                class="ns-tree__toggle-icon"
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M4 3L8 6L4 9"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* Tree container styling */
  .ns-tree {
    position: relative;
    min-height: 200px;
  }

  .ns-tree__container {
    display: flex;
    flex-direction: column;
    gap: var(--ns-spacing-1);
  }

  /* Empty state styling */
  .ns-tree__empty {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    padding: var(--ns-spacing-8);
  }

  .ns-tree__empty-message {
    margin: 0;
    color: var(--ns-color-text-tertiary);
    font-size: var(--ns-font-size-base);
    text-align: center;
  }

  /* Node wrapper for indentation */
  .ns-tree__node-wrapper {
    position: relative;
    transition: margin-left var(--ns-duration-normal) var(--ns-easing-easeInOut);
  }

  /* Toggle button styling */
  .ns-tree__toggle-btn {
    position: absolute;
    top: var(--ns-spacing-4);
    left: calc(var(--ns-spacing-2) * -1);
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    background: var(--ns-color-surface-default);
    border: 1px solid var(--ns-color-border-default);
    border-radius: 50%;
    cursor: pointer;
    transition: all var(--ns-duration-fast) var(--ns-easing-easeInOut);
    color: var(--ns-color-text-secondary);
  }

  .ns-tree__toggle-btn:hover {
    background: var(--ns-color-surface-panel);
    border-color: var(--ns-color-border-strong);
    color: var(--ns-color-text-primary);
    transform: scale(1.1);
  }

  .ns-tree__toggle-btn:focus {
    outline: 2px solid var(--ns-color-primary-500);
    outline-offset: 2px;
  }

  .ns-tree__toggle-btn:active {
    transform: scale(0.95);
  }

  /* Toggle icon rotation */
  .ns-tree__toggle-icon {
    transition: transform var(--ns-duration-fast) var(--ns-easing-easeInOut);
  }

  .ns-tree__toggle-btn--expanded .ns-tree__toggle-icon {
    transform: rotate(90deg);
  }

  /* Tree line connections (optional visual enhancement) */
  .ns-tree__node-wrapper::before {
    content: '';
    position: absolute;
    top: 0;
    left: calc(var(--ns-spacing-2) * -1.5);
    width: 1px;
    height: 100%;
    background: var(--ns-color-border-subtle);
    opacity: 0.5;
  }

  /* Hide line for root nodes */
  .ns-tree__node-wrapper[style*="margin-left: calc(var(--ns-spacing-4) * 0)"]::before {
    display: none;
  }

  /* Responsive adjustments */
  @media (max-width: 768px) {
    .ns-tree__toggle-btn {
      width: 24px;
      height: 24px;
      left: calc(var(--ns-spacing-1) * -1);
    }
    
    .ns-tree__toggle-icon {
      width: 14px;
      height: 14px;
    }
  }

  /* Reduced motion accessibility */
  @media (prefers-reduced-motion: reduce) {
    .ns-tree__node-wrapper,
    .ns-tree__toggle-btn,
    .ns-tree__toggle-icon {
      transition: none;
    }
  }

  /* High contrast mode support */
  @media (forced-colors: active) {
    .ns-tree__toggle-btn {
      border-color: ButtonBorder;
      background: ButtonFace;
      color: ButtonText;
    }
    
    .ns-tree__node-wrapper::before {
      background: CanvasText;
      opacity: 0.3;
    }
  }
</style>