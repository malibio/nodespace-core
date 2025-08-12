<!--
  Bullet-to-Node Conversion Demo
  
  Demonstrates smart bullet point conversion into actual node hierarchy.
  Shows Issue #58 functionality with real-time pattern detection.
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import SmartTextNode from '$lib/design/components/SmartTextNode.svelte';
  import NodeTree from './NodeTree.svelte';
  import type { TreeNodeData } from '$lib/types/tree';
  import type { MarkdownPattern } from '$lib/types/markdownPatterns';
  import { patternIntegrationUtils, mockPatternData } from '$lib/services/markdownPatternUtils';
  import { BulletProcessingUtils } from '$lib/services/bulletToNodeConverter';

  // Demo state
  let nodes: TreeNodeData[] = [];
  let selectedNodeId: string | null = null;
  let showDebugInfo = false;
  let conversionCount = 0;
  let lastConversionTime: Date | null = null;

  // Mock pattern detection (simulating Issue #56 APIs)
  let mockPatterns: MarkdownPattern[] = [];
  
  // Sample content to demonstrate bullet conversion
  const sampleBulletContent = `Research project planning:
- Define project scope and objectives
  * Identify key stakeholders
  * Set timeline and milestones  
- Gather requirements
  + Conduct user interviews
  + Analyze competitor solutions
- Technical architecture
  - Choose technology stack
  - Design system architecture
- Implementation phases
  * Phase 1: Core functionality
  * Phase 2: Advanced features
  * Phase 3: Polish and optimization`;

  // Initialize demo
  onMount(() => {
    // Create initial demo node
    const initialNode: TreeNodeData = {
      id: 'demo-root',
      title: 'Bullet Conversion Demo',
      content: 'Type bullet points (-, *, +) to see them convert to child nodes!',
      nodeType: 'text',
      depth: 0,
      parentId: null,
      children: [],
      expanded: true,
      hasChildren: false
    };
    
    nodes = [initialNode];
    selectedNodeId = initialNode.id;
  });

  // Handle pattern detection requests from SmartTextNode
  function handlePatternDetection(event: CustomEvent<{
    nodeId: string;
    content: string;
    cursorPosition: number;
  }>) {
    const { content, cursorPosition } = event.detail;
    
    // Mock pattern detection - in real implementation, this would use actual pattern detector
    mockPatterns = simulatePatternDetection(content);
    
    // Find the requesting node and send patterns back
    const smartNodeComponent = getSmartNodeComponent(event.detail.nodeId);
    if (smartNodeComponent) {
      smartNodeComponent.processPatterns(mockPatterns, cursorPosition);
    }
  }

  // Handle bullet conversion results
  function handleBulletConversion(event: CustomEvent<{
    nodeId: string;
    newNodes: TreeNodeData[];
    cleanedContent: string;
  }>) {
    const { nodeId, newNodes, cleanedContent } = event.detail;
    
    // Update the source node with cleaned content
    updateNodeContent(nodeId, cleanedContent);
    
    // Add new child nodes to the hierarchy
    addChildNodes(nodeId, newNodes);
    
    // Update conversion statistics
    conversionCount += newNodes.length;
    lastConversionTime = new Date();
  }

  // Update node content
  function updateNodeContent(nodeId: string, newContent: string) {
    const node = findNodeById(nodeId);
    if (node) {
      node.content = newContent;
      node.title = newContent.substring(0, 50) || 'Empty node';
      nodes = [...nodes]; // Trigger reactivity
    }
  }

  // Add child nodes to parent
  function addChildNodes(parentNodeId: string, childNodes: TreeNodeData[]) {
    const parentNode = findNodeById(parentNodeId);
    if (!parentNode) return;
    
    // Mark parent as having children
    parentNode.hasChildren = childNodes.length > 0;
    parentNode.expanded = true;
    
    // Add child nodes
    parentNode.children = [...parentNode.children, ...childNodes];
    
    // Update all nodes list for flat access
    const allNewNodes = flattenNodes(childNodes);
    nodes = [...nodes, ...allNewNodes];
  }

  // Find node by ID (recursive search)
  function findNodeById(nodeId: string): TreeNodeData | null {
    function search(nodeList: TreeNodeData[]): TreeNodeData | null {
      for (const node of nodeList) {
        if (node.id === nodeId) return node;
        const found = search(node.children);
        if (found) return found;
      }
      return null;
    }
    return search(nodes);
  }

  // Flatten node hierarchy to array
  function flattenNodes(nodeList: TreeNodeData[]): TreeNodeData[] {
    const result: TreeNodeData[] = [];
    
    function addNodes(nodes: TreeNodeData[]) {
      for (const node of nodes) {
        result.push(node);
        if (node.children.length > 0) {
          addNodes(node.children);
        }
      }
    }
    
    addNodes(nodeList);
    return result;
  }

  // Get SmartTextNode component reference (simplified for demo)
  function getSmartNodeComponent(nodeId: string): any {
    // In a real implementation, would maintain component references
    // For demo, we'll just return a mock that implements processPatterns
    return {
      processPatterns(patterns: MarkdownPattern[], cursorPosition: number) {
        // This would be handled by the actual SmartTextNode component
        console.log(`Processing ${patterns.length} patterns for node ${nodeId}`);
      }
    };
  }

  // Mock pattern detection (simulates Issue #56 functionality)
  function simulatePatternDetection(content: string): MarkdownPattern[] {
    if (!content) return [];
    
    const patterns: MarkdownPattern[] = [];
    const lines = content.split('\n');
    let position = 0;
    
    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const trimmedLine = line.trim();
      
      // Detect bullet patterns
      const bulletMatch = line.match(/^(\s*)([-*+])\s(.+)$/);
      if (bulletMatch) {
        const [fullMatch, indent, bulletChar, contentText] = bulletMatch;
        const bulletStart = position + line.indexOf(bulletChar);
        const bulletEnd = bulletStart + bulletChar.length + 1; // +1 for space
        const contentStart = bulletEnd;
        const contentEnd = position + line.length;
        
        patterns.push({
          type: 'bullet',
          start: bulletStart,
          end: contentEnd,
          syntax: `${bulletChar} `,
          content: contentText,
          bulletType: bulletChar as '-' | '*' | '+',
          line: lineIndex,
          column: indent.length
        });
      }
      
      position += line.length + 1; // +1 for newline
    }
    
    return patterns;
  }

  // Handle node tree events
  function handleNodeUpdate(event: CustomEvent<{ nodeId: string; content: string }>) {
    updateNodeContent(event.detail.nodeId, event.detail.content);
  }

  function handleNodeSelect(event: CustomEvent<{ nodeId: string; node: TreeNodeData }>) {
    selectedNodeId = event.detail.nodeId;
  }

  function handleNodeExpand(event: CustomEvent<{ nodeId: string; expanded: boolean }>) {
    const node = findNodeById(event.detail.nodeId);
    if (node) {
      node.expanded = event.detail.expanded;
      nodes = [...nodes];
    }
  }

  // Demo actions
  function loadSampleContent() {
    const rootNode = findNodeById('demo-root');
    if (rootNode) {
      rootNode.content = sampleBulletContent;
      nodes = [...nodes];
    }
  }

  function clearAllNodes() {
    const rootNode = findNodeById('demo-root');
    if (rootNode) {
      rootNode.children = [];
      rootNode.hasChildren = false;
      rootNode.content = '';
      nodes = nodes.filter(node => node.id === 'demo-root');
      conversionCount = 0;
      lastConversionTime = null;
    }
  }

  // Get statistics
  $: totalNodes = nodes.length;
  $: bulletNodes = nodes.filter(node => node.parentId !== null).length;
  $: rootNodes = nodes.filter(node => node.parentId === null).length;

  // Format time
  function formatTime(date: Date): string {
    return date.toLocaleTimeString();
  }
