<!--
  AI Integration Demo Component
  
  Demonstrates comprehensive markdown import/export functionality for seamless AI integration.
  Shows export to AI-optimized markdown and import of AI-generated content back to NodeSpace.
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import BaseNode from '$lib/design/components/BaseNode.svelte';
  import NodeTree from '$lib/components/NodeTree.svelte';
  import { 
    aiIntegrationService, 
    chatGPTIntegrationService, 
    claudeIntegrationService,
    AIIntegrationUtils
  } from '$lib/services/aiIntegrationService.js';
  import type { 
    ExportResult, 
    ImportResult, 
    AIIntegrationConfig 
  } from '$lib/services/aiIntegrationService.js';
  import type { TreeNodeData } from '$lib/types/tree.js';

  // Demo sample content
  const sampleNodes: TreeNodeData[] = [
    {
      id: 'node-1',
      title: 'Project Overview',
      content: 'Main project with multiple components and features',
      nodeType: 'text',
      depth: 0,
      parentId: null,
      expanded: true,
      hasChildren: true,
      children: [
        {
          id: 'node-1-1',
          title: 'Core Features',
          content: 'Essential functionality for the application',
          nodeType: 'text',
          depth: 1,
          parentId: 'node-1',
          expanded: true,
          hasChildren: true,
          children: [
            {
              id: 'node-1-1-1',
              title: 'Authentication System',
              content: 'User login and registration with JWT tokens',
              nodeType: 'task',
              depth: 2,
              parentId: 'node-1-1',
              expanded: true,
              hasChildren: false,
              children: []
            },
            {
              id: 'node-1-1-2',
              title: 'Data Management',
              content: 'CRUD operations with real-time synchronization',
              nodeType: 'task',
              depth: 2,
              parentId: 'node-1-1',
              expanded: true,
              hasChildren: false,
              children: []
            }
          ]
        },
        {
          id: 'node-1-2',
          title: 'Performance Requirements',
          content: 'System must handle 1000+ concurrent users with sub-100ms response times',
          nodeType: 'query',
          depth: 1,
          parentId: 'node-1',
          expanded: true,
          hasChildren: false,
          children: []
        }
      ]
    },
    {
      id: 'node-2',
      title: 'Technical Documentation',
      content: 'API endpoints and integration guides',
      nodeType: 'text',
      depth: 0,
      parentId: null,
      expanded: true,
      hasChildren: true,
      children: [
        {
          id: 'node-2-1',
          title: 'REST API',
          content: 'Standard HTTP endpoints for data access',
          nodeType: 'entity',
          depth: 1,
          parentId: 'node-2',
          expanded: true,
          hasChildren: false,
          children: []
        }
      ]
    }
  ];

  // AI response samples for testing import
  const aiResponseSamples = {
    chatgpt: `# Project Enhancement Recommendations

Based on your project overview, here are key recommendations:

## Core Improvements
- **Enhanced Security**: Implement multi-factor authentication
- **Scalability**: Add horizontal scaling with load balancers  
- **Monitoring**: Integrate comprehensive logging and metrics

## Implementation Steps
1. Set up MFA system with TOTP
2. Configure auto-scaling groups
3. Deploy monitoring stack (Prometheus + Grafana)

> **Performance Note**: Current target of sub-100ms is aggressive but achievable with proper caching.

\`\`\`javascript
// Example caching implementation
const cache = new Map();
function getCachedData(key) {
  return cache.get(key) || null;
}
\`\`\``,

    claude: `# Technical Analysis & Recommendations

## Project Assessment
Your current architecture shows solid foundations with the authentication system and data management components.

### Strengths
- Clear separation of concerns
- Well-defined performance targets
- Comprehensive feature coverage

### Areas for Enhancement

**Security Enhancements**
- Implement rate limiting for API endpoints
- Add input validation middleware
- Consider zero-trust architecture principles

**Performance Optimizations**
- Database query optimization
- Implement caching layers (Redis/Memcached)
- Use CDN for static assets

### Implementation Priority
1. **High Priority**: Security hardening
2. **Medium Priority**: Performance optimizations  
3. **Low Priority**: UI/UX improvements

> The 1000+ concurrent user target requires careful capacity planning and stress testing.

\`\`\`typescript
interface PerformanceConfig {
  maxConcurrentUsers: number;
  responseTimeTarget: number;
  cacheTTL: number;
}

const config: PerformanceConfig = {
  maxConcurrentUsers: 1000,
  responseTimeTarget: 100,
  cacheTTL: 300
};
\`\`\``
  };

  // Demo state
  let currentNodes = [...sampleNodes];
  let exportResult: ExportResult | null = null;
  let importResult: ImportResult | null = null;
  let markdownContent = '';
  let importMarkdownContent = '';
  let selectedAIType: 'chatgpt' | 'claude' = 'chatgpt';
  let showAdvancedConfig = false;
  let validationResults: any = null;

  // Configuration
  let config: Partial<AIIntegrationConfig> = {
    exportStyle: 'ai-optimized',
    cleanAIPatterns: true,
    validationLevel: 'moderate',
    processSoftNewlines: true,
    enableWYSIWYG: true,
    maxDepth: 10
  };

  // Performance metrics
  let performanceMetrics = {
    lastExportTime: 0,
    lastImportTime: 0,
    roundTripIntegrity: 0
  };

  // Export functionality
  async function exportNodes() {
    const startTime = performance.now();
    
    try {
      const service = selectedAIType === 'claude' ? claudeIntegrationService : chatGPTIntegrationService;
      exportResult = await service.exportToMarkdown(currentNodes, config);
      markdownContent = exportResult.markdown;
      performanceMetrics.lastExportTime = performance.now() - startTime;
    } catch (error) {
      console.error('Export failed:', error);
      exportResult = null;
    }
  }

  // Import functionality
  async function importMarkdown() {
    if (!importMarkdownContent.trim()) return;
    
    const startTime = performance.now();
    
    try {
      const service = selectedAIType === 'claude' ? claudeIntegrationService : chatGPTIntegrationService;
      importResult = await service.importFromMarkdown(importMarkdownContent, config);
      performanceMetrics.lastImportTime = performance.now() - startTime;
    } catch (error) {
      console.error('Import failed:', error);
      importResult = null;
    }
  }

  // Validate markdown before import
  async function validateMarkdown() {
    if (!importMarkdownContent.trim()) return;
    
    try {
      validationResults = await AIIntegrationUtils.validateMarkdown(importMarkdownContent);
    } catch (error) {
      console.error('Validation failed:', error);
    }
  }

  // Round-trip test
  async function performRoundTripTest() {
    try {
      const service = selectedAIType === 'claude' ? claudeIntegrationService : chatGPTIntegrationService;
      const roundTripResult = await service.validateRoundTrip(currentNodes, config);
      performanceMetrics.roundTripIntegrity = roundTripResult.integrityScore;
      
      // Show results
      exportResult = roundTripResult.exportResult;
      importResult = roundTripResult.importResult;
      markdownContent = roundTripResult.exportResult.markdown;
    } catch (error) {
      console.error('Round-trip test failed:', error);
    }
  }

  // Load sample AI response
  function loadSampleResponse(type: 'chatgpt' | 'claude') {
    importMarkdownContent = aiResponseSamples[type];
    selectedAIType = type;
  }

  // Apply imported nodes
  function applyImportedNodes() {
    if (importResult && importResult.nodes.length > 0) {
      currentNodes = [...importResult.nodes];
    }
  }

  // Update configuration
  function updateConfig() {
    // Configuration is bound to form inputs, automatically updates
    // This function is called when config changes
  }

  // Copy markdown to clipboard
  async function copyToClipboard(content: string) {
    try {
      await navigator.clipboard.writeText(content);
    } catch (error) {
      console.error('Failed to copy to clipboard:', error);
    }
  }

  // Format time display
  function formatTime(ms: number): string {
    return `${ms.toFixed(2)}ms`;
  }

  // Format integrity score
  function formatIntegrity(score: number): string {
    return `${(score * 100).toFixed(1)}%`;
  }

  onMount(() => {
    // Initialize with export of sample nodes
    exportNodes();
  });
