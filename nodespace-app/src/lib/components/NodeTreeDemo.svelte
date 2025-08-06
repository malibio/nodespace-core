<!--
  NodeTree Demo Component
  
  Demonstrates hierarchical display patterns with mock data for testing NodeTree functionality.
  Includes expand/collapse, indentation, and parent-child relationship visualization.
-->

<script lang="ts">
  import NodeTree, { type HierarchicalNode } from './NodeTree.svelte';

  // Mock hierarchical data structure
  let mockHierarchicalNodes: HierarchicalNode[] = [
    // Root level nodes
    {
      id: 'root-1',
      content: 'This is the **root node** of our project. It contains all the main components and documentation.\n\nClick to expand and see the child nodes.',
      title: 'Project Root',
      children: ['child-1-1', 'child-1-2', 'child-1-3'],
      depth: 0
    },
    {
      id: 'root-2',
      content: '## Research Notes\n\nCollection of research findings and insights for the NodeSpace application development.',
      title: 'Research & Development',
      children: ['child-2-1', 'child-2-2'],
      depth: 0
    },
    {
      id: 'root-3',
      content: 'Single node without any children to demonstrate leaf nodes in the hierarchy.',
      title: 'Standalone Node',
      children: [],
      depth: 0
    },

    // First level children of root-1
    {
      id: 'child-1-1',
      content: '### Component Architecture\n\nDefines the structure for reusable UI components with proper TypeScript interfaces.',
      title: 'Components',
      parentId: 'root-1',
      children: ['grandchild-1-1-1', 'grandchild-1-1-2'],
      depth: 1
    },
    {
      id: 'child-1-2',
      content: 'Services layer handles data persistence, API communication, and business logic separation.',
      title: 'Services',
      parentId: 'root-1',
      children: ['grandchild-1-2-1'],
      depth: 1
    },
    {
      id: 'child-1-3',
      content: '**Configuration files** for build tools, TypeScript, and development environment setup.',
      title: 'Configuration',
      parentId: 'root-1',
      children: [],
      depth: 1
    },

    // First level children of root-2
    {
      id: 'child-2-1',
      content: 'Analysis of user interface patterns and design system requirements for optimal user experience.',
      title: 'UI/UX Research',
      parentId: 'root-2',
      children: ['grandchild-2-1-1'],
      depth: 1
    },
    {
      id: 'child-2-2',
      content: '## Technical Implementation\n\n- Svelte 4.x reactivity patterns\n- Tauri desktop integration\n- Performance optimization strategies',
      title: 'Technical Analysis',
      parentId: 'root-2',
      children: [],
      depth: 1
    },

    // Second level children (grandchildren)
    {
      id: 'grandchild-1-1-1',
      content: 'BaseNode component provides foundation for all node types with interaction states and accessibility.',
      title: 'BaseNode.svelte',
      parentId: 'child-1-1',
      children: [],
      depth: 2
    },
    {
      id: 'grandchild-1-1-2',
      content: 'TextNode extends BaseNode with inline editing, markdown support, and auto-save functionality.',
      title: 'TextNode.svelte',
      parentId: 'child-1-1',
      children: ['greatgrand-1-1-2-1'],
      depth: 2
    },
    {
      id: 'grandchild-1-2-1',
      content: '```typescript\n// Mock service for independent development\nexport class MockTextService {\n  // Implementation details...\n}\n```',
      title: 'MockTextService',
      parentId: 'child-1-2',
      children: [],
      depth: 2
    },
    {
      id: 'grandchild-2-1-1',
      content: 'Color schemes, typography scales, spacing system, and component interaction patterns.',
      title: 'Design Tokens',
      parentId: 'child-2-1',
      children: [],
      depth: 2
    },

    // Third level children (great-grandchildren) - testing deep nesting
    {
      id: 'greatgrand-1-1-2-1',
      content: 'Editor functionality with keyboard shortcuts:\n- **Ctrl+Enter**: Save\n- **Escape**: Cancel\n- Auto-resize textarea',
      title: 'Editor Features',
      parentId: 'grandchild-1-1-2',
      children: ['deep-nested-1'],
      depth: 3
    },

    // Fourth level children - testing maximum depth
    {
      id: 'deep-nested-1',
      content: 'This node demonstrates deep nesting capability (depth 4). The tree should handle multiple levels gracefully.',
      title: 'Deep Nested Example',
      parentId: 'greatgrand-1-1-2-1',
      children: ['deep-nested-2'],
      depth: 4
    },

    // Fifth level - maximum recommended depth
    {
      id: 'deep-nested-2',
      content: 'Maximum depth node (level 5). Consider flattening hierarchy if deeper levels are needed.',
      title: 'Maximum Depth Node',
      parentId: 'deep-nested-1',
      children: [],
      depth: 5
    }
  ];

  // Component state
  let selectedNodeId: string | null = null;
  let treeStats = {
    totalNodes: mockHierarchicalNodes.length,
    visibleNodes: 0,
    expandedNodes: 0
  };

  // Event handlers
  function handleNodeToggle(event: CustomEvent<{ nodeId: string; expanded: boolean }>) {
    const { nodeId, expanded } = event.detail;
    console.log(`Node ${nodeId} ${expanded ? 'expanded' : 'collapsed'}`);
  }

  function handleNodeSelect(event: CustomEvent<{ nodeId: string; node: HierarchicalNode }>) {
    const { nodeId, node } = event.detail;
    selectedNodeId = nodeId;
    console.log('Selected node:', node);
  }

  function handleNodeUpdate(event: CustomEvent<{ nodeId: string; content: string; title: string }>) {
    const { nodeId, content, title } = event.detail;
    console.log(`Node ${nodeId} updated:`, { title, content: content.slice(0, 50) + '...' });
    
    // Update the node in our mock data
    const nodeIndex = mockHierarchicalNodes.findIndex(n => n.id === nodeId);
    if (nodeIndex !== -1) {
      mockHierarchicalNodes[nodeIndex] = {
        ...mockHierarchicalNodes[nodeIndex],
        content,
        title
      };
    }
  }

  function handleTreeChange(event: CustomEvent<{ visibleNodes: HierarchicalNode[] }>) {
    const { visibleNodes } = event.detail;
    treeStats.visibleNodes = visibleNodes.length;
  }

  // Demo controls
  let autoExpand = false;
  let maxDepth = 5;

  function resetTree() {
    selectedNodeId = null;
    // Force component re-render by creating new array reference
    mockHierarchicalNodes = [...mockHierarchicalNodes];
  }

  function getNodeByDepth(depth: number): number {
    return mockHierarchicalNodes.filter(n => n.depth === depth).length;
  }
