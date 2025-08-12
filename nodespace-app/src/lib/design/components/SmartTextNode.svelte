<!--
  Smart Text Node with Bullet-to-Node Conversion
  
  Enhanced version of BaseNode that detects bullet patterns and converts
  them to actual child node relationships. Implements Issue #58.
-->

<script lang="ts">
  import { createEventDispatcher, tick } from 'svelte';
  import BaseNode from './BaseNode.svelte';
  import type { TreeNodeData } from '$lib/types/tree';
  import type { MarkdownPattern } from '$lib/types/markdownPatterns';
  import { bulletToNodeConverter, BulletProcessingUtils } from '$lib/services/bulletToNodeConverter';
  import { patternIntegrationUtils } from '$lib/services/markdownPatternUtils';

  // Props extending BaseNode
  export let nodeId: string = '';
  export let content: string = '';
  export let nodeType: 'text' | 'task' | 'ai-chat' | 'entity' | 'query' = 'text';
  export let hasChildren: boolean = false;
  export let editable: boolean = true;
  export let multiline: boolean = true; // Enable multiline for bullet support
  export let placeholder: string = 'Type content or use - for bullet points...';
  
  // Smart conversion props
  export let enableBulletConversion: boolean = true;
  export let autoConvertBullets: boolean = true;
  export let maxNestingDepth: number = 5;
  export let parentNodeId: string | undefined = undefined;

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    click: { nodeId: string; event: MouseEvent };
    focus: { nodeId: string };
    blur: { nodeId: string };
    contentChanged: { nodeId: string; content: string };
    bulletConversion: { 
      nodeId: string; 
      newNodes: TreeNodeData[];
      cleanedContent: string;
    };
    requestPatternDetection: {
      nodeId: string;
      content: string;
      cursorPosition: number;
    };
  }>();

  // Internal state
  let lastContent = content;
  let conversionInProgress = false;
  let currentPatterns: MarkdownPattern[] = [];

  // Handle content changes from BaseNode
  function handleContentChanged(event: CustomEvent<{ nodeId: string; content: string }>) {
    const newContent = event.detail.content;
    
    // Forward the content change
    dispatch('contentChanged', event.detail);
    
    // Check for bullet conversion if enabled
    if (enableBulletConversion && !conversionInProgress) {
      checkForBulletConversion(newContent);
    }
    
    lastContent = newContent;
  }

  // Check if bullet conversion should be triggered
  async function checkForBulletConversion(newContent: string) {
    if (!newContent || newContent === lastContent) return;
    
    // Request pattern detection from parent component
    const selection = window.getSelection();
    const cursorPosition = selection ? getSelectionPosition(selection) : newContent.length;
    
    dispatch('requestPatternDetection', {
      nodeId,
      content: newContent,
      cursorPosition
    });
  }

  // Process patterns received from parent
  export function processPatterns(patterns: MarkdownPattern[], cursorPosition: number) {
    if (!enableBulletConversion) return;
    
    currentPatterns = patterns;
    
    // Check if bullet conversion should be triggered
    if (autoConvertBullets && BulletProcessingUtils.shouldTriggerConversion(
      content, 
      cursorPosition, 
      patterns
    )) {
      performBulletConversion(patterns, cursorPosition);
    }
  }

  // Perform the bullet-to-node conversion
  async function performBulletConversion(patterns: MarkdownPattern[], cursorPosition: number) {
    if (conversionInProgress) return;
    
    conversionInProgress = true;
    
    try {
      const result = bulletToNodeConverter.convertBulletsToNodes(
        content,
        patterns,
        cursorPosition,
        parentNodeId
      );
      
      if (result.hasConversions) {
        // Update content with cleaned version (bullets removed)
        content = result.cleanedContent;
        
        // Dispatch bullet conversion event with new nodes
        dispatch('bulletConversion', {
          nodeId,
          newNodes: result.newNodes,
          cleanedContent: result.cleanedContent
        });
        
        // Update cursor position
        await tick();
        await updateCursorPosition(result.newCursorPosition);
      }
    } catch (error) {
      console.error('Error during bullet conversion:', error);
    } finally {
      conversionInProgress = false;
    }
  }

  // Manual trigger for bullet conversion (for testing or UI controls)
  export async function triggerBulletConversion() {
    if (!enableBulletConversion || !currentPatterns.length) return;
    
    const selection = window.getSelection();
    const cursorPosition = selection ? getSelectionPosition(selection) : content.length;
    
    await performBulletConversion(currentPatterns, cursorPosition);
  }

  // Preview what nodes would be created (for UI feedback)
  export function previewBulletConversion(): TreeNodeData[] {
    if (!enableBulletConversion || !currentPatterns.length) return [];
    
    return BulletProcessingUtils.previewConversion(content, currentPatterns, parentNodeId);
  }

  // Helper function to get cursor position in text
  function getSelectionPosition(selection: Selection): number {
    if (selection.rangeCount === 0) return 0;
    
    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(range.startContainer.parentElement || range.startContainer);
    preCaretRange.setEnd(range.startContainer, range.startOffset);
    
    return preCaretRange.toString().length;
  }

  // Update cursor position after content changes
  async function updateCursorPosition(position: number) {
    await tick();
    
    const selection = window.getSelection();
    if (!selection || !document.activeElement) return;
    
    const textNode = document.activeElement.firstChild;
    if (!textNode || textNode.nodeType !== Node.TEXT_NODE) return;
    
    try {
      const range = document.createRange();
      const safePosition = Math.min(position, textNode.textContent?.length || 0);
      range.setStart(textNode, safePosition);
      range.collapse(true);
      
      selection.removeAllRanges();
      selection.addRange(range);
    } catch (error) {
      console.error('Error updating cursor position:', error);
    }
  }

  // Handle special keyboard shortcuts for bullet operations
  function handleKeyDown(event: KeyboardEvent) {
    if (!enableBulletConversion) return;
    
    // Ctrl/Cmd + Shift + B: Manual bullet conversion trigger
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key === 'B') {
      event.preventDefault();
      triggerBulletConversion();
      return;
    }
    
    // Space after bullet syntax: trigger conversion
    if (event.key === ' ') {
      const selection = window.getSelection();
      if (selection && selection.rangeCount > 0) {
        const cursorPosition = getSelectionPosition(selection);
        const currentLine = getCurrentLine(content, cursorPosition);
        
        // Check if we just completed bullet syntax
        const bulletRegex = /^(\s*)([-*+])$/;
        if (bulletRegex.test(currentLine.trim())) {
          // Allow the space to be inserted, then check for conversion
          setTimeout(() => {
            checkForBulletConversion(content);
          }, 10);
        }
      }
    }
  }

  // Get current line based on cursor position
  function getCurrentLine(text: string, position: number): string {
    const beforeCursor = text.substring(0, position);
    const lastNewlineIndex = beforeCursor.lastIndexOf('\n');
    const lineStart = lastNewlineIndex === -1 ? 0 : lastNewlineIndex + 1;
    
    const afterCursor = text.substring(position);
    const nextNewlineIndex = afterCursor.indexOf('\n');
    const lineEnd = nextNewlineIndex === -1 ? text.length : position + nextNewlineIndex;
    
    return text.substring(lineStart, lineEnd);
  }

  // Generate enhanced placeholder text
  $: enhancedPlaceholder = enableBulletConversion 
    ? `${placeholder} (Use -, *, or + for bullet points)`
    : placeholder;

  // CSS classes for conversion indicators
  $: smartNodeClasses = [
    enableBulletConversion && 'smart-text-node--bullets-enabled',
    conversionInProgress && 'smart-text-node--converting',
    currentPatterns.length > 0 && 'smart-text-node--has-patterns'
  ].filter(Boolean).join(' ');