</script>

<div class="ai-integration-demo">
  <h1>AI Integration Demo</h1>
  
  <p class="demo-description">
    Comprehensive markdown import/export for seamless AI integration. Export NodeSpace content 
    for AI processing (ChatGPT, Claude) and import AI-generated responses back into structured nodes.
  </p>

  <!-- AI Type Selection -->
  <section class="ai-selection">
    <h2>AI Integration Type</h2>
    <div class="ai-type-buttons">
      <button
        class="ai-button"
        class:active={selectedAIType === 'chatgpt'}
        on:click={() => selectedAIType = 'chatgpt'}
      >
        ChatGPT Integration
      </button>
      <button
        class="ai-button"
        class:active={selectedAIType === 'claude'}
        on:click={() => selectedAIType = 'claude'}
      >
        Claude Integration
      </button>
    </div>
  </section>

  <!-- Source Node Tree -->
  <section class="source-section">
    <h2>Source NodeSpace Hierarchy</h2>
    <p>Edit the node structure and export to AI-optimized markdown:</p>
    
    <div class="node-tree-container">
      <NodeTree 
        nodes={currentNodes}
        editable={true}
        on:nodesChanged={(e) => currentNodes = e.detail.nodes}
      />
    </div>

    <div class="export-controls">
      <button class="primary-button" on:click={exportNodes}>
        Export to {selectedAIType === 'claude' ? 'Claude' : 'ChatGPT'} Markdown
      </button>
      <button class="secondary-button" on:click={performRoundTripTest}>
        Test Round-trip Integrity
      </button>
    </div>
  </section>

  <!-- Export Results -->
  {#if exportResult}
    <section class="export-results">
      <div class="section-header">
        <h2>Exported Markdown</h2>
        <button class="copy-button" on:click={() => copyToClipboard(markdownContent)}>
          Copy to Clipboard
        </button>
      </div>
      
      <div class="markdown-container">
        <textarea
          class="markdown-output"
          bind:value={markdownContent}
          readonly
          rows="15"
          placeholder="Exported markdown will appear here..."
        ></textarea>
      </div>

      <div class="export-stats">
        <div class="stat">
          <span class="stat-label">Nodes Processed:</span>
          <span class="stat-value">{exportResult.stats.nodesProcessed}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Export Time:</span>
          <span class="stat-value">{formatTime(exportResult.stats.processingTime)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Characters:</span>
          <span class="stat-value">{exportResult.stats.charactersExported}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Lines Generated:</span>
          <span class="stat-value">{exportResult.stats.linesGenerated}</span>
        </div>
      </div>

      {#if exportResult.warnings.length > 0}
        <details class="warnings">
          <summary>Export Warnings ({exportResult.warnings.length})</summary>
          <ul>
            {#each exportResult.warnings as warning}
              <li>{warning}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </section>
  {/if}

  <!-- Import Section -->
  <section class="import-section">
    <div class="section-header">
      <h2>Import AI Response</h2>
      <div class="sample-buttons">
        <button class="sample-button" on:click={() => loadSampleResponse('chatgpt')}>
          Load ChatGPT Sample
        </button>
        <button class="sample-button" on:click={() => loadSampleResponse('claude')}>
          Load Claude Sample
        </button>
      </div>
    </div>

    <div class="import-controls">
      <textarea
        class="import-textarea"
        bind:value={importMarkdownContent}
        rows="12"
        placeholder="Paste AI-generated markdown here..."
      ></textarea>

      <div class="import-buttons">
        <button class="secondary-button" on:click={validateMarkdown}>
          Validate Markdown
        </button>
        <button class="primary-button" on:click={importMarkdown}>
          Import to NodeSpace
        </button>
      </div>
    </div>

    {#if validationResults}
      <div class="validation-results">
        <h3>Validation Results</h3>
        <div class="validation-summary">
          <span class="validation-status" class:valid={validationResults.isValid}>
            {validationResults.isValid ? '✓ Valid' : '✗ Invalid'}
          </span>
          <span class="pattern-count">{validationResults.patterns.length} patterns detected</span>
        </div>
        
        {#if validationResults.warnings.length > 0}
          <div class="validation-warnings">
            <h4>Warnings:</h4>
            <ul>
              {#each validationResults.warnings as warning}
                <li>{warning}</li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
  </section>

  <!-- Import Results -->
  {#if importResult}
    <section class="import-results">
      <div class="section-header">
        <h2>Import Results</h2>
        <button class="apply-button" on:click={applyImportedNodes}>
          Apply to Node Tree
        </button>
      </div>

      <div class="imported-tree-container">
        <NodeTree 
          nodes={importResult.nodes}
          editable={false}
        />
      </div>

      <div class="import-stats">
        <div class="stat">
          <span class="stat-label">Nodes Created:</span>
          <span class="stat-value">{importResult.stats.nodesCreated}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Import Time:</span>
          <span class="stat-value">{formatTime(importResult.stats.processingTime)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Patterns Processed:</span>
          <span class="stat-value">{importResult.stats.patternsProcessed}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Bullet Conversions:</span>
          <span class="stat-value">{importResult.stats.bulletConversions}</span>
        </div>
      </div>

      {#if importResult.validation}
        <div class="validation-summary">
          <div class="validation-metric">
            <span class="metric-label">Validity:</span>
            <span class="metric-value" class:valid={importResult.validation.isValid}>
              {importResult.validation.isValid ? 'Valid' : 'Invalid'}
            </span>
          </div>
          <div class="validation-metric">
            <span class="metric-label">Integrity Score:</span>
            <span class="metric-value">{formatIntegrity(importResult.validation.integrityScore)}</span>
          </div>
        </div>
      {/if}

      {#if importResult.warnings.length > 0}
        <details class="warnings">
          <summary>Import Warnings ({importResult.warnings.length})</summary>
          <ul>
            {#each importResult.warnings as warning}
              <li>{warning}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </section>
  {/if}

  <!-- Configuration Panel -->
  <section class="config-panel">
    <div class="config-header">
      <h2>AI Integration Configuration</h2>
      <button
        class="toggle-advanced"
        on:click={() => showAdvancedConfig = !showAdvancedConfig}
      >
        {showAdvancedConfig ? 'Hide' : 'Show'} Advanced Options
      </button>
    </div>

    <div class="basic-config">
      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={config.cleanAIPatterns}
          on:change={updateConfig}
        />
        Clean AI-specific Patterns
      </label>

      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={config.processSoftNewlines}
          on:change={updateConfig}
        />
        Process Soft Newlines
      </label>

      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={config.enableWYSIWYG}
          on:change={updateConfig}
        />
        Enable WYSIWYG Processing
      </label>

      <label class="config-item">
        Export Style:
        <select bind:value={config.exportStyle} on:change={updateConfig}>
          <option value="standard">Standard</option>
          <option value="ai-optimized">AI Optimized</option>
          <option value="compact">Compact</option>
        </select>
      </label>
    </div>

    {#if showAdvancedConfig}
      <div class="advanced-config">
        <label class="config-item">
          Max Depth:
          <input
            type="number"
            bind:value={config.maxDepth}
            min="1"
            max="20"
            step="1"
            on:change={updateConfig}
          />
        </label>

        <label class="config-item">
          Validation Level:
          <select bind:value={config.validationLevel} on:change={updateConfig}>
            <option value="permissive">Permissive</option>
            <option value="moderate">Moderate</option>
            <option value="strict">Strict</option>
          </select>
        </label>

        <label class="config-item">
          <input
            type="checkbox"
            bind:checked={config.includeMetadata}
            on:change={updateConfig}
          />
          Include Export Metadata
        </label>

        <label class="config-item">
          <input
            type="checkbox"
            bind:checked={config.preserveNodeIds}
            on:change={updateConfig}
          />
          Preserve Node IDs
        </label>
      </div>
    {/if}
  </section>

  <!-- Performance Metrics -->
  {#if performanceMetrics.lastExportTime > 0 || performanceMetrics.lastImportTime > 0}
    <section class="performance-metrics">
      <h2>Performance Metrics</h2>
      <div class="metrics-grid">
        {#if performanceMetrics.lastExportTime > 0}
          <div class="metric">
            <span class="metric-label">Last Export:</span>
            <span class="metric-value">{formatTime(performanceMetrics.lastExportTime)}</span>
          </div>
        {/if}
        {#if performanceMetrics.lastImportTime > 0}
          <div class="metric">
            <span class="metric-label">Last Import:</span>
            <span class="metric-value">{formatTime(performanceMetrics.lastImportTime)}</span>
          </div>
        {/if}
        {#if performanceMetrics.roundTripIntegrity > 0}
          <div class="metric">
            <span class="metric-label">Round-trip Integrity:</span>
            <span class="metric-value integrity" class:high={performanceMetrics.roundTripIntegrity > 0.9}>
              {formatIntegrity(performanceMetrics.roundTripIntegrity)}
            </span>
          </div>
        {/if}
      </div>
    </section>
  {/if}

  <!-- Usage Guide -->
  <section class="usage-guide">
    <h2>How to Use AI Integration</h2>
    <div class="steps">
      <div class="step">
        <h3>1. Export for AI</h3>
        <p>Select your node hierarchy and export to AI-optimized markdown format.</p>
      </div>
      <div class="step">
        <h3>2. Process with AI</h3>
        <p>Copy the markdown and paste into ChatGPT, Claude, or your preferred AI tool.</p>
      </div>
      <div class="step">
        <h3>3. Import Response</h3>
        <p>Paste the AI response and import back into NodeSpace as structured nodes.</p>
      </div>
      <div class="step">
        <h3>4. Validate & Apply</h3>
        <p>Review the imported structure and apply to your node tree.</p>
      </div>
    </div>
  </section>
</div>

<style>
  .ai-integration-demo {
    max-width: 1400px;
    margin: 0 auto;
    padding: 24px;
    font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
    line-height: 1.6;
  }

  .demo-description {
    font-size: 18px;
    color: #6b7280;
    margin-bottom: 32px;
  }

  section {
    margin-bottom: 48px;
    padding: 24px;
    border: 1px solid #e5e7eb;
    border-radius: 12px;
    background: #fafafa;
  }

  h1 {
    font-size: 32px;
    font-weight: bold;
    margin-bottom: 16px;
  }

  h2 {
    font-size: 24px;
    font-weight: 600;
    margin-bottom: 16px;
    color: #1f2937;
  }

  h3 {
    font-size: 18px;
    font-weight: 600;
    margin-bottom: 8px;
  }

  /* AI Type Selection */
  .ai-type-buttons {
    display: flex;
    gap: 16px;
  }

  .ai-button {
    padding: 12px 24px;
    border: 2px solid #d1d5db;
    border-radius: 8px;
    background: white;
    cursor: pointer;
    font-weight: 500;
    transition: all 0.2s;
  }

  .ai-button:hover {
    background: #f3f4f6;
  }

  .ai-button.active {
    background: #3b82f6;
    color: white;
    border-color: #3b82f6;
  }

  /* Node Tree Container */
  .node-tree-container,
  .imported-tree-container {
    border: 2px dashed #d1d5db;
    border-radius: 8px;
    padding: 16px;
    background: white;
    margin: 16px 0;
    max-height: 400px;
    overflow-y: auto;
  }

  /* Button Styles */
  .primary-button {
    background: #3b82f6;
    color: white;
    border: none;
    padding: 12px 24px;
    border-radius: 6px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.2s;
  }

  .primary-button:hover {
    background: #2563eb;
  }

  .secondary-button {
    background: #6b7280;
    color: white;
    border: none;
    padding: 12px 24px;
    border-radius: 6px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.2s;
  }

  .secondary-button:hover {
    background: #4b5563;
  }

  .export-controls,
  .import-buttons {
    display: flex;
    gap: 12px;
    margin-top: 16px;
  }

  /* Section Headers */
  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .copy-button,
  .apply-button {
    background: #059669;
    color: white;
    border: none;
    padding: 8px 16px;
    border-radius: 4px;
    font-size: 14px;
    cursor: pointer;
  }

  /* Sample buttons */
  .sample-buttons {
    display: flex;
    gap: 8px;
  }

  .sample-button {
    background: #f3f4f6;
    border: 1px solid #d1d5db;
    padding: 6px 12px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }

  .sample-button:hover {
    background: #e5e7eb;
  }

  /* Markdown containers */
  .markdown-container {
    margin: 16px 0;
  }

  .markdown-output,
  .import-textarea {
    width: 100%;
    padding: 16px;
    border: 1px solid #d1d5db;
    border-radius: 8px;
    font-family: monospace;
    font-size: 14px;
    line-height: 1.5;
    resize: vertical;
    background: white;
  }

  .markdown-output {
    background: #f8f9fa;
  }

  /* Statistics */
  .export-stats,
  .import-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
    margin: 16px 0;
  }

  .stat {
    display: flex;
    justify-content: space-between;
    padding: 8px 12px;
    background: white;
    border-radius: 6px;
    border: 1px solid #e5e7eb;
  }

  .stat-label {
    font-weight: 500;
    color: #6b7280;
  }

  .stat-value {
    font-weight: 600;
    color: #1f2937;
  }

  /* Validation */
  .validation-results {
    margin: 16px 0;
    padding: 16px;
    background: white;
    border-radius: 8px;
    border: 1px solid #e5e7eb;
  }

  .validation-summary {
    display: flex;
    gap: 16px;
    align-items: center;
    margin-bottom: 12px;
  }

  .validation-status {
    font-weight: 600;
    padding: 4px 8px;
    border-radius: 4px;
  }

  .validation-status.valid {
    background: #d1fae5;
    color: #059669;
  }

  .validation-status:not(.valid) {
    background: #fee2e2;
    color: #dc2626;
  }

  .pattern-count {
    color: #6b7280;
    font-size: 14px;
  }

  .validation-warnings {
    margin-top: 12px;
  }

  .validation-warnings h4 {
    font-size: 14px;
    color: #d97706;
    margin-bottom: 8px;
  }

  .validation-warnings ul {
    margin: 0;
    padding-left: 20px;
  }

  .validation-warnings li {
    font-size: 14px;
    color: #92400e;
    margin-bottom: 4px;
  }

  /* Configuration */
  .config-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .toggle-advanced {
    background: #6b7280;
    color: white;
    border: none;
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 14px;
  }

  .basic-config,
  .advanced-config {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 16px;
  }

  .advanced-config {
    margin-top: 16px;
    padding-top: 16px;
    border-top: 1px solid #e5e7eb;
  }

  .config-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
  }

  .config-item input[type="checkbox"] {
    width: 16px;
    height: 16px;
  }

  .config-item input[type="number"],
  .config-item select {
    padding: 4px 8px;
    border: 1px solid #d1d5db;
    border-radius: 4px;
    min-width: 80px;
  }

  /* Performance Metrics */
  .metrics-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .metric {
    display: flex;
    justify-content: space-between;
    padding: 12px 16px;
    background: white;
    border-radius: 8px;
    border: 1px solid #e5e7eb;
  }

  .metric-label {
    font-weight: 500;
    color: #6b7280;
  }

  .metric-value {
    font-weight: 600;
    color: #1f2937;
  }

  .metric-value.integrity.high {
    color: #059669;
  }

  .metric-value.integrity:not(.high) {
    color: #dc2626;
  }

  /* Warnings */
  .warnings {
    margin-top: 16px;
    padding: 12px;
    background: #fef3cd;
    border: 1px solid #fbbf24;
    border-radius: 6px;
  }

  .warnings summary {
    font-weight: 500;
    color: #92400e;
    cursor: pointer;
  }

  .warnings ul {
    margin: 8px 0 0 0;
    padding-left: 20px;
  }

  .warnings li {
    color: #92400e;
    font-size: 14px;
    margin-bottom: 4px;
  }

  /* Usage Guide */
  .steps {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 24px;
  }

  .step {
    padding: 20px;
    background: white;
    border-radius: 8px;
    border: 1px solid #e5e7eb;
  }

  .step h3 {
    color: #3b82f6;
    margin-bottom: 8px;
  }

  .step p {
    color: #6b7280;
    margin: 0;
    font-size: 14px;
  }

  /* Responsive Design */
  @media (max-width: 768px) {
    .ai-integration-demo {
      padding: 16px;
    }

    .section-header {
      flex-direction: column;
      align-items: flex-start;
      gap: 12px;
    }

    .ai-type-buttons {
      flex-direction: column;
    }

    .export-controls,
    .import-buttons {
      flex-direction: column;
    }

    .sample-buttons {
      flex-direction: column;
      width: 100%;
    }

    .basic-config,
    .advanced-config {
      grid-template-columns: 1fr;
    }

    .steps {
      grid-template-columns: 1fr;
    }
  }
</style>