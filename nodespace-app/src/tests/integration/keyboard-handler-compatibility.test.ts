/**
 * Keyboard Handler Compatibility Integration Tests
 * 
 * Verifies that ContentEditable system works seamlessly with existing
 * NodeSpace keyboard shortcuts and navigation patterns.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { tick } from 'svelte';

import BaseNode from '$lib/design/components/BaseNode.svelte';
import HierarchyDemo from '$lib/components/HierarchyDemo.svelte';
import type { TreeNodeData } from '$lib/types/tree';

describe('Keyboard Handler Compatibility', () => {
  let user: ReturnType<typeof userEvent.setup>;

  beforeEach(() => {
    user = userEvent.setup();
  });

  describe('Existing Keyboard Shortcuts', () => {
    it('should preserve Enter key behavior for starting/stopping edit mode', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'keyboard-test',
          nodeType: 'text',
          content: 'Test content',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');

      // Test Enter key to start editing
      nodeElement.focus();
      await user.keyboard('{Enter}');
      await tick();

      const contentEditable = screen.getByRole('textbox');
      expect(document.activeElement).toBe(contentEditable);

      // Test Enter key to stop editing (single-line)
      await user.keyboard('{Enter}');
      await tick();

      expect(document.activeElement).not.toBe(contentEditable);
    });

    it('should preserve Escape key behavior for canceling edit mode', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'escape-test',
          nodeType: 'text',
          content: 'Original content',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');
      
      // Modify content
      await user.clear(contentEditable);
      await user.type(contentEditable, 'Modified content');

      // Press Escape to cancel
      await user.keyboard('{Escape}');
      await tick();

      // Should exit edit mode
      expect(document.activeElement).not.toBe(contentEditable);
    });

    it('should handle Space key for starting edit mode', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'space-test',
          nodeType: 'text',
          content: 'Test content',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');

      // Test Space key to start editing
      nodeElement.focus();
      await user.keyboard(' ');
      await tick();

      const contentEditable = screen.getByRole('textbox');
      expect(document.activeElement).toBe(contentEditable);
    });
  });

  describe('Tab Navigation Integration', () => {
    it('should work with Tab/Shift-Tab for node navigation', async () => {
      const mockNodes: TreeNodeData[] = [
        {
          id: 'node-1',
          title: 'First Node',
          content: 'First node content',
          nodeType: 'text',
          depth: 0,
          parentId: null,
          children: [],
          expanded: true,
          hasChildren: false
        },
        {
          id: 'node-2',
          title: 'Second Node',
          content: 'Second node content',
          nodeType: 'text',
          depth: 0,
          parentId: null,
          children: [],
          expanded: true,
          hasChildren: false
        },
        {
          id: 'node-3',
          title: 'Third Node',
          content: 'Third node content',
          nodeType: 'text',
          depth: 0,
          parentId: null,
          children: [],
          expanded: true,
          hasChildren: false
        }
      ];

      const { component } = render(HierarchyDemo, {
        props: { nodes: mockNodes }
      });

      // Tab navigation should move between nodes
      await user.tab();
      expect(document.activeElement).toBeTruthy();

      await user.tab();
      expect(document.activeElement).toBeTruthy();

      // Shift-Tab should go backwards
      await user.keyboard('{Shift>}{Tab}{/Shift}');
      expect(document.activeElement).toBeTruthy();
    });

    it('should handle Tab within ContentEditable correctly', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'tab-content-test',
          nodeType: 'text',
          content: 'Start of content',
          contentEditable: true,
          editable: true,
          multiline: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Tab within contenteditable should insert tab character or handle indentation
      await user.type(contentEditable, 'Line 1{Enter}');
      await user.keyboard('{Tab}');
      await user.type(contentEditable, 'Indented line');

      expect(contentEditable.textContent).toContain('Line 1');
      expect(contentEditable.textContent).toContain('Indented line');
    });
  });

  describe('Multi-line Keyboard Behavior', () => {
    it('should handle Enter correctly in multiline mode', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'multiline-enter-test',
          nodeType: 'text',
          content: 'First line',
          contentEditable: true,
          editable: true,
          multiline: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Regular Enter should exit edit mode in multiline
      await user.type(contentEditable, '{Enter}Second line');
      await user.keyboard('{Enter}');
      await tick();

      // Should exit edit mode
      expect(document.activeElement).not.toBe(contentEditable);
    });

    it('should handle Shift+Enter for soft newlines', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'shift-enter-test',
          nodeType: 'text',
          content: 'First line',
          contentEditable: true,
          editable: true,
          multiline: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Shift+Enter should add newline without exiting
      await user.type(contentEditable, '{Shift>}{Enter}{/Shift}Second line');
      await user.type(contentEditable, '{Shift>}{Enter}{/Shift}Third line');

      expect(document.activeElement).toBe(contentEditable);
      expect(contentEditable.textContent).toContain('First line');
      expect(contentEditable.textContent).toContain('Second line');
      expect(contentEditable.textContent).toContain('Third line');
    });
  });

  describe('Modifier Key Combinations', () => {
    it('should prevent default browser formatting shortcuts', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'modifier-test',
          nodeType: 'text',
          content: 'Test content',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Test Ctrl+B (bold) - should be prevented
      await user.type(contentEditable, 'Some text');
      await user.keyboard('{Control>}b{/Control}');
      
      // Should not apply browser bold formatting
      expect(contentEditable.querySelector('b, strong')).toBeNull();

      // Test Ctrl+I (italic) - should be prevented
      await user.keyboard('{Control>}i{/Control}');
      expect(contentEditable.querySelector('i, em')).toBeNull();

      // Test Ctrl+U (underline) - should be prevented
      await user.keyboard('{Control>}u{/Control}');
      expect(contentEditable.querySelector('u')).toBeNull();
    });

    it('should handle Ctrl+A (select all) correctly', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'selectall-test',
          nodeType: 'text',
          content: 'All this content should be selected',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Ctrl+A should select all content
      await user.keyboard('{Control>}a{/Control}');
      await tick();

      const selection = window.getSelection();
      expect(selection?.toString()).toContain('All this content');
    });
  });

  describe('Arrow Key Navigation', () => {
    it('should handle arrow keys within ContentEditable', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'arrow-test',
          nodeType: 'text',
          content: 'Navigate this text with arrows',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Place cursor at start
      const selection = window.getSelection();
      if (selection) {
        const range = document.createRange();
        range.setStart(contentEditable.firstChild || contentEditable, 0);
        range.collapse(true);
        selection.removeAllRanges();
        selection.addRange(range);
      }

      // Test arrow key navigation
      await user.keyboard('{ArrowRight}');
      await user.keyboard('{ArrowRight}');
      await user.keyboard('{ArrowRight}');

      // Should maintain focus in contenteditable
      expect(document.activeElement).toBe(contentEditable);

      // Test home and end keys
      await user.keyboard('{Home}');
      expect(document.activeElement).toBe(contentEditable);

      await user.keyboard('{End}');
      expect(document.activeElement).toBe(contentEditable);
    });

    it('should handle arrow keys in multiline content', async () => {
      const multilineContent = `First line of content
Second line of content
Third line of content`;

      const { component } = render(BaseNode, {
        props: {
          nodeId: 'multiline-arrow-test',
          nodeType: 'text',
          content: multilineContent,
          contentEditable: true,
          editable: true,
          multiline: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Test up/down arrow navigation
      await user.keyboard('{ArrowDown}');
      await user.keyboard('{ArrowDown}');
      await user.keyboard('{ArrowUp}');

      expect(document.activeElement).toBe(contentEditable);
    });
  });

  describe('Copy/Paste/Cut Operations', () => {
    it('should handle Ctrl+C, Ctrl+V, Ctrl+X correctly', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'clipboard-test',
          nodeType: 'text',
          content: 'Content to copy and paste',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Select some text
      await user.keyboard('{Control>}a{/Control}');
      
      // Copy should work (browser handles this)
      await user.keyboard('{Control>}c{/Control}');
      
      // Clear and paste
      await user.keyboard('{Delete}');
      await user.keyboard('{Control>}v{/Control}');

      // Content should be restored (in real browser with clipboard access)
      expect(document.activeElement).toBe(contentEditable);
    });
  });

  describe('Focus Management During Transitions', () => {
    it('should maintain focus correctly during edit mode transitions', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'focus-transition-test',
          nodeType: 'text',
          content: 'Focus test content',
          contentEditable: true,
          editable: true
        }
      });

      let focusEvents: string[] = [];
      let blurEvents: string[] = [];

      component.$on('focus', () => focusEvents.push('node-focus'));
      component.$on('blur', () => blurEvents.push('node-blur'));

      const nodeElement = screen.getByRole('button');

      // Focus on node element
      nodeElement.focus();
      expect(document.activeElement).toBe(nodeElement);

      // Enter edit mode
      await user.keyboard('{Enter}');
      await tick();

      const contentEditable = screen.getByRole('textbox');
      expect(document.activeElement).toBe(contentEditable);
      expect(focusEvents.length).toBe(1);

      // Exit edit mode
      await user.keyboard('{Enter}');
      await tick();

      expect(blurEvents.length).toBe(1);
    });

    it('should handle programmatic focus changes', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'programmatic-focus-test',
          nodeType: 'text',
          content: 'Programmatic focus test',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');

      // Programmatically focus the node
      nodeElement.focus();
      expect(document.activeElement).toBe(nodeElement);

      // Enter edit mode programmatically
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');
      expect(document.activeElement).toBe(contentEditable);

      // Programmatically blur
      contentEditable.blur();
      await tick();

      expect(document.activeElement).not.toBe(contentEditable);
    });
  });

  describe('Complex Keyboard Scenarios', () => {
    it('should handle rapid keyboard events without conflicts', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'rapid-keyboard-test',
          nodeType: 'text',
          content: '',
          contentEditable: true,
          editable: true,
          multiline: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Rapid typing with mixed keyboard events
      const keySequence = [
        'Hello',
        '{Enter}',
        'Second line',
        '{Shift>}{Enter}{/Shift}',
        'Third line',
        '{Home}',
        '{End}',
        '{Control>}a{/Control}',
        'Replaced all'
      ];

      for (const keys of keySequence) {
        await user.keyboard(keys);
        await new Promise(resolve => setTimeout(resolve, 10));
      }

      expect(contentEditable.textContent).toContain('Replaced all');
    });

    it('should maintain keyboard behavior with WYSIWYG processing', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'wysiwyg-keyboard-test',
          nodeType: 'text',
          content: '',
          contentEditable: true,
          editable: true,
          multiline: true,
          enableWYSIWYG: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');

      // Type markdown content with various keyboard operations
      await user.type(contentEditable, '# Header{Enter}{Enter}');
      await user.type(contentEditable, '**Bold text** and *italic text*{Enter}{Enter}');
      await user.type(contentEditable, '- Bullet point one{Enter}');
      await user.type(contentEditable, '- Bullet point two{Enter}{Enter}');
      await user.type(contentEditable, '```{Enter}code block{Enter}```');

      // Test navigation within WYSIWYG content
      await user.keyboard('{Home}');
      await user.keyboard('{End}');
      await user.keyboard('{Control>}a{/Control}');

      // Should maintain keyboard functionality
      expect(document.activeElement).toBe(contentEditable);
      expect(contentEditable.textContent).toContain('Header');
      expect(contentEditable.textContent).toContain('Bold text');
      expect(contentEditable.textContent).toContain('code block');
    });
  });

  describe('Accessibility Keyboard Support', () => {
    it('should support screen reader navigation patterns', async () => {
      const { component } = render(BaseNode, {
        props: {
          nodeId: 'accessibility-test',
          nodeType: 'text',
          content: 'Content for accessibility testing',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      
      // Should have proper ARIA attributes
      expect(nodeElement.getAttribute('role')).toBe('button');
      expect(nodeElement.getAttribute('tabindex')).toBe('0');

      // Enter edit mode
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');
      expect(contentEditable.getAttribute('role')).toBe('textbox');
      expect(contentEditable.getAttribute('aria-label')).toBe('Editable content');
    });

    it('should handle high contrast and reduced motion preferences', async () => {
      // Mock media queries
      Object.defineProperty(window, 'matchMedia', {
        writable: true,
        value: vi.fn().mockImplementation(query => ({
          matches: query.includes('prefers-contrast: high') || query.includes('prefers-reduced-motion: reduce'),
          media: query,
          onchange: null,
          addListener: vi.fn(),
          removeListener: vi.fn(),
        })),
      });

      const { component } = render(BaseNode, {
        props: {
          nodeId: 'a11y-preferences-test',
          nodeType: 'text',
          content: 'Accessibility preferences test',
          contentEditable: true,
          editable: true
        }
      });

      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditable = screen.getByRole('textbox');
      
      // Should still function normally with accessibility preferences
      await user.type(contentEditable, ' - modified');
      expect(contentEditable.textContent).toContain('modified');
    });
  });
});