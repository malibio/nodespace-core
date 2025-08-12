<!--
  Soft Newline + Markdown Detection Demo
  
  Interactive demo showing how Shift-Enter creates soft newlines and 
  automatic node creation when markdown patterns are detected.
-->

<script lang="ts">
  import BaseNode from '$lib/design/components/BaseNode.svelte';
  import type { NodeCreationSuggestion, SoftNewlineContext } from '$lib/services/softNewlineProcessor.js';

  // Demo state
  let demoNodes: Array<{ id: string; content: string; type: 'text' | 'task' | 'ai-chat' | 'entity' | 'query' }> = [
    { id: 'demo-1', content: 'Try typing some text here, then press Shift-Enter...', type: 'text' }
  ];
  
  let softNewlineContexts: Record<string, SoftNewlineContext> = {};
  let suggestions: NodeCreationSuggestion[] = [];
  let lastActivityLog: string[] = [];
  
  // Handle soft newline context changes
  function handleSoftNewlineContext(event: CustomEvent<{ nodeId: string; context: SoftNewlineContext }>) {
    const { nodeId, context } = event.detail;
    softNewlineContexts[nodeId] = context;
    
    // Log activity
    if (context.hasMarkdownAfterNewline && context.detectedPattern) {
      const log = `🔍 Detected ${context.detectedPattern.type} pattern: "${context.detectedPattern.content}"`;
      addToActivityLog(log);
    }
    
    // Trigger reactivity
    softNewlineContexts = { ...softNewlineContexts };
  }

  // Handle node creation suggestions
  function handleNodeCreationSuggested(event: CustomEvent<{ nodeId: string; suggestion: NodeCreationSuggestion }>) {
    const { suggestion } = event.detail;
    suggestions = [...suggestions, suggestion];
    
    const log = `💡 Suggested ${suggestion.nodeType} node: "${suggestion.content}"`;
    addToActivityLog(log);
  }

  // Accept a node creation suggestion
  function acceptSuggestion(suggestion: NodeCreationSuggestion) {
    // Find the source node and split its content
    const sourceNodeIndex = demoNodes.findIndex(n => n.id === suggestions.find(s => s === suggestion)?.triggerPattern ? 'demo-1' : 'demo-1');
    
    if (sourceNodeIndex !== -1) {
      const sourceNode = demoNodes[sourceNodeIndex];
      
      // Update source node to keep content before the newline
      const context = Object.values(softNewlineContexts)[0];
      if (context) {
        demoNodes[sourceNodeIndex] = {
          ...sourceNode,
          content: context.contentBefore
        };
        
        // Create new node with the suggested content
        const newNode = {
          id: `demo-${Date.now()}`,
          content: suggestion.content,
          type: suggestion.nodeType
        };
        
        // Insert new node after the source node
        demoNodes.splice(sourceNodeIndex + 1, 0, newNode);
        demoNodes = [...demoNodes];
        
        // Clear the suggestion
        suggestions = suggestions.filter(s => s !== suggestion);
        
        addToActivityLog(`✅ Created ${suggestion.nodeType} node: "${suggestion.content}"`);
      }
    }
  }

  // Dismiss a suggestion
  function dismissSuggestion(suggestion: NodeCreationSuggestion) {
    suggestions = suggestions.filter(s => s !== suggestion);
    addToActivityLog(`❌ Dismissed suggestion: "${suggestion.content}"`);
  }

  // Add a new demo node
  function addDemoNode() {
    const newNode = {
      id: `demo-${Date.now()}`,
      content: 'New node - try Shift-Enter here too!',
      type: 'text' as const
    };
    demoNodes = [...demoNodes, newNode];
  }

  // Clear all suggestions
  function clearSuggestions() {
    suggestions = [];
    addToActivityLog('🧹 Cleared all suggestions');
  }

  // Add activity log entry
  function addToActivityLog(message: string) {
    lastActivityLog = [message, ...lastActivityLog].slice(0, 10);
  }

  // Format pattern type for display
  function formatPatternType(type: string): string {
    const typeMap: Record<string, string> = {
      'header': '📋 Header',
      'bullet': '• Bullet',
      'blockquote': '💬 Quote',
      'codeblock': '💻 Code',
      'bold': '**Bold**',
      'italic': '*Italic*',
      'inlinecode': '`Code`'
    };
    return typeMap[type] || type;
  }

  // Format node type for display
  function formatNodeType(type: string): string {
    const typeMap: Record<string, string> = {
      'text': '📝 Text',
      'task': '✅ Task',
      'ai-chat': '🤖 AI Chat',
      'entity': '👤 Entity',
      'query': '🔍 Query'
    };
    return typeMap[type] || type;
  }
