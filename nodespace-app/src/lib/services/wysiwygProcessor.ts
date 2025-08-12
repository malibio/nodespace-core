/**
 * WYSIWYG Processor Service
 * 
 * Real-time WYSIWYG processing that hides markdown syntax characters and applies
 * visual formatting as users type. Integrates with pattern detection system
 * and provides performance optimizations for sub-50ms updates.
 */

import { markdownPatternDetector } from './markdownPatternDetector.js';
import { patternIntegrationUtils } from './markdownPatternUtils.js';
import { multilineBlockProcessor } from './multilineBlockProcessor.js';
import type {
  MarkdownPattern,
  PatternDetectionResult,
  PatternDetectionOptions,
  CursorPosition
} from '$lib/types/markdownPatterns.js';
import type { MultilineBlock } from './multilineBlockProcessor.js';

/**
 * WYSIWYG processing configuration
 */
export interface WYSIWYGConfig {
  /** Enable real-time processing (default: true) */
  enableRealTime?: boolean;
  
  /** Performance mode - skip expensive operations (default: false) */
  performanceMode?: boolean;
  
  /** Maximum processing time before throttling (default: 50ms) */
  maxProcessingTime?: number;
  
  /** Debounce delay for rapid typing (default: 16ms - ~1 frame) */
  debounceDelay?: number;
  
  /** Enable syntax hiding (default: true) */
  hideSyntax?: boolean;
  
  /** Enable visual formatting (default: true) */
  enableFormatting?: boolean;
  
  /** Custom CSS class prefix (default: 'wysiwyg') */
  cssPrefix?: string;
}

/**
 * WYSIWYG processing result
 */
export interface WYSIWYGResult {
  /** Original content */
  originalContent: string;
  
  /** Processed HTML with hidden syntax and formatting */
  processedHTML: string;
  
  /** CSS classes to apply to characters */
  characterClasses: Record<number, string[]>;
  
  /** Detected patterns */
  patterns: MarkdownPattern[];
  
  /** Multi-line blocks detected */
  multilineBlocks: MultilineBlock[];
  
  /** Processing time in milliseconds */
  processingTime: number;
  
  /** Cursor position after processing */
  adjustedCursorPosition?: number;
  
  /** Performance warnings */
  warnings: string[];
}

/**
 * WYSIWYG processing event
 */
export interface WYSIWYGEvent {
  type: 'processed' | 'throttled' | 'error';
  result?: WYSIWYGResult;
  error?: string;
  timestamp: number;
}

/**
 * Main WYSIWYG processor class
 */
export class WYSIWYGProcessor {
  private config: Required<WYSIWYGConfig>;
  private lastProcessingTime = 0;
  private debounceTimeout: number | null = null;
  private subscribers: Array<(event: WYSIWYGEvent) => void> = [];
  private isProcessing = false;

  constructor(config: WYSIWYGConfig = {}) {
    this.config = {
      enableRealTime: true,
      performanceMode: false,
      maxProcessingTime: 50,
      debounceDelay: 16,
      hideSyntax: true,
      enableFormatting: true,
      cssPrefix: 'wysiwyg',
      ...config
    };
  }

