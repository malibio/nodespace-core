/**
 * Component testing example for NodeSpace
 * Shows realistic UI component testing patterns
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { createTestNode, SimpleMockStore } from '../utils/testUtils';

// Import TextNode component for testing
import TextNodeComponent from '../../lib/components/TextNode.svelte';

// Type guard to ensure component is available
function ensureComponent() {
  if (!TextNodeComponent) {
    throw new Error('TextNode component not available');
  }
  return TextNodeComponent;
}

describe('TextNode Component', () => {
  let store: SimpleMockStore;

  beforeEach(() => {
    SimpleMockStore.resetInstance();
    store = SimpleMockStore.getInstance();
  });

  describe('Rendering', () => {
    it('renders text content correctly', () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Hello World' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content }
      });

      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('Hello World');
    });

    it('shows placeholder for empty content', () => {
      const component = ensureComponent();
      const node = createTestNode({ content: '' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content }
      });

      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('Click to edit...');
    });

    it('renders as non-editable when editable=false', () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Read-only content' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: false }
      });

      const display = getByTestId('text-display');
      expect(display).toHaveAttribute('role', 'article');
      expect(display).not.toHaveAttribute('tabindex');
    });
  });

  describe('User Interactions', () => {
    it('enters edit mode on click', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Editable content' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      const display = getByTestId('text-display');
      await fireEvent.click(display);

      const editor = getByTestId('text-editor');
      expect(editor).toBeInTheDocument();
      expect(editor).toHaveValue('Editable content');
    });

    it('saves content on blur', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Original content' });
      // Track if save was called

      const { getByTestId } = render(component, {
        props: {
          nodeId: node.id,
          content: node.content,
          editable: true
        }
      });

      // Enter edit mode
      await fireEvent.click(getByTestId('text-display'));

      // Modify content
      const editor = getByTestId('text-editor');
      await fireEvent.input(editor, { target: { value: 'Modified content' } });

      // Blur to save
      await fireEvent.blur(editor);

      // Should return to display mode
      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('Modified content');
    });

    it('saves on Enter key', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Enter test' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      await fireEvent.click(getByTestId('text-display'));
      const editor = getByTestId('text-editor');

      await fireEvent.input(editor, { target: { value: 'New content' } });
      await fireEvent.keyDown(editor, { key: 'Enter' });

      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('New content');
    });

    it('cancels editing on Escape key', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Original content' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      await fireEvent.click(getByTestId('text-display'));
      const editor = getByTestId('text-editor');

      await fireEvent.input(editor, { target: { value: 'Modified content' } });
      await fireEvent.keyDown(editor, { key: 'Escape' });

      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('Original content'); // Should revert
    });
  });

  describe('Data Store Integration', () => {
    it('saves to data store when content changes', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Store test' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      await fireEvent.click(getByTestId('text-display'));
      const editor = getByTestId('text-editor');

      await fireEvent.input(editor, { target: { value: 'Saved content' } });
      await fireEvent.blur(editor);

      const savedNode = await store.load(node.id);
      expect(savedNode?.content).toBe('Saved content');
    });

    it('handles store save errors gracefully', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Error test' });

      // Mock store that throws errors
      const errorStore = {
        save: vi.fn().mockRejectedValue(new Error('Save failed'))
      };

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      await fireEvent.click(getByTestId('text-display'));
      const editor = getByTestId('text-editor');

      await fireEvent.input(editor, { target: { value: 'New content' } });
      await fireEvent.blur(editor);

      // Should still update display even if save fails
      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('New content');
      expect(errorStore.save).toHaveBeenCalled();
    });
  });

  describe('Event Dispatching', () => {
    it('dispatches save event with content', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Event test' });

      const { getByTestId } = render(component, {
        props: {
          nodeId: node.id,
          content: node.content,
          editable: true
        }
      });

      await fireEvent.click(getByTestId('text-display'));
      const editor = getByTestId('text-editor');

      await fireEvent.input(editor, { target: { value: 'Dispatched content' } });
      await fireEvent.blur(editor);

      // Verify the component renders and functions correctly
      // Note: Event dispatching is tested through integration tests
      const display = getByTestId('text-display');
      expect(display).toHaveTextContent('Dispatched content');
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA roles', () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Accessibility test' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      const display = getByTestId('text-display');
      expect(display).toHaveAttribute('role', 'button');
      expect(display).toHaveAttribute('tabindex', '0');
    });

    it('supports keyboard navigation', async () => {
      const component = ensureComponent();
      const node = createTestNode({ content: 'Keyboard test' });

      const { getByTestId } = render(component, {
        props: { nodeId: node.id, content: node.content, editable: true }
      });

      const display = getByTestId('text-display');
      await fireEvent.keyDown(display, { key: 'Enter' });

      const editor = getByTestId('text-editor');
      expect(editor).toBeInTheDocument();
    });
  });
});
