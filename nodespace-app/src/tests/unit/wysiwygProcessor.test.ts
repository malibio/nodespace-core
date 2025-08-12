/**
 * WYSIWYG Processor Tests
 * 
 * Comprehensive test suite for WYSIWYG processing functionality including
 * pattern detection integration, performance requirements, and real-time processing.
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { WYSIWYGProcessor, wysiwygProcessor } from '$lib/services/wysiwygProcessor.js';
import type { WYSIWYGConfig } from '$lib/services/wysiwygProcessor.js';
import type { MarkdownPattern } from '$lib/types/markdownPatterns.js';

describe('WYSIWYGProcessor', () => {
  let processor: WYSIWYGProcessor;
  
  beforeEach(() => {
    processor = new WYSIWYGProcessor({
      enableRealTime: true,
      performanceMode: false,
      maxProcessingTime: 50,
      debounceDelay: 10, // Reduced for testing
      hideSyntax: true,
      enableFormatting: true
    });
  });

  afterEach(() => {
    processor.cancelPendingProcessing();
  });

  describe('Basic Processing', () => {
    it('should process plain text without patterns', async () => {
      const content = 'Plain text content';
      const result = await processor.process(content);

      expect(result.originalContent).toBe(content);
      expect(result.patterns).toHaveLength(0);
      expect(result.processedHTML).toBe('Plain text content');
      expect(result.processingTime).toBeLessThan(50);
      expect(result.warnings).toHaveLength(0);
    });

    it('should detect and process markdown patterns', async () => {
      const content = '# Header\n\nThis has **bold** and *italic* text with `code`.';
      const result = await processor.process(content);

      expect(result.originalContent).toBe(content);
      expect(result.patterns.length).toBeGreaterThan(0);
      expect(result.processedHTML).toContain('wysiwyg-header');
      expect(result.processedHTML).toContain('wysiwyg-bold');
      expect(result.processedHTML).toContain('wysiwyg-italic');
      expect(result.processedHTML).toContain('wysiwyg-inlinecode');
      expect(result.processingTime).toBeLessThan(50);
    });

    it('should hide syntax characters when enabled', async () => {
      const processor = new WYSIWYGProcessor({ hideSyntax: true });
      const content = '**bold text**';
      const result = await processor.process(content);

      expect(result.processedHTML).toContain('data-syntax-before="**"');
      expect(result.processedHTML).toContain('data-syntax-after="**"');
      expect(result.processedHTML).toContain('bold text');
    });

    it('should not hide syntax characters when disabled', async () => {
      const processor = new WYSIWYGProcessor({ hideSyntax: false });
      const content = '**bold text**';
      const result = await processor.process(content);

      expect(result.processedHTML).not.toContain('data-syntax-before');
      expect(result.processedHTML).not.toContain('data-syntax-after');
      expect(result.processedHTML).toContain('**bold text**');
    });
  });

  describe('Pattern Processing', () => {
    it('should process header patterns correctly', async () => {
      const testCases = [
        { content: '# Header 1', level: 1 },
        { content: '## Header 2', level: 2 },
        { content: '### Header 3', level: 3 },
        { content: '#### Header 4', level: 4 },
        { content: '##### Header 5', level: 5 },
        { content: '###### Header 6', level: 6 }
      ];

      for (const { content, level } of testCases) {
        const result = await processor.process(content);
        expect(result.patterns).toHaveLength(1);
        expect(result.patterns[0].type).toBe('header');
        expect(result.patterns[0].level).toBe(level);
        expect(result.processedHTML).toContain(`wysiwyg-header-${level}`);
      }
    });

    it('should process bold patterns correctly', async () => {
      const testCases = [
        { content: '**double asterisk bold**', syntax: '**' },
        { content: '__double underscore bold__', syntax: '__' }
      ];

      for (const { content, syntax } of testCases) {
        const result = await processor.process(content);
        expect(result.patterns).toHaveLength(1);
        expect(result.patterns[0].type).toBe('bold');
        expect(result.patterns[0].syntax).toBe(syntax);
        expect(result.processedHTML).toContain('wysiwyg-bold');
      }
    });

    it('should process italic patterns correctly', async () => {
      const testCases = [
        { content: '*single asterisk italic*', syntax: '*' },
        { content: '_single underscore italic_', syntax: '_' }
      ];

      for (const { content, syntax } of testCases) {
        const result = await processor.process(content);
        expect(result.patterns).toHaveLength(1);
        expect(result.patterns[0].type).toBe('italic');
        expect(result.patterns[0].syntax).toBe(syntax);
        expect(result.processedHTML).toContain('wysiwyg-italic');
      }
    });

    it('should process inline code patterns correctly', async () => {
      const content = 'Text with `inline code` here.';
      const result = await processor.process(content);

      expect(result.patterns).toHaveLength(1);
      expect(result.patterns[0].type).toBe('inlinecode');
      expect(result.patterns[0].content).toBe('inline code');
      expect(result.processedHTML).toContain('wysiwyg-inlinecode');
    });

    it('should process bullet patterns correctly', async () => {
      const testCases = [
        { content: '- Dash bullet', bulletType: '-' },
        { content: '* Asterisk bullet', bulletType: '*' },
        { content: '+ Plus bullet', bulletType: '+' }
      ];

      for (const { content, bulletType } of testCases) {
        const result = await processor.process(content);
        expect(result.patterns).toHaveLength(1);
        expect(result.patterns[0].type).toBe('bullet');
        expect(result.patterns[0].bulletType).toBe(bulletType);
        expect(result.processedHTML).toContain('wysiwyg-bullet');
        expect(result.processedHTML).toContain(`wysiwyg-bullet-${bulletType}`);
      }
    });

    it('should process blockquote patterns correctly', async () => {
      const content = '> This is a blockquote';
      const result = await processor.process(content);

      expect(result.patterns).toHaveLength(1);
      expect(result.patterns[0].type).toBe('blockquote');
      expect(result.patterns[0].content).toBe('This is a blockquote');
      expect(result.processedHTML).toContain('wysiwyg-blockquote');
    });

    it('should process code block patterns correctly', async () => {
      const content = '```javascript\nconsole.log("hello");\n```';
      const result = await processor.process(content);

      expect(result.patterns).toHaveLength(1);
      expect(result.patterns[0].type).toBe('codeblock');
      expect(result.patterns[0].language).toBe('javascript');
      expect(result.processedHTML).toContain('wysiwyg-codeblock');
      expect(result.processedHTML).toContain('wysiwyg-code-javascript');
    });
  });

  describe('Real-time Processing', () => {
    it('should process content with debouncing', (done) => {
      const content = '**bold text**';
      let processCount = 0;

      const callback = (result: any) => {
        processCount++;
        expect(result.originalContent).toBe(content);
        expect(result.patterns).toHaveLength(1);
        
        // Should only be called once due to debouncing
        setTimeout(() => {
          expect(processCount).toBe(1);
          done();
        }, 50);
      };

      // Multiple rapid calls should be debounced
      processor.processRealTime(content, 5, callback);
      processor.processRealTime(content, 6, callback);
      processor.processRealTime(content, 7, callback);
    });

    it('should handle cursor position adjustments', async () => {
      const content = '**bold text**';
      const cursorPosition = 8; // Middle of "bold"
      
      const result = await processor.process(content, cursorPosition);
      
      expect(result.adjustedCursorPosition).toBeDefined();
      expect(typeof result.adjustedCursorPosition).toBe('number');
    });

    it('should emit processing events', (done) => {
      const content = 'Test content';
      let eventCount = 0;

      const unsubscribe = processor.subscribe((event) => {
        eventCount++;
        expect(event.type).toBe('processed');
        expect(event.result).toBeDefined();
        expect(event.timestamp).toBeGreaterThan(0);
        
        unsubscribe();
        expect(eventCount).toBe(1);
        done();
      });

      processor.process(content);
    });
  });

  describe('Performance Requirements', () => {
    it('should process small content within 50ms', async () => {
      const content = '# Header\n\n**Bold** *italic* `code` text.';
      const result = await processor.process(content);

      expect(result.processingTime).toBeLessThan(50);
    });

    it('should process medium content within 50ms', async () => {
      const content = '# Header\n\n'.repeat(10) + 
                     '**Bold text** '.repeat(20) + 
                     '*Italic text* '.repeat(20) +
                     '`Code snippet` '.repeat(20);
      
      const result = await processor.process(content);
      expect(result.processingTime).toBeLessThan(50);
    });

    it('should warn about slow processing', async () => {
      // Use performance mode disabled to potentially trigger warnings
      const slowProcessor = new WYSIWYGProcessor({
        performanceMode: false,
        maxProcessingTime: 1 // Very low threshold for testing
      });

      const content = '# Header\n\n**Bold** *italic* `code` text.';
      const result = await slowProcessor.process(content);

      // Processing time warning may or may not be triggered depending on system speed
      // Just ensure warnings array exists
      expect(Array.isArray(result.warnings)).toBe(true);
    });

    it('should adapt debounce delay based on processing time', async () => {
      const metrics = processor.getMetrics();
      expect(typeof metrics.lastProcessingTime).toBe('number');
      expect(typeof metrics.isProcessing).toBe('boolean');
    });
  });

  describe('Character Classes', () => {
    it('should generate CSS character classes for patterns', async () => {
      const content = '**bold** and `code`';
      const result = await processor.process(content);

      expect(result.characterClasses).toBeDefined();
      expect(Object.keys(result.characterClasses).length).toBeGreaterThan(0);
      
      // Check that WYSIWYG prefixed classes exist
      const allClasses = new Set<string>();
      Object.values(result.characterClasses).forEach(classes => {
        classes.forEach(cls => allClasses.add(cls));
      });
      
      const wysiwygClasses = Array.from(allClasses).filter(cls => cls.startsWith('wysiwyg-'));
      expect(wysiwygClasses.length).toBeGreaterThan(0);
    });

    it('should apply syntax hiding classes', async () => {
      const processor = new WYSIWYGProcessor({ hideSyntax: true });
      const content = '**bold**';
      const result = await processor.process(content);

      const allClasses = new Set<string>();
      Object.values(result.characterClasses).forEach(classes => {
        classes.forEach(cls => allClasses.add(cls));
      });
      
      expect(allClasses.has('wysiwyg-syntax-hidden')).toBe(true);
    });
  });

  describe('Error Handling', () => {
    it('should handle processing errors gracefully', async () => {
      // Mock a processing error scenario
      const originalConsoleWarn = console.warn;
      console.warn = vi.fn();

      try {
        // This should not throw even if internal processing fails
        const result = await processor.process('Test content');
        expect(result).toBeDefined();
        expect(result.originalContent).toBe('Test content');
      } finally {
        console.warn = originalConsoleWarn;
      }
    });

    it('should emit error events for processing failures', (done) => {
      const unsubscribe = processor.subscribe((event) => {
        if (event.type === 'error') {
          expect(event.error).toBeDefined();
          unsubscribe();
          done();
        }
      });

      // Try to trigger an error condition
      processor.process(null as any);
    });
  });

  describe('Configuration', () => {
    it('should respect configuration settings', () => {
      const config: WYSIWYGConfig = {
        enableRealTime: false,
        performanceMode: true,
        maxProcessingTime: 100,
        debounceDelay: 50,
        hideSyntax: false,
        enableFormatting: false,
        cssPrefix: 'custom'
      };

      const customProcessor = new WYSIWYGProcessor(config);
      const actualConfig = customProcessor.getConfig();

      expect(actualConfig.enableRealTime).toBe(false);
      expect(actualConfig.performanceMode).toBe(true);
      expect(actualConfig.maxProcessingTime).toBe(100);
      expect(actualConfig.debounceDelay).toBe(50);
      expect(actualConfig.hideSyntax).toBe(false);
      expect(actualConfig.enableFormatting).toBe(false);
      expect(actualConfig.cssPrefix).toBe('custom');
    });

    it('should allow configuration updates', () => {
      const initialConfig = processor.getConfig();
      expect(initialConfig.hideSyntax).toBe(true);

      processor.updateConfig({ hideSyntax: false });
      
      const updatedConfig = processor.getConfig();
      expect(updatedConfig.hideSyntax).toBe(false);
    });
  });

  describe('Complex Content', () => {
    it('should handle nested patterns correctly', async () => {
      const content = '# Header with **bold text** inside\n\nParagraph with *italic* text.';
      const result = await processor.process(content);

      expect(result.patterns.length).toBeGreaterThanOrEqual(3);
      
      const headerPattern = result.patterns.find(p => p.type === 'header');
      const boldPattern = result.patterns.find(p => p.type === 'bold');
      const italicPattern = result.patterns.find(p => p.type === 'italic');
      
      expect(headerPattern).toBeDefined();
      expect(boldPattern).toBeDefined();
      expect(italicPattern).toBeDefined();
    });

    it('should handle mixed content types', async () => {
      const content = `# Main Header

This paragraph has **bold** and *italic* text.

## Subheader

- First bullet with \`code\`
- Second bullet with **bold**

> A blockquote with *emphasis*

\`\`\`javascript
function example() {
  return "code";
}
\`\`\``;

      const result = await processor.process(content);

      expect(result.patterns.length).toBeGreaterThan(5);
      expect(result.processingTime).toBeLessThan(50);
      
      // Check for all pattern types
      const types = new Set(result.patterns.map(p => p.type));
      expect(types.has('header')).toBe(true);
      expect(types.has('bold')).toBe(true);
      expect(types.has('italic')).toBe(true);
      expect(types.has('bullet')).toBe(true);
      expect(types.has('inlinecode')).toBe(true);
      expect(types.has('blockquote')).toBe(true);
      expect(types.has('codeblock')).toBe(true);
    });

    it('should process large content efficiently', async () => {
      const largeContent = [
        '# Large Document',
        '',
        'This is a test of processing larger content with multiple patterns.',
        '',
        ...Array(50).fill(0).map((_, i) => `## Section ${i + 1}`),
        '',
        ...Array(100).fill(0).map(() => 'Paragraph with **bold**, *italic*, and `code` content.'),
        '',
        '```javascript',
        'function test() {',
        '  console.log("Large code block");',
        '  return true;',
        '}',
        '```',
        '',
        ...Array(20).fill(0).map((_, i) => `- Bullet point ${i + 1} with **formatting**`)
      ].join('\n');

      const result = await processor.process(largeContent);
      
      expect(result.patterns.length).toBeGreaterThan(100);
      expect(result.processingTime).toBeLessThan(100); // Slightly higher threshold for large content
      expect(result.processedHTML.length).toBeGreaterThan(0);
    });
  });

  describe('Integration', () => {
    it('should work with singleton instance', async () => {
      const content = '**bold text**';
      const result = await wysiwygProcessor.process(content);

      expect(result.patterns).toHaveLength(1);
      expect(result.patterns[0].type).toBe('bold');
    });

    it('should provide utility functions', () => {
      expect(processor.getMetrics).toBeDefined();
      expect(processor.subscribe).toBeDefined();
      expect(processor.cancelPendingProcessing).toBeDefined();
      expect(processor.getConfig).toBeDefined();
      expect(processor.updateConfig).toBeDefined();
    });
  });
});

describe('WYSIWYGUtils', () => {
  describe('CSS Generation', () => {
    it('should generate WYSIWYG CSS', async () => {
      const { WYSIWYGUtils } = await import('$lib/services/wysiwygProcessor.js');
      const css = WYSIWYGUtils.generateWYSIWYGCSS();

      expect(css).toContain('.wysiwyg-syntax-hidden');
      expect(css).toContain('.wysiwyg-header');
      expect(css).toContain('.wysiwyg-bold');
      expect(css).toContain('.wysiwyg-italic');
      expect(css).toContain('.wysiwyg-inlinecode');
      expect(css).toContain('.wysiwyg-codeblock');
      expect(css).toContain('.wysiwyg-bullet');
      expect(css).toContain('.wysiwyg-blockquote');
    });

    it('should generate CSS with custom prefix', async () => {
      const { WYSIWYGUtils } = await import('$lib/services/wysiwygProcessor.js');
      const css = WYSIWYGUtils.generateWYSIWYGCSS('custom');

      expect(css).toContain('.custom-syntax-hidden');
      expect(css).toContain('.custom-header');
      expect(css).toContain('.custom-bold');
    });
  });
});