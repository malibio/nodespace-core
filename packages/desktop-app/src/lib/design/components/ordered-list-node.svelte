<!--
  OrderedListNode - Wraps BaseNode with ordered list-specific functionality

  Responsibilities:
  - Manages "1. " prefix for ordered list items
  - Provides auto-numbering via CSS counters
  - Single-line editing (no multiline like quote-blocks)
  - Supports inline markdown formatting (bold, italic, code, links)
  - Forwards all other events to BaseNode

  Integration:
  - Uses icon registry for proper OrderedListIcon rendering
  - Maintains compatibility with BaseNode API
  - Works seamlessly in node tree structure

  Design System Reference: docs/design-system/components.html → Ordered List Nodes
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from './base-node.svelte';

  // Props using Svelte 5 runes mode - same interface as BaseNode
  let {
    nodeId,
    nodeType = 'ordered-list',
    autoFocus = false,
    content = '',
    children = []
  }: {
    nodeId: string;
    nodeType?: string;
    autoFocus?: boolean;
    content?: string;
    children?: string[];
  } = $props();

  const dispatch = createEventDispatcher();

  // Cursor positioning constant for creating new items
  const PREFIX_LENGTH = 3; // "1. " prefix length

  // Internal reactive state - sync with content prop changes
  let internalContent = $state(content);

  // Sync internalContent when content prop changes externally
  $effect(() => {
    internalContent = content;
  });

  // Ordered lists use single-line editing (prevent multiline with Shift+Enter)
  const editableConfig = { allowMultiline: false, allowMergeInto: false };

  // Ordered list metadata - enable markdown processing
  let listMetadata = $derived({
    disableMarkdown: false // Ordered lists support inline markdown
  });

  /**
   * Extract list content for display (strip "1. " prefix)
   * Edit mode: 1. First item
   * Display mode: First item (auto-numbered via CSS counter)
   */
  function extractListForDisplay(content: string): string {
    // Strip "1. " from the beginning
    return content.replace(/^1\.\s/, '');
  }

  // Display content: Strip "1. " prefix for view mode
  let displayContent = $derived(extractListForDisplay(internalContent));

  /**
   * Handle content changes from BaseNode
   * Ensure content starts with "1. " prefix
   */
  function handleContentChange(event: CustomEvent<{ content: string }>) {
    const userContent = event.detail.content;

    // Ensure content starts with "1. " prefix
    let prefixedContent = userContent;

    if (!prefixedContent.trim().startsWith('1.')) {
      // Add "1. " prefix if missing
      prefixedContent = `1. ${prefixedContent}`;
    } else if (!prefixedContent.startsWith('1. ')) {
      // Fix spacing: "1." → "1. " or "1.text" → "1. text"
      prefixedContent = prefixedContent.replace(/^1\.(\s?)/, '1. ');
    }

    internalContent = prefixedContent;
    dispatch('contentChanged', { content: prefixedContent });
  }

  /**
   * Handle createNewNode event - check if we should convert back to text
   */
  function handleCreateNewNode(event: CustomEvent) {
    const detail = event.detail;

    // If the content no longer starts with "1.", convert to text node
    if (detail.currentContent && !detail.currentContent.trim().startsWith('1.')) {
      // Remove "1. " prefix
      const cleanedContent = detail.currentContent.replace(/^1\.\s/, '');

      detail.currentContent = cleanedContent;
      detail.nodeType = 'text'; // Convert to text node

      // Dispatch node type change
      dispatch('nodeTypeChanged', {
        nodeId,
        newNodeType: 'text',
        cleanedContent
      });
    }

    // For ordered lists: Enter creates new ordered-list below with "1. " prefix
    if (detail.nodeType === 'ordered-list') {
      detail.currentContent = internalContent; // Keep current node unchanged
      detail.newContent = '1. '; // New ordered-list with "1. " prefix ready
      detail.newNodeCursorPosition = PREFIX_LENGTH; // Cursor after "1. "
    }

    dispatch('createNewNode', detail);
  }

  /**
   * Handle node type conversion to ordered-list
   */
  function handleNodeTypeChanged(event: CustomEvent) {
    const detail = event.detail;

    // Just dispatch the event as-is
    // Pattern detection already handles keeping the "1. " prefix (cleanContent: false)
    // So we don't need to add it here - it's already in the content
    dispatch('nodeTypeChanged', detail);
  }

  /**
   * Forward all other events to parent components
   */
  function forwardEvent<T>(eventName: string) {
    return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
  }
</script>

<!-- Wrap BaseNode with ordered-list-specific styling -->
<div class="ordered-list-node-wrapper">
  <BaseNode
    {nodeId}
    {nodeType}
    {autoFocus}
    content={internalContent}
    {displayContent}
    {children}
    {editableConfig}
    metadata={listMetadata}
    on:createNewNode={handleCreateNewNode}
    on:contentChanged={handleContentChange}
    on:indentNode={forwardEvent('indentNode')}
    on:outdentNode={forwardEvent('outdentNode')}
    on:navigateArrow={forwardEvent('navigateArrow')}
    on:combineWithPrevious={forwardEvent('combineWithPrevious')}
    on:deleteNode={forwardEvent('deleteNode')}
    on:focus={forwardEvent('focus')}
    on:blur={forwardEvent('blur')}
    on:nodeReferenceSelected={forwardEvent('nodeReferenceSelected')}
    on:slashCommandSelected={forwardEvent('slashCommandSelected')}
    on:nodeTypeChanged={handleNodeTypeChanged}
    on:iconClick={forwardEvent('iconClick')}
  />
</div>

<style>
  /* Ordered list wrapper */
  .ordered-list-node-wrapper {
    width: 100%;
    display: block;
    position: relative;
  }

  /* Apply ordered list styling to the content area only (BaseNode wraps it) */
  .ordered-list-node-wrapper :global(.node__content) {
    padding-left: 2rem; /* Space for auto-numbered counter */
    position: relative;
    line-height: 1.6;
    white-space: pre-wrap;
  }

  /* Auto-numbering via CSS counter */
  .ordered-list-node-wrapper :global(.node__content::before) {
    counter-increment: ordered-list-counter;
    content: counter(ordered-list-counter) '. ';
    position: absolute;
    left: 0;
    font-weight: 600;
    color: hsl(var(--muted-foreground));
    min-width: 1.5rem;
    text-align: right;
  }
</style>