</script>

<div class="ns-tree-demo">
  <header class="ns-tree-demo__header">
    <h2 class="ns-tree-demo__title">NodeTree Component Demo</h2>
    <p class="ns-tree-demo__description">
      Interactive demonstration of hierarchical node display with expand/collapse functionality,
      tree indentation, and parent-child relationships.
    </p>
  </header>

  <div class="ns-tree-demo__controls">
    <div class="ns-tree-demo__control-group">
      <label class="ns-tree-demo__label">
        <input
          type="checkbox"
          bind:checked={autoExpand}
          class="ns-tree-demo__checkbox"
        />
        Auto-expand tree
      </label>
      
      <label class="ns-tree-demo__label">
        Max Depth:
        <select bind:value={maxDepth} class="ns-tree-demo__select">
          <option value={3}>3 levels</option>
          <option value={5}>5 levels</option>
          <option value={10}>10 levels</option>
        </select>
      </label>
      
      <button
        type="button"
        class="ns-tree-demo__btn"
        on:click={resetTree}
      >
        Reset Tree
      </button>
    </div>
  </div>

  <div class="ns-tree-demo__stats">
    <div class="ns-tree-demo__stat">
      <span class="ns-tree-demo__stat-label">Total Nodes:</span>
      <span class="ns-tree-demo__stat-value">{treeStats.totalNodes}</span>
    </div>
    <div class="ns-tree-demo__stat">
      <span class="ns-tree-demo__stat-label">Visible Nodes:</span>
      <span class="ns-tree-demo__stat-value">{treeStats.visibleNodes}</span>
    </div>
    <div class="ns-tree-demo__stat">
      <span class="ns-tree-demo__stat-label">Selected:</span>
      <span class="ns-tree-demo__stat-value">{selectedNodeId || 'None'}</span>
    </div>
  </div>

  <div class="ns-tree-demo__hierarchy-info">
    <h3 class="ns-tree-demo__info-title">Hierarchy Breakdown:</h3>
    <ul class="ns-tree-demo__depth-list">
      {#each Array(6) as _, depth}
        {@const count = getNodeByDepth(depth)}
        {#if count > 0}
          <li class="ns-tree-demo__depth-item">
            <span class="ns-tree-demo__depth-label">Depth {depth}:</span>
            <span class="ns-tree-demo__depth-count">{count} nodes</span>
          </li>
        {/if}
      {/each}
    </ul>
  </div>

  <div class="ns-tree-demo__tree-container">
    <NodeTree
      nodes={mockHierarchicalNodes}
      {maxDepth}
      {autoExpand}
      className="ns-tree-demo__tree"
      on:nodeToggle={handleNodeToggle}
      on:nodeSelect={handleNodeSelect}
      on:nodeUpdate={handleNodeUpdate}
      on:treeChange={handleTreeChange}
    />
  </div>

  <div class="ns-tree-demo__instructions">
    <h3 class="ns-tree-demo__instructions-title">How to Use:</h3>
    <ul class="ns-tree-demo__instructions-list">
      <li>🔽 <strong>Click toggle buttons</strong> (arrow icons) to expand/collapse parent nodes</li>
      <li>📝 <strong>Click on any node</strong> to select it and enable inline editing</li>
      <li>🌳 <strong>Observe indentation</strong> - each depth level increases left margin</li>
      <li>⭕ <strong>Ring indicators</strong> show on parent nodes (nodes with children)</li>
      <li>⌨️ <strong>Use keyboard shortcuts</strong> in edit mode: Ctrl+Enter to save, Escape to cancel</li>
      <li>🔄 <strong>Auto-expand</strong> checkbox opens entire tree on load</li>
    </ul>
  </div>
</div>

<style>
  /* Demo container styling */
  .ns-tree-demo {
    max-width: 1000px;
    margin: 0 auto;
    padding: var(--ns-spacing-6);
  }

  .ns-tree-demo__header {
    margin-bottom: var(--ns-spacing-6);
    text-align: center;
  }

  .ns-tree-demo__title {
    margin: 0 0 var(--ns-spacing-3) 0;
    font-size: var(--ns-font-size-2xl);
    font-weight: var(--ns-font-weight-bold);
    color: var(--ns-color-text-primary);
  }

  .ns-tree-demo__description {
    margin: 0;
    font-size: var(--ns-font-size-base);
    color: var(--ns-color-text-secondary);
    line-height: var(--ns-line-height-relaxed);
  }

  /* Controls section */
  .ns-tree-demo__controls {
    margin-bottom: var(--ns-spacing-4);
    padding: var(--ns-spacing-4);
    background: var(--ns-color-surface-panel);
    border-radius: var(--ns-radius-lg);
    border: 1px solid var(--ns-color-border-default);
  }

  .ns-tree-demo__control-group {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ns-spacing-4);
    align-items: center;
  }

  .ns-tree-demo__label {
    display: flex;
    align-items: center;
    gap: var(--ns-spacing-2);
    font-size: var(--ns-font-size-sm);
    color: var(--ns-color-text-primary);
  }

  .ns-tree-demo__checkbox,
  .ns-tree-demo__select {
    padding: var(--ns-spacing-1);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-sm);
  }

  .ns-tree-demo__btn {
    padding: var(--ns-spacing-2) var(--ns-spacing-3);
    background: var(--ns-color-primary-500);
    color: white;
    border: none;
    border-radius: var(--ns-radius-sm);
    font-size: var(--ns-font-size-sm);
    cursor: pointer;
    transition: background-color var(--ns-duration-fast) var(--ns-easing-easeInOut);
  }

  .ns-tree-demo__btn:hover {
    background: var(--ns-color-primary-600);
  }

  /* Stats display */
  .ns-tree-demo__stats {
    display: flex;
    gap: var(--ns-spacing-4);
    margin-bottom: var(--ns-spacing-4);
    padding: var(--ns-spacing-3);
    background: var(--ns-color-surface-default);
    border-radius: var(--ns-radius-md);
    border: 1px solid var(--ns-color-border-subtle);
  }

  .ns-tree-demo__stat {
    display: flex;
    gap: var(--ns-spacing-1);
  }

  .ns-tree-demo__stat-label {
    font-size: var(--ns-font-size-sm);
    color: var(--ns-color-text-secondary);
  }

  .ns-tree-demo__stat-value {
    font-size: var(--ns-font-size-sm);
    font-weight: var(--ns-font-weight-medium);
    color: var(--ns-color-text-primary);
  }

  /* Hierarchy info */
  .ns-tree-demo__hierarchy-info {
    margin-bottom: var(--ns-spacing-4);
  }

  .ns-tree-demo__info-title {
    margin: 0 0 var(--ns-spacing-2) 0;
    font-size: var(--ns-font-size-base);
    font-weight: var(--ns-font-weight-semibold);
    color: var(--ns-color-text-primary);
  }

  .ns-tree-demo__depth-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: var(--ns-spacing-3);
  }

  .ns-tree-demo__depth-item {
    display: flex;
    gap: var(--ns-spacing-1);
    font-size: var(--ns-font-size-sm);
  }

  .ns-tree-demo__depth-label {
    color: var(--ns-color-text-secondary);
  }

  .ns-tree-demo__depth-count {
    font-weight: var(--ns-font-weight-medium);
    color: var(--ns-color-text-primary);
  }

  /* Tree container */
  .ns-tree-demo__tree-container {
    margin-bottom: var(--ns-spacing-6);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-lg);
    padding: var(--ns-spacing-4);
    background: var(--ns-color-surface-default);
    min-height: 400px;
  }

  /* Instructions */
  .ns-tree-demo__instructions {
    padding: var(--ns-spacing-4);
    background: var(--ns-color-surface-panel);
    border-radius: var(--ns-radius-lg);
    border-left: 4px solid var(--ns-color-primary-500);
  }

  .ns-tree-demo__instructions-title {
    margin: 0 0 var(--ns-spacing-3) 0;
    font-size: var(--ns-font-size-base);
    font-weight: var(--ns-font-weight-semibold);
    color: var(--ns-color-text-primary);
  }

  .ns-tree-demo__instructions-list {
    margin: 0;
    padding-left: var(--ns-spacing-4);
  }

  .ns-tree-demo__instructions-list li {
    margin-bottom: var(--ns-spacing-2);
    font-size: var(--ns-font-size-sm);
    line-height: var(--ns-line-height-relaxed);
    color: var(--ns-color-text-primary);
  }

  .ns-tree-demo__instructions-list li:last-child {
    margin-bottom: 0;
  }

  /* Responsive adjustments */
  @media (max-width: 768px) {
    .ns-tree-demo {
      padding: var(--ns-spacing-4);
    }

    .ns-tree-demo__control-group {
      flex-direction: column;
      align-items: stretch;
    }

    .ns-tree-demo__stats {
      flex-direction: column;
    }

    .ns-tree-demo__depth-list {
      flex-direction: column;
      gap: var(--ns-spacing-1);
    }
  }
</style>