import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createMockElementForView } from '$lib/design/components/cursor-positioning';
import { mapViewPositionToEditPosition } from '$lib/utils/view-edit-mapper';

describe('Click-to-Cursor Positioning', () => {
  beforeEach(() => {
    // Reset any global state if needed
    vi.clearAllMocks();
  });

  it('creates mock element and finds character position - plain text', () => {
    // Setup a mock view element
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'monospace';
    viewDiv.style.fontSize = '16px';
    viewDiv.style.lineHeight = '1.5';
    viewDiv.style.padding = '0px';
    document.body.appendChild(viewDiv);

    const content = 'Hello world';
    const mockElement = createMockElementForView(viewDiv, content);

    // Verify mock element was created with character spans
    const spans = mockElement.querySelectorAll('[data-position]');
    expect(spans.length).toBeGreaterThan(0);
    expect(spans[0].textContent).toBe('H');
    expect(spans[spans.length - 1].textContent).toBe('d');

    // Clean up
    mockElement.remove();
    viewDiv.remove();
  });

  it('maps plain text positions correctly', () => {
    const view = 'Hello world';
    const edit = 'Hello world';

    // No mapping needed when content is same
    expect(mapViewPositionToEditPosition(0, view, edit)).toBe(0);
    expect(mapViewPositionToEditPosition(5, view, edit)).toBe(5);
    expect(mapViewPositionToEditPosition(11, view, edit)).toBe(11);
  });

  it('maps positions correctly with bold syntax', () => {
    const view = 'Hello world';
    const edit = '**Hello** world';

    // Position 0 -> skip ** -> position 2
    expect(mapViewPositionToEditPosition(0, view, edit)).toBe(2);
    // Position 6 -> skip ** -> position 10
    expect(mapViewPositionToEditPosition(6, view, edit)).toBe(10);
  });

  it('handles emoji and complex Unicode in mock element', () => {
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'monospace';
    viewDiv.style.fontSize = '16px';
    viewDiv.style.padding = '0px';
    document.body.appendChild(viewDiv);

    const content = '😀 Hello';
    const mockElement = createMockElementForView(viewDiv, content);

    // Should handle emoji correctly
    const spans = mockElement.querySelectorAll('[data-position]');
    expect(spans.length).toBe(content.length);

    mockElement.remove();
    viewDiv.remove();
  });

  it('handles multi-line content in mock element', () => {
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'monospace';
    viewDiv.style.fontSize = '16px';
    viewDiv.style.padding = '0px';
    document.body.appendChild(viewDiv);

    const content = 'Line 1\nLine 2\nLine 3';
    const mockElement = createMockElementForView(viewDiv, content);

    // Should create spans for each character including newlines
    const spans = mockElement.querySelectorAll('[data-position]');
    const brs = mockElement.querySelectorAll('br');
    expect(spans.length).toBe(content.length);
    expect(brs.length).toBe(2); // Two newlines

    mockElement.remove();
    viewDiv.remove();
  });

  it('handles empty content gracefully', () => {
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'monospace';
    viewDiv.style.fontSize = '16px';
    viewDiv.style.padding = '0px';
    document.body.appendChild(viewDiv);

    const content = '';
    const mockElement = createMockElementForView(viewDiv, content);

    // Should not crash with empty content
    const spans = mockElement.querySelectorAll('[data-position]');
    expect(spans.length).toBe(0);

    mockElement.remove();
    viewDiv.remove();
  });

  it('handles special characters in content', () => {
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'monospace';
    viewDiv.style.fontSize = '16px';
    viewDiv.style.padding = '0px';
    document.body.appendChild(viewDiv);

    const content = 'Hello @mention #hashtag';
    const mockElement = createMockElementForView(viewDiv, content);

    // Should handle special characters
    const spans = mockElement.querySelectorAll('[data-position]');
    expect(spans.length).toBe(content.length);

    mockElement.remove();
    viewDiv.remove();
  });

  it('mock element copies view styles', () => {
    const viewDiv = document.createElement('div');
    viewDiv.style.fontFamily = 'Arial';
    viewDiv.style.fontSize = '20px';
    viewDiv.style.fontWeight = 'bold';
    viewDiv.style.lineHeight = '2';
    viewDiv.style.padding = '10px';
    document.body.appendChild(viewDiv);

    const content = 'Test';
    const mockElement = createMockElementForView(viewDiv, content);

    // Verify mock element has similar styling
    expect(mockElement.style.fontFamily).toContain('Arial');
    expect(mockElement.style.fontSize).toContain('20');
    expect(mockElement.style.fontWeight).toBe('bold');
    expect(mockElement.style.lineHeight).toContain('2');
    expect(mockElement.style.padding).toContain('10');

    mockElement.remove();
    viewDiv.remove();
  });

  it('handles position mapping with multiple formatting', () => {
    const view = 'Hello world';
    const edit = '**Hello** _world_';

    // Position 0 -> skip ** -> position 2
    expect(mapViewPositionToEditPosition(0, view, edit)).toBe(2);
    // Position 6 -> skip **, then space, then skip _ -> position 10
    expect(mapViewPositionToEditPosition(6, view, edit)).toBe(10);
  });
});
