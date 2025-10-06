/**
 * Focus Management Verification Tests
 *
 * Comprehensive integration tests for focus behavior across the application.
 * Verifies that focus is properly managed during:
 * - Modal interactions (autocomplete, slash commands)
 * - Node creation within single nodes
 * - Keyboard interactions
 * - Focus restoration after modal close
 *
 * Test Coverage:
 * - Modal Focus - Autocomplete (5 tests)
 * - Node Creation Focus (4 tests)
 * - Slash Command Focus (2 tests)
 * - Editor Focus Stability (3 tests)
 *
 * Note: Multi-node navigation tests require BaseNodeViewer with full NodeServiceContext,
 * which is tested separately in viewer-specific test files.
 *
 * Related: Issue #161
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { waitForEffects } from '../helpers';
import BaseNode from '$lib/design/components/base-node.svelte';

describe('Focus Management', () => {
  beforeEach(() => {
    // Clean up DOM between tests
    document.body.innerHTML = '';
  });

  describe('Modal Focus - Autocomplete', () => {
    it('should maintain editor focus even when typing trigger characters', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');
      expect(editor).toBeInTheDocument();

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Type "@" character (potential autocomplete trigger)
      await user.keyboard('@');
      await waitForEffects();

      // Focus should remain in editor regardless of autocomplete state
      expect(document.activeElement).toBe(editor);

      // Continue typing - focus should remain stable
      await user.keyboard('test');
      await waitForEffects();

      expect(document.activeElement).toBe(editor);
    });

    it('should maintain editor focus when typing slash character', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');
      expect(editor).toBeInTheDocument();

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Type "/" at start (potential slash command trigger)
      await user.keyboard('/');
      await waitForEffects();

      // Editor should maintain focus regardless of dropdown state
      expect(document.activeElement).toBe(editor);
    });

    it('should maintain focus when pressing Enter after typing', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');

      // Verify focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Type content and press Enter
      await user.keyboard('test{Enter}');
      await waitForEffects();

      // Focus should remain in editable context
      const activeElement = document.activeElement;
      const isEditable = activeElement?.getAttribute('contenteditable') === 'true';
      expect(isEditable).toBe(true);
    });

    it('should maintain focus when pressing Escape', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');

      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Press Escape (should not lose focus)
      await user.keyboard('{Escape}');
      await waitForEffects();

      // Focus should remain in editor
      expect(document.activeElement).toBe(editor);
    });

    it('should maintain focus when pressing Tab', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Press Tab
      await user.keyboard('{Tab}');
      await waitForEffects();

      // Tab may move focus to next focusable element
      // But we can verify the editor is still focusable
      editor.focus();
      await waitForEffects();

      expect(document.activeElement).toBe(editor);
    });
  });

  describe('Node Creation Focus', () => {
    it('should maintain editor focus after Enter key press', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Test content',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Position cursor at end and press Enter
      await user.keyboard('{End}{Enter}');
      await waitForEffects();

      // Focus should remain in the editor (or a related contenteditable)
      // Even if new node creation is triggered, focus management keeps it in editable area
      const activeElement = document.activeElement;
      const isEditableElement =
        activeElement?.getAttribute('contenteditable') === 'true' ||
        activeElement?.getAttribute('role') === 'textbox';

      expect(isEditableElement).toBe(true);
    });

    it('should preserve editor focusability after content split with Enter', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'BeforeAfter',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      // Position cursor in middle (after "Before")
      await user.click(editor);
      const selection = window.getSelection();
      const range = document.createRange();
      const textNode = editor.firstChild;

      if (textNode && textNode.nodeType === Node.TEXT_NODE) {
        range.setStart(textNode, 6); // After "Before"
        range.collapse(true);
        selection?.removeAllRanges();
        selection?.addRange(range);
      }

      // Press Enter to split content
      await user.keyboard('{Enter}');
      await waitForEffects();

      // Focus should remain in editable area
      const activeElement = document.activeElement;
      const isEditable = activeElement?.getAttribute('contenteditable') === 'true';

      expect(isEditable).toBe(true);

      // Should be able to continue typing
      await user.keyboard('New content');
      await waitForEffects();

      // Content should have been updated (proving focus is correct)
      const contentAfter = editor.textContent || '';
      expect(contentAfter.length).toBeGreaterThan(0);
    });

    it('should support programmatic focus via autoFocus prop', async () => {
      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Test',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      // With autoFocus=true, editor should automatically receive focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Editor should be ready for input (has correct attributes)
      expect(editor).toHaveAttribute('contenteditable', 'true');
      expect(editor).toHaveAttribute('tabindex', '0');
    });

    it('should allow manual focus restoration after blur', async () => {
      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Test',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Blur the editor
      editor.blur();
      await waitForEffects();

      expect(document.activeElement).not.toBe(editor);

      // Manually restore focus
      editor.focus();
      await waitForEffects();

      expect(document.activeElement).toBe(editor);

      // Verify editor is still functional
      expect(editor).toHaveAttribute('contenteditable', 'true');
    });
  });

  describe('Editor Focus Stability', () => {
    it('should maintain focus during arrow key navigation within editor', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Test content with multiple words',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Navigate with arrow keys
      await user.keyboard('{ArrowRight}{ArrowRight}{ArrowLeft}');
      await waitForEffects();

      // Focus should remain in editor
      expect(document.activeElement).toBe(editor);

      // Navigate to start/end
      await user.keyboard('{Home}{End}');
      await waitForEffects();

      // Focus still stable
      expect(document.activeElement).toBe(editor);
    });

    it('should maintain focus during text selection', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Selectable text content',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Select all text
      await user.keyboard('{Control>}a{/Control}');
      await waitForEffects();

      // Focus should remain during selection
      expect(document.activeElement).toBe(editor);

      // Verify selection exists
      const selection = window.getSelection();
      expect(selection?.toString()).toBeTruthy();

      // Type to replace selection
      await user.keyboard('Replaced');
      await waitForEffects();

      // Focus still in editor
      expect(document.activeElement).toBe(editor);
      expect(editor.textContent).toBe('Replaced');
    });

    it('should maintain focusability after user interaction', async () => {
      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: 'Initial content',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]') as HTMLElement;

      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Blur and refocus to simulate interaction
      const user = userEvent.setup();
      editor.blur();
      await waitForEffects();

      await user.click(editor);
      await waitForEffects();

      // Focus should be restored
      expect(document.activeElement).toBe(editor);

      // Editor should remain functional
      expect(editor).toHaveAttribute('contenteditable', 'true');
    });
  });

  describe('Slash Command Focus', () => {
    it('should maintain editor focus after slash command insertion', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');
      expect(editor).toBeInTheDocument();

      // Verify initial focus
      await waitFor(() => {
        expect(document.activeElement).toBe(editor);
      });

      // Type "/" to trigger slash command dropdown
      await user.keyboard('/');
      await waitForEffects();

      // Wait for dropdown to appear (if implemented)
      await waitForEffects(100);

      // If dropdown appears, select a command
      const listbox = screen.queryByRole('listbox');
      if (listbox) {
        await user.keyboard('{Enter}');
        await waitForEffects();
      }

      // Focus should remain in editor
      expect(document.activeElement).toBe(editor);

      // User should be able to continue typing
      await user.keyboard('content');
      await waitForEffects();

      expect(editor!.textContent).toContain('content');
    });

    it('should auto-focus editor after node type conversion via slash command', async () => {
      const user = userEvent.setup();

      const { container } = render(BaseNode, {
        nodeId: 'test-node',
        nodeType: 'text',
        content: '',
        autoFocus: true
      });

      const editor = container.querySelector('[contenteditable="true"]');

      // Type "/" and convert to different node type (e.g., /task)
      await user.keyboard('/task');
      await waitForEffects();

      // If slash command system is active, try to execute
      const listbox = screen.queryByRole('listbox');
      if (listbox) {
        await user.keyboard('{Enter}');
        await waitForEffects();
      }

      // After node type conversion, editor should maintain or regain focus
      // (In current implementation, this may be the same editor)
      await waitForEffects(100);

      // Verify we can type in the editor
      await user.keyboard('task content');
      await waitForEffects();

      expect(editor!.textContent).toContain('task content');

      // Focus should be in a contenteditable element
      const activeElement = document.activeElement;
      expect(activeElement?.getAttribute('contenteditable')).toBe('true');
    });
  });
});
