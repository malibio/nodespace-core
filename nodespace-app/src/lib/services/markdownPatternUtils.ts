/**
 * Markdown Pattern Utilities
 * 
 * Integration utilities for other components and services to consume
 * pattern detection results. Provides conversion, formatting, and
 * cursor management utilities.
 */

import type {
  MarkdownPattern,
  PatternIntegrationUtils,
  CursorPosition,
  MockPatternData
} from '$lib/types/markdownPatterns';

/**
 * Pattern integration utilities implementation
 */
export class PatternIntegrationUtilities implements PatternIntegrationUtils {

  /**
   * Convert patterns to CSS classes for WYSIWYG rendering
   */
  toCSSClasses(patterns: MarkdownPattern[]): Record<number, string[]> {
    const classMap: Record<number, string[]> = {};

    for (const pattern of patterns) {
      for (let pos = pattern.start; pos < pattern.end; pos++) {
        if (!classMap[pos]) {
          classMap[pos] = [];
        }

        // Add type-specific CSS classes
        switch (pattern.type) {
          case 'header':
            classMap[pos].push(`markdown-header`, `markdown-header-${pattern.level}`);
            break;
          case 'bold':
            classMap[pos].push('markdown-bold');
            break;
          case 'italic':
            classMap[pos].push('markdown-italic');
            break;
          case 'inlinecode':
            classMap[pos].push('markdown-inline-code');
            break;
          case 'codeblock':
            classMap[pos].push('markdown-code-block');
            if (pattern.language) {
              classMap[pos].push(`markdown-code-${pattern.language}`);
            }
            break;
          case 'bullet':
            classMap[pos].push('markdown-bullet', `markdown-bullet-${pattern.bulletType}`);
            break;
          case 'blockquote':
            classMap[pos].push('markdown-blockquote');
            break;
        }

        // Add syntax highlighting classes for markdown characters
        if (this.isSyntaxCharacter(pos, pattern)) {
          classMap[pos].push('markdown-syntax', 'markdown-syntax-hidden');
        }
      }
    }

    return classMap;
  }

  /**
   * Convert patterns to HTML structure for rendering
   */
  toHTMLStructure(content: string, patterns: MarkdownPattern[]): HTMLElement {
    const container = document.createElement('div');
    container.className = 'markdown-content';

    if (patterns.length === 0) {
      container.textContent = content;
      return container;
    }

    let currentPos = 0;
    const sortedPatterns = [...patterns].sort((a, b) => a.start - b.start);

    for (const pattern of sortedPatterns) {
      // Add text before pattern
      if (currentPos < pattern.start) {
        const textNode = document.createTextNode(content.substring(currentPos, pattern.start));
        container.appendChild(textNode);
      }

      // Create pattern element
      const patternElement = this.createPatternElement(pattern);
      container.appendChild(patternElement);

      currentPos = pattern.end;
    }

    // Add remaining text
    if (currentPos < content.length) {
      const textNode = document.createTextNode(content.substring(currentPos));
      container.appendChild(textNode);
    }

    return container;
  }

  /**
   * Handle cursor positioning around patterns
   */
  adjustCursorForPatterns(content: string, position: number, patterns: MarkdownPattern[]): number {
    const pattern = patterns.find(p => position >= p.start && position < p.end);
    
    if (!pattern) {
      return position;
    }

    // Calculate syntax length
    const syntaxLength = this.getSyntaxLength(pattern);
    
    // If cursor is in syntax area, adjust to content area
    if (position < pattern.start + syntaxLength) {
      return pattern.start + syntaxLength;
    }
    
    // If cursor is in end syntax area, adjust to before end syntax
    if (position > pattern.end - syntaxLength) {
      return pattern.end - syntaxLength;
    }

    return position;
  }

  /**
   * Extract bullet patterns for node conversion
   */
  extractBulletPatterns(patterns: MarkdownPattern[]): MarkdownPattern[] {
    return patterns
      .filter(p => p.type === 'bullet')
      .sort((a, b) => a.start - b.start);
  }

  /**
   * Detect soft newline context for better line handling
   */
  detectSoftNewlineContext(content: string, position: number, patterns: MarkdownPattern[]): boolean {
    const lineStart = content.lastIndexOf('\n', position - 1) + 1;
    const lineEnd = content.indexOf('\n', position);
    const actualLineEnd = lineEnd === -1 ? content.length : lineEnd;
    const currentLine = content.substring(lineStart, actualLineEnd);

    // Check if we're in a pattern context that should use soft newlines
    const currentPattern = patterns.find(p => position >= p.start && position <= p.end);
    
    if (currentPattern) {
      switch (currentPattern.type) {
        case 'codeblock':
        case 'blockquote':
          return true;
        case 'bullet':
          // Soft newline if we're continuing a bullet point
          return position > currentPattern.start + currentPattern.syntax.length;
        default:
          return false;
      }
    }

    // Check if the current line starts with a pattern that suggests continuation
    const trimmedLine = currentLine.trim();
    return trimmedLine.startsWith('>') || 
           trimmedLine.match(/^[\s]*[-*+]\s/) !== null ||
           trimmedLine.startsWith('```');
  }

