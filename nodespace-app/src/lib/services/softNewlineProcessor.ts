/**
 * Soft Newline Processor Service
 * 
 * Handles intelligent soft newline behavior where Shift-Enter creates a new line 
 * within the current node, but if the user then types markdown syntax, the system 
 * automatically creates a new appropriate node type.
 * 
 * Key Innovation:
 * User types: "Project overview:"
 * User presses Shift-Enter (soft newline - stays in same node)  
 * User continues: "- First task"
 * → System detects markdown after soft newline
 * → Creates new child node with "First task" content
 * → Maintains natural typing flow
 */

import { markdownPatternDetector } from './markdownPatternDetector.js';
import { patternIntegrationUtils } from './markdownPatternUtils.js';
import { multilineBlockProcessor } from './multilineBlockProcessor.js';
import type {
  MarkdownPattern,
  MarkdownPatternType,
  PatternDetectionResult,
  CursorPosition
} from '$lib/types/markdownPatterns';
import type { MultilineBlock, BlockContinuationContext } from './multilineBlockProcessor.js';

/**
 * Soft newline context detection result
 */
export interface SoftNewlineContext {
  /** Whether the content after soft newline contains markdown patterns */
  hasMarkdownAfterNewline: boolean;
  
  /** The detected markdown pattern after the soft newline */
  detectedPattern?: MarkdownPattern;
  
  /** Content before the soft newline (stays in current node) */
  contentBefore: string;
  
  /** Content after the soft newline (potentially becomes new node) */
  contentAfter: string;
  
  /** Position of the soft newline in the original content */
  newlinePosition: number;
  
  /** Suggested node type for the content after newline */
  suggestedNodeType?: 'text' | 'task' | 'ai-chat' | 'entity' | 'query';
  
  /** Whether to create a new node */
  shouldCreateNewNode: boolean;
  
  /** Multi-line block continuation context */
  multilineBlockContext?: BlockContinuationContext;
  
  /** Whether this is within a multi-line block that should continue */
  isMultilineBlockContinuation: boolean;
}

/**
 * Node creation suggestion based on detected patterns
 */
export interface NodeCreationSuggestion {
  /** Type of node to create */
  nodeType: 'text' | 'task' | 'ai-chat' | 'entity' | 'query';
  
  /** Content for the new node (without markdown syntax) */
  content: string;
  
  /** Original content with markdown syntax */
  rawContent: string;
  
  /** Pattern that triggered the suggestion */
  triggerPattern: MarkdownPattern;
  
  /** Position where the new node should be created */
  insertPosition: number;
  
  /** Whether this is a child or sibling relationship */
  relationship: 'child' | 'sibling';
}

/**
 * Soft newline processing options
 */
export interface SoftNewlineProcessingOptions {
  /** Minimum content length after newline to trigger detection */
  minContentLength?: number;
  
  /** Debounce time in milliseconds for real-time detection */
  debounceTime?: number;
  
  /** Whether to auto-create nodes or just suggest */
  autoCreateNodes?: boolean;
  
  /** Maximum time to wait for pattern completion (ms) */
  patternCompletionTimeout?: number;
}

/**
 * Main soft newline processor class
 */
export class SoftNewlineProcessor {
  private lastDetection: SoftNewlineContext | null = null;
  private processingTimeouts = new Map<string, NodeJS.Timeout>();
  private eventCallbacks = new Set<(context: SoftNewlineContext) => void>();
  
  constructor(
    private options: SoftNewlineProcessingOptions = {}
  ) {
    this.options = {
      minContentLength: 2,
      debounceTime: 300,
      autoCreateNodes: false,
      patternCompletionTimeout: 2000,
      ...options
    };
  }

  /**
   * Detect Shift-Enter in keyboard events
   */
  isShiftEnter(event: KeyboardEvent): boolean {
    return event.key === 'Enter' && event.shiftKey && !event.ctrlKey && !event.metaKey;
  }

