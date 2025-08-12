<!--
  Simplified Base Node Component
  
  Clean foundational visual pattern for all node types in NodeSpace.
  Simplified from ~400 lines to ~120-150 lines by removing over-engineering.
-->

<script context="module" lang="ts">
  // Node type for styling variants
  export type NodeType = 'text' | 'task' | 'ai-chat' | 'entity' | 'query';
</script>

<script lang="ts">
  import { createEventDispatcher, tick, onMount, onDestroy } from 'svelte';
  import Icon, { type IconName } from '../icons/index.js';
  import { SoftNewlineIntegration, softNewlineProcessor } from '$lib/services/softNewlineProcessor.js';
  import type { SoftNewlineContext, NodeCreationSuggestion } from '$lib/services/softNewlineProcessor.js';
  import { wysiwygProcessor, type WYSIWYGResult } from '$lib/services/wysiwygProcessor.js';

  // Essential Props (Simplified to 5 core + processing)
  export let nodeType: NodeType = 'text';
  export let nodeId: string = '';
  export let content: string = '';
  export let hasChildren: boolean = false;
  export let className: string = '';
  export let editable: boolean = true;
  export let contentEditable: boolean = true; // Can content be directly edited by user?
  export let multiline: boolean = false; // Single-line by default
  export let placeholder: string = 'Click to add content...';

  // SVG Icon System
  export let iconName: IconName | undefined = undefined;

  // Processing State System
  export let isProcessing: boolean = false;
  export let processingAnimation: 'blink' | 'pulse' | 'spin' | 'fade' = 'pulse';
  export let processingIcon: IconName | undefined = undefined;

  // WYSIWYG Processing
  export let enableWYSIWYG: boolean = true;
  export let wysiwygConfig = {
    enableRealTime: true,
    performanceMode: false,
    maxProcessingTime: 50,
    debounceDelay: 16,
    hideSyntax: true,
    enableFormatting: true
  };

  // Edit state
  let focused = false;
  let contentEditableElement: HTMLDivElement;
  let wysiwygResult: WYSIWYGResult | null = null;
  let isWysiwygProcessing = false;

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    click: { nodeId: string; event: MouseEvent };
    focus: { nodeId: string };
    blur: { nodeId: string };
    contentChanged: { nodeId: string; content: string };
    softNewlineContext: { nodeId: string; context: SoftNewlineContext };
    nodeCreationSuggested: { nodeId: string; suggestion: NodeCreationSuggestion };
  }>();

  // Handle display click to start editing
  function handleDisplayClick(event: MouseEvent) {
    if (!editable || !contentEditable || focused) return;
    startEditing(event);
  }

  // Handle general node clicks
  function handleClick(event: MouseEvent) {
    dispatch('click', { nodeId, event });

    // If clicking on display content, delegate to handleDisplayClick
    if (!focused && editable && contentEditable) {
      const target = event.target as HTMLElement;
      const displayElement = target.closest('.ns-node__display--clickable');
      if (displayElement) {
        handleDisplayClick(event);
      }
    }
  }

  // Start editing mode
  async function startEditing(clickEvent?: MouseEvent) {
    focused = true;
    dispatch('focus', { nodeId });

    await tick();
    if (contentEditableElement) {
      contentEditableElement.focus();

      // Position cursor based on click location
      if (clickEvent) {
        await tick(); // Wait for contenteditable to be fully rendered
        positionCursorFromClick(clickEvent);
      } else {
        // No click event, place cursor at end
        placeCursorAtEnd();
      }
    }
  }

  // Position cursor based on click coordinates using native Selection API
  async function positionCursorFromClick(clickEvent: MouseEvent) {
    if (!contentEditableElement) return;

    try {
      // Use document.caretPositionFromPoint or document.caretRangeFromPoint
      let caretPosition;

      if ((document as any).caretPositionFromPoint) {
        // Firefox
        caretPosition = (document as any).caretPositionFromPoint(clickEvent.clientX, clickEvent.clientY);
        if (caretPosition) {
          const selection = window.getSelection();
          const range = document.createRange();
          range.setStart(caretPosition.offsetNode, caretPosition.offset);
          range.collapse(true);
          selection?.removeAllRanges();
          selection?.addRange(range);
        }
      } else if (document.caretRangeFromPoint) {
        // Chrome, Safari
        const range = document.caretRangeFromPoint(clickEvent.clientX, clickEvent.clientY);
        if (range) {
          const selection = window.getSelection();
          selection?.removeAllRanges();
          selection?.addRange(range);
        }
      } else {
        // Fallback: place cursor at end
        placeCursorAtEnd();
      }
    } catch (error) {
      console.error('Error positioning cursor from click:', error);
      placeCursorAtEnd();
    }
  }

  // Helper function to place cursor at the end of content
  function placeCursorAtEnd() {
    if (!contentEditableElement) return;

    const selection = window.getSelection();
    const range = document.createRange();

    // Move to the end of the contenteditable element
    range.selectNodeContents(contentEditableElement);
    range.collapse(false); // false = collapse to end

    selection?.removeAllRanges();
    selection?.addRange(range);
  }

  // Handle contenteditable input
  function handleInput(event: Event & { currentTarget: HTMLDivElement }) {
    const target = event.currentTarget;
    let newContent = target.textContent || '';

    // For single-line, replace newlines with spaces
    if (!multiline) {
      newContent = newContent.replace(/\n/g, ' ');
      // Update the element content if we modified it
      if (newContent !== target.textContent) {
        target.textContent = newContent;
        placeCursorAtEnd();
      }
    }

    content = newContent;
    
    // Process with WYSIWYG if enabled
    if (enableWYSIWYG && focused) {
      processContentWYSIWYG(newContent);
    }
    
    dispatch('contentChanged', { nodeId, content });

    // Process soft newline context for multiline nodes with content changes
    if (multiline && newContent.includes('\n')) {
      processSoftNewlineContext(newContent);
    }
  }

  // Process soft newline context and emit events
  async function processSoftNewlineContext(content: string) {
    if (!contentEditableElement) return;

    try {
      const cursorPosition = SoftNewlineIntegration.getCursorPosition(contentEditableElement);
      
      // Use debounced processing for real-time detection
      const context = await SoftNewlineIntegration.handleInputChange(
        content,
        cursorPosition,
        nodeId
      );

      // Dispatch context to parent components
      dispatch('softNewlineContext', { nodeId, context });

      // If a node creation is suggested, dispatch that event too
      const suggestion = softNewlineProcessor.getNodeCreationSuggestion(context);
      if (suggestion) {
        dispatch('nodeCreationSuggested', { nodeId, suggestion });
      }
    } catch (error) {
      console.warn('Error processing soft newline context:', error);
    }
  }

  // ContentEditable automatically adjusts height based on content
  // No manual resizing needed like with textarea

  // Handle blur
  function handleBlur() {
    if (!focused) return;
    focused = false;
    dispatch('blur', { nodeId });
  }

  // Handle keyboard shortcuts
  function handleKeyDown(event: KeyboardEvent) {
    if (focused) {
      // When editing, handle Escape and Enter
      if (event.key === 'Escape') {
        event.preventDefault();
        focused = false;
        dispatch('blur', { nodeId });
        return;
      }

      // Handle soft newline detection (Shift-Enter)
      if (multiline && event.key === 'Enter' && event.shiftKey) {
        // For multiline content, allow Shift-Enter for soft newlines
        // The soft newline processor will handle pattern detection on subsequent typing
        return; // Allow default behavior (insert newline)
      }

      // For single-line, Enter saves and exits
      if (!multiline && event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        focused = false;
        dispatch('blur', { nodeId });
        return;
      }

      // For multiline, regular Enter also saves and exits
      if (multiline && event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        focused = false;
        dispatch('blur', { nodeId });
        return;
      }

      // Prevent default formatting in ContentEditable
      if (event.ctrlKey || event.metaKey) {
        const formatKeys = ['b', 'i', 'u', 'k']; // bold, italic, underline, link
        if (formatKeys.includes(event.key.toLowerCase())) {
          event.preventDefault();
          return;
        }
      }

      // Other keys pass through naturally
      return;
    }

    // When not editing, handle Enter/Space to start editing
    if (editable && contentEditable && (event.code === 'Enter' || event.code === 'Space')) {
      event.preventDefault();
      startEditing();
    }
  }

  // Handle paste events to ensure plain text only
  function handlePaste(event: Event & { currentTarget: HTMLDivElement }) {
    event.preventDefault();
    const paste = (event as any).clipboardData?.getData('text/plain') || '';

    // For single-line, replace newlines with spaces
    const cleanPaste = multiline ? paste : paste.replace(/\n/g, ' ');

    // Insert text at cursor position
    const selection = window.getSelection();
    if (selection && selection.rangeCount > 0) {
      const range = selection.getRangeAt(0);
      range.deleteContents();
      range.insertNode(document.createTextNode(cleanPaste));
      range.collapse(false);
      selection.removeAllRanges();
      selection.addRange(range);
    }

    // Update content and dispatch event
    if (contentEditableElement) {
      content = contentEditableElement.textContent || '';
      dispatch('contentChanged', { nodeId, content });
    }
  }

  // WYSIWYG processing functions
  async function processContentWYSIWYG(newContent: string) {
    if (!enableWYSIWYG || !contentEditableElement) return;

    try {
      isWysiwygProcessing = true;
      
      // Get cursor position
      const selection = window.getSelection();
      const cursorPosition = selection ? getCursorPosition() : undefined;

      // Process with real-time WYSIWYG
      wysiwygProcessor.processRealTime(newContent, cursorPosition || 0, (result) => {
        wysiwygResult = result;
        applyWYSIWYGFormatting(result);
        isWysiwygProcessing = false;
      });

    } catch (error) {
      console.warn('WYSIWYG processing error:', error);
      isWysiwygProcessing = false;
    }
  }

  function getCursorPosition(): number {
    if (!contentEditableElement) return 0;
    
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return 0;

    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(contentEditableElement);
    preCaretRange.setEnd(range.endContainer, range.endOffset);
    
    return preCaretRange.toString().length;
  }

  function applyWYSIWYGFormatting(result: WYSIWYGResult) {
    if (!contentEditableElement || !result.characterClasses) return;

    // Apply CSS classes to the element
    // For now, we apply pattern-based classes to the entire element
    // A more sophisticated implementation would wrap individual characters
    const existingClasses = Array.from(contentEditableElement.classList)
      .filter(cls => !cls.startsWith('wysiwyg-'));
    
    contentEditableElement.className = existingClasses.join(' ');
    
    // Add WYSIWYG classes based on detected patterns
    const allClasses = new Set<string>();
    Object.values(result.characterClasses).forEach(classes => {
      classes.forEach(cls => allClasses.add(cls));
    });

    allClasses.forEach(cls => {
      if (cls.startsWith('wysiwyg-')) {
        contentEditableElement.classList.add(cls);
      }
    });

    // Store patterns for reference
    if (result.patterns.length > 0) {
      contentEditableElement.dataset.patterns = JSON.stringify(result.patterns.length);
    }
  }

  // Initialize WYSIWYG processor configuration
  onMount(() => {
    if (enableWYSIWYG) {
      wysiwygProcessor.updateConfig(wysiwygConfig);
    }
  });

  // Get node type label for accessibility
  function getNodeTypeLabel(type: NodeType): string {
    switch (type) {
      case 'text':
        return 'Text';
      case 'task':
        return 'Task';
      case 'ai-chat':
        return 'AI Chat';
      case 'entity':
        return 'Entity';
      case 'query':
        return 'Query';
      default:
        return 'Node';
    }
  }

  // CSS classes
  $: nodeClasses = [
    'ns-node',
    `ns-node--${nodeType}`,
    hasChildren && 'ns-node--has-children',
    isProcessing && 'ns-node--processing',
    className
  ]
    .filter(Boolean)
    .join(' ');

  // Icon selection: processing icon overrides regular icon when processing
  $: displayIcon = isProcessing && processingIcon ? processingIcon : iconName;

  // Animation class for processing state
  $: iconAnimationClass = isProcessing ? `ns-node__icon--${processingAnimation}` : '';

  // Cleanup soft newline processor when component is destroyed
  onDestroy(() => {
    softNewlineProcessor.cancelProcessing(nodeId);
  });