</script>

<div class="soft-newline-demo">
  <div class="demo-header">
    <h2>Soft Newline + Markdown Detection Demo</h2>
    <p class="demo-description">
      Try this: Type some text, press <kbd>Shift+Enter</kbd> to create a soft newline, 
      then type markdown syntax like <code># Header</code>, <code>- Bullet</code>, 
      or <code>> Quote</code> to see automatic node suggestions.
    </p>
  </div>

  <div class="demo-container">
    <!-- Demo Nodes -->
    <div class="demo-nodes">
      <h3>Demo Nodes</h3>
      {#each demoNodes as node (node.id)}
        <div class="demo-node-wrapper">
          <div class="node-type-badge">{formatNodeType(node.type)}</div>
          <BaseNode
            nodeId={node.id}
            nodeType={node.type}
            content={node.content}
            multiline={true}
            editable={true}
            contentEditable={true}
            placeholder="Start typing... (try Shift-Enter then markdown syntax)"
            on:softNewlineContext={handleSoftNewlineContext}
            on:nodeCreationSuggested={handleNodeCreationSuggested}
          />
        </div>
      {/each}
      
      <button class="add-node-btn" on:click={addDemoNode}>
        + Add Demo Node
      </button>
    </div>

    <!-- Context Information -->
    <div class="demo-sidebar">
      <!-- Soft Newline Contexts -->
      <div class="context-panel">
        <h3>Soft Newline Contexts</h3>
        {#each Object.entries(softNewlineContexts) as [nodeId, context]}
          <div class="context-item" class:active={context.hasMarkdownAfterNewline}>
            <div class="context-header">
              <span class="node-id">Node: {nodeId.replace('demo-', '#')}</span>
              {#if context.hasMarkdownAfterNewline}
                <span class="pattern-badge">
                  {formatPatternType(context.detectedPattern?.type || '')}
                </span>
              {/if}
            </div>
            
            {#if context.hasMarkdownAfterNewline && context.detectedPattern}
              <div class="context-details">
                <div class="content-split">
                  <div class="content-before">
                    <strong>Before:</strong> "{context.contentBefore}"
                  </div>
                  <div class="content-after">
                    <strong>After:</strong> "{context.contentAfter}"
                  </div>
                </div>
                <div class="pattern-info">
                  <strong>Pattern:</strong> {context.detectedPattern.syntax} → "{context.detectedPattern.content}"
                </div>
                {#if context.suggestedNodeType}
                  <div class="suggestion-info">
                    <strong>Suggests:</strong> {formatNodeType(context.suggestedNodeType)}
                  </div>
                {/if}
              </div>
            {:else}
              <div class="no-context">No markdown patterns detected</div>
            {/if}
          </div>
        {:else}
          <div class="empty-state">No soft newline contexts yet</div>
        {/each}
      </div>

      <!-- Node Creation Suggestions -->
      <div class="suggestions-panel">
        <div class="suggestions-header">
          <h3>Node Creation Suggestions</h3>
          {#if suggestions.length > 0}
            <button class="clear-btn" on:click={clearSuggestions}>Clear All</button>
          {/if}
        </div>
        
        {#each suggestions as suggestion (suggestion)}
          <div class="suggestion-item">
            <div class="suggestion-header">
              <span class="suggestion-type">{formatNodeType(suggestion.nodeType)}</span>
              <span class="suggestion-relationship">({suggestion.relationship})</span>
            </div>
            <div class="suggestion-content">"{suggestion.content}"</div>
            <div class="suggestion-actions">
              <button class="accept-btn" on:click={() => acceptSuggestion(suggestion)}>
                ✅ Accept
              </button>
              <button class="dismiss-btn" on:click={() => dismissSuggestion(suggestion)}>
                ❌ Dismiss
              </button>
            </div>
          </div>
        {:else}
          <div class="empty-state">No suggestions yet</div>
        {/each}
      </div>

      <!-- Activity Log -->
      <div class="activity-panel">
        <h3>Activity Log</h3>
        <div class="activity-log">
          {#each lastActivityLog as logEntry}
            <div class="log-entry">{logEntry}</div>
          {:else}
            <div class="empty-state">No activity yet</div>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .soft-newline-demo {
    max-width: 1200px;
    margin: 0 auto;
    padding: 20px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .demo-header {
    margin-bottom: 24px;
    text-align: center;
  }

  .demo-header h2 {
    color: hsl(var(--foreground));
    margin: 0 0 12px 0;
    font-size: 24px;
  }

  .demo-description {
    color: hsl(var(--muted-foreground));
    font-size: 14px;
    line-height: 1.5;
    max-width: 600px;
    margin: 0 auto;
  }

  .demo-description kbd {
    background: hsl(var(--muted));
    padding: 2px 4px;
    border-radius: 3px;
    font-size: 12px;
  }

  .demo-description code {
    background: hsl(var(--muted));
    padding: 2px 4px;
    border-radius: 3px;
    font-family: monospace;
    font-size: 12px;
  }

  .demo-container {
    display: grid;
    grid-template-columns: 1fr 400px;
    gap: 24px;
    align-items: start;
  }

  .demo-nodes {
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 8px;
    padding: 20px;
  }

  .demo-nodes h3 {
    margin: 0 0 16px 0;
    color: hsl(var(--foreground));
    font-size: 18px;
  }

  .demo-node-wrapper {
    position: relative;
    margin-bottom: 16px;
    border: 1px solid hsl(var(--border));
    border-radius: 6px;
    overflow: hidden;
  }

  .node-type-badge {
    position: absolute;
    top: -8px;
    left: 12px;
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 500;
    z-index: 1;
  }

  .add-node-btn {
    width: 100%;
    padding: 12px;
    background: hsl(var(--secondary));
    border: 1px dashed hsl(var(--border));
    border-radius: 6px;
    color: hsl(var(--secondary-foreground));
    cursor: pointer;
    font-size: 14px;
  }

  .add-node-btn:hover {
    background: hsl(var(--secondary) / 0.8);
  }

  .demo-sidebar {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .context-panel, .suggestions-panel, .activity-panel {
    background: hsl(var(--card));
    border: 1px solid hsl(var(--border));
    border-radius: 8px;
    padding: 16px;
  }

  .context-panel h3, .suggestions-panel h3, .activity-panel h3 {
    margin: 0 0 12px 0;
    color: hsl(var(--foreground));
    font-size: 16px;
  }

  .suggestions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .clear-btn {
    background: none;
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted-foreground));
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }

  .clear-btn:hover {
    background: hsl(var(--muted));
  }

  .context-item {
    padding: 12px;
    border: 1px solid hsl(var(--border));
    border-radius: 6px;
    margin-bottom: 8px;
  }

  .context-item.active {
    border-color: hsl(var(--ring));
    background: hsl(var(--ring) / 0.05);
  }

  .context-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .node-id {
    font-weight: 500;
    color: hsl(var(--foreground));
    font-size: 13px;
  }

  .pattern-badge {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 500;
  }

  .context-details {
    font-size: 12px;
    color: hsl(var(--muted-foreground));
  }

  .content-split {
    margin-bottom: 6px;
  }

  .content-before, .content-after {
    margin-bottom: 4px;
  }

  .pattern-info, .suggestion-info {
    margin-top: 6px;
    font-weight: 500;
  }

  .no-context, .empty-state {
    color: hsl(var(--muted-foreground));
    font-style: italic;
    font-size: 13px;
    text-align: center;
    padding: 12px;
  }

  .suggestion-item {
    padding: 12px;
    border: 1px solid hsl(var(--border));
    border-radius: 6px;
    margin-bottom: 8px;
  }

  .suggestion-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .suggestion-type {
    font-weight: 500;
    color: hsl(var(--foreground));
    font-size: 13px;
  }

  .suggestion-relationship {
    color: hsl(var(--muted-foreground));
    font-size: 11px;
  }

  .suggestion-content {
    color: hsl(var(--muted-foreground));
    font-size: 13px;
    margin-bottom: 8px;
  }

  .suggestion-actions {
    display: flex;
    gap: 8px;
  }

  .accept-btn, .dismiss-btn {
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    cursor: pointer;
    border: 1px solid;
  }

  .accept-btn {
    background: hsl(var(--primary) / 0.1);
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
  }

  .accept-btn:hover {
    background: hsl(var(--primary) / 0.2);
  }

  .dismiss-btn {
    background: hsl(var(--destructive) / 0.1);
    border-color: hsl(var(--destructive));
    color: hsl(var(--destructive));
  }

  .dismiss-btn:hover {
    background: hsl(var(--destructive) / 0.2);
  }

  .activity-log {
    max-height: 200px;
    overflow-y: auto;
  }

  .log-entry {
    padding: 6px 8px;
    border-bottom: 1px solid hsl(var(--border));
    font-size: 12px;
    color: hsl(var(--muted-foreground));
    line-height: 1.3;
  }

  .log-entry:last-child {
    border-bottom: none;
  }

  /* Responsive design */
  @media (max-width: 768px) {
    .demo-container {
      grid-template-columns: 1fr;
      gap: 16px;
    }

    .demo-sidebar {
      order: -1;
    }
  }
</style>