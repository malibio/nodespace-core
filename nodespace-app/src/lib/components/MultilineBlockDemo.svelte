<!--
  Multi-line Block Behavior Demo
  
  Interactive demonstration of multi-line block behavior for blockquotes and code blocks.
  Shows continuation patterns, termination detection, and WYSIWYG integration.
-->

<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { 
    multilineBlockProcessor, 
    MultilineBlockIntegration,
    type MultilineBlock,
    type BlockContinuationContext 
  } from '$lib/services/multilineBlockProcessor';
  import { wysiwygProcessor } from '$lib/services/wysiwygProcessor';
  import { markdownPatternDetector } from '$lib/services/markdownPatternDetector';

  // Demo content
  let content = `> This is a multi-line blockquote
> that spans several lines
> and demonstrates continuation

Here's some regular text.

\`\`\`javascript
function example() {
  console.log("Multi-line code block");
  return "with proper syntax highlighting";
}
\`\`\`

> Another blockquote
> with proper indentation
>
> And separated paragraphs within the quote`;

  let editorElement: HTMLElement;
  let cursorPosition = 0;
  let blocks: MultilineBlock[] = [];
  let blockContext: BlockContinuationContext | null = null;
  let isProcessing = false;
  let processingTime = 0;
  
  // Demo state
  let showDebugInfo = true;
  let enableAutoContinuation = true;
  let showRawHTML = false;
  let processedHTML = '';

  // Unsubscribe functions
  let unsubscribeBlocks: (() => void) | null = null;

  onMount(() => {
    // Subscribe to block changes
    unsubscribeBlocks = multilineBlockProcessor.subscribe((newBlocks) => {
      blocks = newBlocks;
    });

    // Initial processing
    processContent();

    // Set up editor event handlers
    if (editorElement) {
      editorElement.addEventListener('keydown', handleKeydown);
      editorElement.addEventListener('input', handleInput);
      editorElement.addEventListener('click', handleClick);
    }
  });

  onDestroy(() => {
    unsubscribeBlocks?.();
    
    if (editorElement) {
      editorElement.removeEventListener('keydown', handleKeydown);
      editorElement.removeEventListener('input', handleInput);
      editorElement.removeEventListener('click', handleClick);
    }
  });

  function handleKeydown(event: KeyboardEvent) {
    updateCursorPosition();
    
    // Handle multi-line block keyboard events
    const handled = MultilineBlockIntegration.handleKeyboardEvent(
      event,
      content,
      cursorPosition
    );
    
    if (handled) {
      // Update content from DOM after handling
      setTimeout(() => {
        content = editorElement.textContent || '';
        processContent();
      }, 0);
    }
  }

  function handleInput(event: InputEvent) {
    content = editorElement.textContent || '';
    updateCursorPosition();
    processContent();
  }

  function handleClick(event: MouseEvent) {
    setTimeout(() => {
      updateCursorPosition();
      updateBlockContext();
    }, 0);
  }

  function updateCursorPosition() {
    if (!editorElement) return;
    
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;

    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(editorElement);
    preCaretRange.setEnd(range.endContainer, range.endOffset);
    
    cursorPosition = preCaretRange.toString().length;
    updateBlockContext();
  }

  function updateBlockContext() {
    blockContext = multilineBlockProcessor.getBlockContinuationContext(content, cursorPosition);
  }

  async function processContent() {
    const startTime = performance.now();
    isProcessing = true;

    try {
      // Detect multi-line blocks
      blocks = multilineBlockProcessor.detectMultilineBlocks(content, cursorPosition);
      
      // Process with WYSIWYG
      const wysiwygResult = await wysiwygProcessor.process(content, cursorPosition);
      processedHTML = wysiwygResult.processedHTML;
      
      // Update block context
      updateBlockContext();
      
    } finally {
      isProcessing = false;
      processingTime = performance.now() - startTime;
    }
  }

  function handleExampleSelect(example: string) {
    switch (example) {
      case 'blockquote':
        content = `> This is the first line of a quote
> This is the second line
> And this continues the same quote block

Regular text here breaks the quote.

> This starts a new quote block
> With its own continuation`;
        break;
        
      case 'codeblock':
        content = `Here's a JavaScript example:

\`\`\`javascript
function multilineExample() {
  const blocks = detectBlocks(content);
  return blocks.filter(b => b.type === 'codeblock');
}
\`\`\`

And here's some Python:

\`\`\`python
def another_example():
    print("Multi-line code blocks")
    return "work great!"
\`\`\``;
        break;
        
      case 'mixed':
        content = `# Mixed Multi-line Content

> Here's a blockquote that spans
> multiple lines with proper
> continuation behavior

Some regular text in between.

\`\`\`typescript
// Code block with TypeScript
interface BlockType {
  type: 'blockquote' | 'codeblock';
  lines: string[];
  incomplete: boolean;
}
\`\`\`

> Another quote after the code
> with more content
> and proper formatting

Final regular text.`;
        break;
        
      case 'nested':
        content = `> Top-level quote
> with multiple lines
>
> > Nested quote within
> > the main quote block
> > continues here
>
> Back to main quote level
> and finishing up`;
        break;
    }
    
    // Update editor content
    if (editorElement) {
      editorElement.textContent = content;
    }
    
    processContent();
  }

  function formatBlockType(type: string): string {
    return type.charAt(0).toUpperCase() + type.slice(1);
  }

  function formatLineNumbers(numbers: number[]): string {
    if (numbers.length <= 3) {
      return numbers.join(', ');
    }
    return `${numbers[0]}-${numbers[numbers.length - 1]} (${numbers.length} lines)`;
  }

  // Reactive updates
  $: if (enableAutoContinuation !== undefined) {
    multilineBlockProcessor.options.autoContinue = enableAutoContinuation;
  }
