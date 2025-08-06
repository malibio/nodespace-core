<!--
  TextNode Component
  
  Extends BaseNode with text-specific functionality and inline editing.
  Supports click-to-edit, keyboard shortcuts, auto-save, and basic markdown rendering.
-->

<script lang="ts">
  import { createEventDispatcher, onMount, tick } from 'svelte';
  import BaseNode from '$lib/design/components/BaseNode.svelte';
  import { mockTextService, type TextNodeData, type TextSaveResult } from '$lib/services/mockTextService';
  import { parseMarkdown, stripMarkdown, validateMarkdown } from '$lib/services/markdownUtils';

  // Props
  export let nodeId: string;
  export let title: string = '';
  export let content: string = '';
  export let editable: boolean = true;
  export let markdown: boolean = true;
  export let placeholder: string = 'Click to add text...';
  export let autoSave: boolean = true;
  export let autoSaveDelay: number = 2000;

  // Component state
  let isEditing = false;
  let editContent = content;
  let editTitle = title;
  let saveStatus: 'saved' | 'saving' | 'unsaved' | 'error' = 'saved';
  let saveError = '';
  let textareaElement: HTMLTextAreaElement;
  let textNodeData: TextNodeData | null = null;
  let lastSavedContent = content;
  let lastSavedTitle = title;

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    save: { nodeId: string; content: string; title: string };
    cancel: { nodeId: string };
    error: { nodeId: string; error: string };
    focus: { nodeId: string };
    blur: { nodeId: string };
  }>();

  // Load existing node data on mount if nodeId provided
  onMount(async () => {
    if (nodeId && nodeId !== 'new') {
      try {
        textNodeData = await mockTextService.loadTextNode(nodeId);
        if (textNodeData) {
          content = textNodeData.content;
          title = textNodeData.title;
          editContent = content;
          editTitle = title;
          lastSavedContent = content;
          lastSavedTitle = title;
          saveStatus = 'saved';
        }
      } catch (error) {
        console.error('Failed to load text node:', error);
        saveError = error instanceof Error ? error.message : 'Failed to load content';
        saveStatus = 'error';
      }
    }
  });

  // Auto-save functionality
  let autoSaveTimeout: NodeJS.Timeout;
  $: if (autoSave && isEditing && (editContent !== lastSavedContent || editTitle !== lastSavedTitle)) {
    clearTimeout(autoSaveTimeout);
    saveStatus = 'unsaved';
    
    autoSaveTimeout = setTimeout(async () => {
      await handleAutoSave();
    }, autoSaveDelay);
  }

  // Handle click-to-edit
  async function handleClick() {
    if (!editable || isEditing) return;
    
    await startEditing();
  }

  // Start editing mode
  async function startEditing() {
    isEditing = true;
    editContent = content;
    editTitle = title;
    saveStatus = content === lastSavedContent && title === lastSavedTitle ? 'saved' : 'unsaved';
    
    // Focus the textarea after DOM update
    await tick();
    if (textareaElement) {
      textareaElement.focus();
      // Auto-resize on focus
      autoResizeTextarea();
    }
    
    dispatch('focus', { nodeId });
  }

  // Cancel editing
  function cancelEditing() {
    if (!isEditing) return;
    
    // Clear auto-save timeout
    clearTimeout(autoSaveTimeout);
    if (autoSave && nodeId) {
      mockTextService.cancelAutoSave(nodeId);
    }
    
    // Revert changes
    editContent = lastSavedContent;
    editTitle = lastSavedTitle;
    isEditing = false;
    saveStatus = 'saved';
    saveError = '';
    
    dispatch('cancel', { nodeId });
  }

  // Save changes
  async function saveChanges(): Promise<void> {
    if (!isEditing) return;
    
    saveStatus = 'saving';
    saveError = '';
    
    try {
      // Validate markdown if enabled
      if (markdown) {
        const validation = validateMarkdown(editContent);
        if (!validation.isValid) {
          saveError = `Markdown errors: ${validation.errors.join(', ')}`;
          saveStatus = 'error';
          return;
        }
      }
      
      let result: TextSaveResult;
      
      if (nodeId && nodeId !== 'new') {
        // Update existing node
        result = await mockTextService.saveTextNode(nodeId, editContent, editTitle);
      } else {
        // Create new node
        const newNodeData = await mockTextService.createTextNode(editContent, editTitle);
        nodeId = newNodeData.id;
        result = { success: true, id: newNodeData.id, timestamp: new Date() };
      }
      
      if (result.success) {
        // Update local state
        content = editContent;
        title = editTitle;
        lastSavedContent = editContent;
        lastSavedTitle = editTitle;
        saveStatus = 'saved';
        isEditing = false;
        
        // Update node data
        textNodeData = await mockTextService.loadTextNode(nodeId);
        
        dispatch('save', { nodeId, content: editContent, title: editTitle });
      } else {
        saveError = result.error || 'Save failed';
        saveStatus = 'error';
        dispatch('error', { nodeId, error: saveError });
      }
    } catch (error) {
      saveError = error instanceof Error ? error.message : 'Unknown error';
      saveStatus = 'error';
      dispatch('error', { nodeId, error: saveError });
    }
  }

  // Auto-save handler
  async function handleAutoSave() {
    if (!autoSave || !nodeId || nodeId === 'new') return;
    
    saveStatus = 'saving';
    
    try {
      const result = await mockTextService.scheduleAutoSave(nodeId, editContent, editTitle, 100);
      
      if (result.success) {
        lastSavedContent = editContent;
        lastSavedTitle = editTitle;
        saveStatus = 'saved';
        
        // Update displayed content
        content = editContent;
        title = editTitle;
      } else {
        saveError = result.error || 'Auto-save failed';
        saveStatus = 'error';
      }
    } catch (error) {
      saveError = error instanceof Error ? error.message : 'Auto-save error';
      saveStatus = 'error';
    }
  }

  // Handle keyboard shortcuts
  function handleKeyDown(event: KeyboardEvent) {
    if (!isEditing) return;
    
    // Ctrl+Enter or Cmd+Enter to save
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      saveChanges();
      return;
    }
    
    // Escape to cancel
    if (event.key === 'Escape') {
      event.preventDefault();
      cancelEditing();
      return;
    }
    
    // Auto-resize on input
    if (event.target === textareaElement) {
      // Use requestAnimationFrame to resize after content change
      requestAnimationFrame(() => {
        autoResizeTextarea();
      });
    }
  }

  // Auto-resize textarea to fit content
  function autoResizeTextarea() {
    if (!textareaElement) return;
    
    // Reset height to get accurate scrollHeight
    textareaElement.style.height = 'auto';
    
    // Set height to scrollHeight with some padding
    const newHeight = Math.max(textareaElement.scrollHeight + 4, 80); // minimum 80px
    textareaElement.style.height = `${newHeight}px`;
  }

  // Handle input events for auto-resize
  function handleInput() {
    autoResizeTextarea();
  }

  // Handle blur (save on blur if auto-save is disabled)
  async function handleBlur() {
    if (!isEditing) return;
    
    dispatch('blur', { nodeId });
    
    // If auto-save is disabled, save on blur
    if (!autoSave) {
      await saveChanges();
    }
  }

  // Render markdown content
  $: renderedContent = markdown && content ? parseMarkdown(content) : content;

  // Get display title (fallback to content preview)
  $: displayTitle = title || (content ? stripMarkdown(content).substring(0, 50) + (content.length > 50 ? '...' : '') : 'Untitled');

  // Save status indicator
  $: saveStatusClass = {
    'saved': 'ns-text-node-status--saved',
    'saving': 'ns-text-node-status--saving',
    'unsaved': 'ns-text-node-status--unsaved',
    'error': 'ns-text-node-status--error'
  }[saveStatus];