  /**
   * Process content after a soft newline to detect markdown patterns
   */
  processSoftNewlineContent(
    content: string,
    cursorPosition: number,
    nodeId: string = 'default'
  ): SoftNewlineContext {
    // Find the last newline before cursor position
    const contentBeforeCursor = content.substring(0, cursorPosition);
    const lastNewlineIndex = contentBeforeCursor.lastIndexOf('\n');
    
    if (lastNewlineIndex === -1) {
      // No newlines found, not a soft newline scenario
      return this.createEmptyContext(content, cursorPosition);
    }

    // Split content around the soft newline
    const contentBefore = content.substring(0, lastNewlineIndex);
    const contentAfter = content.substring(lastNewlineIndex + 1);
    
    // Check if content after newline meets minimum length requirement
    if (contentAfter.trim().length < (this.options.minContentLength || 2)) {
      return this.createEmptyContext(content, cursorPosition, lastNewlineIndex);
    }

    // Detect patterns in the content after the newline
    const patternResult = markdownPatternDetector.detectPatternsRealtime(
      contentAfter,
      cursorPosition - lastNewlineIndex - 1
    );

    // Check for multi-line block continuation context
    const multilineBlockContext = multilineBlockProcessor.getBlockContinuationContext(content, cursorPosition);
    
    // Find the most relevant pattern at the beginning of the line
    const relevantPattern = this.findRelevantPattern(contentAfter, patternResult.patterns);
    
    // Determine if this is a multi-line block continuation
    const isMultilineBlockContinuation = multilineBlockContext.inBlock && 
                                          multilineBlockContext.shouldContinue;
    
    const context: SoftNewlineContext = {
      hasMarkdownAfterNewline: relevantPattern !== null,
      detectedPattern: relevantPattern || undefined,
      contentBefore,
      contentAfter,
      newlinePosition: lastNewlineIndex,
      shouldCreateNewNode: relevantPattern !== null && 
                           this.shouldCreateNodeForPattern(relevantPattern) &&
                           !isMultilineBlockContinuation, // Don't create new nodes within multi-line blocks
      suggestedNodeType: relevantPattern ? this.getNodeTypeForPattern(relevantPattern) : undefined,
      multilineBlockContext,
      isMultilineBlockContinuation
    };

    // Cache the last detection
    this.lastDetection = context;
    
    // Emit event to subscribers
    this.emitContextChange(context);
    
    // Handle auto node creation if enabled
    if (context.shouldCreateNewNode && this.options.autoCreateNodes) {
      this.scheduleNodeCreation(context, nodeId);
    }

    return context;
  }

  /**
   * Get node creation suggestion for detected pattern
   */
  getNodeCreationSuggestion(context: SoftNewlineContext): NodeCreationSuggestion | null {
    if (!context.detectedPattern || !context.shouldCreateNewNode) {
      return null;
    }

    const pattern = context.detectedPattern;
    const nodeType = this.getNodeTypeForPattern(pattern);
    
    // Extract content without markdown syntax
    const cleanContent = this.extractCleanContent(pattern);
    
    return {
      nodeType,
      content: cleanContent,
      rawContent: context.contentAfter,
      triggerPattern: pattern,
      insertPosition: context.newlinePosition + 1,
      relationship: this.getNodeRelationship(pattern)
    };
  }