  /**
   * Process content for WYSIWYG display
   */
  async process(
    content: string, 
    cursorPosition?: number,
    options: PatternDetectionOptions = {}
  ): Promise<WYSIWYGResult> {
    const startTime = performance.now();
    const warnings: string[] = [];

    try {
      this.isProcessing = true;

      // Detect patterns with performance optimization
      const detectionOptions = {
        performanceMode: this.config.performanceMode,
        ...options
      };

      const detectionResult: PatternDetectionResult = cursorPosition !== undefined
        ? markdownPatternDetector.detectPatternsRealtime(content, cursorPosition, detectionOptions)
        : markdownPatternDetector.detectPatterns(content, detectionOptions);

      // Check for performance warnings from detection
      warnings.push(...detectionResult.warnings);

      // Detect multi-line blocks
      const multilineBlocks = multilineBlockProcessor.detectMultilineBlocks(content, cursorPosition);

      // Process patterns for WYSIWYG display, incorporating multi-line block context
      const characterClasses = this.config.enableFormatting 
        ? this.generateEnhancedCSSClasses(detectionResult.patterns, multilineBlocks)
        : {};

      // Generate processed HTML with multi-line block support
      const processedHTML = await this.generateProcessedHTML(content, detectionResult.patterns, multilineBlocks);

      // Adjust cursor position if needed
      let adjustedCursorPosition = cursorPosition;
      if (cursorPosition !== undefined) {
        adjustedCursorPosition = patternIntegrationUtils.adjustCursorForPatterns(
          content, 
          cursorPosition, 
          detectionResult.patterns
        );
      }

      const processingTime = performance.now() - startTime;
      this.lastProcessingTime = processingTime;

      // Performance warning
      if (processingTime > this.config.maxProcessingTime) {
        warnings.push(`WYSIWYG processing took ${processingTime.toFixed(2)}ms, exceeding ${this.config.maxProcessingTime}ms target`);
      }

      const result: WYSIWYGResult = {
        originalContent: content,
        processedHTML,
        characterClasses: this.addWYSIWYGClasses(characterClasses, detectionResult.patterns, multilineBlocks),
        patterns: detectionResult.patterns,
        multilineBlocks,
        processingTime,
        adjustedCursorPosition,
        warnings
      };

      // Emit processed event
      this.emitEvent({
        type: 'processed',
        result,
        timestamp: Date.now()
      });

      return result;

    } catch (error) {
      const processingTime = performance.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : 'Unknown processing error';
      
      warnings.push(`Processing error: ${errorMessage}`);

      this.emitEvent({
        type: 'error',
        error: errorMessage,
        timestamp: Date.now()
      });

      // Return fallback result
      return {
        originalContent: content,
        processedHTML: this.escapeHTML(content),
        characterClasses: {},
        patterns: [],
        multilineBlocks: [],
        processingTime,
        warnings
      };
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * Process content with debouncing for real-time typing
   */
  processRealTime(
    content: string,
    cursorPosition: number,
    callback: (result: WYSIWYGResult) => void
  ): void {
    if (!this.config.enableRealTime) {
      return;
    }

    // Clear existing debounce
    if (this.debounceTimeout !== null) {
      clearTimeout(this.debounceTimeout);
    }

    // Performance throttling - if last processing took too long, increase debounce
    const adaptiveDelay = this.lastProcessingTime > this.config.maxProcessingTime 
      ? this.config.debounceDelay * 2 
      : this.config.debounceDelay;

    this.debounceTimeout = setTimeout(async () => {
      try {
        const result = await this.process(content, cursorPosition);
        callback(result);
      } catch (error) {
        console.warn('Real-time WYSIWYG processing error:', error);
      }
      this.debounceTimeout = null;
    }, adaptiveDelay) as unknown as number;
  }

  /**
   * Generate processed HTML with hidden syntax and formatting
   */
  private async generateProcessedHTML(content: string, patterns: MarkdownPattern[], multilineBlocks: MultilineBlock[] = []): Promise<string> {
    if (!this.config.hideSyntax && !this.config.enableFormatting) {
      return this.escapeHTML(content);
    }

    // Sort patterns by position for sequential processing
    const sortedPatterns = [...patterns].sort((a, b) => a.start - b.start);
    
    let result = '';
    let currentPos = 0;

    for (const pattern of sortedPatterns) {
      // Add text before pattern
      if (currentPos < pattern.start) {
        result += this.escapeHTML(content.substring(currentPos, pattern.start));
      }

      // Process pattern
      result += await this.processPattern(pattern, content.substring(pattern.start, pattern.end));
      currentPos = pattern.end;
    }

    // Add remaining text
    if (currentPos < content.length) {
      result += this.escapeHTML(content.substring(currentPos));
    }

    return result;
  }

  /**
   * Process individual pattern for WYSIWYG display
   */
  private async processPattern(pattern: MarkdownPattern, originalText: string): Promise<string> {
    const cssClasses = this.getPatternCSSClasses(pattern);
    const escapedContent = this.escapeHTML(pattern.content);

    switch (pattern.type) {
      case 'header':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)} ">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'bold':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="${this.escapeHTML(pattern.syntax)}" data-syntax-after="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'italic':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="${this.escapeHTML(pattern.syntax)}" data-syntax-after="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'inlinecode':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="\`" data-syntax-after="\`">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'bullet':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'blockquote':
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      case 'codeblock':
        if (this.config.hideSyntax) {
          const openSyntax = pattern.language ? `\`\`\`${pattern.language}` : '```';
          return `<span class="${cssClasses}" data-syntax-before="${openSyntax}" data-syntax-after="\`\`\`">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }

      default:
        return this.escapeHTML(originalText);
    }
  }

  /**
   * Get CSS classes for pattern
   */
  private getPatternCSSClasses(pattern: MarkdownPattern): string {
    const classes = [`${this.config.cssPrefix}-${pattern.type}`];

    switch (pattern.type) {
      case 'header':
        classes.push(`${this.config.cssPrefix}-header-${pattern.level}`);
        break;
      case 'bullet':
        classes.push(`${this.config.cssPrefix}-bullet-${pattern.bulletType}`);
        break;
      case 'codeblock':
        if (pattern.language) {
          classes.push(`${this.config.cssPrefix}-code-${pattern.language}`);
        }
        break;
    }

    if (this.config.hideSyntax) {
      classes.push(`${this.config.cssPrefix}-syntax-hidden`);
    }

    return classes.join(' ');
  }

  /**
   * Generate enhanced CSS classes incorporating multi-line block context
   */
  private generateEnhancedCSSClasses(patterns: MarkdownPattern[], multilineBlocks: MultilineBlock[]): Record<number, string[]> {
    const baseClasses = patternIntegrationUtils.toCSSClasses(patterns);
    const result = { ...baseClasses };

    // Add multi-line block specific classes
    for (const block of multilineBlocks) {
      for (let pos = block.start; pos < block.end; pos++) {
        if (!result[pos]) {
          result[pos] = [];
        }

        // Add block-level classes
        result[pos].push(`${this.config.cssPrefix}-multiline-${block.type}`);
        
        if (block.incomplete) {
          result[pos].push(`${this.config.cssPrefix}-multiline-incomplete`);
        }

        if (block.indentLevel > 0) {
          result[pos].push(`${this.config.cssPrefix}-multiline-indent-${block.indentLevel}`);
        }

        // Add language-specific classes for code blocks
        if (block.type === 'codeblock' && block.language) {
          result[pos].push(`${this.config.cssPrefix}-multiline-lang-${block.language}`);
        }
      }
    }

    return result;
  }

  /**
   * Add WYSIWYG-specific CSS classes to character class map
   */
  private addWYSIWYGClasses(
    characterClasses: Record<number, string[]>, 
    patterns: MarkdownPattern[],
    multilineBlocks: MultilineBlock[] = []
  ): Record<number, string[]> {
    const result = { ...characterClasses };

    for (const pattern of patterns) {
      for (let pos = pattern.start; pos < pattern.end; pos++) {
        if (!result[pos]) {
          result[pos] = [];
        }

        // Add WYSIWYG prefix to existing classes
        const wysiwygClasses = result[pos].map(cls => `${this.config.cssPrefix}-${cls}`);
        result[pos] = [...result[pos], ...wysiwygClasses];

        // Add syntax hiding classes
        if (this.config.hideSyntax && this.isSyntaxPosition(pos, pattern)) {
          result[pos].push(`${this.config.cssPrefix}-syntax`, `${this.config.cssPrefix}-syntax-hidden`);
        }
      }
    }

    return result;
  }

  /**
   * Check if position is a syntax character
   */
  private isSyntaxPosition(position: number, pattern: MarkdownPattern): boolean {
    const syntaxLength = this.getSyntaxLength(pattern);
    
    // Check if position is in start syntax
    if (position < pattern.start + syntaxLength) {
      return true;
    }
    
    // Check if position is in end syntax (for patterns with closing syntax)
    if (this.hasClosingSyntax(pattern) && position >= pattern.end - syntaxLength) {
      return true;
    }

    return false;
  }

  /**
   * Get syntax length for pattern
   */
  private getSyntaxLength(pattern: MarkdownPattern): number {
    switch (pattern.type) {
      case 'header':
        return pattern.syntax.length + 1; // # + space
      case 'bold':
      case 'italic':
        return pattern.syntax.length;
      case 'inlinecode':
        return 1;
      case 'bullet':
        return pattern.syntax.length;
      case 'blockquote':
        return pattern.syntax.length;
      case 'codeblock':
        return 3; // ```
      default:
        return 0;
    }
  }

