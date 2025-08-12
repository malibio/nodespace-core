/**
 * Multi-line Block Processor Tests
 * 
 * Comprehensive test suite for multi-line block behavior including
 * blockquotes, code blocks, continuation patterns, and termination.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { 
  MultilineBlockProcessor,
  multilineBlockProcessor,
  MultilineBlockIntegration,
  MultilineBlockMetrics,
  type MultilineBlock,
  type BlockContinuationContext,
  type MultilineBlockOptions 
} from '$lib/services/multilineBlockProcessor';

describe('MultilineBlockProcessor', () => {
  let processor: MultilineBlockProcessor;

  beforeEach(() => {
    processor = new MultilineBlockProcessor();
    MultilineBlockMetrics.clearMetrics();
  });

  describe('Multi-line Blockquote Detection', () => {
    test('detects consecutive blockquote lines as single block', () => {
      const content = `> First line of quote
> Second line of quote
> Third line of quote`;

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].type).toBe('blockquote');
      expect(blocks[0].lineNumbers).toEqual([0, 1, 2]);
      expect(blocks[0].combinedContent).toBe('First line of quote\nSecond line of quote\nThird line of quote');
    });

    test('handles indented blockquotes', () => {
      const content = `  > Indented quote line 1
  > Indented quote line 2`;

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].indentLevel).toBe(1); // 2 spaces = level 1
    });

    test('splits blockquotes separated by empty lines', () => {
      const content = `> First block line 1
> First block line 2

> Second block line 1
> Second block line 2`;

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(2);
      expect(blocks[0].lineNumbers).toEqual([0, 1]);
      expect(blocks[1].lineNumbers).toEqual([3, 4]);
    });

    test('ignores single-line blockquotes', () => {
      const content = `> Single line quote`;

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(0); // Should not create multi-line block for single line
    });
  });

  describe('Multi-line Code Block Detection', () => {
    test('detects fenced code blocks', () => {
      const content = '```javascript\nconsole.log("hello");\nconst x = 42;\n```';

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].type).toBe('codeblock');
      expect(blocks[0].language).toBe('javascript');
      expect(blocks[0].combinedContent).toBe('console.log("hello");\nconst x = 42;');
    });

    test('detects code blocks without language specification', () => {
      const content = '```\nsome code here\nmore code\n```';

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].type).toBe('codeblock');
      expect(blocks[0].language).toBeUndefined();
    });

    test('handles incomplete code blocks', () => {
      const content = '```javascript\nconsole.log("incomplete");';

      const blocks = processor.detectMultilineBlocks(content);
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].incomplete).toBe(true);
    });
  });

  describe('Block Continuation Context', () => {
    test('provides continuation context for blockquotes', () => {
      const content = `> Line 1
> Line 2
> Line 3`;
      const cursorPosition = content.length; // End of content

      const context = processor.getBlockContinuationContext(content, cursorPosition);
      
      expect(context.inBlock).toBe(true);
      expect(context.currentBlock?.type).toBe('blockquote');
      expect(context.expectedContinuation).toBe('> ');
      expect(context.shouldContinue).toBe(true);
    });

    test('provides continuation context for indented blockquotes', () => {
      const content = `  > Indented line 1
  > Indented line 2`;
      const cursorPosition = content.length;

      const context = processor.getBlockContinuationContext(content, cursorPosition);
      
      expect(context.inBlock).toBe(true);
      expect(context.expectedContinuation).toBe('  > '); // Maintains indentation
    });

    test('provides no continuation for code blocks', () => {
      const content = '```javascript\nconsole.log("test");';
      const cursorPosition = content.length;

      const context = processor.getBlockContinuationContext(content, cursorPosition);
      
      expect(context.inBlock).toBe(true);
      expect(context.currentBlock?.type).toBe('codeblock');
      expect(context.expectedContinuation).toBe('  '); // Just indentation
    });

    test('detects cursor outside blocks', () => {
      const content = `> Quote block
> More quote

Regular text here`;
      const cursorPosition = content.lastIndexOf('Regular');

      const context = processor.getBlockContinuationContext(content, cursorPosition);
      
      expect(context.inBlock).toBe(false);
      expect(context.shouldContinue).toBe(false);
    });
  });

  describe('Enter Key Handling', () => {
    test('handles Enter in blockquote with continuation', () => {
      const content = `> First line
> Second line`;
      const cursorPosition = content.length;

      const result = processor.handleEnterInBlock(content, cursorPosition);
      
      expect(result.shouldPreventDefault).toBe(true);
      expect(result.newContent).toBe(`> First line
> Second line
> `);
      expect(result.continuation).toBe('> ');
    });

    test('handles Enter in indented blockquote', () => {
      const content = `  > Indented line`;
      const cursorPosition = content.length;

      const result = processor.handleEnterInBlock(content, cursorPosition);
      
      expect(result.shouldPreventDefault).toBe(true);
      expect(result.continuation).toBe('  > ');
    });

    test('handles Enter in code block', () => {
      const content = '```javascript\nconsole.log("test");';
      const cursorPosition = content.length;

      const result = processor.handleEnterInBlock(content, cursorPosition);
      
      expect(result.shouldPreventDefault).toBe(true);
      expect(result.continuation).toBe('  '); // Indentation only
    });

    test('does not handle Enter outside blocks', () => {
      const content = 'Regular text';
      const cursorPosition = content.length;

      const result = processor.handleEnterInBlock(content, cursorPosition);
      
      expect(result.shouldPreventDefault).toBe(false);
    });
  });

  describe('Block Termination', () => {
    test('detects empty line termination for blockquotes', () => {
      const content = `> Quote line 1
> Quote line 2

`;
      const cursorPosition = content.length - 1; // At empty line
      const blocks = processor.detectMultilineBlocks(content, cursorPosition);
      const block = blocks[0];

      const termination = processor.checkBlockTermination(block, content, cursorPosition);
      
      expect(termination.shouldTerminate).toBe(true);
      expect(termination.reason).toBe('empty_line');
    });

    test('detects explicit code block termination', () => {
      const content = `\`\`\`javascript
console.log("test");
\`\`\``;
      const cursorPosition = content.length;
      const blocks = processor.detectMultilineBlocks(content, cursorPosition);
      const block = blocks[0];

      const termination = processor.checkBlockTermination(block, content, cursorPosition);
      
      expect(termination.shouldTerminate).toBe(true);
      expect(termination.reason).toBe('explicit_end');
    });

    test('detects different pattern termination', () => {
      const content = `> Quote line 1
> Quote line 2
- Bullet point here`;
      const cursorPosition = content.lastIndexOf('- Bullet');
      const blocks = processor.detectMultilineBlocks(content, cursorPosition);
      const block = blocks.find(b => b.type === 'blockquote');

      if (block) {
        const termination = processor.checkBlockTermination(block, content, cursorPosition);
        
        expect(termination.shouldTerminate).toBe(true);
        expect(termination.reason).toBe('different_pattern');
      }
    });
  });

  describe('Real-time Processing', () => {
    test('processes blocks with debouncing', async () => {
      const content = `> Line 1
> Line 2
> Line 3`;
      const cursorPosition = content.length;

      const promise = processor.processRealtimeTyping(content, cursorPosition);
      const blocks = await promise;
      
      expect(blocks).toHaveLength(1);
      expect(blocks[0].type).toBe('blockquote');
    });

    test('cancels previous processing when new input arrives', () => {
      const content1 = `> Line 1`;
      const content2 = `> Line 1\n> Line 2`;

      // Start first processing
      processor.processRealtimeTyping(content1, content1.length);
      
      // Start second processing (should cancel first)
      const promise = processor.processRealtimeTyping(content2, content2.length);
      
      expect(promise).toBeInstanceOf(Promise);
    });
  });

  describe('Performance Optimization', () => {
    test('handles large blocks efficiently', () => {
      const largeContent = Array(1000).fill('> Large quote line').join('\n');
      const startTime = performance.now();
      
      const blocks = processor.detectMultilineBlocks(largeContent);
      const endTime = performance.now();
      
      expect(blocks).toHaveLength(1);
      expect(endTime - startTime).toBeLessThan(100); // Should complete in under 100ms
    });

    test('provides optimized cursor positioning for large blocks', () => {
      const largeContent = Array(500).fill('> Line').join('\n');
      const blocks = processor.detectMultilineBlocks(largeContent);
      const block = blocks[0];
      
      const targetPosition = Math.floor(largeContent.length / 2);
      const optimizedPosition = processor.getOptimizedCursorPosition(block, targetPosition, largeContent);
      
      expect(optimizedPosition).toBeGreaterThanOrEqual(block.start);
      expect(optimizedPosition).toBeLessThanOrEqual(block.end);
    });
  });

  describe('Integration Features', () => {
    test('subscribes to block changes', () => {
      let receivedBlocks: MultilineBlock[] = [];
      
      const unsubscribe = processor.subscribe((blocks) => {
        receivedBlocks = blocks;
      });

      const content = `> Line 1
> Line 2`;
      processor.detectMultilineBlocks(content);

      expect(receivedBlocks).toHaveLength(1);
      expect(receivedBlocks[0].type).toBe('blockquote');
      
      unsubscribe();
    });

    test('gets last detected blocks', () => {
      const content = `> Quote
> More quote

\`\`\`js
code here
\`\`\``;
      
      processor.detectMultilineBlocks(content);
      const lastBlocks = processor.getLastBlocks();
      
      expect(lastBlocks).toHaveLength(2);
      expect(lastBlocks[0].type).toBe('blockquote');
      expect(lastBlocks[1].type).toBe('codeblock');
    });
  });
});

describe('MultilineBlockIntegration', () => {
  test('handles keyboard events for Enter in blocks', () => {
    const content = `> Quote line`;
    const cursorPosition = content.length;

    // Create mock keyboard event
    const mockEvent = {
      key: 'Enter',
      shiftKey: false,
      preventDefault: vi.fn(),
      target: {
        textContent: content
      }
    } as unknown as KeyboardEvent;

    const handled = MultilineBlockIntegration.handleKeyboardEvent(
      mockEvent, 
      content, 
      cursorPosition
    );

    expect(handled).toBe(true);
    expect(mockEvent.preventDefault).toHaveBeenCalled();
  });

  test('provides block context for WYSIWYG integration', () => {
    const content = `> Quote line 1
> Quote line 2`;
    const cursorPosition = content.length;

    const context = MultilineBlockIntegration.getBlockContext(content, cursorPosition);
    
    expect(context.inMultilineBlock).toBe(true);
    expect(context.blockType).toBe('blockquote');
    expect(context.continuationPattern).toBe('> ');
  });
});

describe('MultilineBlockMetrics', () => {
  test('records and retrieves processing metrics', () => {
    MultilineBlockMetrics.recordBlockProcessingTime('blockquote', 25);
    MultilineBlockMetrics.recordBlockProcessingTime('codeblock', 15);
    MultilineBlockMetrics.recordContinuationTime(5);

    const metrics = MultilineBlockMetrics.getMetrics();
    
    expect(metrics['block_processing_blockquote']).toBeDefined();
    expect(metrics['block_processing_blockquote'].average).toBe(25);
    expect(metrics['block_processing_codeblock']).toBeDefined();
    expect(metrics['continuation_processing']).toBeDefined();
  });

  test('maintains metrics history with limits', () => {
    // Add more than the limit of measurements
    for (let i = 0; i < 60; i++) {
      MultilineBlockMetrics.recordBlockProcessingTime('blockquote', i);
    }

    const metrics = MultilineBlockMetrics.getMetrics();
    
    // Should only keep last 50 measurements
    expect(metrics['block_processing_blockquote'].samples).toBe(50);
  });
});