  /**
   * Handle real-time typing after soft newline with debouncing
   */
  processRealtimeTyping(
    content: string,
    cursorPosition: number,
    nodeId: string = 'default'
  ): Promise<SoftNewlineContext> {
    return new Promise((resolve) => {
      // Clear existing timeout for this node
      const existingTimeout = this.processingTimeouts.get(nodeId);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }

      // Set new debounced processing
      const timeout = setTimeout(() => {
        const context = this.processSoftNewlineContent(content, cursorPosition, nodeId);
        this.processingTimeouts.delete(nodeId);
        resolve(context);
      }, this.options.debounceTime);

      this.processingTimeouts.set(nodeId, timeout);
    });
  }

  /**
   * Cancel processing for a specific node (cleanup)
   */
  cancelProcessing(nodeId: string): void {
    const timeout = this.processingTimeouts.get(nodeId);
    if (timeout) {
      clearTimeout(timeout);
      this.processingTimeouts.delete(nodeId);
    }
  }

  /**
   * Subscribe to soft newline context changes
   */
  subscribe(callback: (context: SoftNewlineContext) => void): () => void {
    this.eventCallbacks.add(callback);
    
    return () => {
      this.eventCallbacks.delete(callback);
    };
  }

  /**
   * Get the last detected context (useful for testing)
   */
  getLastContext(): SoftNewlineContext | null {
    return this.lastDetection;
  }

  /**
   * Private helper methods
   */

  private createEmptyContext(content: string, cursorPosition: number, newlinePosition?: number): SoftNewlineContext {
    return {
      hasMarkdownAfterNewline: false,
      contentBefore: newlinePosition !== undefined ? content.substring(0, newlinePosition) : content,
      contentAfter: newlinePosition !== undefined ? content.substring(newlinePosition + 1) : '',
      newlinePosition: newlinePosition || -1,
      shouldCreateNewNode: false,
      isMultilineBlockContinuation: false
    };
  }

  private findRelevantPattern(content: string, patterns: MarkdownPattern[]): MarkdownPattern | null {
    if (patterns.length === 0) {
      return null;
    }

    // Look for patterns that start near the beginning of the content
    const earlyPatterns = patterns.filter(p => p.start <= 3); // Allow for some whitespace
    
    if (earlyPatterns.length === 0) {
      return null;
    }

    // Prioritize block patterns over inline patterns
    const blockPatterns = earlyPatterns.filter(p => 
      ['header', 'bullet', 'blockquote', 'codeblock'].includes(p.type)
    );
    
    if (blockPatterns.length > 0) {
      return blockPatterns[0]; // Return the first block pattern
    }

    // If no block patterns, return the first inline pattern
    return earlyPatterns[0];
  }

  private shouldCreateNodeForPattern(pattern: MarkdownPattern): boolean {
    // Only create nodes for block-level patterns
    const blockPatterns = ['header', 'bullet', 'blockquote'];
    return blockPatterns.includes(pattern.type);
  }

  private getNodeTypeForPattern(pattern: MarkdownPattern): 'text' | 'task' | 'ai-chat' | 'entity' | 'query' {
    switch (pattern.type) {
      case 'header':
        return 'text'; // Headers become text nodes with special styling
        
      case 'bullet':
        // Smart bullet interpretation
        const content = pattern.content.toLowerCase();
        if (content.includes('todo') || content.includes('task') || content.includes('do ')) {
          return 'task';
        }
        if (content.includes('ask') || content.includes('question') || content.includes('?')) {
          return 'ai-chat';
        }
        if (content.includes('@') || content.includes('person') || content.includes('contact')) {
          return 'entity';
        }
        if (content.includes('search') || content.includes('find') || content.includes('query')) {
          return 'query';
        }
        return 'text';
        
      case 'blockquote':
        return 'ai-chat'; // Quotes often represent conversations or AI interactions
        
      case 'codeblock':
        return 'text'; // Code becomes text node with code styling
        
      default:
        return 'text';
    }
  }

  private extractCleanContent(pattern: MarkdownPattern): string {
    // Return content without markdown syntax
    return pattern.content.trim();
  }

  private getNodeRelationship(pattern: MarkdownPattern): 'child' | 'sibling' {
    // Headers typically create sibling relationships
    // Bullets and quotes typically create child relationships
    return pattern.type === 'header' ? 'sibling' : 'child';
  }

  private scheduleNodeCreation(context: SoftNewlineContext, nodeId: string): void {
    // This would integrate with the actual node creation system
    // For now, just emit an event that the UI can listen to
    const suggestion = this.getNodeCreationSuggestion(context);
    if (suggestion) {
      this.emitNodeCreationSuggestion(suggestion, nodeId);
    }
  }

  private emitContextChange(context: SoftNewlineContext): void {
    this.eventCallbacks.forEach(callback => {
      try {
        callback(context);
      } catch (error) {
        console.warn('Soft newline context callback error:', error);
      }
    });
  }

  private emitNodeCreationSuggestion(suggestion: NodeCreationSuggestion, nodeId: string): void {
    // Emit custom event that can be caught by the UI components
    const event = new CustomEvent('nodespace:node-creation-suggestion', {
      detail: { suggestion, sourceNodeId: nodeId }
    });
    
    // Dispatch on document for global handling
    document.dispatchEvent(event);
  }
}

/**
 * Default singleton instance for convenience
 */
export const softNewlineProcessor = new SoftNewlineProcessor();

/**
 * Utility functions for BaseNode integration
 */
export class SoftNewlineIntegration {
  