  /**
   * Check if pattern has closing syntax
   */
  private hasClosingSyntax(pattern: MarkdownPattern): boolean {
    return ['bold', 'italic', 'inlinecode', 'codeblock'].includes(pattern.type);
  }

  /**
   * Escape HTML characters
   */
  private escapeHTML(text: string): string {
    if (!text || typeof text !== 'string') {
      return '';
    }
    
    const escapeMap: Record<string, string> = {
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;',
      '"': '&quot;',
      "'": '&#39;'
    };

    return text.replace(/[&<>"']/g, (char) => escapeMap[char]);
  }

  /**
   * Subscribe to WYSIWYG processing events
   */
  subscribe(callback: (event: WYSIWYGEvent) => void): () => void {
    this.subscribers.push(callback);
    
    return () => {
      const index = this.subscribers.indexOf(callback);
      if (index > -1) {
        this.subscribers.splice(index, 1);
      }
    };
  }

  /**
   * Get current configuration
   */
  getConfig(): Required<WYSIWYGConfig> {
    return { ...this.config };
  }

  /**
   * Update configuration
   */
  updateConfig(newConfig: Partial<WYSIWYGConfig>): void {
    this.config = { ...this.config, ...newConfig };
  }

  /**
   * Get processing metrics
   */
  getMetrics(): {
    lastProcessingTime: number;
    isProcessing: boolean;
    averageProcessingTime: number;
  } {
    return {
      lastProcessingTime: this.lastProcessingTime,
      isProcessing: this.isProcessing,
      averageProcessingTime: this.lastProcessingTime // TODO: Track average over time
    };
  }

  /**
   * Clear debounce timeout
   */
  cancelPendingProcessing(): void {
    if (this.debounceTimeout !== null) {
      clearTimeout(this.debounceTimeout);
      this.debounceTimeout = null;
    }
  }

  /**
   * Emit processing event
   */
  private emitEvent(event: WYSIWYGEvent): void {
    this.subscribers.forEach(callback => {
      try {
        callback(event);
      } catch (error) {
        console.warn('WYSIWYG event callback error:', error);
      }
    });
  }
}

/**
 * Default singleton instance for convenience
 */
export const wysiwygProcessor = new WYSIWYGProcessor();

/**
 * Utility functions for WYSIWYG integration
 */
export class WYSIWYGUtils {
  /**
   * Apply WYSIWYG processing to ContentEditable element
   */
  static async applyToElement(
    element: HTMLElement,
    processor: WYSIWYGProcessor = wysiwygProcessor
  ): Promise<WYSIWYGResult> {
    const content = element.textContent || '';
    const selection = window.getSelection();
    const cursorPosition = selection ? this.getCursorPosition(element, selection) : undefined;

    const result = await processor.process(content, cursorPosition);
    
    if (processor.getConfig().enableFormatting) {
      // Apply CSS classes to element
      this.applyCSSClasses(element, result.characterClasses);
    }

    return result;
  }

  /**
   * Get cursor position within element
   */
  private static getCursorPosition(element: HTMLElement, selection: Selection): number {
    if (selection.rangeCount === 0) return 0;

    const range = selection.getRangeAt(0);
    const preCaretRange = range.cloneRange();
    preCaretRange.selectNodeContents(element);
    preCaretRange.setEnd(range.endContainer, range.endOffset);
    
    return preCaretRange.toString().length;
  }

  /**
   * Apply CSS classes to element characters
   */
  private static applyCSSClasses(element: HTMLElement, characterClasses: Record<number, string[]>): void {
    // For now, we'll apply classes to the entire element
    // A more sophisticated implementation would wrap individual characters
    const allClasses = new Set<string>();
    
    Object.values(characterClasses).forEach(classes => {
      classes.forEach(cls => allClasses.add(cls));
    });

    allClasses.forEach(cls => element.classList.add(cls));
  }

  /**
   * Generate CSS for WYSIWYG styling
   */
  static generateWYSIWYGCSS(cssPrefix = 'wysiwyg'): string {
    return `
/* WYSIWYG Processor Styles */
.${cssPrefix}-syntax-hidden {
  opacity: 0.3;
  font-size: 0.8em;
}

.${cssPrefix}-header {
  font-weight: bold;
}

.${cssPrefix}-header-1 { font-size: 2em; }
.${cssPrefix}-header-2 { font-size: 1.5em; }
.${cssPrefix}-header-3 { font-size: 1.17em; }
.${cssPrefix}-header-4 { font-size: 1em; }
.${cssPrefix}-header-5 { font-size: 0.83em; }
.${cssPrefix}-header-6 { font-size: 0.67em; }

.${cssPrefix}-bold {
  font-weight: bold;
}

.${cssPrefix}-italic {
  font-style: italic;
}

.${cssPrefix}-inlinecode {
  font-family: monospace;
  background-color: rgba(175, 184, 193, 0.2);
  padding: 2px 4px;
  border-radius: 3px;
  font-size: 0.9em;
}

.${cssPrefix}-codeblock {
  font-family: monospace;
  background-color: rgba(175, 184, 193, 0.1);
  padding: 12px;
  border-radius: 6px;
  border-left: 3px solid #d0d7de;
  display: block;
  white-space: pre;
}

.${cssPrefix}-bullet {
  display: list-item;
  margin-left: 1.5em;
}

.${cssPrefix}-blockquote {
  border-left: 3px solid #d0d7de;
  padding-left: 16px;
  color: #656d76;
  font-style: italic;
}

/* Multi-line block styles */
.${cssPrefix}-multiline-blockquote {
  display: block;
  border-left: 4px solid #d0d7de;
  padding-left: 16px;
  margin: 8px 0;
  background-color: rgba(175, 184, 193, 0.05);
}

.${cssPrefix}-multiline-codeblock {
  display: block;
  background-color: rgba(175, 184, 193, 0.1);
  padding: 12px;
  border-radius: 6px;
  border-left: 3px solid #d0d7de;
  font-family: monospace;
  white-space: pre-wrap;
  margin: 8px 0;
}

.${cssPrefix}-multiline-incomplete {
  border-left-style: dashed;
  opacity: 0.8;
}

.${cssPrefix}-multiline-indent-1 { margin-left: 20px; }
.${cssPrefix}-multiline-indent-2 { margin-left: 40px; }
.${cssPrefix}-multiline-indent-3 { margin-left: 60px; }
.${cssPrefix}-multiline-indent-4 { margin-left: 80px; }
.${cssPrefix}-multiline-indent-5 { margin-left: 100px; }

/* Language-specific code block styling */
.${cssPrefix}-multiline-lang-javascript,
.${cssPrefix}-multiline-lang-js {
  border-left-color: #f1e05a;
}

.${cssPrefix}-multiline-lang-typescript,
.${cssPrefix}-multiline-lang-ts {
  border-left-color: #2b7489;
}

.${cssPrefix}-multiline-lang-python,
.${cssPrefix}-multiline-lang-py {
  border-left-color: #3572A5;
}

.${cssPrefix}-multiline-lang-rust,
.${cssPrefix}-multiline-lang-rs {
  border-left-color: #dea584;
}

/* Syntax characters with data attributes */
[data-syntax]::before {
  content: attr(data-syntax);
  opacity: 0.3;
  font-size: 0.8em;
}

[data-syntax-before]::before {
  content: attr(data-syntax-before);
  opacity: 0.3;
  font-size: 0.8em;
}

[data-syntax-after]::after {
  content: attr(data-syntax-after);
  opacity: 0.3;
  font-size: 0.8em;
}
`;
  }
}