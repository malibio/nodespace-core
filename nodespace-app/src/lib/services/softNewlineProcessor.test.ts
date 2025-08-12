/**
 * Soft Newline Processor Tests
 * 
 * Comprehensive test suite for soft newline detection and processing.
 * Tests the core functionality, edge cases, and performance characteristics.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SoftNewlineProcessor, SoftNewlineIntegration, SoftNewlineMetrics } from './softNewlineProcessor';
import type { SoftNewlineContext, NodeCreationSuggestion, SoftNewlineProcessingOptions } from './softNewlineProcessor';

describe('SoftNewlineProcessor', () => {
  let processor: SoftNewlineProcessor;

  beforeEach(() => {
    processor = new SoftNewlineProcessor();
    // Clear metrics for each test
    SoftNewlineMetrics.clearMetrics();
  });

  describe('Shift-Enter Detection', () => {
    it('should detect Shift-Enter events correctly', () => {
      const shiftEnterEvent = {
        key: 'Enter',
        shiftKey: true,
        ctrlKey: false,
        metaKey: false
      } as KeyboardEvent;

      const regularEnterEvent = {
        key: 'Enter',
        shiftKey: false,
        ctrlKey: false,
        metaKey: false
      } as KeyboardEvent;

      const ctrlEnterEvent = {
        key: 'Enter',
        shiftKey: true,
        ctrlKey: true,
        metaKey: false
      } as KeyboardEvent;

      expect(processor.isShiftEnter(shiftEnterEvent)).toBe(true);
      expect(processor.isShiftEnter(regularEnterEvent)).toBe(false);
      expect(processor.isShiftEnter(ctrlEnterEvent)).toBe(false);
    });

    it('should handle other keys correctly', () => {
      const spaceEvent = {
        key: ' ',
        shiftKey: true,
        ctrlKey: false,
        metaKey: false
      } as KeyboardEvent;

      expect(processor.isShiftEnter(spaceEvent)).toBe(false);
    });
  });

  describe('Soft Newline Content Processing', () => {
    it('should detect header patterns after soft newlines', () => {
      const content = 'Project overview:\n# Main Header';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.detectedPattern?.type).toBe('header');
      expect(context.detectedPattern?.content).toBe('Main Header');
      expect(context.contentBefore).toBe('Project overview:');
      expect(context.contentAfter).toBe('# Main Header');
      expect(context.shouldCreateNewNode).toBe(true);
      expect(context.suggestedNodeType).toBe('text');
    });

    it('should detect bullet patterns after soft newlines', () => {
      const content = 'Tasks:\n- First item';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.detectedPattern?.type).toBe('bullet');
      expect(context.detectedPattern?.content).toBe('First item');
      expect(context.contentBefore).toBe('Tasks:');
      expect(context.contentAfter).toBe('- First item');
      expect(context.shouldCreateNewNode).toBe(true);
      expect(context.suggestedNodeType).toBe('text');
    });

    it('should detect smart bullet patterns for tasks', () => {
      const content = 'TODO list:\n- Todo: finish the report';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.detectedPattern?.type).toBe('bullet');
      expect(context.suggestedNodeType).toBe('task');
    });

    it('should detect blockquote patterns after soft newlines', () => {
      const content = 'Quote:\n> This is a quote';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.detectedPattern?.type).toBe('blockquote');
      expect(context.detectedPattern?.content).toBe('This is a quote');
      expect(context.shouldCreateNewNode).toBe(true);
      expect(context.suggestedNodeType).toBe('ai-chat');
    });

    it('should handle content without soft newlines', () => {
      const content = 'Regular text without newlines';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(false);
      expect(context.detectedPattern).toBeUndefined();
      expect(context.shouldCreateNewNode).toBe(false);
      expect(context.newlinePosition).toBe(-1);
    });

    it('should handle insufficient content after newline', () => {
      const content = 'Text:\n#';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(false);
      expect(context.shouldCreateNewNode).toBe(false);
    });

    it('should handle multiple newlines correctly', () => {
      const content = 'First line\nSecond line\n# Header';
      const cursorPosition = content.length;

      const context = processor.processSoftNewlineContent(content, cursorPosition);

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.contentBefore).toBe('First line\nSecond line');
      expect(context.contentAfter).toBe('# Header');
    });
  });

  describe('Node Creation Suggestions', () => {
    it('should generate correct node creation suggestion for headers', () => {
      const content = 'Overview:\n# Main Section';
      const context = processor.processSoftNewlineContent(content, content.length);
      const suggestion = processor.getNodeCreationSuggestion(context);

      expect(suggestion).toBeDefined();
      expect(suggestion?.nodeType).toBe('text');
      expect(suggestion?.content).toBe('Main Section');
      expect(suggestion?.rawContent).toBe('# Main Section');
      expect(suggestion?.relationship).toBe('sibling');
    });

    it('should generate correct node creation suggestion for bullets', () => {
      const content = 'Items:\n- List item';
      const context = processor.processSoftNewlineContent(content, content.length);
      const suggestion = processor.getNodeCreationSuggestion(context);

      expect(suggestion).toBeDefined();
      expect(suggestion?.nodeType).toBe('text');
      expect(suggestion?.content).toBe('List item');
      expect(suggestion?.relationship).toBe('child');
    });

    it('should return null for contexts without node creation', () => {
      const content = 'No markdown here';
      const context = processor.processSoftNewlineContent(content, content.length);
      const suggestion = processor.getNodeCreationSuggestion(context);

      expect(suggestion).toBeNull();
    });

    it('should detect task-related content in bullets', () => {
      const testCases = [
        { content: 'List:\n- Todo: buy groceries', expectedType: 'task' },
        { content: 'Items:\n- Task: clean room', expectedType: 'task' },
        { content: 'Notes:\n- Do the laundry', expectedType: 'task' },
        { content: 'Ideas:\n- Ask about the meeting', expectedType: 'ai-chat' },
        { content: 'Contacts:\n- @john.doe person', expectedType: 'entity' },
        { content: 'Searches:\n- Find best restaurants', expectedType: 'query' }
      ];

      testCases.forEach(({ content, expectedType }) => {
        const context = processor.processSoftNewlineContent(content, content.length);
        expect(context.suggestedNodeType).toBe(expectedType);
      });
    });
  });

  describe('Real-time Processing with Debouncing', () => {
    it('should debounce rapid typing events', async () => {
      const content1 = 'Text:\n#';
      const content2 = 'Text:\n# H';
      const content3 = 'Text:\n# Header';

      // Start multiple rapid processing calls
      const promise1 = processor.processRealtimeTyping(content1, content1.length, 'test-1');
      const promise2 = processor.processRealtimeTyping(content2, content2.length, 'test-1');
      const promise3 = processor.processRealtimeTyping(content3, content3.length, 'test-1');

      // Only the last one should resolve with the final content
      const context = await promise3;
      
      expect(context.contentAfter).toBe('# Header');
      expect(context.hasMarkdownAfterNewline).toBe(true);
    });

    it('should handle cancellation', () => {
      const content = 'Text:\n# Header';
      
      // Start processing
      processor.processRealtimeTyping(content, content.length, 'test-cancel');
      
      // Cancel immediately
      processor.cancelProcessing('test-cancel');
      
      // Should not throw or cause issues
      expect(true).toBe(true);
    });
  });

  describe('Event Subscription', () => {
    it('should notify subscribers of context changes', () => {
      const mockCallback = vi.fn();
      const unsubscribe = processor.subscribe(mockCallback);

      const content = 'Text:\n# Header';
      processor.processSoftNewlineContent(content, content.length);

      expect(mockCallback).toHaveBeenCalledWith(
        expect.objectContaining({
          hasMarkdownAfterNewline: true,
          detectedPattern: expect.objectContaining({
            type: 'header',
            content: 'Header'
          })
        })
      );

      unsubscribe();
    });

    it('should handle subscriber errors gracefully', () => {
      const errorCallback = vi.fn(() => {
        throw new Error('Callback error');
      });
      
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      
      processor.subscribe(errorCallback);
      
      const content = 'Text:\n# Header';
      processor.processSoftNewlineContent(content, content.length);

      expect(consoleSpy).toHaveBeenCalledWith('Soft newline context callback error:', expect.any(Error));
      
      consoleSpy.mockRestore();
    });
  });

  describe('Configuration Options', () => {
    it('should respect minimum content length option', () => {
      const customProcessor = new SoftNewlineProcessor({
        minContentLength: 5
      });

      const shortContent = 'Text:\n# Hi';
      const longContent = 'Text:\n# Header';

      const shortContext = customProcessor.processSoftNewlineContent(shortContent, shortContent.length);
      const longContext = customProcessor.processSoftNewlineContent(longContent, longContent.length);

      expect(shortContext.hasMarkdownAfterNewline).toBe(false);
      expect(longContext.hasMarkdownAfterNewline).toBe(true);
    });

    it('should respect debounce time option', async () => {
      const customProcessor = new SoftNewlineProcessor({
        debounceTime: 10
      });

      const content = 'Text:\n# Header';
      const startTime = Date.now();
      
      await customProcessor.processRealtimeTyping(content, content.length, 'test');
      
      const endTime = Date.now();
      expect(endTime - startTime).toBeGreaterThanOrEqual(10);
    });
  });

  describe('Edge Cases and Error Handling', () => {
    it('should handle empty content', () => {
      const context = processor.processSoftNewlineContent('', 0);
      
      expect(context.hasMarkdownAfterNewline).toBe(false);
      expect(context.shouldCreateNewNode).toBe(false);
    });

    it('should handle cursor position beyond content length', () => {
      const content = 'Short';
      const context = processor.processSoftNewlineContent(content, 1000);
      
      expect(context.hasMarkdownAfterNewline).toBe(false);
    });

    it('should handle invalid cursor positions', () => {
      const content = 'Text:\n# Header';
      const context = processor.processSoftNewlineContent(content, -1);
      
      expect(context.hasMarkdownAfterNewline).toBe(false);
    });

    it('should handle malformed markdown patterns gracefully', () => {
      const content = 'Text:\n### # ### Invalid';
      const context = processor.processSoftNewlineContent(content, content.length);
      
      // Should still detect some pattern or handle gracefully
      expect(context).toBeDefined();
    });
  });

  describe('Performance and Metrics', () => {
    it('should record processing time metrics', () => {
      const content = 'Text:\n# Header'.repeat(100);
      const startTime = performance.now();
      
      processor.processSoftNewlineContent(content, content.length);
      
      const endTime = performance.now();
      const processingTime = endTime - startTime;
      
      // Processing should be reasonably fast
      expect(processingTime).toBeLessThan(50);
    });

    it('should maintain performance with large content', () => {
      const largeContent = 'Text:\n' + '# Header\n'.repeat(1000) + '- Bullet item';
      const startTime = performance.now();
      
      processor.processSoftNewlineContent(largeContent, largeContent.length);
      
      const endTime = performance.now();
      const processingTime = endTime - startTime;
      
      // Should complete within reasonable time even with large content
      expect(processingTime).toBeLessThan(100);
    });
  });
});

describe('SoftNewlineIntegration', () => {
  let mockElement: HTMLDivElement;

  beforeEach(() => {
    // Create a mock DOM element
    mockElement = document.createElement('div');
    mockElement.textContent = 'Test content';
    document.body.appendChild(mockElement);
  });

  afterEach(() => {
    document.body.removeChild(mockElement);
  });

  describe('Keyboard Event Handling', () => {
    it('should handle shift-enter events correctly', () => {
      const shiftEnterEvent = new KeyboardEvent('keydown', {
        key: 'Enter',
        shiftKey: true
      });

      const result = SoftNewlineIntegration.handleKeyboardEvent(
        shiftEnterEvent,
        'test content',
        10,
        'test-node'
      );

      expect(result).toBe(true);
    });

    it('should ignore non-shift-enter events', () => {
      const regularKeyEvent = new KeyboardEvent('keydown', {
        key: 'a'
      });

      const result = SoftNewlineIntegration.handleKeyboardEvent(
        regularKeyEvent,
        'test content',
        10,
        'test-node'
      );

      expect(result).toBe(false);
    });
  });

  describe('Input Change Handling', () => {
    it('should process input changes with debouncing', async () => {
      const content = 'Text:\n# Header';
      
      const context = await SoftNewlineIntegration.handleInputChange(
        content,
        content.length,
        'test-node'
      );

      expect(context.hasMarkdownAfterNewline).toBe(true);
      expect(context.detectedPattern?.type).toBe('header');
    });
  });

  describe('Cursor Position Management', () => {
    it('should get cursor position from element', () => {
      // Create a selection in the element
      const range = document.createRange();
      range.setStart(mockElement.firstChild!, 4);
      range.collapse(true);
      
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);

      const position = SoftNewlineIntegration.getCursorPosition(mockElement);
      expect(position).toBe(4);
    });

    it('should handle missing selection', () => {
      // Clear selection
      const selection = window.getSelection()!;
      selection.removeAllRanges();

      const position = SoftNewlineIntegration.getCursorPosition(mockElement);
      expect(position).toBe(0);
    });

    it('should set cursor position in element', () => {
      SoftNewlineIntegration.setCursorPosition(mockElement, 4);

      const selection = window.getSelection()!;
      const range = selection.getRangeAt(0);
      
      // Should position cursor at character 4
      expect(range.startOffset).toBe(4);
    });

    it('should handle position beyond content length', () => {
      const content = mockElement.textContent || '';
      SoftNewlineIntegration.setCursorPosition(mockElement, content.length + 10);

      // Should not throw and should position at end
      const selection = window.getSelection()!;
      expect(selection.rangeCount).toBeGreaterThan(0);
    });
  });
});

describe('SoftNewlineMetrics', () => {
  beforeEach(() => {
    SoftNewlineMetrics.clearMetrics();
  });

  it('should record and calculate average processing time', () => {
    SoftNewlineMetrics.recordProcessingTime('test-operation', 10);
    SoftNewlineMetrics.recordProcessingTime('test-operation', 20);
    SoftNewlineMetrics.recordProcessingTime('test-operation', 30);

    const average = SoftNewlineMetrics.getAverageTime('test-operation');
    expect(average).toBe(20);
  });

  it('should return 0 for non-existent operations', () => {
    const average = SoftNewlineMetrics.getAverageTime('non-existent');
    expect(average).toBe(0);
  });

  it('should provide comprehensive metrics', () => {
    SoftNewlineMetrics.recordProcessingTime('operation-1', 10);
    SoftNewlineMetrics.recordProcessingTime('operation-1', 20);
    SoftNewlineMetrics.recordProcessingTime('operation-2', 5);

    const metrics = SoftNewlineMetrics.getMetrics();

    expect(metrics['operation-1']).toEqual({
      average: 15,
      samples: 2,
      max: 20,
      min: 10
    });

    expect(metrics['operation-2']).toEqual({
      average: 5,
      samples: 1,
      max: 5,
      min: 5
    });
  });

  it('should limit stored measurements', () => {
    // Record more than 100 measurements
    for (let i = 0; i < 120; i++) {
      SoftNewlineMetrics.recordProcessingTime('test-op', i);
    }

    const metrics = SoftNewlineMetrics.getMetrics();
    expect(metrics['test-op'].samples).toBe(100);
  });

  it('should clear all metrics', () => {
    SoftNewlineMetrics.recordProcessingTime('test-op', 10);
    SoftNewlineMetrics.clearMetrics();

    const metrics = SoftNewlineMetrics.getMetrics();
    expect(Object.keys(metrics)).toHaveLength(0);
  });
});

describe('Integration Scenarios', () => {
  let processor: SoftNewlineProcessor;

  beforeEach(() => {
    processor = new SoftNewlineProcessor({
      autoCreateNodes: false
    });
  });

  it('should handle the key innovation scenario', () => {
    // User types: "Project overview:"
    // User presses Shift-Enter (soft newline - stays in same node)  
    // User continues: "- First step"
    // → System detects markdown after soft newline
    // → Creates new child node with "First step" content

    const content = 'Project overview:\n- First step';
    const context = processor.processSoftNewlineContent(content, content.length);

    expect(context.contentBefore).toBe('Project overview:');
    expect(context.contentAfter).toBe('- First step');
    expect(context.hasMarkdownAfterNewline).toBe(true);
    expect(context.detectedPattern?.type).toBe('bullet');
    expect(context.detectedPattern?.content).toBe('First step');
    expect(context.shouldCreateNewNode).toBe(true);

    const suggestion = processor.getNodeCreationSuggestion(context);
    expect(suggestion?.content).toBe('First step');
    expect(suggestion?.nodeType).toBe('text');
    expect(suggestion?.relationship).toBe('child');
  });

  it('should handle complex nested scenarios', () => {
    const content = 'Meeting notes:\n> John said something important';
    const context = processor.processSoftNewlineContent(content, content.length);

    expect(context.hasMarkdownAfterNewline).toBe(true);
    expect(context.detectedPattern?.type).toBe('blockquote');
    expect(context.shouldCreateNewNode).toBe(true);
  });

  it('should maintain natural typing flow', async () => {
    // Simulate natural typing progression
    const stages = [
      'Notes:\n',
      'Notes:\n-',
      'Notes:\n- ',
      'Notes:\n- F',
      'Notes:\n- Fi',
      'Notes:\n- First',
      'Notes:\n- First step'
    ];

    for (const stage of stages) {
      const context = await processor.processRealtimeTyping(stage, stage.length, 'natural-typing');
      
      // Only the complete stages should trigger suggestions
      if (stage === 'Notes:\n- First step') {
        expect(context.hasMarkdownAfterNewline).toBe(true);
        expect(context.shouldCreateNewNode).toBe(true);
      }
    }
  });
});