</script>

<BaseNode
  {nodeId}
  title={displayTitle}
  nodeType="text"
  hasChildren={false}
  clickable={editable && !isEditing}
  loading={saveStatus === 'saving'}
  className="ns-text-node {isEditing ? 'ns-text-node--editing' : ''}"
  on:click={handleClick}
>
  <!-- Main content slot -->
  <div class="ns-text-node__content" slot="content">
    {#if isEditing}
      <!-- Editing mode -->
      <div class="ns-text-node__editor">
        <!-- Title editor (if title is separate from content) -->
        {#if title !== content}
          <input
            type="text"
            class="ns-text-node__title-input"
            bind:value={editTitle}
            placeholder="Node title..."
            on:keydown={handleKeyDown}
          />
        {/if}
        
        <!-- Content editor -->
        <textarea
          bind:this={textareaElement}
          bind:value={editContent}
          placeholder={placeholder}
          class="ns-text-node__textarea"
          on:keydown={handleKeyDown}
          on:input={handleInput}
          on:blur={handleBlur}
          rows="3"
          aria-label="Edit text content"
        ></textarea>
        
        <!-- Editor controls -->
        <div class="ns-text-node__controls">
          <button
            type="button"
            class="ns-text-node__btn ns-text-node__btn--primary"
            on:click={saveChanges}
            disabled={saveStatus === 'saving'}
          >
            {saveStatus === 'saving' ? 'Saving...' : 'Save'}
          </button>
          
          <button
            type="button"
            class="ns-text-node__btn ns-text-node__btn--secondary"
            on:click={cancelEditing}
            disabled={saveStatus === 'saving'}
          >
            Cancel
          </button>
          
          {#if markdown}
            <span class="ns-text-node__hint">
              Tip: Use **bold**, *italic*, # headers
            </span>
          {/if}
        </div>
      </div>
    {:else}
      <!-- Display mode -->
      <div class="ns-text-node__display">
        {#if content}
          {#if markdown}
            <div class="ns-text-node__markdown">
              {@html renderedContent}
            </div>
          {:else}
            <div class="ns-text-node__plain">
              {content}
            </div>
          {/if}
        {:else}
          <div class="ns-text-node__empty">
            {placeholder}
          </div>
        {/if}
      </div>
    {/if}
  </div>
  
  <!-- Footer with save status -->
  <div slot="footer" class="ns-text-node__footer">
    {#if saveStatus !== 'saved' || saveError}
      <div class="ns-text-node__status {saveStatusClass}">
        {#if saveStatus === 'saving'}
          <span class="ns-text-node__status-icon">⏳</span>
          Saving...
        {:else if saveStatus === 'unsaved'}
          <span class="ns-text-node__status-icon">●</span>
          Unsaved changes
        {:else if saveStatus === 'error'}
          <span class="ns-text-node__status-icon">⚠️</span>
          {saveError || 'Save error'}
        {/if}
      </div>
    {:else if textNodeData}
      <div class="ns-text-node__meta">
        <span class="ns-text-node__word-count">
          {textNodeData.metadata.wordCount} words
        </span>
        <span class="ns-text-node__updated">
          Updated {textNodeData.updatedAt.toLocaleDateString()}
        </span>
      </div>
    {/if}
  </div>
</BaseNode>

<style>
  /* Text node content styling */
  .ns-text-node__content {
    width: 100%;
    min-height: 60px;
  }

  /* Editor styling */
  .ns-text-node__editor {
    width: 100%;
  }

  .ns-text-node__title-input {
    width: 100%;
    padding: var(--ns-spacing-2) var(--ns-spacing-3);
    margin-bottom: var(--ns-spacing-2);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-md);
    font-size: var(--ns-font-size-base);
    font-weight: var(--ns-font-weight-semibold);
    background: var(--ns-color-surface-default);
    color: var(--ns-color-text-primary);
    outline: none;
    transition: border-color var(--ns-duration-fast) var(--ns-easing-easeInOut);
  }

  .ns-text-node__title-input:focus {
    border-color: var(--ns-color-primary-500);
  }

  .ns-text-node__textarea {
    width: 100%;
    min-height: 80px;
    padding: var(--ns-spacing-3);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-md);
    font-family: inherit;
    font-size: var(--ns-font-size-sm);
    line-height: var(--ns-line-height-relaxed);
    background: var(--ns-color-surface-default);
    color: var(--ns-color-text-primary);
    resize: none;
    outline: none;
    transition: border-color var(--ns-duration-fast) var(--ns-easing-easeInOut);
  }

  .ns-text-node__textarea:focus {
    border-color: var(--ns-color-primary-500);
  }

  .ns-text-node__textarea::placeholder {
    color: var(--ns-color-text-placeholder);
  }

  /* Editor controls */
  .ns-text-node__controls {
    display: flex;
    align-items: center;
    gap: var(--ns-spacing-2);
    margin-top: var(--ns-spacing-2);
    flex-wrap: wrap;
  }

  .ns-text-node__btn {
    padding: var(--ns-spacing-1) var(--ns-spacing-3);
    border: 1px solid var(--ns-color-border-default);
    border-radius: var(--ns-radius-sm);
    font-size: var(--ns-font-size-sm);
    cursor: pointer;
    transition: all var(--ns-duration-fast) var(--ns-easing-easeInOut);
  }

  .ns-text-node__btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .ns-text-node__btn--primary {
    background: var(--ns-color-primary-500);
    color: white;
    border-color: var(--ns-color-primary-500);
  }

  .ns-text-node__btn--primary:not(:disabled):hover {
    background: var(--ns-color-primary-600);
    border-color: var(--ns-color-primary-600);
  }

  .ns-text-node__btn--secondary {
    background: var(--ns-color-surface-default);
    color: var(--ns-color-text-secondary);
  }

  .ns-text-node__btn--secondary:not(:disabled):hover {
    background: var(--ns-color-surface-panel);
  }

  .ns-text-node__hint {
    font-size: var(--ns-font-size-xs);
    color: var(--ns-color-text-tertiary);
    margin-left: auto;
  }

  /* Display mode styling */
  .ns-text-node__display {
    min-height: 40px;
    cursor: pointer;
  }

  .ns-text-node__plain {
    white-space: pre-wrap;
    word-break: break-word;
    font-size: var(--ns-font-size-sm);
    line-height: var(--ns-line-height-relaxed);
    color: var(--ns-color-text-primary);
  }

  .ns-text-node__empty {
    font-size: var(--ns-font-size-sm);
    color: var(--ns-color-text-placeholder);
    font-style: italic;
  }

  /* Markdown rendering */
  .ns-text-node__markdown {
    font-size: var(--ns-font-size-sm);
    line-height: var(--ns-line-height-relaxed);
    color: var(--ns-color-text-primary);
  }

  /* Markdown element styling */
  :global(.ns-markdown-heading) {
    margin: var(--ns-spacing-2) 0 var(--ns-spacing-1) 0;
    font-weight: var(--ns-font-weight-semibold);
    color: var(--ns-color-text-primary);
  }

  :global(.ns-markdown-h1) { font-size: var(--ns-font-size-xl); }
  :global(.ns-markdown-h2) { font-size: var(--ns-font-size-lg); }
  :global(.ns-markdown-h3) { font-size: var(--ns-font-size-base); }
  :global(.ns-markdown-h4) { font-size: var(--ns-font-size-sm); }
  :global(.ns-markdown-h5) { font-size: var(--ns-font-size-sm); }
  :global(.ns-markdown-h6) { font-size: var(--ns-font-size-xs); }

  :global(.ns-markdown-paragraph) {
    margin: var(--ns-spacing-2) 0;
  }

  :global(.ns-markdown-bold) {
    font-weight: var(--ns-font-weight-semibold);
  }

  :global(.ns-markdown-italic) {
    font-style: italic;
  }

  :global(.ns-markdown-code) {
    padding: var(--ns-spacing-1) calc(var(--ns-spacing-1) * 1.5);
    background: var(--ns-color-surface-panel);
    border-radius: var(--ns-radius-sm);
    font-family: var(--ns-font-family-mono);
    font-size: calc(var(--ns-font-size-sm) * 0.9);
    color: var(--ns-color-primary-700);
  }

  :global(.ns-markdown-link) {
    color: var(--ns-color-primary-500);
    text-decoration: underline;
  }

  :global(.ns-markdown-link:hover) {
    color: var(--ns-color-primary-600);
  }

  /* Footer styling */
  .ns-text-node__footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--ns-spacing-2);
    font-size: var(--ns-font-size-xs);
  }

  /* Save status styling */
  .ns-text-node__status {
    display: flex;
    align-items: center;
    gap: var(--ns-spacing-1);
    font-weight: var(--ns-font-weight-medium);
  }

  .ns-text-node__status-icon {
    font-size: var(--ns-font-size-xs);
  }

  .ns-text-node-status--saved {
    color: var(--ns-color-success-600);
  }

  .ns-text-node-status--saving {
    color: var(--ns-color-primary-500);
  }

  .ns-text-node-status--unsaved {
    color: var(--ns-color-warning-600);
  }

  .ns-text-node-status--error {
    color: var(--ns-color-error-600);
  }

  /* Meta information */
  .ns-text-node__meta {
    display: flex;
    gap: var(--ns-spacing-3);
    color: var(--ns-color-text-tertiary);
  }

  /* Editing state modifier */
  :global(.ns-text-node--editing) {
    border-color: var(--ns-color-primary-500);
    box-shadow: 0 0 0 1px var(--ns-color-primary-500);
  }

  /* Responsive adjustments */
  @media (max-width: 640px) {
    .ns-text-node__controls {
      flex-direction: column;
      align-items: stretch;
    }
    
    .ns-text-node__hint {
      margin-left: 0;
      text-align: center;
    }
    
    .ns-text-node__footer {
      flex-direction: column;
      align-items: flex-start;
      gap: var(--ns-spacing-1);
    }
  }
</style>