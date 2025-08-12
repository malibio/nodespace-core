<!--
  WYSIWYG Processing Demo Component
  
  Demonstrates the WYSIWYG processing capabilities with real-time markdown
  formatting, syntax hiding, and visual formatting as users type.
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import BaseNode from '$lib/design/components/BaseNode.svelte';
  import { wysiwygProcessor, WYSIWYGUtils } from '$lib/services/wysiwygProcessor.js';
  import type { WYSIWYGEvent } from '$lib/services/wysiwygProcessor.js';

  // Demo content samples
  const demoSamples = [
    {
      title: 'Headers',
      content: '# Main Header\n## Subheader\n### Section Header'
    },
    {
      title: 'Text Formatting',
      content: 'This has **bold text**, *italic text*, and `inline code`.'
    },
    {
      title: 'Lists and Quotes',
      content: '- First item\n- Second item\n\n> This is a blockquote\n> with multiple lines'
    },
    {
      title: 'Code Blocks',
      content: '```javascript\nfunction example() {\n  console.log("Hello, World!");\n  return true;\n}\n```'
    },
    {
      title: 'Mixed Content',
      content: '# Project Overview\n\nThis project includes **several features**:\n\n- *Real-time* markdown processing\n- `WYSIWYG` formatting\n- Syntax highlighting\n\n> Performance target: **<50ms** processing time\n\n```typescript\ninterface Config {\n  enableFormatting: boolean;\n  hideSyntax: boolean;\n}\n```'
    }
  ];

  // Demo state
  let currentSample = demoSamples[4]; // Start with mixed content
  let processingMetrics: any = null;
  let eventLog: WYSIWYGEvent[] = [];
  let showAdvancedOptions = false;
  let wysiwygConfig = {
    enableRealTime: true,
    performanceMode: false,
    maxProcessingTime: 50,
    debounceDelay: 16,
    hideSyntax: true,
    enableFormatting: true,
    cssPrefix: 'wysiwyg'
  };

  // Subscribe to WYSIWYG events
  onMount(() => {
    const unsubscribe = wysiwygProcessor.subscribe((event) => {
      eventLog = [event, ...eventLog.slice(0, 9)]; // Keep last 10 events
      processingMetrics = wysiwygProcessor.getMetrics();
    });

    // Initial configuration
    wysiwygProcessor.updateConfig(wysiwygConfig);

    return unsubscribe;
  });

  function selectSample(sample: typeof demoSamples[0]) {
    currentSample = sample;
  }

  function updateConfig() {
    wysiwygProcessor.updateConfig(wysiwygConfig);
  }

  function clearEventLog() {
    eventLog = [];
  }

  // Format timestamp for display
  function formatTime(timestamp: number): string {
    return new Date(timestamp).toLocaleTimeString();
  }

  // Format processing time
  function formatProcessingTime(time: number): string {
    return `${time.toFixed(2)}ms`;
  }
</script>