  /**
   * Private helper methods
   */

  private isSyntaxCharacter(position: number, pattern: MarkdownPattern): boolean {
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

  private getSyntaxLength(pattern: MarkdownPattern): number {
    switch (pattern.type) {
      case 'header':
        return pattern.syntax.length + 1; // # + space
      case 'bold':
        return pattern.syntax.length; // ** or __
      case 'italic':
        return pattern.syntax.length; // * or _
      case 'inlinecode':
        return 1; // `
      case 'bullet':
        return pattern.syntax.length; // - or * or + (with space and indent)
      case 'blockquote':
        return pattern.syntax.length; // > (with space)
      case 'codeblock':
        return 3; // ```
      default:
        return 0;
    }
  }

  private hasClosingSyntax(pattern: MarkdownPattern): boolean {
    return ['bold', 'italic', 'inlinecode', 'codeblock'].includes(pattern.type);
  }

  private createPatternElement(pattern: MarkdownPattern): HTMLElement {
    let element: HTMLElement;

    switch (pattern.type) {
      case 'header':
        element = document.createElement(`h${pattern.level}`);
        element.className = `markdown-header markdown-header-${pattern.level}`;
        break;
        
      case 'bold':
        element = document.createElement('strong');
        element.className = 'markdown-bold';
        break;
        
      case 'italic':
        element = document.createElement('em');
        element.className = 'markdown-italic';
        break;
        
      case 'inlinecode':
        element = document.createElement('code');
        element.className = 'markdown-inline-code';
        break;
        
      case 'codeblock':
        element = document.createElement('pre');
        const codeElement = document.createElement('code');
        if (pattern.language) {
          codeElement.className = `language-${pattern.language}`;
        }
        codeElement.textContent = pattern.content;
        element.appendChild(codeElement);
        element.className = 'markdown-code-block';
        return element;
        
      case 'bullet':
        element = document.createElement('li');
        element.className = `markdown-bullet markdown-bullet-${pattern.bulletType}`;
        break;
        
      case 'blockquote':
        element = document.createElement('blockquote');
        element.className = 'markdown-blockquote';
        break;
        
      default:
        element = document.createElement('span');
        element.className = 'markdown-unknown';
    }

    element.textContent = pattern.content;
    return element;
  }
}

/**
 * Mock pattern data for parallel development by other agents
 */
export const mockPatternData: MockPatternData = {
  sampleContent: `# Main Header
This is a paragraph with **bold text** and *italic text* and \`inline code\`.

## Secondary Header
Here's a bullet list:
- First item with **bold** content
- Second item with \`code\` content  
- Third item with *italic* content

> This is a blockquote with **bold** and *italic* formatting.
> It can span multiple lines.

\`\`\`javascript
function example() {
  console.log("This is code");
  return **not bold in code**;
}
\`\`\`

### Another Header
More content with mixed formatting: **bold *and italic* together**.`,

  expectedPatterns: [
    {
      type: 'header',
      start: 0,
      end: 13,
      syntax: '#',
      content: 'Main Header',
      level: 1,
      line: 0,
      column: 0
    },
    {
      type: 'bold',
      start: 45,
      end: 59,
      syntax: '**',
      content: 'bold text',
      line: 1,
      column: 31
    },
    {
      type: 'italic',
      start: 65,
      end: 78,
      syntax: '*',
      content: 'italic text',
      line: 1,
      column: 51
    },
    {
      type: 'inlinecode',
      start: 84,
      end: 97,
      syntax: '`',
      content: 'inline code',
      line: 1,
      column: 70
    },
    {
      type: 'header',
      start: 100,
      end: 118,
      syntax: '##',
      content: 'Secondary Header',
      level: 2,
      line: 3,
      column: 0
    },
    {
      type: 'bullet',
      start: 143,
      end: 179,
      syntax: '- ',
      content: 'First item with **bold** content',
      bulletType: '-',
      line: 5,
      column: 0
    },
    // ... additional patterns would be listed here
  ],

  cursorScenarios: [
    {
      position: 50,
      expectedPattern: {
        type: 'bold',
        start: 45,
        end: 59,
        syntax: '**',
        content: 'bold text',
        line: 1,
        column: 31
      },
      description: 'Cursor in middle of bold text'
    },
    {
      position: 5,
      expectedPattern: {
        type: 'header',
        start: 0,
        end: 13,
        syntax: '#',
        content: 'Main Header',
        level: 1,
        line: 0,
        column: 0
      },
      description: 'Cursor in header content'
    },
    {
      position: 25,
      expectedPattern: null,
      description: 'Cursor in plain text (no pattern)'
    }
  ],

  performanceScenarios: [
    {
      content: `# Header\n`.repeat(1000) + `**Bold text** `.repeat(1000) + `\`code\` `.repeat(1000),
      description: 'Large document with many patterns',
      expectedMaxTime: 50
    },
    {
      content: `This is plain text with no patterns.\n`.repeat(1000),
      description: 'Large document with no patterns',
      expectedMaxTime: 10
    },
    {
      content: `# ${'Very long header content '.repeat(100)}`,
      description: 'Single pattern with very long content',
      expectedMaxTime: 5
    }
  ]
};

/**
 * Utility functions for integration testing
 */
export class PatternTestUtils {
  