</script>

<div class="multiline-demo">
  <div class="demo-header">
    <h2>Multi-line Block Behavior Demo</h2>
    <p>Interactive demonstration of blockquotes and code blocks that span multiple lines.</p>
  </div>

  <!-- Demo Controls -->
  <div class="demo-controls">
    <div class="control-section">
      <h3>Examples</h3>
      <div class="button-group">
        <button on:click={() => handleExampleSelect('blockquote')}>
          Blockquotes
        </button>
        <button on:click={() => handleExampleSelect('codeblock')}>
          Code Blocks
        </button>
        <button on:click={() => handleExampleSelect('mixed')}>
          Mixed Content
        </button>
        <button on:click={() => handleExampleSelect('nested')}>
          Nested Quotes
        </button>
      </div>
    </div>

    <div class="control-section">
      <h3>Options</h3>
      <label>
        <input 
          type="checkbox" 
          bind:checked={enableAutoContinuation}
        />
        Auto-continuation on Enter
      </label>
      <label>
        <input 
          type="checkbox" 
          bind:checked={showDebugInfo}
        />
        Show debug info
      </label>
      <label>
        <input 
          type="checkbox" 
          bind:checked={showRawHTML}
        />
        Show processed HTML
      </label>
    </div>
  </div>

  <!-- Main Editor -->
  <div class="editor-container">
    <div class="editor-header">
      <h3>Editor</h3>
      <div class="editor-stats">
        <span class="stat">
          Cursor: {cursorPosition}
        </span>
        <span class="stat">
          Blocks: {blocks.length}
        </span>
        <span class="stat">
          Processing: {processingTime.toFixed(2)}ms
        </span>
        {#if isProcessing}
          <span class="stat processing">Processing...</span>
        {/if}
      </div>
    </div>
    
    <div 
      class="editor"
      contenteditable="true"
      bind:this={editorElement}
      spellcheck="false"
    >
      {content}
    </div>
  </div>

  <!-- Debug Information -->
  {#if showDebugInfo}
    <div class="debug-section">
      <div class="debug-panel">
        <h3>Detected Multi-line Blocks</h3>
        {#if blocks.length === 0}
          <p class="no-data">No multi-line blocks detected</p>
        {:else}
          <div class="blocks-list">
            {#each blocks as block, index}
              <div class="block-item" class:incomplete={block.incomplete}>
                <div class="block-header">
                  <span class="block-type">{formatBlockType(block.type)}</span>
                  {#if block.language}
                    <span class="block-language">({block.language})</span>
                  {/if}
                  {#if block.incomplete}
                    <span class="incomplete-badge">Incomplete</span>
                  {/if}
                </div>
                <div class="block-details">
                  <span>Lines: {formatLineNumbers(block.lineNumbers)}</span>
                  <span>Position: {block.start}-{block.end}</span>
                  <span>Indent: {block.indentLevel}</span>
                </div>
                <div class="block-content">
                  <pre>{block.combinedContent}</pre>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="debug-panel">
        <h3>Block Context at Cursor</h3>
        {#if blockContext}
          <div class="context-info">
            <div class="context-row">
              <span class="label">In Block:</span>
              <span class="value" class:active={blockContext.inBlock}>
                {blockContext.inBlock ? 'Yes' : 'No'}
              </span>
            </div>
            {#if blockContext.inBlock && blockContext.currentBlock}
              <div class="context-row">
                <span class="label">Block Type:</span>
                <span class="value">{formatBlockType(blockContext.currentBlock.type)}</span>
              </div>
              <div class="context-row">
                <span class="label">Should Continue:</span>
                <span class="value" class:active={blockContext.shouldContinue}>
                  {blockContext.shouldContinue ? 'Yes' : 'No'}
                </span>
              </div>
              {#if blockContext.expectedContinuation}
                <div class="context-row">
                  <span class="label">Continuation:</span>
                  <span class="value code">"{blockContext.expectedContinuation}"</span>
                </div>
              {/if}
            {/if}
          </div>
        {:else}
          <p class="no-data">No context available</p>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Processed HTML View -->
  {#if showRawHTML}
    <div class="html-section">
      <h3>Processed HTML Output</h3>
      <pre class="html-output">{processedHTML}</pre>
    </div>
  {/if}

  <!-- Instructions -->
  <div class="instructions">
    <h3>Try This:</h3>
    <ul>
      <li><strong>Blockquotes:</strong> Type <code>&gt;</code> at the beginning of a line, then press Enter to auto-continue</li>
      <li><strong>Code blocks:</strong> Type <code>```</code> to start, language optional, Enter continues within block</li>
      <li><strong>Termination:</strong> Empty line or different markdown pattern terminates blocks</li>
      <li><strong>Indentation:</strong> Indented blocks maintain their indentation in continuations</li>
      <li><strong>Mixed content:</strong> Blocks work alongside other markdown patterns</li>
    </ul>
  </div>
</div>

<style>
  .multiline-demo {
    max-width: 1200px;
    margin: 0 auto;
    padding: 24px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .demo-header {
    margin-bottom: 32px;
    text-align: center;
  }

  .demo-header h2 {
    color: #1f2937;
    margin-bottom: 8px;
  }

  .demo-header p {
    color: #6b7280;
    font-size: 16px;
  }

  .demo-controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
    margin-bottom: 32px;
    padding: 20px;
    background: #f8fafc;
    border-radius: 8px;
  }

  .control-section h3 {
    margin-bottom: 12px;
    color: #374151;
    font-size: 14px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .button-group {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .button-group button {
    padding: 8px 16px;
    background: #3b82f6;
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .button-group button:hover {
    background: #2563eb;
  }

  .control-section label {
    display: block;
    margin-bottom: 8px;
    font-size: 14px;
    color: #374151;
  }

  .control-section input[type="checkbox"] {
    margin-right: 8px;
  }

  .editor-container {
    margin-bottom: 32px;
  }

  .editor-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid #e5e7eb;
  }

  .editor-header h3 {
    margin: 0;
    color: #1f2937;
  }

  .editor-stats {
    display: flex;
    gap: 16px;
    font-size: 12px;
  }

  .stat {
    color: #6b7280;
    font-family: monospace;
  }

  .stat.processing {
    color: #f59e0b;
    animation: pulse 1s infinite;
  }

  .editor {
    min-height: 300px;
    padding: 16px;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    background: white;
    font-family: monospace;
    font-size: 14px;
    line-height: 1.6;
    outline: none;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }

  .editor:focus {
    border-color: #3b82f6;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
  }

  .debug-section {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
    margin-bottom: 32px;
  }

  .debug-panel {
    background: #f8fafc;
    border-radius: 8px;
    padding: 20px;
    border: 1px solid #e5e7eb;
  }

  .debug-panel h3 {
    margin-bottom: 16px;
    color: #1f2937;
    font-size: 16px;
  }

  .no-data {
    color: #6b7280;
    font-style: italic;
    text-align: center;
    padding: 20px;
  }

  .blocks-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .block-item {
    padding: 12px;
    background: white;
    border-radius: 6px;
    border: 1px solid #e5e7eb;
  }

  .block-item.incomplete {
    border-left: 4px solid #f59e0b;
  }

  .block-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .block-type {
    font-weight: 600;
    color: #1f2937;
  }

  .block-language {
    font-size: 12px;
    color: #6b7280;
  }

  .incomplete-badge {
    padding: 2px 6px;
    background: #fef3c7;
    color: #d97706;
    font-size: 10px;
    border-radius: 4px;
    text-transform: uppercase;
    font-weight: 600;
  }

  .block-details {
    display: flex;
    gap: 16px;
    margin-bottom: 8px;
    font-size: 12px;
    color: #6b7280;
  }

  .block-content {
    background: #f9fafb;
    border-radius: 4px;
    padding: 8px;
  }

  .block-content pre {
    margin: 0;
    font-size: 12px;
    color: #374151;
    white-space: pre-wrap;
  }

  .context-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .context-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .label {
    font-weight: 500;
    color: #374151;
  }

  .value {
    color: #6b7280;
  }

  .value.active {
    color: #059669;
    font-weight: 500;
  }

  .value.code {
    font-family: monospace;
    background: #f3f4f6;
    padding: 2px 4px;
    border-radius: 3px;
  }

  .html-section {
    margin-bottom: 32px;
  }

  .html-section h3 {
    margin-bottom: 12px;
    color: #1f2937;
  }

  .html-output {
    background: #1f2937;
    color: #f9fafb;
    padding: 16px;
    border-radius: 8px;
    font-size: 12px;
    overflow-x: auto;
    white-space: pre-wrap;
  }

  .instructions {
    background: #eff6ff;
    border: 1px solid #bfdbfe;
    border-radius: 8px;
    padding: 20px;
  }

  .instructions h3 {
    margin-bottom: 12px;
    color: #1e40af;
  }

  .instructions ul {
    margin: 0;
    padding-left: 20px;
  }

  .instructions li {
    margin-bottom: 8px;
    color: #374151;
  }

  .instructions code {
    background: #dbeafe;
    padding: 2px 4px;
    border-radius: 3px;
    font-family: monospace;
    font-size: 13px;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  @media (max-width: 768px) {
    .demo-controls,
    .debug-section {
      grid-template-columns: 1fr;
    }
    
    .editor-stats {
      flex-wrap: wrap;
      gap: 8px;
    }
    
    .button-group {
      gap: 4px;
    }
    
    .button-group button {
      font-size: 12px;
      padding: 6px 12px;
    }
  }
</style>