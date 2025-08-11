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
  import { createEventDispatcher, onMount, onDestroy } from 'svelte';
  import Icon, { type IconName } from '../icons/index.js';
  import CodeMirrorEditor from './CodeMirrorEditor.svelte';

  // Essential Props (Simplified to 5 core + processing)
  export let nodeType: NodeType = 'text';
  export let nodeId: string = '';
  export let content: string = '';
  export let hasChildren: boolean = false;
  export let className: string = '';
  export let editable: boolean = true;
  export let contentEditable: boolean = true; // Can content be directly edited by user?
  export let multiline: boolean = false; // Single-line by default
  export let markdown: boolean = false; // Plain text by default
  export let placeholder: string = 'Click to add content...';

  // SVG Icon System
  export let iconName: IconName | undefined = undefined;

  // Processing State System
  export let isProcessing: boolean = false;
  export let processingAnimation: 'blink' | 'pulse' | 'spin' | 'fade' = 'pulse';
  export let processingIcon: IconName | undefined = undefined;

  // References
  let editorRef: CodeMirrorEditor;
  let nodeElement: HTMLDivElement;
  
  // Focus state for edit/display mode switching
  let focused = false;

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    click: { nodeId: string; event: MouseEvent };
    contentChanged: { nodeId: string; content: string };
  }>();

  // Handle display mode clicks - enter edit mode with cursor positioning
  function handleDisplayClick(event: MouseEvent) {
    console.log('Display click handler called!', { editable, contentEditable, focused });
    if (editable && contentEditable) {
      focused = true;
      
      // Capture click coordinates for cursor positioning
      const clickX = event.clientX;
      const clickY = event.clientY;
      
      console.log('Setting focused to true, coordinates:', { clickX, clickY });
      
      // Focus and position cursor after editor is rendered
      setTimeout(() => {
        console.log('Focusing editor, editorRef exists:', !!editorRef);
        if (editorRef) {
          editorRef.focus();
          // Position cursor at click location
          editorRef.setCursorAtCoords(clickX, clickY);
        } else {
          console.log('editorRef not ready, retrying in 50ms...');
          // Retry if editor not ready yet
          setTimeout(() => {
            console.log('Retry - editorRef exists:', !!editorRef);
            if (editorRef) {
              editorRef.focus();
              editorRef.setCursorAtCoords(clickX, clickY);
            }
          }, 50);
        }
      }, 0);
    }
    dispatch('click', { nodeId, event });
  }
  
  // Handle general node clicks (for container)
  function handleClick(event: MouseEvent) {
    dispatch('click', { nodeId, event });
  }
  
  // Handle blur - exit edit mode
  function handleBlur() {
    focused = false;
  }

  // Handle content changes from CodeMirror
  function handleContentChanged(event: CustomEvent<{ content: string }>) {
    content = event.detail.content;
    dispatch('contentChanged', { nodeId, content });
  }

  // Handle keyboard shortcuts for display mode
  function handleDisplayKeyDown(event: KeyboardEvent) {
    // Enter/Space to enter edit mode
    if (editable && contentEditable && (event.code === 'Enter' || event.code === 'Space')) {
      event.preventDefault();
      focused = true;
      setTimeout(() => {
        if (editorRef) {
          editorRef.focus();
          // For keyboard entry, position cursor at end of content
          editorRef.setCursorAtEnd();
        } else {
          // Retry if editor not ready yet
          setTimeout(() => {
            if (editorRef) {
              editorRef.focus();
              editorRef.setCursorAtEnd();
            }
          }, 50);
        }
      }, 0);
    }
  }
  
  // Handle keyboard shortcuts for container
  function handleKeyDown(event: KeyboardEvent) {
    // Escape key exits edit mode
    if (event.key === 'Escape' && focused) {
      event.preventDefault();
      focused = false;
      return;
    }
  }

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
    focused && 'ns-node--focused',
    className
  ]
    .filter(Boolean)
    .join(' ');

  // Icon selection: processing icon overrides regular icon when processing
  $: displayIcon = isProcessing && processingIcon ? processingIcon : iconName;

  // Animation class for processing state
  $: iconAnimationClass = isProcessing ? `ns-node__icon--${processingAnimation}` : '';
  
  // Handle clicks outside the node to blur
  function handleGlobalClick(event: MouseEvent) {
    if (focused && nodeElement && !nodeElement.contains(event.target as HTMLElement)) {
      focused = false;
    }
  }
  
  // Setup global click listener
  onMount(() => {
    document.addEventListener('click', handleGlobalClick);
  });
  
  onDestroy(() => {
    document.removeEventListener('click', handleGlobalClick);
  });
</script>

<!-- Node container with edit/display mode support -->
<div
  bind:this={nodeElement}
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

    <!-- Node content - edit/display mode switching -->
    <div class="ns-node__content">
      <slot name="content">
        {#if contentEditable && focused}
          <!-- EDIT MODE: Real-time hybrid CodeMirror -->
          <div style="background: lightgreen; padding: 4px; margin: 2px;">DEBUG: EDIT MODE</div>
          <CodeMirrorEditor
            bind:this={editorRef}
            bind:content
            {multiline}
            {markdown}
            {editable}
            {placeholder}
            on:contentChanged={handleContentChanged}
            on:blur={handleBlur}
          />
        {:else if contentEditable}
          <!-- DISPLAY MODE: Clean formatted rendering -->
          <div style="background: lightblue; padding: 4px; margin: 2px;">DEBUG: DISPLAY MODE (contentEditable={contentEditable}, focused={focused})</div>
          <div 
            class="ns-node__display" 
            role="button"
            tabindex="0"
            on:click={handleDisplayClick}
            on:keydown={handleDisplayKeyDown}
          >
            <slot name="display-content">
              {#if content}
                <span class="ns-node__text">{content}</span>
              {:else}
                <span class="ns-node__empty">{placeholder}</span>
              {/if}
            </slot>
          </div>
        {:else}
          <!-- Non-editable content -->
          <div style="background: lightcoral; padding: 4px; margin: 2px;">DEBUG: NON-EDITABLE MODE</div>
          <div class="ns-node__display" role="region">
            <slot name="display-content">
              {#if content}
                <span class="ns-node__text">{content}</span>
              {:else}
                <span class="ns-node__empty">{placeholder}</span>
              {/if}
            </slot>
          </div>
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

  /* Edit mode styles */
  .ns-node--focused {
    background-color: hsl(var(--muted) / 0.3);
    border: 1px solid hsl(var(--border));
  }
  
  .ns-node--focused .ns-node__content {
    /* Ensure CodeMirror editor fits seamlessly */
    min-height: 20px;
  }
  
  /* Display mode styles */
  .ns-node__display {
    cursor: text;
    min-height: 20px;
    padding: 2px 0;
  }
  
  .ns-node__display:hover {
    background-color: hsl(var(--muted) / 0.1);
    border-radius: 4px;
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
</style>