  /**
   * Create a minimal pattern for testing
   */
  static createTestPattern(type: MarkdownPattern['type'], start: number, end: number, content: string): MarkdownPattern {
    const basePattern = {
      type,
      start,
      end,
      content,
      line: 0,
      column: 0
    };

    switch (type) {
      case 'header':
        return { ...basePattern, syntax: '#', level: 1 as const };
      case 'bold':
        return { ...basePattern, syntax: '**' };
      case 'italic':
        return { ...basePattern, syntax: '*' };
      case 'inlinecode':
        return { ...basePattern, syntax: '`' };
      case 'bullet':
        return { ...basePattern, syntax: '- ', bulletType: '-' as const };
      case 'blockquote':
        return { ...basePattern, syntax: '> ' };
      case 'codeblock':
        return { ...basePattern, syntax: '```' };
      default:
        return { ...basePattern, syntax: '' };
    }
  }

  /**
   * Validate pattern detection results for testing
   */
  static validatePatterns(patterns: MarkdownPattern[]): { isValid: boolean; errors: string[] } {
    const errors: string[] = [];

    // Sort patterns by position for overlap checking
    const sortedPatterns = [...patterns].sort((a, b) => a.start - b.start);

    // Check for invalid overlapping patterns (excluding valid inline-within-block cases)
    for (let i = 0; i < sortedPatterns.length - 1; i++) {
      const current = sortedPatterns[i];
      const next = sortedPatterns[i + 1];
      
      if (current.end > next.start) {
        // Allow inline patterns within block patterns
        const isValidOverlap = this.isValidPatternOverlap(current, next);
        if (!isValidOverlap) {
          errors.push(`Invalid overlapping patterns: ${current.type} (${current.start}-${current.end}) and ${next.type} (${next.start}-${next.end})`);
        }
      }
    }

    // Check for invalid ranges
    for (const pattern of patterns) {
      if (pattern.start >= pattern.end) {
        errors.push(`Invalid range for ${pattern.type}: start (${pattern.start}) >= end (${pattern.end})`);
      }
      
      if (pattern.start < 0) {
        errors.push(`Negative start position for ${pattern.type}: ${pattern.start}`);
      }
    }

    return {
      isValid: errors.length === 0,
      errors
    };
  }

  /**
   * Check if pattern overlap is valid (inline within block)
   */
  private static isValidPatternOverlap(outerPattern: MarkdownPattern, innerPattern: MarkdownPattern): boolean {
    const blockTypes = ['header', 'bullet', 'blockquote', 'codeblock'];
    const inlineTypes = ['bold', 'italic', 'inlinecode'];
    
    const isOuterBlock = blockTypes.includes(outerPattern.type);
    const isInnerInline = inlineTypes.includes(innerPattern.type);
    
    // Allow inline patterns within block patterns
    if (isOuterBlock && isInnerInline) {
      return innerPattern.start >= outerPattern.start && innerPattern.end <= outerPattern.end;
    }
    
    // Allow nested block patterns (like nested blockquotes)
    if (outerPattern.type === 'blockquote' && innerPattern.type === 'blockquote') {
      return innerPattern.start >= outerPattern.start && innerPattern.end <= outerPattern.end;
    }
    
    // Code blocks should not allow other patterns inside
    if (outerPattern.type === 'codeblock') {
      return false;
    }
    
    // Other overlaps are not valid
    return false;
  }

  /**
   * Generate performance test content
   */
  static generatePerformanceTestContent(patternCount: number): string {
    const patterns = [
      '# Header',
      '**Bold text**',
      '*Italic text*',
      '`inline code`',
      '- Bullet item',
      '> Blockquote',
      '```\ncode block\n```'
    ];

    let content = '';
    for (let i = 0; i < patternCount; i++) {
      const pattern = patterns[i % patterns.length];
      content += pattern + ' ';
      if ((i + 1) % 10 === 0) {
        content += '\n';
      }
    }

    return content;
  }
}

/**
 * Export singleton instance for convenience
 */
export const patternIntegrationUtils = new PatternIntegrationUtilities();