<div class="wysiwyg-demo">
  <h1>WYSIWYG Processing Demo</h1>
  
  <p class="demo-description">
    Real-time WYSIWYG processing that hides markdown syntax and applies visual formatting as you type.
    Performance optimized for sub-50ms processing times.
  </p>

  <!-- Sample Selection -->
  <section class="sample-selection">
    <h2>Demo Samples</h2>
    <div class="sample-buttons">
      {#each demoSamples as sample}
        <button
          class="sample-button"
          class:active={currentSample === sample}
          on:click={() => selectSample(sample)}
        >
          {sample.title}
        </button>
      {/each}
    </div>
  </section>

  <!-- WYSIWYG Node Demonstration -->
  <section class="wysiwyg-showcase">
    <h2>Interactive WYSIWYG Node</h2>
    <p>Click to edit and see real-time markdown processing:</p>
    
    <div class="node-container">
      <BaseNode
        nodeType="text"
        nodeId="wysiwyg-demo"
        content={currentSample.content}
        enableWYSIWYG={true}
        {wysiwygConfig}
        contentEditable={true}
        editable={true}
        multiline={true}
        placeholder="Type some markdown here..."
        on:contentChanged={(e) => {
          currentSample.content = e.detail.content;
        }}
      />
    </div>

    <!-- Raw Content Display -->
    <details class="raw-content">
      <summary>Raw Content (with markdown syntax)</summary>
      <pre class="raw-content-display">{currentSample.content}</pre>
    </details>
  </section>

  <!-- Configuration Panel -->
  <section class="config-panel">
    <h2>WYSIWYG Configuration</h2>
    
    <div class="config-grid">
      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={wysiwygConfig.enableRealTime}
          on:change={updateConfig}
        />
        Enable Real-time Processing
      </label>

      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={wysiwygConfig.hideSyntax}
          on:change={updateConfig}
        />
        Hide Syntax Characters
      </label>

      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={wysiwygConfig.enableFormatting}
          on:change={updateConfig}
        />
        Enable Visual Formatting
      </label>

      <label class="config-item">
        <input
          type="checkbox"
          bind:checked={wysiwygConfig.performanceMode}
          on:change={updateConfig}
        />
        Performance Mode
      </label>
    </div>

    <button
      class="toggle-advanced"
      on:click={() => showAdvancedOptions = !showAdvancedOptions}
    >
      {showAdvancedOptions ? 'Hide' : 'Show'} Advanced Options
    </button>

    {#if showAdvancedOptions}
      <div class="advanced-config">
        <label class="config-item">
          Max Processing Time (ms):
          <input
            type="number"
            bind:value={wysiwygConfig.maxProcessingTime}
            min="10"
            max="500"
            step="10"
            on:change={updateConfig}
          />
        </label>

        <label class="config-item">
          Debounce Delay (ms):
          <input
            type="number"
            bind:value={wysiwygConfig.debounceDelay}
            min="0"
            max="100"
            step="1"
            on:change={updateConfig}
          />
        </label>

        <label class="config-item">
          CSS Prefix:
          <input
            type="text"
            bind:value={wysiwygConfig.cssPrefix}
            on:change={updateConfig}
          />
        </label>
      </div>
    {/if}
  </section>

  <!-- Performance Metrics -->
  {#if processingMetrics}
    <section class="metrics-panel">
      <h2>Performance Metrics</h2>
      <div class="metrics-grid">
        <div class="metric">
          <span class="metric-label">Last Processing Time:</span>
          <span class="metric-value" class:slow={processingMetrics.lastProcessingTime > 50}>
            {formatProcessingTime(processingMetrics.lastProcessingTime)}
          </span>
        </div>
        <div class="metric">
          <span class="metric-label">Currently Processing:</span>
          <span class="metric-value" class:processing={processingMetrics.isProcessing}>
            {processingMetrics.isProcessing ? 'Yes' : 'No'}
          </span>
        </div>
      </div>
    </section>
  {/if}

  <!-- Event Log -->
  {#if eventLog.length > 0}
    <section class="event-log">
      <div class="event-log-header">
        <h2>Event Log</h2>
        <button class="clear-log" on:click={clearEventLog}>Clear</button>
      </div>
      
      <div class="events-container">
        {#each eventLog as event}
          <div class="event-item" data-event-type={event.type}>
            <span class="event-time">{formatTime(event.timestamp)}</span>
            <span class="event-type">{event.type}</span>
            {#if event.result}
              <span class="event-details">
                {event.result.patterns.length} patterns,
                {formatProcessingTime(event.result.processingTime)}
                {#if event.result.warnings.length > 0}
                  <span class="warnings">({event.result.warnings.length} warnings)</span>
                {/if}
              </span>
            {/if}
            {#if event.error}
              <span class="event-error">{event.error}</span>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Usage Examples -->
  <section class="usage-examples">
    <h2>Try These Examples</h2>
    <div class="examples-grid">
      <div class="example">
        <h3>Headers</h3>
        <p>Type: <code># Header 1</code> or <code>## Header 2</code></p>
      </div>
      <div class="example">
        <h3>Bold & Italic</h3>
        <p>Type: <code>**bold**</code> or <code>*italic*</code></p>
      </div>
      <div class="example">
        <h3>Code</h3>
        <p>Type: <code>`inline code`</code> or <code>```code block```</code></p>
      </div>
      <div class="example">
        <h3>Lists</h3>
        <p>Type: <code>- item</code> or <code>* item</code> or <code>+ item</code></p>
      </div>
      <div class="example">
        <h3>Quotes</h3>
        <p>Type: <code>> blockquote text</code></p>
      </div>
      <div class="example">
        <h3>Mixed</h3>
        <p>Combine patterns: <code># **Bold Header**</code></p>
      </div>
    </div>
  </section>
</div>

<style>
  .wysiwyg-demo {
    max-width: 1200px;
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

  /* Sample Selection */
  .sample-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .sample-button {
    padding: 8px 16px;
    border: 1px solid #d1d5db;
    border-radius: 6px;
    background: white;
    cursor: pointer;
    transition: all 0.2s;
  }

  .sample-button:hover {
    background: #f3f4f6;
  }

  .sample-button.active {
    background: #3b82f6;
    color: white;
    border-color: #3b82f6;
  }

  /* WYSIWYG Showcase */
  .node-container {
    border: 2px dashed #d1d5db;
    border-radius: 8px;
    padding: 16px;
    background: white;
    margin: 16px 0;
  }

  .raw-content {
    margin-top: 16px;
  }

  .raw-content-display {
    background: #f8f9fa;
    padding: 12px;
    border-radius: 6px;
    font-family: monospace;
    font-size: 14px;
    overflow-x: auto;
    white-space: pre-wrap;
  }

  /* Configuration */
  .config-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 16px;
    margin-bottom: 16px;
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
  .config-item input[type="text"] {
    padding: 4px 8px;
    border: 1px solid #d1d5db;
    border-radius: 4px;
    width: 80px;
  }

  .toggle-advanced {
    padding: 8px 16px;
    background: #6b7280;
    color: white;
    border: none;
    border-radius: 6px;
    cursor: pointer;
  }

  .advanced-config {
    margin-top: 16px;
    padding-top: 16px;
    border-top: 1px solid #e5e7eb;
  }

  /* Metrics */
  .metrics-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .metric {
    display: flex;
    justify-content: space-between;
    padding: 8px 12px;
    background: white;
    border-radius: 6px;
  }

  .metric-label {
    font-weight: 500;
  }

  .metric-value.slow {
    color: #dc2626;
    font-weight: bold;
  }

  .metric-value.processing {
    color: #059669;
    font-weight: bold;
  }

  /* Event Log */
  .event-log-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .clear-log {
    padding: 6px 12px;
    background: #ef4444;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
  }

  .events-container {
    max-height: 300px;
    overflow-y: auto;
    border: 1px solid #e5e7eb;
    border-radius: 6px;
    background: white;
  }

  .event-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    font-size: 14px;
    border-bottom: 1px solid #f3f4f6;
  }

  .event-item:last-child {
    border-bottom: none;
  }

  .event-time {
    color: #6b7280;
    font-family: monospace;
    font-size: 12px;
    white-space: nowrap;
  }

  .event-type {
    font-weight: 600;
    text-transform: capitalize;
    min-width: 80px;
  }

  .event-item[data-event-type="processed"] .event-type {
    color: #059669;
  }

  .event-item[data-event-type="error"] .event-type {
    color: #dc2626;
  }

  .event-details {
    color: #6b7280;
    font-size: 12px;
  }

  .warnings {
    color: #d97706;
    font-weight: 500;
  }

  .event-error {
    color: #dc2626;
    font-size: 12px;
  }

  /* Examples */
  .examples-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 16px;
  }

  .example {
    padding: 16px;
    background: white;
    border-radius: 8px;
    border: 1px solid #e5e7eb;
  }

  .example code {
    background: #f3f4f6;
    padding: 2px 6px;
    border-radius: 4px;
    font-family: monospace;
    font-size: 14px;
  }

  .example p {
    margin: 8px 0 0 0;
    color: #6b7280;
  }

  /* Responsive */
  @media (max-width: 768px) {
    .wysiwyg-demo {
      padding: 16px;
    }

    .config-grid {
      grid-template-columns: 1fr;
    }

    .examples-grid {
      grid-template-columns: 1fr;
    }

    .sample-buttons {
      flex-direction: column;
    }

    .metrics-grid {
      grid-template-columns: 1fr;
    }
  }
</style>