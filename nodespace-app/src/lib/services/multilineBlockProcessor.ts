/**
 * Multi-line Block Processor Service
 * 
 * Handles proper behavior for multi-line markdown blocks (blockquotes and code blocks)
 * that span multiple lines within a single node. Provides intelligent continuation
 * patterns, termination detection, and integration with existing WYSIWYG features.
 * 
 * Key features:
 * - Multi-line blockquote support (`> Line 1\n> Line 2`)
 * - Multi-line code block support (````\ncode\n````)
 * - Block continuation patterns (typing `>` on new line in blockquote)
 * - Block termination detection (empty line or different markdown)
 * - Cursor positioning within multi-line blocks
 * - Performance optimization for large blocks
 */

import { markdownPatternDetector } from './markdownPatternDetector.js';
import { wysiwygProcessor } from './wysiwygProcessor.js';
import { softNewlineProcessor } from './softNewlineProcessor.js';
import { patternIntegrationUtils } from './markdownPatternUtils.js';
import type {
  MarkdownPattern,
  MarkdownPatternType,
  PatternDetectionResult,
  CursorPosition
} from '$lib/types/markdownPatterns';

/**
 * Multi-line block types that this processor handles
 */
export type MultilineBlockType = 'blockquote' | 'codeblock';

/**
 * Multi-line block detection result
 */
export interface MultilineBlock {
  /** Type of multi-line block */
  type: MultilineBlockType;
  
  /** Start position in content */
  start: number;
  
  /** End position in content */
  end: number;
  
  /** Array of line patterns that make up this block */
  linePatterns: MarkdownPattern[];
  
  /** Combined content of all lines */
  combinedContent: string;
  
  /** Original raw content including syntax */
  rawContent: string;
  
  /** Line numbers covered by this block */
  lineNumbers: number[];
  
  /** Whether this block is currently being typed (incomplete) */
  incomplete: boolean;
  
  /** Language for code blocks */
  language?: string;
  
  /** Indentation level for nested blocks */
  indentLevel: number;
}

/**
 * Block continuation context
 */
export interface BlockContinuationContext {
  /** Whether cursor is within a multi-line block */
  inBlock: boolean;
  
  /** Current block being typed in (if any) */
  currentBlock?: MultilineBlock;
  
  /** Expected continuation pattern for next line */
  expectedContinuation?: string;
  
  /** Whether next line should auto-continue the block */
  shouldContinue: boolean;
  
  /** Cursor position relative to block */
  cursorInBlock?: number;
  
  /** Line number within the block */
  blockLine?: number;
}

/**
 * Block processing options
 */
export interface MultilineBlockOptions {
  /** Enable auto-continuation on Enter key */
  autoContinue?: boolean;
  
  /** Maximum block size before performance optimizations kick in */
  maxBlockSize?: number;
  
  /** Minimum lines required to form a multi-line block */
  minBlockLines?: number;
  
  /** Enable smart termination detection */
  smartTermination?: boolean;
  
  /** Debounce time for real-time processing */
  debounceTime?: number;
}

/**
 * Block termination result
 */
export interface BlockTermination {
  /** Whether the block should be terminated */
  shouldTerminate: boolean;
  
  /** Reason for termination */
  reason?: 'empty_line' | 'different_pattern' | 'explicit_end' | 'max_lines';
  
  /** Position where termination occurs */
  terminationPoint?: number;
  
  /** Suggested action after termination */
  suggestedAction?: 'new_node' | 'continue_text' | 'end_editing';
}

/**
 * Main multi-line block processor class
 */
export class MultilineBlockProcessor {
  private options: Required<MultilineBlockOptions>;
  private lastBlocks: MultilineBlock[] = [];
  private processingTimeouts = new Map<string, NodeJS.Timeout>();
  private subscribers = new Set<(blocks: MultilineBlock[]) => void>();

  constructor(options: MultilineBlockOptions = {}) {
    this.options = {
      autoContinue: true,
      maxBlockSize: 10000, // 10KB per block
      minBlockLines: 2,
      smartTermination: true,
      debounceTime: 150,
      ...options
    };
  }