  /**
   * Handle keyboard event in BaseNode and detect soft newlines
   */
  static handleKeyboardEvent(
    event: KeyboardEvent,
    content: string,
    cursorPosition: number,
    nodeId: string,
    processor: SoftNewlineProcessor = softNewlineProcessor
  ): boolean {
    // Handle regular Enter key in multi-line blocks
    if (event.key === 'Enter' && !event.shiftKey) {
      const multilineBlockContext = multilineBlockProcessor.getBlockContinuationContext(content, cursorPosition);
      
      if (multilineBlockContext.inBlock && multilineBlockContext.shouldContinue) {
        // Let the multi-line block processor handle Enter within blocks
        const result = multilineBlockProcessor.handleEnterInBlock(content, cursorPosition);
        
        if (result.shouldPreventDefault) {
          event.preventDefault();
          
          // Update the DOM element with new content
          const target = event.target as HTMLElement;
          if (result.newContent !== undefined) {
            target.textContent = result.newContent;
            
            // Set cursor position
            if (result.newCursorPosition !== undefined) {
              SoftNewlineIntegration.setCursorPosition(target, result.newCursorPosition);
            }
          }
          
          return true; // Event handled
        }
      }
    }
    
    if (processor.isShiftEnter(event)) {
      // Allow the soft newline to be inserted
      // The real processing happens on subsequent typing
      return true; // Allow default behavior (insert newline)
    }
    
    return false; // Not a soft newline event
  }

  /**
   * Handle input changes after soft newline
   */
  static handleInputChange(
    content: string,
    cursorPosition: number,
    nodeId: string,
    processor: SoftNewlineProcessor = softNewlineProcessor
  ): Promise<SoftNewlineContext> {
    return processor.processRealtimeTyping(content, cursorPosition, nodeId);
  }

  /**
   * Get cursor position for DOM content editable element
   */
  static getCursorPosition(element: HTMLElement): number {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) {
      return 0;
    }

    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(element);
    preCaretRange.setEnd(range.endContainer, range.endOffset);
    
    return preCaretRange.toString().length;
  }

  /**
   * Set cursor position in DOM content editable element (public method)
   */
  static setCursorPosition(element: HTMLElement, position: number): void {
    const selection = window.getSelection();
    if (!selection) return;

    const walker = document.createTreeWalker(
      element,
      NodeFilter.SHOW_TEXT,
      null
    );

    let currentPosition = 0;
    let textNode = walker.nextNode() as Text;

    while (textNode) {
      const nodeLength = textNode.textContent?.length || 0;
      
      if (currentPosition + nodeLength >= position) {
        const offset = position - currentPosition;
        const range = document.createRange();
        range.setStart(textNode, Math.max(0, Math.min(offset, nodeLength)));
        range.collapse(true);
        
        selection.removeAllRanges();
        selection.addRange(range);
        return;
      }
      
      currentPosition += nodeLength;
      textNode = walker.nextNode() as Text;
    }

    // If we get here, position is at the end
    const range = document.createRange();
    range.selectNodeContents(element);
    range.collapse(false);
    selection.removeAllRanges();
    selection.addRange(range);
  }
}

/**
 * Performance monitoring utilities
 */
export class SoftNewlineMetrics {
  private static measurements = new Map<string, number[]>();

  static recordProcessingTime(operation: string, time: number): void {
    if (!this.measurements.has(operation)) {
      this.measurements.set(operation, []);
    }
    
    const times = this.measurements.get(operation)!;
    times.push(time);
    
    // Keep only the last 100 measurements
    if (times.length > 100) {
      times.shift();
    }
  }

  static getAverageTime(operation: string): number {
    const times = this.measurements.get(operation);
    if (!times || times.length === 0) {
      return 0;
    }
    
    return times.reduce((sum, time) => sum + time, 0) / times.length;
  }

  static getMetrics(): Record<string, { average: number; samples: number; max: number; min: number }> {
    const metrics: Record<string, { average: number; samples: number; max: number; min: number }> = {};
    
    for (const [operation, times] of this.measurements) {
      if (times.length > 0) {
        metrics[operation] = {
          average: times.reduce((sum, time) => sum + time, 0) / times.length,
          samples: times.length,
          max: Math.max(...times),
          min: Math.min(...times)
        };
      }
    }
    
    return metrics;
  }

  static clearMetrics(): void {
    this.measurements.clear();
  }
}