</script>

<div class="bullet-demo">
  <header class="bullet-demo__header">
    <h2>Smart Bullet-to-Node Conversion Demo</h2>
    <p>Type bullet points using <code>-</code>, <code>*</code>, or <code>+</code> to see them automatically convert to child nodes!</p>
  </header>

  <div class="bullet-demo__controls">
    <button type="button" class="demo-button" on:click={loadSampleContent}>
      Load Sample Content
    </button>
    <button type="button" class="demo-button demo-button--secondary" on:click={clearAllNodes}>
      Clear All
    </button>
    <label class="demo-checkbox">
      <input type="checkbox" bind:checked={showDebugInfo} />
      Show Debug Info
    </label>
  </div>

  <div class="bullet-demo__stats">
    <div class="stats-grid">
      <div class="stat-item">
        <span class="stat-label">Total Nodes:</span>
        <span class="stat-value">{totalNodes}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">Child Nodes:</span>
        <span class="stat-value">{bulletNodes}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">Conversions:</span>
        <span class="stat-value">{conversionCount}</span>
      </div>
      {#if lastConversionTime}
        <div class="stat-item">
          <span class="stat-label">Last Conversion:</span>
          <span class="stat-value">{formatTime(lastConversionTime)}</span>
        </div>
      {/if}
    </div>
  </div>

  <div class="bullet-demo__content">
    <div class="bullet-demo__editor">
      <h3>Smart Text Editor</h3>
      <div class="editor-container">
        {#if selectedNodeId}
          {@const selectedNode = findNodeById(selectedNodeId)}
          {#if selectedNode}
            <SmartTextNode
              nodeId={selectedNode.id}
              bind:content={selectedNode.content}
              nodeType={selectedNode.nodeType}
              hasChildren={selectedNode.hasChildren}
              enableBulletConversion={true}
              autoConvertBullets={true}
              maxNestingDepth={5}
              parentNodeId={selectedNode.id}
              on:requestPatternDetection={handlePatternDetection}
              on:bulletConversion={handleBulletConversion}
            />
          {/if}
        {:else}
          <p class="no-selection">Select a node from the tree to edit it</p>
        {/if}
      </div>
    </div>

    <div class="bullet-demo__tree">
      <h3>Node Hierarchy</h3>
      <div class="tree-container">
        <NodeTree
          {nodes}
          allowEdit={false}
          showExpandControls={true}
          on:nodeUpdate={handleNodeUpdate}
          on:nodeSelect={handleNodeSelect}
          on:nodeExpand={handleNodeExpand}
        />
      </div>
    </div>
  </div>

  {#if showDebugInfo}
    <div class="bullet-demo__debug">
      <h3>Debug Information</h3>
      <div class="debug-grid">
        <div class="debug-section">
          <h4>Current Patterns</h4>
          {#if mockPatterns.length > 0}
            <ul class="debug-list">
              {#each mockPatterns as pattern}
                <li class="debug-item">
                  <code>{pattern.type}</code>: "{pattern.content}" 
                  <span class="debug-meta">({pattern.start}-{pattern.end})</span>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="debug-empty">No patterns detected</p>
          {/if}
        </div>

        <div class="debug-section">
          <h4>Node Structure</h4>
          <pre class="debug-code">{JSON.stringify(nodes, null, 2)}</pre>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .bullet-demo {
    max-width: 1200px;
    margin: 0 auto;
    padding: 24px;
  }

  .bullet-demo__header {
    text-align: center;
    margin-bottom: 32px;
  }

  .bullet-demo__header h2 {
    color: hsl(var(--foreground));
    margin-bottom: 8px;
  }

  .bullet-demo__header p {
    color: hsl(var(--muted-foreground));
    max-width: 600px;
    margin: 0 auto;
  }

  .bullet-demo__header code {
    background: hsl(var(--muted));
    padding: 2px 4px;
    border-radius: 3px;
    font-family: monospace;
  }

  .bullet-demo__controls {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 16px;
    margin-bottom: 24px;
    flex-wrap: wrap;
  }

  .demo-button {
    padding: 8px 16px;
    border-radius: 6px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    cursor: pointer;
    font-size: 14px;
    transition: all 0.2s ease;
  }

  .demo-button:hover {
    background: hsl(var(--accent));
  }

  .demo-button--secondary {
    background: hsl(var(--muted));
    color: hsl(var(--muted-foreground));
  }

  .demo-checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    color: hsl(var(--muted-foreground));
  }

  .bullet-demo__stats {
    background: hsl(var(--muted) / 0.5);
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 32px;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 16px;
  }

  .stat-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .stat-label {
    font-size: 14px;
    color: hsl(var(--muted-foreground));
  }

  .stat-value {
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .bullet-demo__content {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 32px;
    margin-bottom: 32px;
  }

  .bullet-demo__editor,
  .bullet-demo__tree {
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 8px;
    padding: 20px;
  }

  .bullet-demo__editor h3,
  .bullet-demo__tree h3 {
    margin: 0 0 16px 0;
    color: hsl(var(--foreground));
  }

  .editor-container {
    min-height: 200px;
  }

  .tree-container {
    min-height: 200px;
    max-height: 500px;
    overflow-y: auto;
  }

  .no-selection {
    color: hsl(var(--muted-foreground));
    font-style: italic;
    text-align: center;
    padding: 40px 20px;
  }

  .bullet-demo__debug {
    background: hsl(var(--muted) / 0.3);
    border-radius: 8px;
    padding: 20px;
  }

  .bullet-demo__debug h3 {
    margin: 0 0 16px 0;
    color: hsl(var(--foreground));
  }

  .debug-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }

  .debug-section h4 {
    margin: 0 0 12px 0;
    color: hsl(var(--muted-foreground));
    font-size: 14px;
    font-weight: 600;
  }

  .debug-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .debug-item {
    padding: 8px;
    background: hsl(var(--card));
    border-radius: 4px;
    margin-bottom: 4px;
    font-family: monospace;
    font-size: 12px;
  }

  .debug-item code {
    background: hsl(var(--primary) / 0.1);
    color: hsl(var(--primary));
    padding: 2px 4px;
    border-radius: 2px;
  }

  .debug-meta {
    color: hsl(var(--muted-foreground));
  }

  .debug-empty {
    color: hsl(var(--muted-foreground));
    font-style: italic;
    margin: 0;
  }

  .debug-code {
    background: hsl(var(--card));
    padding: 12px;
    border-radius: 4px;
    font-size: 11px;
    max-height: 300px;
    overflow-y: auto;
    color: hsl(var(--muted-foreground));
    margin: 0;
  }

  /* Responsive design */
  @media (max-width: 768px) {
    .bullet-demo {
      padding: 16px;
    }

    .bullet-demo__content {
      grid-template-columns: 1fr;
      gap: 24px;
    }

    .bullet-demo__controls {
      flex-direction: column;
      gap: 12px;
    }

    .stats-grid {
      grid-template-columns: 1fr;
      gap: 8px;
    }

    .debug-grid {
      grid-template-columns: 1fr;
      gap: 16px;
    }
  }
</style>