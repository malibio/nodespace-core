/**
 * Markdown Pattern Integration Tests
 * 
 * Integration tests verifying that pattern detection works correctly
 * with ContentEditable components and provides the APIs needed by
 * other agents (#57, #58, #59, #61).
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { MarkdownPatternDetector } from '$lib/services/markdownPatternDetector';
import { patternIntegrationUtils, mockPatternData } from '$lib/services/markdownPatternUtils';
import type { MarkdownPattern } from '$lib/types/markdownPatterns';

// Mock component for testing integration
const MockContentEditableComponent = `
<script lang="ts">
  import { MarkdownPatternDetector } from '$lib/services/markdownPatternDetector';
  import { createEventDispatcher } from 'svelte';
  
  export let content = '';
  
  const detector = new MarkdownPatternDetector();
  const dispatch = createEventDispatcher();
  
  let detectedPatterns: MarkdownPattern[] = [];
  let contentElement: HTMLElement;
  
  function handleInput(event: Event) {
    const target = event.target as HTMLElement;
    content = target.textContent || '';
    
    // Detect patterns in real-time
    const result = detector.detectPatternsRealtime(content, getCursorPosition());
    detectedPatterns = result.patterns;
    
    // Dispatch patterns for parent components
    dispatch('patternsDetected', { patterns: detectedPatterns, content });
    
    // Test WYSIWYG integration
    applyCSSClasses();
  }
  
  function getCursorPosition(): number {
    const selection = window.getSelection();
    if (!selection?.rangeCount || !contentElement) return 0;
    
    const range = selection.getRangeAt(0);
    return range.startOffset;
  }
  
  function applyCSSClasses() {
    if (!contentElement) return;
    
    // Use integration utilities to apply CSS classes
    const cssClasses = patternIntegrationUtils.toCSSClasses(detectedPatterns);
    
    // Apply classes to character spans (simplified for testing)
    contentElement.classList.toggle('has-patterns', detectedPatterns.length > 0);
  }
</script>

<div
  bind:this={contentElement}
  contenteditable="true"
  on:input={handleInput}
  data-testid="content-editable"
>
  {content}
</div>

<div data-testid="pattern-count">{detectedPatterns.length}</div>
<div data-testid="pattern-types">{detectedPatterns.map(p => p.type).join(',')}</div>
`;

describe('Markdown Pattern Integration', () => {
  let detector: MarkdownPatternDetector;

  beforeEach(() => {
    detector = new MarkdownPatternDetector();
  });

  describe('Real-time Pattern Detection', () => {
    it('should detect patterns as user types', async () => {
      const component = render(MockContentEditableComponent);
      const contentEditable = component.getByTestId('content-editable');
      const patternCount = component.getByTestId('pattern-count');
      
      // Simulate typing a header
      contentEditable.textContent = '# He';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('0'); // Incomplete pattern
      
      // Complete the header
      contentEditable.textContent = '# Header';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('1');
      
      // Add bold text
      contentEditable.textContent = '# Header\n**bold**';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('2');
    });

    it('should handle pattern removal during editing', async () => {
      const component = render(MockContentEditableComponent);
      const contentEditable = component.getByTestId('content-editable');
      const patternCount = component.getByTestId('pattern-count');
      
      // Start with patterns
      contentEditable.textContent = '**bold** text';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('1');
      
      // Remove pattern syntax
      contentEditable.textContent = 'bold text';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('0');
    });

    it('should emit events for other components', async () => {
      const eventHandler = vi.fn();
      const component = render(MockContentEditableComponent);
      
      component.component.$on('patternsDetected', eventHandler);
      
      const contentEditable = component.getByTestId('content-editable');
      contentEditable.textContent = '**bold**';
      await fireEvent.input(contentEditable);
      
      expect(eventHandler).toHaveBeenCalledWith(
        expect.objectContaining({
          detail: expect.objectContaining({
            patterns: expect.any(Array),
            content: '**bold**'
          })
        })
      );
    });
  });

  describe('WYSIWYG Integration APIs', () => {
    it('should provide CSS classes for visual formatting', () => {
      const content = '**bold** and *italic*';
      const result = detector.detectPatterns(content);
      
      const cssClasses = patternIntegrationUtils.toCSSClasses(result.patterns);
      
      expect(cssClasses).toBeDefined();
      expect(Object.keys(cssClasses).length).toBeGreaterThan(0);
      
      // Check that syntax positions have hiding classes
      const syntaxPositions = Object.keys(cssClasses).map(Number)
        .filter(pos => cssClasses[pos].includes('markdown-syntax-hidden'));
      
      expect(syntaxPositions.length).toBeGreaterThan(0);
    });

    it('should convert patterns to HTML structure', () => {
      const content = '# Header\n**bold** text';
      const result = detector.detectPatterns(content);
      
      const htmlElement = patternIntegrationUtils.toHTMLStructure(content, result.patterns);
      
      expect(htmlElement.tagName).toBe('DIV');
      expect(htmlElement.className).toBe('markdown-content');
      expect(htmlElement.children.length).toBeGreaterThan(0);
    });

    it('should adjust cursor positions around patterns', () => {
      const content = '**bold text**';
      const result = detector.detectPatterns(content);
      
      // Cursor at start of syntax should move to content
      const adjustedStart = patternIntegrationUtils.adjustCursorForPatterns(content, 1, result.patterns);
      expect(adjustedStart).toBeGreaterThan(1);
      
      // Cursor at end of syntax should move before end syntax
      const adjustedEnd = patternIntegrationUtils.adjustCursorForPatterns(content, content.length - 1, result.patterns);
      expect(adjustedEnd).toBeLessThan(content.length - 1);
    });
  });

  describe('Bullet-to-Node Conversion APIs', () => {
    it('should extract bullet patterns for Issue #58', () => {
      const content = `- First item
- Second item  
* Different bullet
+ Another type`;
      
      const result = detector.detectPatterns(content);
      const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(result.patterns);
      
      expect(bulletPatterns).toHaveLength(4);
      expect(bulletPatterns.every(p => p.type === 'bullet')).toBe(true);
      expect(bulletPatterns[0].bulletType).toBe('-');
      expect(bulletPatterns[2].bulletType).toBe('*');
      expect(bulletPatterns[3].bulletType).toBe('+');
    });

    it('should maintain bullet order for hierarchical conversion', () => {
      const content = `- Parent item
  - Child item
    * Grandchild item
- Another parent`;
      
      const result = detector.detectPatterns(content);
      const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(result.patterns);
      
      expect(bulletPatterns).toHaveLength(4);
      
      // Check that patterns are in document order
      for (let i = 1; i < bulletPatterns.length; i++) {
        expect(bulletPatterns[i].start).toBeGreaterThan(bulletPatterns[i - 1].start);
      }
    });

    it('should provide indentation information for nested bullets', () => {
      const content = `- Level 1
  - Level 2
    - Level 3`;
      
      const result = detector.detectPatterns(content);
      const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(result.patterns);
      
      expect(bulletPatterns[0].column).toBe(0);   // No indent
      expect(bulletPatterns[1].column).toBe(2);   // 2-space indent
      expect(bulletPatterns[2].column).toBe(4);   // 4-space indent
    });
  });

  describe('Soft Newline Detection for Issue #59', () => {
    it('should detect soft newline contexts', () => {
      const content = `> This is a blockquote
> with continuation`;
      const cursorAfterFirst = content.indexOf('blockquote') + 'blockquote'.length;
      
      const result = detector.detectPatterns(content);
      const isSoftNewlineContext = patternIntegrationUtils.detectSoftNewlineContext(
        content, 
        cursorAfterFirst, 
        result.patterns
      );
      
      expect(isSoftNewlineContext).toBe(true);
    });

    it('should detect continuation in bullet points', () => {
      const content = `- First bullet point with some content
- Second bullet`;
      const cursorInFirst = content.indexOf('content') + 'content'.length;
      
      const result = detector.detectPatterns(content);
      const isSoftNewlineContext = patternIntegrationUtils.detectSoftNewlineContext(
        content,
        cursorInFirst,
        result.patterns
      );
      
      expect(isSoftNewlineContext).toBe(true);
    });

    it('should detect code block continuation', () => {
      const content = `\`\`\`javascript
function example() {
  return true;
}
\`\`\``;
      const cursorInCode = content.indexOf('function') + 5;
      
      const result = detector.detectPatterns(content);
      const isSoftNewlineContext = patternIntegrationUtils.detectSoftNewlineContext(
        content,
        cursorInCode,
        result.patterns
      );
      
      expect(isSoftNewlineContext).toBe(true);
    });

    it('should not detect soft newline in plain text', () => {
      const content = 'This is plain text without patterns.';
      const cursor = 10;
      
      const result = detector.detectPatterns(content);
      const isSoftNewlineContext = patternIntegrationUtils.detectSoftNewlineContext(
        content,
        cursor,
        result.patterns
      );
      
      expect(isSoftNewlineContext).toBe(false);
    });
  });

  describe('Import/Export Integration for Issue #61', () => {
    it('should extract content for AI processing', () => {
      const result = detector.detectPatterns(mockPatternData.sampleContent);
      const extractedContent = detector.extractPatternContent(result.patterns);
      
      expect(extractedContent).toBeDefined();
      expect(extractedContent.length).toBeGreaterThan(0);
      expect(extractedContent.every(content => typeof content === 'string')).toBe(true);
    });

    it('should provide pattern metadata for AI context', () => {
      const result = detector.detectPatterns(mockPatternData.sampleContent);
      
      // Patterns should include all metadata needed for AI processing
      for (const pattern of result.patterns) {
        expect(pattern.type).toBeDefined();
        expect(pattern.start).toBeGreaterThanOrEqual(0);
        expect(pattern.end).toBeGreaterThan(pattern.start);
        expect(pattern.content).toBeDefined();
        expect(pattern.syntax).toBeDefined();
        expect(pattern.line).toBeGreaterThanOrEqual(0);
        expect(pattern.column).toBeGreaterThanOrEqual(0);
      }
    });

    it('should support pattern replacement for AI edits', () => {
      const content = 'Original **bold** text';
      const result = detector.detectPatterns(content);
      const boldPattern = result.patterns.find(p => p.type === 'bold')!;
      
      const replacements = [{
        pattern: boldPattern,
        replacement: '**AI-generated content**'
      }];
      
      const newContent = detector.replacePatterns(content, replacements);
      expect(newContent).toBe('Original **AI-generated content** text');
    });
  });

  describe('Performance Under Load', () => {
    it('should handle real-time typing performance', async () => {
      const component = render(MockContentEditableComponent);
      const contentEditable = component.getByTestId('content-editable');
      
      let content = '';
      const startTime = performance.now();
      
      // Simulate rapid typing
      for (let i = 0; i < 50; i++) {
        content += i % 5 === 0 ? '**bold** ' : 'text ';
        contentEditable.textContent = content;
        await fireEvent.input(contentEditable);
      }
      
      const endTime = performance.now();
      const totalTime = endTime - startTime;
      
      // Should handle 50 input events efficiently
      expect(totalTime).toBeLessThan(500); // 10ms per input event on average
      
      const patternCount = component.getByTestId('pattern-count');
      expect(Number(patternCount.textContent)).toBeGreaterThan(0);
    });

    it('should handle large documents efficiently', () => {
      const largeContent = mockPatternData.sampleContent.repeat(100);
      
      const startTime = performance.now();
      const result = detector.detectPatterns(largeContent);
      const endTime = performance.now();
      
      expect(endTime - startTime).toBeLessThan(100); // Should be well under 50ms per detection
      expect(result.patterns.length).toBeGreaterThan(100);
    });
  });

  describe('Error Handling and Edge Cases', () => {
    it('should handle malformed input gracefully', async () => {
      const component = render(MockContentEditableComponent);
      const contentEditable = component.getByTestId('content-editable');
      
      // Test various malformed inputs
      const malformedInputs = [
        '**unclosed bold',
        '`unclosed code',
        '######## too many hashes',
        '**nested **bold** patterns**',
        '\0null\0characters\0',
      ];
      
      for (const input of malformedInputs) {
        expect(() => {
          contentEditable.textContent = input;
          fireEvent.input(contentEditable);
        }).not.toThrow();
      }
    });

    it('should handle cursor position edge cases', () => {
      const content = '**bold**';
      const result = detector.detectPatterns(content);
      
      // Test cursor at various positions
      const positions = [-1, 0, 1, 4, 7, 8, 100];
      
      for (const pos of positions) {
        expect(() => {
          patternIntegrationUtils.adjustCursorForPatterns(content, pos, result.patterns);
        }).not.toThrow();
      }
    });

    it('should handle empty and whitespace-only content', async () => {
      const component = render(MockContentEditableComponent);
      const contentEditable = component.getByTestId('content-editable');
      const patternCount = component.getByTestId('pattern-count');
      
      // Empty content
      contentEditable.textContent = '';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('0');
      
      // Whitespace only
      contentEditable.textContent = '   \n  \t  ';
      await fireEvent.input(contentEditable);
      expect(patternCount.textContent).toBe('0');
    });
  });

  describe('Event System Integration', () => {
    it('should provide event subscription for other components', () => {
      const events: any[] = [];
      const unsubscribe = detector.subscribe((event) => {
        events.push(event);
      });
      
      detector.detectPatterns('**bold** text');
      detector.detectPatternsRealtime('**bold** and *italic*', 10);
      
      expect(events.length).toBeGreaterThanOrEqual(2);
      expect(events.some(e => e.type === 'patterns_detected')).toBe(true);
      expect(events.some(e => e.type === 'patterns_changed')).toBe(true);
      
      unsubscribe();
    });

    it('should handle multiple subscribers', () => {
      const events1: any[] = [];
      const events2: any[] = [];
      
      const unsubscribe1 = detector.subscribe(e => events1.push(e));
      const unsubscribe2 = detector.subscribe(e => events2.push(e));
      
      detector.detectPatterns('**test**');
      
      expect(events1.length).toBe(events2.length);
      expect(events1.length).toBeGreaterThan(0);
      
      unsubscribe1();
      unsubscribe2();
    });

    it('should clean up subscriptions properly', () => {
      const events: any[] = [];
      const unsubscribe = detector.subscribe(e => events.push(e));
      
      detector.detectPatterns('test1');
      const countAfterFirst = events.length;
      
      unsubscribe();
      detector.detectPatterns('test2');
      
      expect(events.length).toBe(countAfterFirst); // No new events after unsubscribe
    });
  });
});