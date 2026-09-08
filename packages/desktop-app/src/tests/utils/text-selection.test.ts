import { describe, it, expect } from 'vitest';
import { isActiveTextSelection } from '$lib/utils/text-selection';

/**
 * Guards the view-container click handler's decision: focus (click-to-edit) only
 * on a collapsed selection; leave a real drag-selection alone so it can be copied
 * across node boundaries.
 */
describe('isActiveTextSelection', () => {
  it('is false for no selection', () => {
    expect(isActiveTextSelection(null)).toBe(false);
  });

  it('is false for a collapsed selection (a genuine click)', () => {
    const el = document.createElement('div');
    el.textContent = 'hello world';
    document.body.appendChild(el);
    const range = document.createRange();
    range.setStart(el.firstChild!, 3);
    range.collapse(true);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);
    expect(sel.isCollapsed).toBe(true);
    expect(isActiveTextSelection(sel)).toBe(false);
    el.remove();
  });

  it('is true for a non-empty range (a drag-selection)', () => {
    const el = document.createElement('div');
    el.textContent = 'hello world';
    document.body.appendChild(el);
    const range = document.createRange();
    range.setStart(el.firstChild!, 0);
    range.setEnd(el.firstChild!, 5);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);
    expect(sel.isCollapsed).toBe(false);
    expect(isActiveTextSelection(sel)).toBe(true);
    el.remove();
  });

  it('is false when there are no ranges', () => {
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    expect(isActiveTextSelection(sel)).toBe(false);
  });
});