</script>

<!-- Simplified node container - div approach to avoid button/contenteditable conflicts -->
<div
  class={nodeClasses}
  role="button"
  tabindex="0"
  aria-label="{getNodeTypeLabel(nodeType)}: {content || 'Empty node'}"
  data-node-id={nodeId}
  data-node-type={nodeType}
  on:click={handleClick}
  on:keydown={handleKeyDown}
>
  <!-- Node header -->
  <header class="ns-node__header">
    <!-- Icon or Circle Indicator -->
    <div class="ns-node__indicator" data-node-type={nodeType}>
      {#if displayIcon}
        <Icon name={displayIcon} size={16} className="ns-node__icon {iconAnimationClass}" />
      {:else}
        <!-- Circle fallback indicator -->
        <div
          class="ns-node__circle {iconAnimationClass}"
          class:ns-node__circle--has-children={hasChildren}
        ></div>
      {/if}
    </div>

    <!-- Node content with edit/display logic -->
    <div class="ns-node__content">
      <slot name="content">
        {#if focused && contentEditable}
          <!-- Edit mode (only if content is directly editable) -->
          <div
            bind:this={contentEditableElement}
            class="ns-node__contenteditable {multiline
              ? 'ns-node__contenteditable--multiline'
              : 'ns-node__contenteditable--single'} {enableWYSIWYG ? 'ns-node__contenteditable--wysiwyg' : ''} {isWysiwygProcessing ? 'ns-node__contenteditable--processing' : ''}"
            contenteditable="true"
            bind:textContent={content}
            on:input={handleInput}
            on:blur={handleBlur}
            on:keydown={handleKeyDown}
            on:paste={handlePaste}
            on:keydown|stopPropagation
            data-placeholder={placeholder}
            data-wysiwyg-enabled={enableWYSIWYG}
            role="textbox"
            tabindex="0"
            aria-label="Editable content"
          >
            {content}
          </div>
        {:else}
          <!-- Display mode -->
          {#if editable && contentEditable}
            <div
              class="ns-node__display ns-node__display--clickable"
              role="button"
              tabindex="0"
              on:click={handleDisplayClick}
              on:keydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  startEditing();
                }
              }}
              aria-label="Click to edit content"
            >
              <slot name="display-content">
                {#if content}
                  <span class="ns-node__text {enableWYSIWYG ? 'ns-node__text--wysiwyg' : ''}">
                    {#if enableWYSIWYG && wysiwygResult}
                      {@html wysiwygResult.processedHTML}
                    {:else}
                      {content}
                    {/if}
                  </span>
                {:else}
                  <span class="ns-node__empty">{placeholder}</span>
                {/if}
              </slot>
            </div>
          {:else}
            <!-- Non-editable display -->
            <div class="ns-node__display" role="region">
              <slot name="display-content">
                {#if content}
                  <span class="ns-node__text {enableWYSIWYG ? 'ns-node__text--wysiwyg' : ''}">
                    {#if enableWYSIWYG && wysiwygResult}
                      {@html wysiwygResult.processedHTML}
                    {:else}
                      {content}
                    {/if}
                  </span>
                {:else}
                  <span class="ns-node__empty">{placeholder}</span>
                {/if}
              </slot>
            </div>
          {/if}
        {/if}
      </slot>

      <!-- Additional node-specific content -->
      <slot></slot>
    </div>
  </header>
</div>

<style>
  /* Simplified Base Node Styling */
  .ns-node {
    /* Layout */
    display: flex;
    width: 100%;
    padding: 12px;
    margin: 4px 0;

    /* Styling */
    background-color: transparent;
    border: none;
    border-radius: 8px;
    cursor: text;
    outline: none;
    transition: all 200ms ease;

    /* Text styling */
    font: inherit;
    text-align: left;
  }

  /* Hover state - minimal for clean appearance - no styles needed */

  /* Focus state */
  .ns-node:focus-visible {
    outline: 2px solid hsl(var(--ring));
    outline-offset: 2px;
  }

  /* Processing state - indication through circle animation, no border styles needed */

  /* Header layout */
  .ns-node__header {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
  }

  /* Indicator container */
  .ns-node__indicator {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* SVG Icon styling */
  .ns-node__icon {
    color: currentColor;
  }

  /* Circle fallback indicator */
  .ns-node__circle {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    position: relative;
  }

  /* Parent nodes get a ring effect */
  .ns-node__circle--has-children::before {
    content: '';
    position: absolute;
    top: -3px;
    left: -3px;
    right: -3px;
    bottom: -3px;
    border-radius: 50%;
    border: 1px solid currentColor;
    opacity: 0.3;
  }

  /* Node type colors for circles */
  .ns-node--text .ns-node__circle {
    background-color: hsl(142 71% 45%);
  }
  .ns-node--task .ns-node__circle {
    background-color: hsl(25 95% 53%);
  }
  .ns-node--ai-chat .ns-node__circle {
    background-color: hsl(221 83% 53%);
  }
  .ns-node--entity .ns-node__circle {
    background-color: hsl(271 81% 56%);
  }
  .ns-node--query .ns-node__circle {
    background-color: hsl(330 81% 60%);
  }

  /* Content section */
  .ns-node__content {
    flex: 1;
    min-width: 0; /* Prevent overflow */
  }

  /* ContentEditable styling */
  .ns-node__contenteditable {
    width: 100%;
    min-height: 20px;
    padding: 0;
    border: none;
    border-radius: 0;
    font-family: inherit;
    font-size: 14px;
    line-height: 1.4;
    background: transparent;
    color: hsl(var(--foreground));
    outline: none;
    overflow: hidden;
  }

  /* Single-line contenteditable (default) */
  .ns-node__contenteditable--single {
    height: 20px;
    white-space: nowrap;
    overflow-x: auto;
    overflow-y: hidden;
  }

  /* Multi-line contenteditable */
  .ns-node__contenteditable--multiline {
    min-height: 20px;
    white-space: pre-wrap;
    overflow-y: auto;
  }

  /* Placeholder styling for contenteditable */
  .ns-node__contenteditable:empty::before {
    content: attr(data-placeholder);
    color: hsl(var(--muted-foreground));
    font-style: italic;
    pointer-events: none;
  }

  /* Prevent default contenteditable formatting */
  .ns-node__contenteditable * {
    font-weight: inherit !important;
    font-style: inherit !important;
    text-decoration: none !important;
  }

  /* Display styling */
  .ns-node__display {
    min-height: 20px;
  }

  .ns-node__display--clickable {
    cursor: text;
  }

  .ns-node__text {
    color: hsl(var(--foreground));
    font-size: 14px;
    line-height: 1.4;
    word-break: break-word;
  }

  .ns-node__empty {
    color: hsl(var(--muted-foreground));
    font-size: 14px;
    font-style: italic;
  }

  /* Processing Animations */
  .ns-node__icon--pulse,
  .ns-node__circle--pulse {
    animation: pulse 2s infinite;
  }

  .ns-node__icon--blink,
  .ns-node__circle--blink {
    animation: blink 1s infinite;
  }

  .ns-node__icon--spin,
  .ns-node__circle--spin {
    animation: spin 1s linear infinite;
  }

  .ns-node__icon--fade,
  .ns-node__circle--fade {
    animation: fade 2s infinite;
  }

  /* Animation keyframes */
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.7;
      transform: scale(1.1);
    }
  }

  @keyframes blink {
    0%,
    50% {
      opacity: 1;
    }
    51%,
    100% {
      opacity: 0.3;
    }
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes fade {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  /* Responsive adjustments */
  @media (max-width: 640px) {
    .ns-node {
      padding: 8px;
      margin: 2px 0;
    }

    .ns-node__header {
      gap: 8px;
    }

    .ns-node__text {
      font-size: 13px;
    }
  }

  /* WYSIWYG Processing Styles */
  .ns-node__contenteditable--wysiwyg {
    /* Enhanced contenteditable for WYSIWYG */
  }

  .ns-node__contenteditable--processing {
    opacity: 0.8;
    transition: opacity 0.2s ease;
  }

  .ns-node__text--wysiwyg {
    /* Enhanced text display for WYSIWYG */
  }

  /* WYSIWYG Pattern Styles */
  :global(.wysiwyg-syntax-hidden) {
    opacity: 0.3;
    font-size: 0.8em;
    color: hsl(var(--muted-foreground));
  }

  :global(.wysiwyg-header) {
    font-weight: bold;
  }

  :global(.wysiwyg-header-1) { font-size: 2em; line-height: 1.2; }
  :global(.wysiwyg-header-2) { font-size: 1.5em; line-height: 1.2; }
  :global(.wysiwyg-header-3) { font-size: 1.17em; line-height: 1.3; }
  :global(.wysiwyg-header-4) { font-size: 1em; line-height: 1.4; }
  :global(.wysiwyg-header-5) { font-size: 0.83em; line-height: 1.4; }
  :global(.wysiwyg-header-6) { font-size: 0.67em; line-height: 1.4; }

  :global(.wysiwyg-bold) {
    font-weight: bold;
  }

  :global(.wysiwyg-italic) {
    font-style: italic;
  }

  :global(.wysiwyg-inlinecode) {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Consolas, 'Liberation Mono', Menlo, monospace;
    background-color: rgba(175, 184, 193, 0.2);
    padding: 2px 4px;
    border-radius: 3px;
    font-size: 0.9em;
  }

  :global(.wysiwyg-codeblock) {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Consolas, 'Liberation Mono', Menlo, monospace;
    background-color: rgba(175, 184, 193, 0.1);
    padding: 12px;
    border-radius: 6px;
    border-left: 3px solid #d0d7de;
    display: block;
    white-space: pre;
    margin: 8px 0;
  }

  :global(.wysiwyg-bullet) {
    display: list-item;
    margin-left: 1.5em;
    list-style-type: disc;
  }

  :global(.wysiwyg-bullet-dash) {
    list-style-type: '- ';
  }

  :global(.wysiwyg-bullet-star) {
    list-style-type: '* ';
  }

  :global(.wysiwyg-bullet-plus) {
    list-style-type: '+ ';
  }

  :global(.wysiwyg-blockquote) {
    border-left: 3px solid #d0d7de;
    padding-left: 16px;
    color: #656d76;
    font-style: italic;
    margin: 8px 0;
  }

  /* Syntax characters with data attributes */
  :global([data-syntax]::before) {
    content: attr(data-syntax);
    opacity: 0.3;
    font-size: 0.8em;
    color: hsl(var(--muted-foreground));
  }

  :global([data-syntax-before]::before) {
    content: attr(data-syntax-before);
    opacity: 0.3;
    font-size: 0.8em;
    color: hsl(var(--muted-foreground));
  }

  :global([data-syntax-after]::after) {
    content: attr(data-syntax-after);
    opacity: 0.3;
    font-size: 0.8em;
    color: hsl(var(--muted-foreground));
  }
</style>