</script>

<!-- Enhanced BaseNode with bullet conversion capabilities -->
<div class="smart-text-node {smartNodeClasses}">
  <BaseNode
    {nodeId}
    bind:content
    {nodeType}
    {hasChildren}
    {editable}
    {multiline}
    placeholder={enhancedPlaceholder}
    on:click
    on:focus
    on:blur
    on:contentChanged={handleContentChanged}
    on:keydown={handleKeyDown}
  />
  
  <!-- Conversion indicator -->
  {#if conversionInProgress}
    <div class="smart-text-node__conversion-indicator">
      <span class="smart-text-node__conversion-text">Converting bullets to nodes...</span>
    </div>
  {/if}
  
  <!-- Debug info (only in development) -->
  {#if import.meta.env.DEV && currentPatterns.length > 0}
    <div class="smart-text-node__debug">
      <details>
        <summary>Debug: {currentPatterns.length} patterns detected</summary>
        <ul>
          {#each currentPatterns as pattern}
            <li>{pattern.type}: "{pattern.content}" ({pattern.start}-{pattern.end})</li>
          {/each}
        </ul>
      </details>
    </div>
  {/if}
</div>

<style>
  .smart-text-node {
    position: relative;
  }

  /* Visual indicator when bullets are being converted */
  .smart-text-node__conversion-indicator {
    position: absolute;
    top: 0;
    right: 0;
    padding: 4px 8px;
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    border-radius: 4px;
    font-size: 12px;
    opacity: 0.9;
    pointer-events: none;
    transform: translateY(-100%);
    z-index: 10;
  }

  .smart-text-node__conversion-text {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .smart-text-node__conversion-text::before {
    content: '⚡';
    animation: pulse 1s infinite;
  }

  /* Visual feedback for bullet-enabled nodes */
  .smart-text-node--bullets-enabled {
    position: relative;
  }

  .smart-text-node--bullets-enabled::before {
    content: '';
    position: absolute;
    left: -2px;
    top: 0;
    bottom: 0;
    width: 2px;
    background: linear-gradient(
      to bottom,
      hsl(var(--primary)) 0%,
      hsl(var(--primary) / 0.3) 50%,
      hsl(var(--primary)) 100%
    );
    border-radius: 1px;
    opacity: 0;
    transition: opacity 0.2s ease;
  }

  .smart-text-node--has-patterns::before {
    opacity: 1;
  }

  /* Debug information styling */
  .smart-text-node__debug {
    margin-top: 8px;
    padding: 8px;
    background: hsl(var(--muted));
    border-radius: 4px;
    font-size: 11px;
    font-family: monospace;
  }

  .smart-text-node__debug summary {
    cursor: pointer;
    font-weight: bold;
    margin-bottom: 4px;
  }

  .smart-text-node__debug ul {
    margin: 4px 0 0 16px;
    list-style: disc;
  }

  .smart-text-node__debug li {
    margin: 2px 0;
    color: hsl(var(--muted-foreground));
  }

  /* Animation keyframes */
  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.7; transform: scale(1.1); }
  }

  /* Accessibility improvements */
  @media (prefers-reduced-motion: reduce) {
    .smart-text-node__conversion-text::before {
      animation: none;
    }
    
    .smart-text-node--bullets-enabled::before {
      transition: none;
    }
  }

  /* High contrast support */
  @media (prefers-contrast: high) {
    .smart-text-node__conversion-indicator {
      border: 1px solid hsl(var(--border));
    }
    
    .smart-text-node--bullets-enabled::before {
      background: hsl(var(--foreground));
    }
  }
</style>