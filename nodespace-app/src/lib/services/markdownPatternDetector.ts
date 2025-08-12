/**
 * Markdown Pattern Detector Service
 * 
 * Comprehensive markdown pattern detection system supporting 4 block types
 * and 3 inline types with real-time detection and cursor management.
 */

import type {
  MarkdownPattern,
  MarkdownPatternType,
  PatternDetectionResult,
  PatternDetectionOptions,
  PatternDetectionEvent,
  CursorPosition,
  DetectionMetrics,
  PatternValidation,
  PatternReplacement,
  IMarkdownPatternDetector,
  HeaderLevel,
  BulletType
} from '$lib/types/markdownPatterns';

/**
 * Core markdown pattern detector implementation
 */
export class MarkdownPatternDetector implements IMarkdownPatternDetector {
  private subscribers: Array<(event: PatternDetectionEvent) => void> = [];
  private lastDetectionMetrics: DetectionMetrics;
  private lastPatterns: MarkdownPattern[] = [];

  constructor() {
    this.lastDetectionMetrics = this.createEmptyMetrics();
  }

  /**
   * Detect all patterns in content
   */
  detectPatterns(content: string, options: PatternDetectionOptions = {}): PatternDetectionResult {
    const startTime = performance.now();
    const opts = this.mergeDefaultOptions(options);
    const warnings: string[] = [];
    const patterns: MarkdownPattern[] = [];

    try {
      // Detect block patterns first (they establish document structure)
      const blockDetectionStart = performance.now();
      
      if (opts.detectHeaders) {
        patterns.push(...this.detectHeaders(content, opts));
      }
      
      if (opts.detectBullets) {
        patterns.push(...this.detectBullets(content, opts));
      }
      
      if (opts.detectBlockquotes) {
        patterns.push(...this.detectBlockquotes(content, opts));
      }
      
      if (opts.detectCodeBlocks) {
        patterns.push(...this.detectCodeBlocks(content, opts));
      }

      const blockDetectionEnd = performance.now();

      // Detect inline patterns within non-code-block content
      const inlineDetectionStart = performance.now();
      
      if (opts.detectBold || opts.detectItalic || opts.detectInlineCode) {
        patterns.push(...this.detectInlinePatterns(content, patterns, opts));
      }

      const inlineDetectionEnd = performance.now();
      const totalTime = performance.now() - startTime;

      // Sort patterns by position
      patterns.sort((a, b) => a.start - b.start);

      // Update metrics
      this.lastDetectionMetrics = {
        blockDetectionTime: blockDetectionEnd - blockDetectionStart,
        inlineDetectionTime: inlineDetectionEnd - inlineDetectionStart,
        totalTime,
        regexOperations: this.countRegexOperations(opts),
        contentLength: content.length,
        patternsPerMs: patterns.length / (totalTime || 1)
      };

      // Check for performance warnings
      if (totalTime > 50) {
        warnings.push(`Detection took ${totalTime.toFixed(2)}ms, exceeding 50ms target`);
      }

      const result: PatternDetectionResult = {
        patterns,
        detectionTime: totalTime,
        linesProcessed: content.split('\n').length,
        contentLength: content.length,
        warnings
      };

      // Emit detection event
      this.emitPatternEvent('patterns_detected', patterns, content);
      this.lastPatterns = patterns;

      return result;
      
    } catch (error) {
      warnings.push(`Detection error: ${error instanceof Error ? error.message : 'Unknown error'}`);
      
      return {
        patterns: [],
        detectionTime: performance.now() - startTime,
        linesProcessed: 0,
        contentLength: content.length,
        warnings
      };
    }
  }

  /**
   * Real-time detection with cursor information
   */
  detectPatternsRealtime(content: string, cursorPosition: number, options: PatternDetectionOptions = {}): PatternDetectionResult {
    const result = this.detectPatterns(content, options);
    
    // Add cursor-specific information
    const cursorInfo = this.getCursorPosition(content, cursorPosition, result.patterns);
    
    // Emit real-time event with cursor info
    this.emitPatternEvent('patterns_changed', result.patterns, content, cursorInfo);
    
    return result;
  }

  /**
   * Detect header patterns (# ## ### #### ##### ######)
   */
  private detectHeaders(content: string, options: PatternDetectionOptions): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    const lines = content.split('\n');
    let currentPosition = 0;
    const maxLevel = options.maxHeaderLevel || 6;

    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
      
