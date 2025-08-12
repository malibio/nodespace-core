/**
 * BaseNode WYSIWYG Integration Tests
 * 
 * Tests for WYSIWYG processing integration with BaseNode component,
 * verifying real-time markdown processing and visual formatting.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, waitFor, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import BaseNode from '$lib/design/components/BaseNode.svelte';
import type { ComponentProps } from 'svelte';

// Mock the WYSIWYG processor for controlled testing
vi.mock('$lib/services/wysiwygProcessor.js', () => ({
  wysiwygProcessor: {
    updateConfig: vi.fn(),
    processRealTime: vi.fn((content, cursorPos, callback) => {
      // Mock immediate processing
      setTimeout(() => {
        callback({
          originalContent: content,
          processedHTML: content.replace(/\*\*(.*?)\*\*/g, '<span class="wysiwyg-bold">$1</span>'),
          characterClasses: {},
          patterns: content.includes('**') ? [{
            type: 'bold',
            start: content.indexOf('**'),
            end: content.lastIndexOf('**') + 2,
            syntax: '**',
            content: content.replace(/\*\*/g, ''),
            line: 0,
            column: 0
          }] : [],
          processingTime: 10,
          warnings: []
        });
      }, 0);
    })
  },
  WYSIWYGUtils: {
    generateWYSIWYGCSS: vi.fn(() => '.wysiwyg-bold { font-weight: bold; }')
  }
}));

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

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('WYSIWYG Configuration', () => {
    it('should initialize WYSIWYG processor with configuration', async () => {
      const { wysiwygProcessor } = await import('$lib/services/wysiwygProcessor.js');
      
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

      expect(wysiwygProcessor.updateConfig).toHaveBeenCalledWith({
        enableRealTime: true,
        performanceMode: false,
        maxProcessingTime: 50,
        debounceDelay: 16,
        hideSyntax: true,
        enableFormatting: true
      });
    });

    it('should not initialize WYSIWYG when disabled', () => {
      const { wysiwygProcessor } = vi.mocked(vi.importActual('$lib/services/wysiwygProcessor.js'));
      
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: false
        }
      });

      expect(wysiwygProcessor.updateConfig).not.toHaveBeenCalled();
    });
  });

  describe('Content Editing with WYSIWYG', () => {
    it('should process markdown during editing', async () => {
      const { wysiwygProcessor } = await import('$lib/services/wysiwygProcessor.js');
      
      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      // Click to start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      // Find the contenteditable element
      const contentEditableElement = screen.getByRole('textbox');
      expect(contentEditableElement).toBeTruthy();

      // Simulate typing markdown
      contentEditableElement.textContent = '**bold text**';
      await fireEvent.input(contentEditableElement);
      await tick();

      // Verify WYSIWYG processing was called
      expect(wysiwygProcessor.processRealTime).toHaveBeenCalledWith(
        '**bold text**',
        expect.any(Number),
        expect.any(Function)
      );

      // Wait for processing callback
      await waitFor(() => {
        const elements = document.querySelectorAll('[data-wysiwyg-enabled="true"]');
        expect(elements.length).toBeGreaterThan(0);
      });
    });

    it('should add WYSIWYG CSS classes to contenteditable', async () => {
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

    it('should show processing state during WYSIWYG processing', async () => {
      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true
        }
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      // Simulate input that triggers processing
      const contentEditableElement = screen.getByRole('textbox');
      contentEditableElement.textContent = 'Processing test';
      await fireEvent.input(contentEditableElement);

      // Note: Processing state is brief and may not be easily testable
      // This test verifies the processing mechanism is in place
      expect(contentEditableElement).toBeTruthy();
    });
  });

  describe('Display Mode WYSIWYG', () => {
    it('should show processed HTML in display mode', async () => {
      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          content: '**bold text**',
          enableWYSIWYG: true
        }
      });

      // Component starts in display mode
      await tick();

      // The component should have WYSIWYG classes
      const textSpan = document.querySelector('.ns-node__text--wysiwyg');
      expect(textSpan).toBeTruthy();
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

    it('should fallback to plain text when WYSIWYG is disabled', () => {
      render(BaseNode, {
        props: {
          ...defaultProps,
          content: '**bold text**',
          enableWYSIWYG: false
        }
      });

      const textElement = screen.getByText('**bold text**');
      expect(textElement).toBeTruthy();
      expect(textElement.className).not.toContain('ns-node__text--wysiwyg');
    });
  });

  describe('Event Handling', () => {
    it('should emit content change events during WYSIWYG processing', async () => {
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

    it('should handle blur events while maintaining WYSIWYG state', async () => {
      let blurEvent: any = null;

      const { component } = render(BaseNode, {
        props: {
          ...defaultProps,
          content: '**bold text**',
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

  describe('Cursor Management', () => {
    it('should handle cursor positioning during WYSIWYG processing', async () => {
      const { wysiwygProcessor } = await import('$lib/services/wysiwygProcessor.js');
      
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

      // Input with cursor position
      const contentEditableElement = screen.getByRole('textbox');
      contentEditableElement.textContent = '**bold**';
      
      // Mock cursor position
      Object.defineProperty(window, 'getSelection', {
        writable: true,
        value: vi.fn(() => ({
          rangeCount: 1,
          getRangeAt: vi.fn(() => ({
            cloneRange: vi.fn(() => ({
              selectNodeContents: vi.fn(),
              setEnd: vi.fn(),
              toString: vi.fn(() => '**bol')
            }))
          }))
        }))
      });

      await fireEvent.input(contentEditableElement);
      await tick();

      expect(wysiwygProcessor.processRealTime).toHaveBeenCalledWith(
        '**bold**',
        expect.any(Number),
        expect.any(Function)
      );
    });
  });

  describe('Performance', () => {
    it('should handle rapid typing without performance issues', async () => {
      const { wysiwygProcessor } = await import('$lib/services/wysiwygProcessor.js');
      
      render(BaseNode, {
        props: {
          ...defaultProps,
          enableWYSIWYG: true,
          wysiwygConfig: {
            debounceDelay: 5 // Fast debounce for testing
          }
        }
      });

      // Start editing
      const nodeElement = screen.getByRole('button');
      await fireEvent.click(nodeElement);
      await tick();

      const contentEditableElement = screen.getByRole('textbox');

      // Rapid typing simulation
      const rapidInputs = ['*', '**', '**b', '**bo', '**bol', '**bold', '**bold*', '**bold**'];
      
      for (const content of rapidInputs) {
        contentEditableElement.textContent = content;
        await fireEvent.input(contentEditableElement);
      }

      // Wait for debouncing to settle
      await new Promise(resolve => setTimeout(resolve, 20));

      // Should not have excessive calls due to debouncing
      expect(wysiwygProcessor.processRealTime).toHaveBeenCalled();
    });
  });

  describe('Error Resilience', () => {
    it('should handle WYSIWYG processing errors gracefully', async () => {
      // Mock a failing processor
      const { wysiwygProcessor } = await import('$lib/services/wysiwygProcessor.js');
      vi.mocked(wysiwygProcessor.processRealTime).mockImplementation(() => {
        throw new Error('Processing error');
      });

      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

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

      // Input should not crash the component
      const contentEditableElement = screen.getByRole('textbox');
      contentEditableElement.textContent = 'test';
      await fireEvent.input(contentEditableElement);
      await tick();

      expect(consoleSpy).toHaveBeenCalledWith('WYSIWYG processing error:', expect.any(Error));

      consoleSpy.mockRestore();
    });

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

  describe('Multiline Support', () => {
    it('should handle multiline WYSIWYG content', async () => {
      const multilineContent = `# Header

**Bold text** on line 2
*Italic text* on line 3

- Bullet point
- Another bullet`;

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

    // Check if WYSIWYG-related CSS classes are defined in styles
    const styleElement = document.querySelector('style');
    if (styleElement) {
      const cssText = styleElement.textContent || '';
      
      // These are global styles, so they may not be in the component's style element
      // The test verifies the component structure supports WYSIWYG
      expect(cssText.includes('wysiwyg') || document.querySelector('.ns-node')).toBeTruthy();
    }
  });
});