  /**
   * Detect and process multi-line blocks in content
   */
  detectMultilineBlocks(
    content: string,
    cursorPosition?: number,
    nodeId: string = 'default'
  ): MultilineBlock[] {
    const startTime = performance.now();
    
    // First get all patterns from existing detector
    const patternResult = cursorPosition !== undefined
      ? markdownPatternDetector.detectPatternsRealtime(content, cursorPosition)
      : markdownPatternDetector.detectPatterns(content);

    // Group patterns into multi-line blocks
    const multilineBlocks = this.groupIntoMultilineBlocks(content, patternResult.patterns);
    
    // Process each block for completion and context
    const processedBlocks = multilineBlocks.map(block => 
      this.processBlockContext(block, content, cursorPosition)
    );

    this.lastBlocks = processedBlocks;
    
    // Emit to subscribers
    this.emitBlocksChanged(processedBlocks);
    
    const processingTime = performance.now() - startTime;
    if (processingTime > 50) {
      console.warn(`Multi-line block processing took ${processingTime.toFixed(2)}ms`);
    }

    return processedBlocks;
  }

  /**
   * Get block continuation context for current cursor position
   */
  getBlockContinuationContext(
    content: string,
    cursorPosition: number
  ): BlockContinuationContext {
    const blocks = this.detectMultilineBlocks(content, cursorPosition);
    
    // Find block containing cursor
    const currentBlock = blocks.find(block => 
      cursorPosition >= block.start && cursorPosition <= block.end
    );

    if (!currentBlock) {
      return {
        inBlock: false,
        shouldContinue: false
      };
    }

    // Determine continuation pattern and context
    const continuation = this.determineContinuationPattern(currentBlock, content, cursorPosition);
    
    return {
      inBlock: true,
      currentBlock,
      expectedContinuation: continuation.pattern,
      shouldContinue: continuation.shouldContinue && this.options.autoContinue,
      cursorInBlock: cursorPosition - currentBlock.start,
      blockLine: this.getBlockLineNumber(currentBlock, cursorPosition, content)
    };
  }

  /**
   * Handle Enter key press within multi-line block
   */
  handleEnterInBlock(
    content: string,
    cursorPosition: number
  ): {
    shouldPreventDefault: boolean;
    newContent?: string;
    newCursorPosition?: number;
    continuation?: string;
  } {
    const context = this.getBlockContinuationContext(content, cursorPosition);
    
    if (!context.inBlock || !context.currentBlock || !this.options.autoContinue) {
      return { shouldPreventDefault: false };
    }

    // Check if we're at the end of a line within the block
    const lines = content.split('\n');
    const currentLine = this.getCurrentLineFromPosition(content, cursorPosition);
    const lineIndex = lines.indexOf(currentLine);
    
    if (lineIndex === -1) {
      return { shouldPreventDefault: false };
    }

    // Check for termination conditions
    const termination = this.checkBlockTermination(context.currentBlock, content, cursorPosition);
    if (termination.shouldTerminate) {
      return { shouldPreventDefault: false };
    }

    // Generate continuation
    const continuation = context.expectedContinuation || this.getDefaultContinuation(context.currentBlock);
    
    const beforeCursor = content.substring(0, cursorPosition);
    const afterCursor = content.substring(cursorPosition);
    const newContent = beforeCursor + '\n' + continuation + afterCursor;
    const newCursorPosition = cursorPosition + 1 + continuation.length;

    return {
      shouldPreventDefault: true,
      newContent,
      newCursorPosition,
      continuation
    };
  }

