/**
 * Integration tests for Smart Bullet-to-Node Conversion (Issue #58)
 * 
 * Tests the complete bullet conversion workflow from pattern detection
 * through node creation and hierarchy building.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { render, fireEvent, waitFor, screen } from '@testing-library/svelte';
import { BulletToNodeConverter, BulletProcessingUtils, bulletToNodeConverter } from '$lib/services/bulletToNodeConverter';
import type { MarkdownPattern, BulletConversionConfig } from '$lib/types/markdownPatterns';
import type { TreeNodeData } from '$lib/types/tree';

describe('BulletToNodeConverter Service', () => {
  let converter: BulletToNodeConverter;
  
  beforeEach(() => {
    converter = new BulletToNodeConverter();
  });

  describe('Basic bullet detection and conversion', () => {
    it('should detect simple bullet patterns', () => {
      const content = '- First item\n* Second item\n+ Third item';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 0,
          end: 12,
          syntax: '- ',
          content: 'First item',
          bulletType: '-',
          line: 0,
          column: 0
        },
        {
          type: 'bullet',
          start: 13,
          end: 26,
          syntax: '* ',
          content: 'Second item',
          bulletType: '*',
          line: 1,
          column: 0
        },
        {
          type: 'bullet',
          start: 27,
          end: 39,
          syntax: '+ ',
          content: 'Third item',
          bulletType: '+',
          line: 2,
          column: 0
        }
      ];

      const result = converter.convertBulletsToNodes(content, mockPatterns, 0);

      expect(result.hasConversions).toBe(true);
      expect(result.newNodes).toHaveLength(3);
      expect(result.newNodes[0].content).toBe('First item');
      expect(result.newNodes[1].content).toBe('Second item');
      expect(result.newNodes[2].content).toBe('Third item');
    });

    it('should create proper node hierarchy for nested bullets', () => {
      const content = '- Parent item\n  - Child item\n    * Grandchild item';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 0,
          end: 13,
          syntax: '- ',
          content: 'Parent item',
          bulletType: '-',
          line: 0,
          column: 0
        },
        {
          type: 'bullet',
          start: 14,
          end: 29,
          syntax: '- ',
          content: 'Child item',
          bulletType: '-',
          line: 1,
          column: 2
        },
        {
          type: 'bullet',
          start: 30,
          end: 49,
          syntax: '* ',
          content: 'Grandchild item',
          bulletType: '*',
          line: 2,
          column: 4
        }
      ];

      const result = converter.convertBulletsToNodes(content, mockPatterns, 0);

      expect(result.hasConversions).toBe(true);
      expect(result.newNodes).toHaveLength(1); // Only root nodes in top level
      
      const rootNode = result.newNodes[0];
      expect(rootNode.content).toBe('Parent item');
      expect(rootNode.depth).toBe(0);
      expect(rootNode.hasChildren).toBe(true);
      expect(rootNode.children).toHaveLength(1);
      
      const childNode = rootNode.children[0];
      expect(childNode.content).toBe('Child item');
      expect(childNode.depth).toBe(1);
      expect(childNode.hasChildren).toBe(true);
      expect(childNode.children).toHaveLength(1);
      
      const grandchildNode = childNode.children[0];
      expect(grandchildNode.content).toBe('Grandchild item');
      expect(grandchildNode.depth).toBe(2);
      expect(grandchildNode.hasChildren).toBe(false);
    });

    it('should remove bullet syntax from cleaned content', () => {
      const content = 'Text before\n- First item\nText between\n* Second item\nText after';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 12,
          end: 24,
          syntax: '- ',
          content: 'First item',
          bulletType: '-',
          line: 1,
          column: 0
        },
        {
          type: 'bullet',
          start: 39,
          end: 52,
          syntax: '* ',
          content: 'Second item',
          bulletType: '*',
          line: 3,
          column: 0
        }
      ];

      const result = converter.convertBulletsToNodes(content, mockPatterns, 0);

      expect(result.cleanedContent).toBe('Text before\nFirst item\nText between\nSecond item\nText after');
    });
  });

  describe('Cursor positioning', () => {
    it('should calculate correct cursor position after bullet removal', () => {
      const content = 'Start - bullet text end';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 6,
          end: 19,
          syntax: '- ',
          content: 'bullet text',
          bulletType: '-',
          line: 0,
          column: 6
        }
      ];
      const originalCursor = 15; // Middle of 'bullet text'

      const result = converter.convertBulletsToNodes(content, mockPatterns, originalCursor);

      // Cursor should be adjusted by the length of removed syntax ('- ')
      expect(result.newCursorPosition).toBe(13); // 15 - 2 = 13
    });

    it('should handle cursor before bullet patterns correctly', () => {
      const content = 'Text - bullet';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 5,
          end: 13,
          syntax: '- ',
          content: 'bullet',
          bulletType: '-',
          line: 0,
          column: 5
        }
      ];
      const originalCursor = 3; // Before the bullet

      const result = converter.convertBulletsToNodes(content, mockPatterns, originalCursor);

      // Cursor position should remain unchanged
      expect(result.newCursorPosition).toBe(3);
    });
  });

  describe('Real-time bullet typing detection', () => {
    it('should detect bullet typing patterns', () => {
      const content = 'Line 1\n- ';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 7,
          end: 9,
          syntax: '- ',
          content: '',
          bulletType: '-',
          line: 1,
          column: 0
        }
      ];

      const shouldConvert = converter.detectBulletTyping(content, mockPatterns, 9);
      expect(shouldConvert).toBe(true);
    });

    it('should not trigger on partial bullet patterns', () => {
      const content = 'Line 1\n-';
      const mockPatterns: MarkdownPattern[] = [];

      const shouldConvert = converter.detectBulletTyping(content, mockPatterns, 8);
      expect(shouldConvert).toBe(false);
    });
  });

  describe('Configuration options', () => {
    it('should respect maxDepth configuration', () => {
      const config = { maxDepth: 2 };
      const converter = new BulletToNodeConverter(config);
      
      const content = '- Level 0\n  - Level 1\n    - Level 2\n      - Level 3 (should be clamped)';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 0,
          end: 9,
          syntax: '- ',
          content: 'Level 0',
          bulletType: '-',
          line: 0,
          column: 0
        },
        {
          type: 'bullet',
          start: 10,
          end: 21,
          syntax: '- ',
          content: 'Level 1',
          bulletType: '-',
          line: 1,
          column: 2
        },
        {
          type: 'bullet',
          start: 22,
          end: 33,
          syntax: '- ',
          content: 'Level 2',
          bulletType: '-',
          line: 2,
          column: 4
        },
        {
          type: 'bullet',
          start: 34,
          end: 67,
          syntax: '- ',
          content: 'Level 3 (should be clamped)',
          bulletType: '-',
          line: 3,
          column: 6
        }
      ];

      const result = converter.convertBulletsToNodes(content, mockPatterns, 0);
      
      // Find the deepest node
      function getMaxDepth(nodes: TreeNodeData[]): number {
        let max = 0;
        for (const node of nodes) {
          max = Math.max(max, node.depth);
          if (node.children.length > 0) {
            max = Math.max(max, getMaxDepth(node.children));
          }
        }
        return max;
      }

      expect(getMaxDepth(result.newNodes)).toBeLessThanOrEqual(2);
    });

    it('should use correct default node type', () => {
      const config = { defaultNodeType: 'task' as const };
      const converter = new BulletToNodeConverter(config);
      
      const content = '- Task item';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 0,
          end: 11,
          syntax: '- ',
          content: 'Task item',
          bulletType: '-',
          line: 0,
          column: 0
        }
      ];

      const result = converter.convertBulletsToNodes(content, mockPatterns, 0);
      
      expect(result.newNodes[0].nodeType).toBe('task');
    });
  });
});

describe('BulletProcessingUtils', () => {
  describe('Utility functions', () => {
    it('should identify bullet patterns near cursor', () => {
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 10,
          end: 20,
          syntax: '- ',
          content: 'item',
          bulletType: '-',
          line: 1,
          column: 0
        }
      ];

      const nearPatterns = BulletProcessingUtils.getBulletPatternsNearCursor(mockPatterns, 15, 10);
      expect(nearPatterns).toHaveLength(1);
      expect(nearPatterns[0].content).toBe('item');

      const farPatterns = BulletProcessingUtils.getBulletPatternsNearCursor(mockPatterns, 50, 10);
      expect(farPatterns).toHaveLength(0);
    });

    it('should preview conversion results', () => {
      const content = '- First\n- Second';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 0,
          end: 7,
          syntax: '- ',
          content: 'First',
          bulletType: '-',
          line: 0,
          column: 0
        },
        {
          type: 'bullet',
          start: 8,
          end: 16,
          syntax: '- ',
          content: 'Second',
          bulletType: '-',
          line: 1,
          column: 0
        }
      ];

      const preview = BulletProcessingUtils.previewConversion(content, mockPatterns);
      
      expect(preview).toHaveLength(2);
      expect(preview[0].content).toBe('First');
      expect(preview[1].content).toBe('Second');
    });

    it('should detect conversion triggers correctly', () => {
      const content = 'Text\n- item';
      const mockPatterns: MarkdownPattern[] = [
        {
          type: 'bullet',
          start: 5,
          end: 11,
          syntax: '- ',
          content: 'item',
          bulletType: '-',
          line: 1,
          column: 0
        }
      ];

      const shouldTrigger = BulletProcessingUtils.shouldTriggerConversion(content, 11, mockPatterns);
      expect(shouldTrigger).toBe(true);
    });
  });
});

describe('Performance and edge cases', () => {
  it('should handle empty content gracefully', () => {
    const result = bulletToNodeConverter.convertBulletsToNodes('', [], 0);
    
    expect(result.hasConversions).toBe(false);
    expect(result.newNodes).toHaveLength(0);
    expect(result.cleanedContent).toBe('');
  });

  it('should handle content without bullets', () => {
    const content = 'Just regular text\nwith no bullets\nat all';
    const result = bulletToNodeConverter.convertBulletsToNodes(content, [], 0);
    
    expect(result.hasConversions).toBe(false);
    expect(result.newNodes).toHaveLength(0);
    expect(result.cleanedContent).toBe(content);
  });

  it('should handle mixed content correctly', () => {
    const content = 'Header text\n- Bullet one\nRegular text\n* Bullet two\nFooter text';
    const mockPatterns: MarkdownPattern[] = [
      {
        type: 'bullet',
        start: 12,
        end: 24,
        syntax: '- ',
        content: 'Bullet one',
        bulletType: '-',
        line: 1,
        column: 0
      },
      {
        type: 'bullet',
        start: 38,
        end: 50,
        syntax: '* ',
        content: 'Bullet two',
        bulletType: '*',
        line: 3,
        column: 0
      }
    ];

    const result = bulletToNodeConverter.convertBulletsToNodes(content, mockPatterns, 0);
    
    expect(result.hasConversions).toBe(true);
    expect(result.newNodes).toHaveLength(2);
    expect(result.cleanedContent).toBe('Header text\nBullet one\nRegular text\nBullet two\nFooter text');
  });

  it('should handle large bullet lists efficiently', () => {
    const startTime = performance.now();
    
    // Generate large content with many bullets
    const bulletCount = 1000;
    let content = '';
    const patterns: MarkdownPattern[] = [];
    
    for (let i = 0; i < bulletCount; i++) {
      const bulletText = `- Bullet item ${i}\n`;
      const start = content.length;
      const end = start + bulletText.length - 1; // -1 to exclude newline
      
      content += bulletText;
      patterns.push({
        type: 'bullet',
        start,
        end,
        syntax: '- ',
        content: `Bullet item ${i}`,
        bulletType: '-',
        line: i,
        column: 0
      });
    }

    const result = bulletToNodeConverter.convertBulletsToNodes(content, patterns, 0);
    
    const endTime = performance.now();
    const duration = endTime - startTime;
    
    expect(result.hasConversions).toBe(true);
    expect(result.newNodes).toHaveLength(bulletCount);
    expect(duration).toBeLessThan(100); // Should complete within 100ms
  });
});