/**
 * Markdown Pattern Detector Tests
 * 
 * Comprehensive test suite covering all pattern detection functionality,
 * edge cases, performance requirements, and integration scenarios.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { MarkdownPatternDetector } from './markdownPatternDetector';
import { PatternTestUtils, mockPatternData } from './markdownPatternUtils';
import type { MarkdownPattern, PatternDetectionOptions } from '$lib/types/markdownPatterns';

describe('MarkdownPatternDetector', () => {
  let detector: MarkdownPatternDetector;

  beforeEach(() => {
    detector = new MarkdownPatternDetector();
  });

  describe('Header Detection', () => {
    it('should detect all header levels (1-6)', () => {
      const content = `# Header 1
## Header 2
### Header 3
#### Header 4
##### Header 5
###### Header 6`;
      
      const result = detector.detectPatterns(content);
      const headers = result.patterns.filter(p => p.type === 'header');
      
      expect(headers).toHaveLength(6);
      expect(headers[0]).toMatchObject({
        type: 'header',
        level: 1,
        content: 'Header 1',
        syntax: '#'
      });
      expect(headers[5]).toMatchObject({
        type: 'header', 
        level: 6,
        content: 'Header 6',
        syntax: '######'
      });
    });

    it('should require space after hash symbols', () => {
      const content = `#NoSpace
# With Space`;
      
      const result = detector.detectPatterns(content);
      const headers = result.patterns.filter(p => p.type === 'header');
      
      expect(headers).toHaveLength(1);
      expect(headers[0].content).toBe('With Space');
    });

    it('should respect maxHeaderLevel option', () => {
      const content = `# H1
## H2  
### H3
#### H4`;
      
      const options: PatternDetectionOptions = { maxHeaderLevel: 2 };
      const result = detector.detectPatterns(content, options);
      const headers = result.patterns.filter(p => p.type === 'header');
      
      expect(headers).toHaveLength(2);
      expect(headers.every(h => h.level! <= 2)).toBe(true);
    });

    it('should handle empty headers', () => {
      const content = '# \n## ';
      
      const result = detector.detectPatterns(content);
      const headers = result.patterns.filter(p => p.type === 'header');
      
      expect(headers).toHaveLength(0); // Empty headers should not be detected
    });

    it('should calculate correct positions for headers', () => {
      const content = 'Some text\n# Header\nMore text';
      
      const result = detector.detectPatterns(content);
      const header = result.patterns.find(p => p.type === 'header');
      
      expect(header).toMatchObject({
        start: 10, // After 'Some text\n'
        end: 18,   // Before '\nMore text'
        line: 1,
        column: 0
      });
    });
  });

  describe('Bullet List Detection', () => {
    it('should detect all bullet types (-, *, +)', () => {
      const content = `- Dash bullet
* Star bullet  
+ Plus bullet`;
      
      const result = detector.detectPatterns(content);
      const bullets = result.patterns.filter(p => p.type === 'bullet');
      
      expect(bullets).toHaveLength(3);
      expect(bullets[0].bulletType).toBe('-');
      expect(bullets[1].bulletType).toBe('*');
      expect(bullets[2].bulletType).toBe('+');
    });

    it('should handle indented bullets', () => {
      const content = `- Level 1
  - Level 2
    * Level 3`;
      
      const result = detector.detectPatterns(content);
      const bullets = result.patterns.filter(p => p.type === 'bullet');
      
      expect(bullets).toHaveLength(3);
      expect(bullets[1].syntax).toBe('  - ');
      expect(bullets[2].syntax).toBe('    * ');
    });

    it('should require content after bullet', () => {
      const content = `- Item with content
-   
- Another item`;
      
      const result = detector.detectPatterns(content);
      const bullets = result.patterns.filter(p => p.type === 'bullet');
      
      expect(bullets).toHaveLength(2); // Empty bullet should not match
      expect(bullets[0].content).toBe('Item with content');
      expect(bullets[1].content).toBe('Another item');
    });

    it('should handle multi-line bullet content', () => {
      const content = `- First line of bullet
  continued on second line
- Second bullet`;
      
      const result = detector.detectPatterns(content);
      const bullets = result.patterns.filter(p => p.type === 'bullet');
      
      expect(bullets).toHaveLength(2);
      expect(bullets[0].content).toBe('First line of bullet');
    });
  });

  describe('Blockquote Detection', () => {
    it('should detect simple blockquotes', () => {
      const content = `> This is a quote
> Another line`;
      
      const result = detector.detectPatterns(content);
      const quotes = result.patterns.filter(p => p.type === 'blockquote');
      
      expect(quotes).toHaveLength(2);
      expect(quotes[0].content).toBe('This is a quote');
      expect(quotes[1].content).toBe('Another line');
    });

    it('should detect nested blockquotes', () => {
      const content = `> Level 1 quote
>> Level 2 quote
>>> Level 3 quote`;
      
      const result = detector.detectPatterns(content);
      const quotes = result.patterns.filter(p => p.type === 'blockquote');
      
      expect(quotes).toHaveLength(3);
      expect(quotes[0].syntax).toBe('> ');
      expect(quotes[1].syntax).toBe('>> ');
      expect(quotes[2].syntax).toBe('>>> ');
    });

    it('should handle empty blockquotes', () => {
      const content = `> Quote with content
>
> Another quote`;
      
      const result = detector.detectPatterns(content);
      const quotes = result.patterns.filter(p => p.type === 'blockquote');
      
      expect(quotes).toHaveLength(3);
      expect(quotes[1].content).toBe(''); // Empty quote should be detected
    });

    it('should handle indented blockquotes', () => {
      const content = `  > Indented quote
    > More indented`;
      
      const result = detector.detectPatterns(content);
      const quotes = result.patterns.filter(p => p.type === 'blockquote');
      
      expect(quotes).toHaveLength(2);
      expect(quotes[0].column).toBe(2);
      expect(quotes[1].column).toBe(4);
    });
  });

  describe('Code Block Detection', () => {
    it('should detect fenced code blocks', () => {
      const content = `\`\`\`
function test() {
  return true;
}
\`\`\``;
      
      const result = detector.detectPatterns(content);
      const codeBlocks = result.patterns.filter(p => p.type === 'codeblock');
      
      expect(codeBlocks).toHaveLength(1);
      expect(codeBlocks[0].content).toBe('function test() {\n  return true;\n}');
      expect(codeBlocks[0].syntax).toBe('```');
    });

    it('should detect language-specified code blocks', () => {
      const content = `\`\`\`javascript
console.log('hello');
\`\`\`

\`\`\`python
print("world")
\`\`\``;
      
      const result = detector.detectPatterns(content);
      const codeBlocks = result.patterns.filter(p => p.type === 'codeblock');
      
      expect(codeBlocks).toHaveLength(2);
      expect(codeBlocks[0].language).toBe('javascript');
      expect(codeBlocks[1].language).toBe('python');
    });

    it('should handle empty code blocks', () => {
      const content = `\`\`\`

\`\`\``;
      
      const result = detector.detectPatterns(content);
      const codeBlocks = result.patterns.filter(p => p.type === 'codeblock');
      
      expect(codeBlocks).toHaveLength(1);
      expect(codeBlocks[0].content).toBe(''); // Empty content for empty code block
    });

    it('should preserve whitespace in code blocks', () => {
      const content = `\`\`\`
  indented code
    more indented
\`\`\``;
      
      const result = detector.detectPatterns(content);
      const codeBlocks = result.patterns.filter(p => p.type === 'codeblock');
      
      expect(codeBlocks[0].content).toBe('  indented code\n    more indented');
    });
  });

  describe('Bold Text Detection', () => {
    it('should detect **bold** syntax', () => {
      const content = 'This is **bold text** in a sentence.';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      expect(bold).toHaveLength(1);
      expect(bold[0]).toMatchObject({
        type: 'bold',
        content: 'bold text',
        syntax: '**'
      });
    });

    it('should detect __bold__ syntax', () => {
      const content = 'This is __bold text__ in a sentence.';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      expect(bold).toHaveLength(1);
      expect(bold[0]).toMatchObject({
        type: 'bold',
        content: 'bold text',
        syntax: '__'
      });
    });

    it('should handle multiple bold patterns', () => {
      const content = '**First bold** and **second bold** and __third bold__.';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      expect(bold).toHaveLength(3);
      expect(bold[0].content).toBe('First bold');
      expect(bold[1].content).toBe('second bold');
      expect(bold[2].content).toBe('third bold');
    });

    it('should not detect incomplete bold markers', () => {
      const content = 'This **has incomplete bold\nAnd this **is also incomplete';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      // Should not match across newlines 
      expect(bold).toHaveLength(0);
    });

    it('should handle bold in different contexts', () => {
      const content = `# Header with **bold**
- Bullet with **bold content**
> Quote with **bold quote**`;
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      expect(bold).toHaveLength(3);
    });
  });

  describe('Italic Text Detection', () => {
    it('should detect *italic* syntax', () => {
      const content = 'This is *italic text* in a sentence.';
      
      const result = detector.detectPatterns(content);
      const italic = result.patterns.filter(p => p.type === 'italic');
      
      expect(italic).toHaveLength(1);
      expect(italic[0]).toMatchObject({
        type: 'italic',
        content: 'italic text',
        syntax: '*'
      });
    });

    it('should detect _italic_ syntax', () => {
      const content = 'This is _italic text_ in a sentence.';
      
      const result = detector.detectPatterns(content);
      const italic = result.patterns.filter(p => p.type === 'italic');
      
      expect(italic).toHaveLength(1);
      expect(italic[0]).toMatchObject({
        type: 'italic',
        content: 'italic text',
        syntax: '_'
      });
    });

    it('should not conflict with bold **text**', () => {
      const content = 'This has **bold** and *italic* text.';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      const italic = result.patterns.filter(p => p.type === 'italic');
      
      expect(bold).toHaveLength(1);
      expect(italic).toHaveLength(1);
      expect(bold[0].content).toBe('bold');
      expect(italic[0].content).toBe('italic');
    });

    it('should not conflict with bold __text__', () => {
      const content = 'This has __bold__ and _italic_ text.';
      
      const result = detector.detectPatterns(content);
      const bold = result.patterns.filter(p => p.type === 'bold');
      const italic = result.patterns.filter(p => p.type === 'italic');
      
      expect(bold).toHaveLength(1);
      expect(italic).toHaveLength(1);
      expect(bold[0].content).toBe('bold');
      expect(italic[0].content).toBe('italic');
    });

    it('should handle nested formatting', () => {
      const content = '**bold with *italic* inside** and *italic with **bold** inside*';
      
      const result = detector.detectPatterns(content);
      const patterns = result.patterns;
      
      // Should detect outer patterns (inner patterns would be more complex)
      const bold = patterns.filter(p => p.type === 'bold');
      const italic = patterns.filter(p => p.type === 'italic');
      
      expect(bold.length).toBeGreaterThan(0);
      expect(italic.length).toBeGreaterThan(0);
    });
  });

  describe('Inline Code Detection', () => {
    it('should detect `inline code` syntax', () => {
      const content = 'This is `inline code` in a sentence.';
      
      const result = detector.detectPatterns(content);
      const inlineCode = result.patterns.filter(p => p.type === 'inlinecode');
      
      expect(inlineCode).toHaveLength(1);
      expect(inlineCode[0]).toMatchObject({
        type: 'inlinecode',
        content: 'inline code',
        syntax: '`'
      });
    });

    it('should handle multiple inline code patterns', () => {
      const content = 'Use `console.log()` and `alert()` functions.';
      
      const result = detector.detectPatterns(content);
      const inlineCode = result.patterns.filter(p => p.type === 'inlinecode');
      
      expect(inlineCode).toHaveLength(2);
      expect(inlineCode[0].content).toBe('console.log()');
      expect(inlineCode[1].content).toBe('alert()');
    });

    it('should not detect incomplete code markers', () => {
      const content = 'This `has incomplete code';
      
      const result = detector.detectPatterns(content);
      const inlineCode = result.patterns.filter(p => p.type === 'inlinecode');
      
      expect(inlineCode).toHaveLength(0);
    });

    it('should not interfere with code blocks', () => {
      const content = `\`\`\`javascript
const code = 'block';
\`\`\`

And \`inline code\`.`;
      
      const result = detector.detectPatterns(content);
      const inlineCode = result.patterns.filter(p => p.type === 'inlinecode');
      const codeBlocks = result.patterns.filter(p => p.type === 'codeblock');
      
      expect(codeBlocks).toHaveLength(1);
      expect(inlineCode).toHaveLength(1);
      expect(inlineCode[0].content).toBe('inline code');
    });
  });

  describe('Mixed Patterns', () => {
    it('should handle complex mixed content', () => {
      const result = detector.detectPatterns(mockPatternData.sampleContent);
      
      expect(result.patterns.length).toBeGreaterThan(10);
      
      const patternTypes = result.patterns.map(p => p.type);
      expect(patternTypes).toContain('header');
      expect(patternTypes).toContain('bold');
      expect(patternTypes).toContain('italic');
      expect(patternTypes).toContain('inlinecode');
      expect(patternTypes).toContain('bullet');
      expect(patternTypes).toContain('blockquote');
      expect(patternTypes).toContain('codeblock');
    });

    it('should maintain correct pattern order', () => {
      const content = '**Bold** then *italic* then `code`.';
      
      const result = detector.detectPatterns(content);
      
      expect(result.patterns).toHaveLength(3);
      expect(result.patterns[0].type).toBe('bold');
      expect(result.patterns[1].type).toBe('italic');
      expect(result.patterns[2].type).toBe('inlinecode');
      expect(result.patterns[0].start).toBeLessThan(result.patterns[1].start);
      expect(result.patterns[1].start).toBeLessThan(result.patterns[2].start);
    });

    it('should validate pattern integrity', () => {
      const result = detector.detectPatterns(mockPatternData.sampleContent);
      const validation = PatternTestUtils.validatePatterns(result.patterns);
      
      if (!validation.isValid) {
        console.log('Pattern validation errors:', validation.errors);
        console.log('Patterns causing issues:', result.patterns.slice(0, 5));
      }
      
      expect(validation.isValid).toBe(true);
      expect(validation.errors).toHaveLength(0);
    });
  });

  describe('Performance', () => {
    it('should detect patterns in large content within time limit', () => {
      const largeContent = PatternTestUtils.generatePerformanceTestContent(1000);
      
      const startTime = performance.now();
      const result = detector.detectPatterns(largeContent);
      const endTime = performance.now();
      
      expect(endTime - startTime).toBeLessThan(50); // Under 50ms requirement
      expect(result.patterns.length).toBeGreaterThan(0);
      expect(result.warnings.length).toBe(0);
    });

    it('should handle empty content efficiently', () => {
      const startTime = performance.now();
      const result = detector.detectPatterns('');
      const endTime = performance.now();
      
      expect(endTime - startTime).toBeLessThan(1);
      expect(result.patterns).toHaveLength(0);
    });

    it('should provide accurate performance metrics', () => {
      detector.detectPatterns(mockPatternData.sampleContent);
      const metrics = detector.getMetrics();
      
      expect(metrics.totalTime).toBeGreaterThan(0);
      expect(metrics.blockDetectionTime).toBeGreaterThan(0);
      expect(metrics.inlineDetectionTime).toBeGreaterThan(0);
      expect(metrics.contentLength).toBe(mockPatternData.sampleContent.length);
      expect(metrics.regexOperations).toBeGreaterThan(0);
      expect(metrics.patternsPerMs).toBeGreaterThan(0);
    });

    it('should warn when performance degrades', () => {
      // Create content that might cause performance issues
      const heavyContent = 'a'.repeat(100000) + '**bold**'.repeat(1000);
      
      const result = detector.detectPatterns(heavyContent);
      
      if (result.detectionTime > 50) {
        expect(result.warnings.some(w => w.includes('exceeding 50ms'))).toBe(true);
      }
    });
  });

  describe('Real-time Detection', () => {
    it('should detect patterns with cursor position', () => {
      const content = 'This **bold** text';
      const cursorPos = 10; // Inside bold text
      
      const result = detector.detectPatternsRealtime(content, cursorPos);
      
      expect(result.patterns).toHaveLength(1);
      expect(result.patterns[0].type).toBe('bold');
    });

    it('should emit pattern events', () => {
      const mockCallback = vi.fn();
      const unsubscribe = detector.subscribe(mockCallback);
      
      detector.detectPatterns('**bold text**');
      
      expect(mockCallback).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'patterns_detected',
          patterns: expect.any(Array)
        })
      );
      
      unsubscribe();
    });

    it('should handle event subscription cleanup', () => {
      const mockCallback = vi.fn();
      const unsubscribe = detector.subscribe(mockCallback);
      
      unsubscribe();
      detector.detectPatterns('**bold text**');
      
      expect(mockCallback).not.toHaveBeenCalled();
    });
  });

  describe('Pattern Utilities', () => {
    it('should get pattern at position', () => {
      const content = 'Some **bold** text';
      const pattern = detector.getPatternAt(content, 7); // Inside bold
      
      expect(pattern).toBeDefined();
      expect(pattern?.type).toBe('bold');
      expect(pattern?.content).toBe('bold');
    });

    it('should get patterns by type', () => {
      const content = '**bold** and *italic* and **more bold**';
      const boldPatterns = detector.getPatternsByType(content, 'bold');
      
      expect(boldPatterns).toHaveLength(2);
      expect(boldPatterns.every(p => p.type === 'bold')).toBe(true);
    });

    it('should extract pattern content', () => {
      const content = '**first** and **second** and **third**';
      const result = detector.detectPatterns(content);
      const boldPatterns = result.patterns.filter(p => p.type === 'bold');
      const extractedContent = detector.extractPatternContent(boldPatterns);
      
      expect(extractedContent).toEqual(['first', 'second', 'third']);
    });

    it('should replace patterns in content', () => {
      const content = '**bold** text';
      const result = detector.detectPatterns(content);
      const boldPattern = result.patterns.find(p => p.type === 'bold')!;
      
      const replacements = [{
        pattern: boldPattern,
        replacement: 'REPLACED'
      }];
      
      const newContent = detector.replacePatterns(content, replacements);
      expect(newContent).toBe('REPLACED text');
    });

    it('should validate patterns', () => {
      const validPattern = PatternTestUtils.createTestPattern('bold', 0, 8, 'bold');
      const validation = detector.validatePattern(validPattern);
      
      expect(validation.isValid).toBe(true);
      expect(validation.errors).toHaveLength(0);
    });
  });

  describe('Configuration Options', () => {
    it('should respect detection options', () => {
      const content = '# Header\n**bold** *italic* `code`';
      const options: PatternDetectionOptions = {
        detectHeaders: false,
        detectBold: false
      };
      
      const result = detector.detectPatterns(content, options);
      const patternTypes = result.patterns.map(p => p.type);
      
      expect(patternTypes).not.toContain('header');
      expect(patternTypes).not.toContain('bold');
      expect(patternTypes).toContain('italic');
      expect(patternTypes).toContain('inlinecode');
    });

    it('should handle performance mode', () => {
      const content = mockPatternData.sampleContent;
      const options: PatternDetectionOptions = { performanceMode: true };
      
      const result = detector.detectPatterns(content, options);
      
      expect(result.patterns.length).toBeGreaterThan(0);
      expect(result.warnings.length).toBe(0); // Should skip expensive validations
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty content', () => {
      const result = detector.detectPatterns('');
      
      expect(result.patterns).toHaveLength(0);
      expect(result.detectionTime).toBeGreaterThan(0);
      expect(result.linesProcessed).toBe(1);
      expect(result.contentLength).toBe(0);
    });

    it('should handle content with only whitespace', () => {
      const result = detector.detectPatterns('   \n  \n   ');
      
      expect(result.patterns).toHaveLength(0);
      expect(result.linesProcessed).toBe(3);
    });

    it('should handle malformed markdown gracefully', () => {
      const content = '###***```**__`_';
      
      expect(() => {
        const result = detector.detectPatterns(content);
        expect(result.patterns).toBeDefined();
      }).not.toThrow();
    });

    it('should handle very long lines', () => {
      const longLine = '**' + 'a'.repeat(10000) + '**';
      
      const result = detector.detectPatterns(longLine);
      const bold = result.patterns.filter(p => p.type === 'bold');
      
      expect(bold).toHaveLength(1);
      expect(bold[0].content.length).toBe(10000);
    });

    it('should handle unicode content', () => {
      const content = '**粗体文字** and *斜体文字* and `代码`';
      
      const result = detector.detectPatterns(content);
      
      expect(result.patterns).toHaveLength(3);
      expect(result.patterns[0].content).toBe('粗体文字');
      expect(result.patterns[1].content).toBe('斜体文字');
      expect(result.patterns[2].content).toBe('代码');
    });
  });

  describe('Mock Data Validation', () => {
    it('should validate mock pattern data consistency', () => {
      const result = detector.detectPatterns(mockPatternData.sampleContent);
      
      expect(result.patterns.length).toBeGreaterThanOrEqual(mockPatternData.expectedPatterns.length);
      
      // Check that expected patterns are found
      for (const expectedPattern of mockPatternData.expectedPatterns) {
        const found = result.patterns.find(p => 
          p.type === expectedPattern.type && 
          p.content === expectedPattern.content
        );
        expect(found).toBeDefined();
      }
    });

    it('should validate cursor scenarios', () => {
      for (const scenario of mockPatternData.cursorScenarios) {
        const pattern = detector.getPatternAt(mockPatternData.sampleContent, scenario.position);
        
        if (scenario.expectedPattern) {
          expect(pattern).toBeDefined();
          expect(pattern?.type).toBe(scenario.expectedPattern.type);
        } else {
          expect(pattern).toBeNull();
        }
      }
    });

    it('should meet performance scenario requirements', () => {
      for (const scenario of mockPatternData.performanceScenarios) {
        const startTime = performance.now();
        const result = detector.detectPatterns(scenario.content);
        const endTime = performance.now();
        
        expect(endTime - startTime).toBeLessThan(scenario.expectedMaxTime);
        expect(result.patterns).toBeDefined();
      }
    });
  });
});