      if (headerMatch && headerMatch[1].length <= maxLevel) {
        const syntax = headerMatch[1];
        const headerContent = headerMatch[2];
        const level = syntax.length as HeaderLevel;
        
        patterns.push({
          type: 'header',
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: headerContent,
          level,
          line: lineIndex,
          column: 0
        });
      }
      
      currentPosition += line.length + 1; // +1 for newline
    }

    return patterns;
  }

  /**
   * Detect bullet list patterns (- * +)
   */
  private detectBullets(content: string, options: PatternDetectionOptions): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    const lines = content.split('\n');
    let currentPosition = 0;

    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const bulletMatch = line.match(/^(\s*)([-*+])\s+(.+)$/);
      
      if (bulletMatch && bulletMatch[3].trim().length > 0) { // Require actual content
        const indent = bulletMatch[1];
        const bulletChar = bulletMatch[2] as BulletType;
        const bulletContent = bulletMatch[3];
        const syntax = `${indent}${bulletChar} `;
        
        patterns.push({
          type: 'bullet',
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: bulletContent,
          bulletType: bulletChar,
          line: lineIndex,
          column: indent.length
        });
      }
      
      currentPosition += line.length + 1;
    }

    return patterns;
  }

  /**
   * Detect blockquote patterns (> text)
   */
  private detectBlockquotes(content: string, options: PatternDetectionOptions): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    const lines = content.split('\n');
    let currentPosition = 0;

    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const quoteMatch = line.match(/^(\s*)(>+)\s*(.*)$/);
      
      if (quoteMatch) {
        const indent = quoteMatch[1];
        const quoteChars = quoteMatch[2];
        const quoteContent = quoteMatch[3];
        const syntax = `${indent}${quoteChars} `;
        
        patterns.push({
          type: 'blockquote',
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: quoteContent,
          line: lineIndex,
          column: indent.length
        });
      }
      
      currentPosition += line.length + 1;
    }

    return patterns;
  }

  /**
   * Detect code block patterns (```code```)
   */
  private detectCodeBlocks(content: string, options: PatternDetectionOptions): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    // Updated regex to handle empty code blocks properly
    const codeBlockRegex = /^```(\w+)?\n?([\s\S]*?)\n?```$/gm;
    
    let match;
    while ((match = codeBlockRegex.exec(content)) !== null) {
      const language = match[1] || '';
      const codeContent = match[2] || ''; // Handle empty content
      const syntax = language ? `\`\`\`${language}` : '```';
      
      // Calculate line and column position
      const beforeMatch = content.substring(0, match.index);
      const lines = beforeMatch.split('\n');
      const line = lines.length - 1;
      const column = lines[lines.length - 1].length;
      
      patterns.push({
        type: 'codeblock',
        start: match.index,
        end: match.index + match[0].length,
        syntax,
        content: codeContent,
        language,
        line,
        column
      });
    }

    return patterns;
  }

  /**
   * Detect inline patterns (bold, italic, inline code)
   */
  private detectInlinePatterns(content: string, blockPatterns: MarkdownPattern[], options: PatternDetectionOptions): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    
    // Get non-code-block content for inline pattern detection
    const processableRanges = this.getProcessableRanges(content, blockPatterns);
    
    for (const range of processableRanges) {
      const rangeContent = content.substring(range.start, range.end);
      const rangeOffset = range.start;
      
      if (options.detectBold) {
        patterns.push(...this.detectBoldInRange(rangeContent, rangeOffset));
      }
      
      if (options.detectItalic) {
        patterns.push(...this.detectItalicInRange(rangeContent, rangeOffset));
      }
      
      if (options.detectInlineCode) {
        patterns.push(...this.detectInlineCodeInRange(rangeContent, rangeOffset));
      }
    }

    return patterns;
  }

  /**
   * Detect bold patterns (**text** and __text__)
   */
  private detectBoldInRange(content: string, offset: number): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    
    // Detect **bold** patterns - improved regex to avoid incomplete patterns
    const doubleStar = /\*\*([^*\n]+)\*\*/g;
    let match;
    while ((match = doubleStar.exec(content)) !== null) {
      // Validate that we have proper opening and closing
      if (match[1] && match[1].trim().length > 0) {
        const position = this.getLineColumn(content, match.index);
        patterns.push({
          type: 'bold',
          start: offset + match.index,
          end: offset + match.index + match[0].length,
          syntax: '**',
          content: match[1],
          line: position.line,
          column: position.column
        });
      }
    }
    
    // Detect __bold__ patterns
    const doubleUnderscore = /__([^_\n]+)__/g;
    while ((match = doubleUnderscore.exec(content)) !== null) {
      if (match[1] && match[1].trim().length > 0) {
        const position = this.getLineColumn(content, match.index);
        patterns.push({
          type: 'bold',
          start: offset + match.index,
          end: offset + match.index + match[0].length,
          syntax: '__',
          content: match[1],
          line: position.line,
          column: position.column
        });
      }
    }

    return patterns;
  }

  /**
   * Detect italic patterns (*text* and _text_)
   */
  private detectItalicInRange(content: string, offset: number): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    
    // Detect *italic* patterns (avoiding **bold** conflict)
    const singleStar = /(?<!\*)\*([^*]+)\*(?!\*)/g;
    let match;
    while ((match = singleStar.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: 'italic',
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: '*',
        content: match[1],
        line: position.line,
        column: position.column
      });
    }
    
    // Detect _italic_ patterns (avoiding __bold__ conflict)
    const singleUnderscore = /(?<!_)_([^_]+)_(?!_)/g;
    while ((match = singleUnderscore.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: 'italic',
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: '_',
        content: match[1],
        line: position.line,
        column: position.column
      });
    }

    return patterns;
  }

  /**
   * Detect inline code patterns (`code`)
   */
  private detectInlineCodeInRange(content: string, offset: number): MarkdownPattern[] {
    const patterns: MarkdownPattern[] = [];
    const inlineCodeRegex = /`([^`]+)`/g;
    
    let match;
    while ((match = inlineCodeRegex.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: 'inlinecode',
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: '`',
        content: match[1],
        line: position.line,
        column: position.column
      });
    }

    return patterns;
  }

  /**
   * Get processable ranges (excluding code blocks)
   */
  private getProcessableRanges(content: string, blockPatterns: MarkdownPattern[]): Array<{ start: number; end: number }> {
    const codeBlocks = blockPatterns.filter(p => p.type === 'codeblock');
    
    if (codeBlocks.length === 0) {
      return [{ start: 0, end: content.length }];
    }

    const ranges: Array<{ start: number; end: number }> = [];
    let currentPos = 0;

    for (const block of codeBlocks) {
      if (currentPos < block.start) {
        ranges.push({ start: currentPos, end: block.start });
      }
      currentPos = block.end;
    }

    if (currentPos < content.length) {
      ranges.push({ start: currentPos, end: content.length });
    }

    return ranges;
  }

  /**
   * Get line and column position for character index
   */
  private getLineColumn(content: string, index: number): { line: number; column: number } {
    const beforeIndex = content.substring(0, index);
    const lines = beforeIndex.split('\n');
    return {
      line: lines.length - 1,
      column: lines[lines.length - 1].length
    };
  }

  /**
   * Get pattern at specific position
   */
  getPatternAt(content: string, position: number): MarkdownPattern | null {
    const result = this.detectPatterns(content);
    return result.patterns.find(p => position >= p.start && position < p.end) || null;
  }

  /**
   * Get patterns by type
   */
  getPatternsByType(content: string, type: MarkdownPatternType): MarkdownPattern[] {
    const result = this.detectPatterns(content);
    return result.patterns.filter(p => p.type === type);
  }

  /**
   * Extract content from patterns
   */
  extractPatternContent(patterns: MarkdownPattern[]): string[] {
    return patterns.map(p => p.content);
  }

  /**
   * Replace patterns in content
   */
  replacePatterns(content: string, replacements: PatternReplacement[]): string {
    let result = content;
    
    // Sort replacements by position (descending) to avoid position shifts
    const sortedReplacements = replacements.sort((a, b) => b.pattern.start - a.pattern.start);
    
    for (const replacement of sortedReplacements) {
      const before = result.substring(0, replacement.pattern.start);
      const after = result.substring(replacement.pattern.end);
      result = before + replacement.replacement + after;
    }

    return result;
  }

  /**
   * Validate pattern syntax
   */
  validatePattern(pattern: MarkdownPattern): PatternValidation {
    const errors: string[] = [];
    const warnings: string[] = [];
    const suggestions: string[] = [];

    // Validate pattern structure
    if (pattern.start >= pattern.end) {
      errors.push('Invalid pattern range: start must be less than end');
    }

    if (!pattern.content && pattern.type !== 'codeblock') {
      warnings.push('Pattern has no content');
      suggestions.push('Consider removing empty pattern');
    }

    // Type-specific validation
    switch (pattern.type) {
      case 'header':
        if (!pattern.level || pattern.level < 1 || pattern.level > 6) {
          errors.push('Header level must be between 1 and 6');
        }
        break;
        
      case 'bullet':
        if (!pattern.bulletType || !['*', '-', '+'].includes(pattern.bulletType)) {
          errors.push('Invalid bullet type, must be *, -, or +');
        }
        break;
        
      case 'codeblock':
        if (pattern.language && !/^[a-zA-Z][a-zA-Z0-9-]*$/.test(pattern.language)) {
          warnings.push('Language specification contains unusual characters');
        }
        break;
    }

    return {
      isValid: errors.length === 0,
      errors,
      warnings,
      suggestions
    };
  }

  /**
   * Get performance metrics
   */
  getMetrics(): DetectionMetrics {
    return { ...this.lastDetectionMetrics };
  }

  /**
   * Subscribe to pattern detection events
   */
  subscribe(callback: (event: PatternDetectionEvent) => void): () => void {
    this.subscribers.push(callback);
    
    return () => {
      const index = this.subscribers.indexOf(callback);
      if (index > -1) {
        this.subscribers.splice(index, 1);
      }
    };
  }

  /**
   * Helper methods
   */
  
  private mergeDefaultOptions(options: PatternDetectionOptions): Required<PatternDetectionOptions> {
    return {
      detectHeaders: true,
      detectBullets: true,
      detectBlockquotes: true,
      detectCodeBlocks: true,
      detectBold: true,
      detectItalic: true,
      detectInlineCode: true,
      maxHeaderLevel: 6,
      includePositions: true,
      performanceMode: false,
      ...options
    };
  }

  private createEmptyMetrics(): DetectionMetrics {
    return {
      blockDetectionTime: 0,
      inlineDetectionTime: 0,
      totalTime: 0,
      regexOperations: 0,
      contentLength: 0,
      patternsPerMs: 0
    };
  }

  private countRegexOperations(options: Required<PatternDetectionOptions>): number {
    let count = 0;
    if (options.detectHeaders) count++;
    if (options.detectBullets) count++;
    if (options.detectBlockquotes) count++;
    if (options.detectCodeBlocks) count++;
    if (options.detectBold) count += 2; // ** and __
    if (options.detectItalic) count += 2; // * and _
    if (options.detectInlineCode) count++;
    return count;
  }

  private getCursorPosition(content: string, position: number, patterns: MarkdownPattern[]): CursorPosition {
    const lineCol = this.getLineColumn(content, position);
    const currentPattern = patterns.find(p => position >= p.start && position < p.end);
    
    return {
      position,
      line: lineCol.line,
      column: lineCol.column,
      atPatternStart: currentPattern?.start === position,
      atPatternEnd: currentPattern?.end === position,
      currentPattern
    };
  }

  private emitPatternEvent(
    type: PatternDetectionEvent['type'], 
    patterns: MarkdownPattern[], 
    content: string,
    cursorPosition?: CursorPosition
  ): void {
    const event: PatternDetectionEvent = {
      type,
      patterns,
      content,
      cursorPosition,
      timestamp: Date.now()
    };

    // Calculate changes for change events
    if (type === 'patterns_changed') {
      const addedPatterns = patterns.filter(p => 
        !this.lastPatterns.some(lp => this.patternsEqual(p, lp))
      );
      const removedPatterns = this.lastPatterns.filter(lp => 
        !patterns.some(p => this.patternsEqual(p, lp))
      );
      
      event.addedPatterns = addedPatterns;
      event.removedPatterns = removedPatterns;
    }

    this.subscribers.forEach(callback => {
      try {
        callback(event);
      } catch (error) {
        console.warn('Pattern detection event callback error:', error);
      }
    });
  }

  private patternsEqual(a: MarkdownPattern, b: MarkdownPattern): boolean {
    return a.type === b.type && 
           a.start === b.start && 
           a.end === b.end && 
           a.content === b.content &&
           a.syntax === b.syntax;
  }
}

/**
 * Default singleton instance for convenience
 */
export const markdownPatternDetector = new MarkdownPatternDetector();