  /**
   * Process real-time typing within multi-line blocks
   */
  processRealtimeTyping(
    content: string,
    cursorPosition: number,
    nodeId: string = 'default'
  ): Promise<MultilineBlock[]> {
    return new Promise((resolve) => {
      // Clear existing timeout
      const existingTimeout = this.processingTimeouts.get(nodeId);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }

      // Set debounced processing
      const timeout = setTimeout(() => {
        const blocks = this.detectMultilineBlocks(content, cursorPosition, nodeId);
        this.processingTimeouts.delete(nodeId);
        resolve(blocks);
      }, this.options.debounceTime);

      this.processingTimeouts.set(nodeId, timeout);
    });
  }

  /**
   * Check if block should be terminated
   */
  checkBlockTermination(
    block: MultilineBlock,
    content: string,
    cursorPosition: number
  ): BlockTermination {
    if (!this.options.smartTermination) {
      return { shouldTerminate: false };
    }

    const lines = content.split('\n');
    const currentLineIndex = this.getLineIndexFromPosition(content, cursorPosition);
    
    if (currentLineIndex < 0 || currentLineIndex >= lines.length) {
      return { shouldTerminate: false };
    }

    const currentLine = lines[currentLineIndex];
    const trimmedLine = currentLine.trim();

    // Check for empty line termination
    if (trimmedLine === '') {
      return {
        shouldTerminate: true,
        reason: 'empty_line',
        terminationPoint: cursorPosition,
        suggestedAction: 'continue_text'
      };
    }

    // Check for explicit code block end
    if (block.type === 'codeblock' && trimmedLine === '```') {
      return {
        shouldTerminate: true,
        reason: 'explicit_end',
        terminationPoint: cursorPosition,
        suggestedAction: 'continue_text'
      };
    }

    // Check for different pattern (blockquote switching to bullet, etc.)
    if (block.type === 'blockquote' && !currentLine.match(/^\s*>/)) {
      const hasDifferentPattern = this.hasNonBlockquotePattern(currentLine);
      if (hasDifferentPattern) {
        return {
          shouldTerminate: true,
          reason: 'different_pattern',
          terminationPoint: cursorPosition,
          suggestedAction: 'new_node'
        };
      }
    }

    // Check for max size
    if (block.rawContent.length > this.options.maxBlockSize) {
      return {
        shouldTerminate: true,
        reason: 'max_lines',
        terminationPoint: cursorPosition,
        suggestedAction: 'new_node'
      };
    }

    return { shouldTerminate: false };
  }

  /**
   * Get optimized cursor positioning for large blocks
   */
  getOptimizedCursorPosition(
    block: MultilineBlock,
    targetPosition: number,
    content: string
  ): number {
    // For large blocks, provide optimized cursor positioning
    if (block.rawContent.length > this.options.maxBlockSize) {
      // Use binary search approach for large blocks
      return this.binarySearchCursorPosition(block, targetPosition, content);
    }

    return targetPosition;
  }

  /**
   * Subscribe to multi-line block changes
   */
  subscribe(callback: (blocks: MultilineBlock[]) => void): () => void {
    this.subscribers.add(callback);
    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Get last detected blocks
   */
  getLastBlocks(): MultilineBlock[] {
    return [...this.lastBlocks];
  }

  /**
   * Cancel processing for specific node
   */
  cancelProcessing(nodeId: string): void {
    const timeout = this.processingTimeouts.get(nodeId);
    if (timeout) {
      clearTimeout(timeout);
      this.processingTimeouts.delete(nodeId);
    }
  }

  /**
   * Private helper methods
   */

  private groupIntoMultilineBlocks(content: string, patterns: MarkdownPattern[]): MultilineBlock[] {
    const blocks: MultilineBlock[] = [];
    const lines = content.split('\n');
    
    // Group blockquote patterns
    const blockquoteGroups = this.groupConsecutivePatterns(patterns, 'blockquote', lines);
    blocks.push(...blockquoteGroups.map(group => this.createBlockquoteBlock(group, content)));

    // Group code block patterns (these are already multi-line from the detector)
    const codeblocks = patterns.filter(p => p.type === 'codeblock');
    blocks.push(...codeblocks.map(pattern => this.createCodeblockBlock(pattern, content)));

    return blocks.sort((a, b) => a.start - b.start);
  }

  private groupConsecutivePatterns(
    patterns: MarkdownPattern[],
    type: MarkdownPatternType,
    lines: string[]
  ): MarkdownPattern[][] {
    const typePatterns = patterns.filter(p => p.type === type);
    if (typePatterns.length < this.options.minBlockLines) {
      return [];
    }

    const groups: MarkdownPattern[][] = [];
    let currentGroup: MarkdownPattern[] = [];

    for (let i = 0; i < typePatterns.length; i++) {
      const pattern = typePatterns[i];
      
      if (currentGroup.length === 0) {
        currentGroup.push(pattern);
        continue;
      }

      const lastPattern = currentGroup[currentGroup.length - 1];
      const isConsecutive = pattern.line === lastPattern.line + 1;
      
      if (isConsecutive) {
        currentGroup.push(pattern);
      } else {
        // Gap found, finish current group if it meets minimum size
        if (currentGroup.length >= this.options.minBlockLines) {
          groups.push([...currentGroup]);
        }
        currentGroup = [pattern];
      }
    }

    // Add final group if it meets minimum size
    if (currentGroup.length >= this.options.minBlockLines) {
      groups.push(currentGroup);
    }

    return groups;
  }

  private createBlockquoteBlock(patterns: MarkdownPattern[], content: string): MultilineBlock {
    const start = patterns[0].start;
    const end = patterns[patterns.length - 1].end;
    const combinedContent = patterns.map(p => p.content).join('\n');
    const rawContent = content.substring(start, end);
    const lineNumbers = patterns.map(p => p.line);
    
    // Determine indentation level
    const firstPattern = patterns[0];
    const indentLevel = this.getIndentationLevel(content, firstPattern);

    return {
      type: 'blockquote',
      start,
      end,
      linePatterns: patterns,
      combinedContent,
      rawContent,
      lineNumbers,
      incomplete: this.isBlockIncomplete(patterns, 'blockquote'),
      indentLevel
    };
  }

  private createCodeblockBlock(pattern: MarkdownPattern, content: string): MultilineBlock {
    const lines = content.split('\n');
    const startLine = pattern.line;
    const endLine = this.findCodeBlockEndLine(content, pattern);
    
    const lineNumbers: number[] = [];
    for (let i = startLine; i <= endLine; i++) {
      lineNumbers.push(i);
    }

    const indentLevel = this.getIndentationLevel(content, pattern);

    return {
      type: 'codeblock',
      start: pattern.start,
      end: pattern.end,
      linePatterns: [pattern],
      combinedContent: pattern.content,
      rawContent: content.substring(pattern.start, pattern.end),
      lineNumbers,
      incomplete: this.isBlockIncomplete([pattern], 'codeblock'),
      language: pattern.language,
      indentLevel
    };
  }

  private processBlockContext(
    block: MultilineBlock,
    content: string,
    cursorPosition?: number
  ): MultilineBlock {
    // Add cursor-specific processing if cursor is within block
    if (cursorPosition !== undefined && 
        cursorPosition >= block.start && 
        cursorPosition <= block.end) {
      
      // Check if block is being actively typed
      const isActivelyTyping = this.isActivelyTyping(block, content, cursorPosition);
      block.incomplete = block.incomplete || isActivelyTyping;
    }

    return block;
  }

  private determineContinuationPattern(
    block: MultilineBlock,
    content: string,
    cursorPosition: number
  ): { pattern: string; shouldContinue: boolean } {
    switch (block.type) {
      case 'blockquote':
        const indentation = ' '.repeat(block.indentLevel * 2);
        return {
          pattern: indentation + '> ',
          shouldContinue: true
        };
        
      case 'codeblock':
        // Code blocks don't have continuation patterns within the block
        const codeIndentation = ' '.repeat(block.indentLevel * 2);
        return {
          pattern: codeIndentation,
          shouldContinue: true
        };
        
      default:
        return {
          pattern: '',
          shouldContinue: false
        };
    }
  }

  private getDefaultContinuation(block: MultilineBlock): string {
    const indentation = ' '.repeat(block.indentLevel * 2);
    
    switch (block.type) {
      case 'blockquote':
        return indentation + '> ';
      case 'codeblock':
        return indentation;
      default:
        return '';
    }
  }

  private getBlockLineNumber(block: MultilineBlock, cursorPosition: number, content: string): number {
    const blockContent = content.substring(block.start, cursorPosition);
    return blockContent.split('\n').length - 1;
  }

  private getCurrentLineFromPosition(content: string, position: number): string {
    const lines = content.split('\n');
    let currentPos = 0;
    
    for (const line of lines) {
      if (position <= currentPos + line.length) {
        return line;
      }
      currentPos += line.length + 1; // +1 for newline
    }
    
    return lines[lines.length - 1] || '';
  }

  private getLineIndexFromPosition(content: string, position: number): number {
    const beforePosition = content.substring(0, position);
    return beforePosition.split('\n').length - 1;
  }

  private hasNonBlockquotePattern(line: string): boolean {
    // Check for bullet patterns
    if (line.match(/^\s*[-*+]\s/)) return true;
    
    // Check for header patterns
    if (line.match(/^\s*#{1,6}\s/)) return true;
    
    // Check for code block patterns
    if (line.match(/^\s*```/)) return true;
    
    return false;
  }

  private binarySearchCursorPosition(
    block: MultilineBlock,
    targetPosition: number,
    content: string
  ): number {
    // Simplified binary search for demonstration
    // In practice, would implement more sophisticated cursor positioning
    return Math.max(block.start, Math.min(targetPosition, block.end));
  }

  private findCodeBlockEndLine(content: string, pattern: MarkdownPattern): number {
    const lines = content.split('\n');
    const startLine = pattern.line;
    
    // Look for closing ``` after the opening
    for (let i = startLine + 1; i < lines.length; i++) {
      if (lines[i].trim() === '```') {
        return i;
      }
    }
    
    // If no closing found, it's incomplete
    return lines.length - 1;
  }

  private getIndentationLevel(content: string, pattern: MarkdownPattern): number {
    const lines = content.split('\n');
    const line = lines[pattern.line];
    if (!line) return 0;
    
    const match = line.match(/^(\s*)/);
    const spaces = match ? match[1].length : 0;
    
    return Math.floor(spaces / 2); // 2 spaces per indent level
  }

  private isBlockIncomplete(patterns: MarkdownPattern[], type: MarkdownPatternType): boolean {
    switch (type) {
      case 'blockquote':
        // Blockquotes are incomplete if they don't end with an empty line or different pattern
        return true; // Always consider incomplete during typing
        
      case 'codeblock':
        // Check if code block has closing ```
        const lastPattern = patterns[patterns.length - 1];
        return !lastPattern.content.endsWith('```');
        
      default:
        return false;
    }
  }

  private isActivelyTyping(block: MultilineBlock, content: string, cursorPosition: number): boolean {
    // Check if cursor is at the end of the last line in the block
    const lastLineStart = content.lastIndexOf('\n', block.end - 1);
    const lastLineEnd = block.end;
    
    return cursorPosition >= lastLineStart && cursorPosition <= lastLineEnd;
  }

  private emitBlocksChanged(blocks: MultilineBlock[]): void {
    this.subscribers.forEach(callback => {
      try {
        callback(blocks);
      } catch (error) {
        console.warn('Multi-line block callback error:', error);
      }
    });
  }
}

/**
 * Default singleton instance
 */
export const multilineBlockProcessor = new MultilineBlockProcessor();

/**
 * Integration utilities for BaseNode and other components
 */
export class MultilineBlockIntegration {
  /**
   * Handle keyboard events for multi-line blocks
   */
  static handleKeyboardEvent(
    event: KeyboardEvent,
    content: string,
    cursorPosition: number,
    processor: MultilineBlockProcessor = multilineBlockProcessor
  ): boolean {
    if (event.key === 'Enter' && !event.shiftKey) {
      const result = processor.handleEnterInBlock(content, cursorPosition);
      
      if (result.shouldPreventDefault) {
        event.preventDefault();
        
        // Update content in the DOM element
        const target = event.target as HTMLElement;
        if (result.newContent !== undefined) {
          target.textContent = result.newContent;
          
          // Set cursor position
          if (result.newCursorPosition !== undefined) {
            this.setCursorPosition(target, result.newCursorPosition);
          }
        }
        
        return true;
      }
    }
    
    return false;
  }

  /**
   * Get multi-line block context for WYSIWYG integration
   */
  static getBlockContext(
    content: string,
    cursorPosition: number,
    processor: MultilineBlockProcessor = multilineBlockProcessor
  ): {
    inMultilineBlock: boolean;
    blockType?: MultilineBlockType;
    blockContent?: string;
    continuationPattern?: string;
  } {
    const context = processor.getBlockContinuationContext(content, cursorPosition);
    
    return {
      inMultilineBlock: context.inBlock,
      blockType: context.currentBlock?.type,
      blockContent: context.currentBlock?.combinedContent,
      continuationPattern: context.expectedContinuation
    };
  }

  /**
   * Set cursor position in DOM element
   */
  private static setCursorPosition(element: HTMLElement, position: number): void {
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
 * Performance monitoring for multi-line blocks
 */
export class MultilineBlockMetrics {
  private static measurements = new Map<string, number[]>();

  static recordBlockProcessingTime(blockType: MultilineBlockType, time: number): void {
    const key = `block_processing_${blockType}`;
    this.recordTime(key, time);
  }

  static recordContinuationTime(time: number): void {
    this.recordTime('continuation_processing', time);
  }

  private static recordTime(operation: string, time: number): void {
    if (!this.measurements.has(operation)) {
      this.measurements.set(operation, []);
    }
    
    const times = this.measurements.get(operation)!;
    times.push(time);
    
    // Keep only last 50 measurements
    if (times.length > 50) {
      times.shift();
    }
  }

  static getMetrics(): Record<string, { average: number; samples: number; max: number }> {
    const metrics: Record<string, { average: number; samples: number; max: number }> = {};
    
    for (const [operation, times] of this.measurements) {
      if (times.length > 0) {
        metrics[operation] = {
          average: times.reduce((sum, time) => sum + time, 0) / times.length,
          samples: times.length,
          max: Math.max(...times)
        };
      }
    }
    
    return metrics;
  }

  static clearMetrics(): void {
    this.measurements.clear();
  }
}