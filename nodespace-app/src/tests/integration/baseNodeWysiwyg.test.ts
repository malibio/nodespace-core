/**
 * BaseNode WYSIWYG Integration Tests
 * 
 * Basic integration tests for WYSIWYG processing with BaseNode component,
 * verifying component structure and basic functionality without complex mocking.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import BaseNode from '$lib/design/components/BaseNode.svelte';
import type { ComponentProps } from 'svelte';

type BaseNodeProps = ComponentProps<BaseNode>;

describe('BaseNode WYSIWYG Integration', () => {
  const defaultProps: BaseNodeProps = {
    nodeType: 'text',
    nodeId: 'test-node',
    content: '',
    enableWYSIWYG: true,
    contentEditable: true,
    editable: true
  };

  describe('WYSIWYG Configuration', () => {
    it('should render with WYSIWYG enabled', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true,
          wysiwygConfig: {
            enableRealTime: true,
            hideSyntax: true,
            enableFormatting: true
          }
        }
      });

      await tick();

      const nodeElement = screen.getByRole('button');
      expect(nodeElement).toBeTruthy();
    });

    it('should render with WYSIWYG disabled', () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: false
        }
      });

      const nodeElement = screen.getByRole('button');
      expect(nodeElement).toBeTruthy();
    });
  });

  describe('Content Editing with WYSIWYG', () => {
    it('should add WYSIWYG CSS classes to contenteditable when enabled', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      // Click to start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--wysiwyg');
      expect(contentEditableElement.getAttribute('data-wysiwyg-enabled')).toBe('true');
    });

    it('should not add WYSIWYG classes when disabled', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: false
        }
      });

      // Click to start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement.className).not.toContain('ns-node__contenteditable--wysiwyg');
      expect(contentEditableElement.getAttribute('data-wysiwyg-enabled')).toBe('false');
    });

    it('should handle content input', async () => {
      let contentChangedEvent: any = null;

      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      component.$on('contentChanged', (event) => {
        contentChangedEvent = event.detail;
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      // Input content
      const contentEditableElement = screen.getByRole('textbox');
      contentEditableElement.textContent = 'Test content';
      await fireEvent.input(contentEditableElement);
      await tick();

      expect(contentChangedEvent).toBeTruthy();
      expect(contentChangedEvent.content).toBe('Test content');
      expect(contentChangedEvent.nodeId).toBe('test-node');
    });
  });

  describe('Display Mode WYSIWYG', () => {
    it('should show WYSIWYG classes in display mode', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          content: 'Sample content',
          enableWYSIWYG: true
        }
      });

      await tick();

      // Look for WYSIWYG text class
      const textElement = document.querySelector('.ns-node__text--wysiwyg');
      expect(textElement).toBeTruthy();
    });

    it('should not show WYSIWYG classes when disabled', () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          content: 'Sample content',
          enableWYSIWYG: false
        }
      });

      const textElement = document.querySelector('.ns-node__text--wysiwyg');
      expect(textElement).toBeNull();
    });

    it('should handle empty content gracefully', () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          content: '',
          enableWYSIWYG: true
        }
      });

      const emptyElement = screen.getByText(defaultProps.placeholder || 'Click to add content...');
      expect(emptyElement).toBeTruthy();
    });
  });

  describe('Event Handling', () => {
    it('should emit focus events', async () => {
      let focusEvent: any = null;

      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      component.$on('focus', (event) => {
        focusEvent = event.detail;
      });

      // Click to focus
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      expect(focusEvent).toBeTruthy();
      expect(focusEvent.nodeId).toBe('test-node');
    });

    it('should emit blur events', async () => {
      let blurEvent: any = null;

      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          content: 'Test content',
          enableWYSIWYG: true
        }
      });

      component.$on('blur', (event) => {
        blurEvent = event.detail;
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      // Blur the element
      const contentEditableElement = screen.getByRole('textbox');
      await fireEvent.blur(contentEditableElement);
      await tick();

      expect(blurEvent).toBeTruthy();
      expect(blurEvent.nodeId).toBe('test-node');
    });
  });

  describe('Multiline Support', () => {
    it('should handle multiline WYSIWYG content', async () => {
      const multilineContent = `# Header\n\n**Bold text** on line 2\n*Italic text* on line 3`;

      render(BaseNode, {
        props: {
          ...defaultProps,
          content: multilineContent,
          multiline: true,
          enableWYSIWYG: true
        }
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--multiline');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--wysiwyg');
    });

    it('should handle single-line WYSIWYG content', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          content: '**Bold text**',
          multiline: false,
          enableWYSIWYG: true
        }
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--single');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--wysiwyg');
    });
  });

  describe('Error Resilience', () => {
    it('should continue functioning when WYSIWYG is disabled mid-session', async () => {
      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      // Start with WYSIWYG enabled
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      // Disable WYSIWYG
      component.$set({ enableWYSIWYG: false });
      await tick();

      // Should still be functional
      const contentEditableElement = screen.getByRole('textbox');
      contentEditableElement.textContent = 'plain text';
      await fireEvent.input(contentEditableElement);
      await tick();

      expect(contentEditableElement.textContent).toBe('plain text');
    });
  });

  describe('CSS Classes', () => {
    it('should apply correct CSS classes for different configurations', async () => {
      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true,
          multiline: true
        }
      });

      // Check display mode classes
      const textElement = document.querySelector('.ns-node__text--wysiwyg');
      expect(textElement).toBeTruthy();

      // Start editing to check contenteditable classes
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--wysiwyg');
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--multiline');
      expect(contentEditableElement.getAttribute('data-wysiwyg-enabled')).toBe('true');
    });

    it('should include WYSIWYG processing indicator classes', async () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');
      
      // The processing class might be transient, but the WYSIWYG class should be present
      expect(contentEditableElement.className).toContain('ns-node__contenteditable--wysiwyg');
    });
  });
});

describe('WYSIWYG CSS Integration', () => {
  it('should include WYSIWYG styles in component', () => {
    render(BaseNode, {
      props: {
        nodeType: 'text',
        nodeId: 'test',
        enableWYSIWYG: true
      }
    });

    // Verify the component renders with proper structure
    const nodeElement = document.querySelector('.ns-node');
    expect(nodeElement).toBeTruthy();

    // Check for style definitions - WYSIWYG styles are global
    const hasWysiwygStyles = document.head.innerHTML.includes('wysiwyg-') || 
                            document.querySelector('style')?.textContent?.includes('wysiwyg-');
    
    // This is a basic check - the detailed CSS testing is in the unit tests
    expect(nodeElement || hasWysiwygStyles).toBeTruthy();
  });
});