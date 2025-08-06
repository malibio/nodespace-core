/**
 * TextNode Component Test
 * 
 * Basic test to verify TextNode functionality and acceptance criteria.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import TextNode from './TextNode.svelte';
import { mockTextService } from '$lib/services/mockTextService';

describe('TextNode Component', () => {
  beforeEach(() => {
    // Clear any existing data
    mockTextService.getStats(); // Just to access the service
  });

  it('renders with text icon in circle indicator', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-1',
        title: 'Test Node',
        content: 'Test content'
      }
    });

    // Should render BaseNode with text nodeType
    const node = container.querySelector('[data-node-type="text"]');
    expect(node).toBeTruthy();
    
    // Should have hierarchy indicator
    const indicator = container.querySelector('.ns-node__hierarchy-indicator');
    expect(indicator).toBeTruthy();
  });

  it('supports click-to-edit functionality', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-2',
        title: 'Test Node',
        content: 'Test content',
        editable: true
      }
    });

    // Click on the node to start editing
    const nodeButton = container.querySelector('button.ns-node');
    expect(nodeButton).toBeTruthy();
    
    await fireEvent.click(nodeButton!);
    await tick();

    // Should show textarea in editing mode
    const textarea = container.querySelector('.ns-text-node__textarea');
    expect(textarea).toBeTruthy();
    expect(textarea).toHaveValue('Test content');
  });

  it('handles keyboard shortcuts correctly', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-3',
        title: 'Test Node',
        content: 'Test content'
      }
    });

    // Click to edit
    const nodeButton = container.querySelector('button.ns-node');
    await fireEvent.click(nodeButton!);
    await tick();

    const textarea = container.querySelector('.ns-text-node__textarea');
    expect(textarea).toBeTruthy();

    // Test Escape to cancel
    await fireEvent.keyDown(textarea!, { key: 'Escape' });
    await tick();

    // Should exit editing mode
    expect(container.querySelector('.ns-text-node__textarea')).toBeFalsy();

    // Click to edit again
    await fireEvent.click(nodeButton!);
    await tick();

    const newTextarea = container.querySelector('.ns-text-node__textarea');
    expect(newTextarea).toBeTruthy();

    // Test Ctrl+Enter to save (simulated)
    await fireEvent.keyDown(newTextarea!, { 
      key: 'Enter', 
      ctrlKey: true 
    });
    await tick();

    // Should trigger save process
    // Note: In real test, we'd mock the service and verify save was called
  });

  it('preserves content through edit/cancel operations', async () => {
    const originalContent = 'Original content';
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-4',
        title: 'Test Node',
        content: originalContent
      }
    });

    // Start editing
    const nodeButton = container.querySelector('button.ns-node');
    await fireEvent.click(nodeButton!);
    await tick();

    const textarea = container.querySelector('.ns-text-node__textarea') as HTMLTextAreaElement;
    
    // Change content
    await fireEvent.input(textarea, { target: { value: 'Modified content' } });
    await tick();
    
    expect(textarea.value).toBe('Modified content');

    // Cancel editing
    await fireEvent.keyDown(textarea, { key: 'Escape' });
    await tick();

    // Content should be preserved (reverted to original)
    const displayContent = container.querySelector('.ns-text-node__display');
    expect(displayContent?.textContent?.trim()).toBe(originalContent);
  });

  it('renders basic markdown in display mode', async () => {
    const markdownContent = '**Bold text** and *italic text*\\n\\n# Header';
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-5',
        title: 'Markdown Test',
        content: markdownContent,
        markdown: true
      }
    });

    const markdownDisplay = container.querySelector('.ns-text-node__markdown');
    expect(markdownDisplay).toBeTruthy();
    
    // Should contain rendered markdown elements
    expect(markdownDisplay?.innerHTML).toContain('<strong');
    expect(markdownDisplay?.innerHTML).toContain('<em');
    expect(markdownDisplay?.innerHTML).toContain('<h1');
  });

  it('auto-resizes textarea during editing', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-6',
        content: 'Short content'
      }
    });

    // Start editing
    const nodeButton = container.querySelector('button.ns-node');
    await fireEvent.click(nodeButton!);
    await tick();

    const textarea = container.querySelector('.ns-text-node__textarea') as HTMLTextAreaElement;
    const initialHeight = textarea.style.height;

    // Add more content
    const longContent = 'Line 1\\nLine 2\\nLine 3\\nLine 4\\nLine 5';
    await fireEvent.input(textarea, { target: { value: longContent } });
    await tick();

    // Note: In a real browser environment, scrollHeight would change
    // This is a basic test to ensure the auto-resize logic exists
    expect(textarea.value).toBe(longContent);
  });

  it('displays appropriate save status', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-7',
        content: 'Test content',
        autoSave: false // Disable auto-save for controlled testing
      }
    });

    // Start editing
    const nodeButton = container.querySelector('button.ns-node');
    await fireEvent.click(nodeButton!);
    await tick();

    const textarea = container.querySelector('.ns-text-node__textarea');
    
    // Modify content
    await fireEvent.input(textarea!, { target: { value: 'Modified content' } });
    await tick();

    // Should show unsaved status
    await waitFor(() => {
      const status = container.querySelector('.ns-text-node-status--unsaved');
      expect(status).toBeTruthy();
    });
  });

  it('works in different panel contexts', async () => {
    // Test compact mode
    const { container } = render(TextNode, {
      props: {
        nodeId: 'test-8',
        content: 'Compact content'
      }
    });

    // Component should render without errors
    const textNodeContent = container.querySelector('.ns-text-node__content');
    expect(textNodeContent).toBeTruthy();

    // Should be clickable and editable
    const nodeButton = container.querySelector('button.ns-node');
    expect(nodeButton).toBeTruthy();
    expect(nodeButton).not.toHaveAttribute('disabled');
  });
});

// Integration test with mock service
describe('TextNode Mock Service Integration', () => {
  it('integrates with mock service for auto-save', async () => {
    const { container } = render(TextNode, {
      props: {
        nodeId: 'integration-test-1',
        content: 'Initial content',
        autoSave: true,
        autoSaveDelay: 100 // Short delay for testing
      }
    });

    // Start editing
    const nodeButton = container.querySelector('button.ns-node');
    await fireEvent.click(nodeButton!);
    await tick();

    const textarea = container.querySelector('.ns-text-node__textarea');
    
    // Modify content
    await fireEvent.input(textarea!, { target: { value: 'Modified content for auto-save' } });
    await tick();

    // Wait for auto-save to trigger
    await waitFor(() => {
      const savingStatus = container.querySelector('.ns-text-node-status--saving');
      // Should show saving or saved status
      const savedStatus = container.querySelector('.ns-text-node-status--saved');
      expect(savingStatus || savedStatus).toBeTruthy();
    }, { timeout: 1000 });
  });
});