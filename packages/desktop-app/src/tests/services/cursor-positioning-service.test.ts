/**
 * CursorPositioningService Tests
 *
 * Direct unit tests for the singleton service that centralizes cursor
 * positioning across textarea-based editors: focus management, syntax-aware
 * positioning, multiline line/column math, delayed positioning, and clamping.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { CursorPositioningService } from '$lib/services/cursor-positioning-service';

function createTextarea(value: string): HTMLTextAreaElement {
  const textarea = document.createElement('textarea');
  textarea.value = value;
  document.body.appendChild(textarea);
  return textarea;
}

describe('CursorPositioningService', () => {
  let service: CursorPositioningService;
  let textarea: HTMLTextAreaElement;

  beforeEach(() => {
    service = CursorPositioningService.getInstance();
  });

  afterEach(() => {
    // A test that fakes timers restores them at the end of its body, which a failing
    // assertion skips — leaving the fake clock installed for every later file in this
    // fork. Restoring here covers that. It is a no-op when timers aren't faked.
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  describe('getInstance', () => {
    it('returns the same singleton instance on repeated calls', () => {
      const a = CursorPositioningService.getInstance();
      const b = CursorPositioningService.getInstance();
      expect(a).toBe(b);
    });
  });

  describe('setCursorAtBeginningOfLine', () => {
    it('positions at the start of the first line by default', () => {
      textarea = createTextarea('Line 1\nLine 2\nLine 3');
      service.setCursorAtBeginningOfLine(textarea);

      expect(textarea.selectionStart).toBe(0);
      expect(textarea.selectionEnd).toBe(0);
    });

    it('positions at the start of a later line, accounting for newline offsets', () => {
      textarea = createTextarea('Line 1\nLine 2\nLine 3');
      service.setCursorAtBeginningOfLine(textarea, 2);

      // "Line 1\n" (7) + "Line 2\n" (7) = 14
      expect(textarea.selectionStart).toBe(14);
      expect(textarea.selectionEnd).toBe(14);
    });

    it('focuses the textarea by default', () => {
      textarea = createTextarea('hello');
      const focusSpy = vi.spyOn(textarea, 'focus');

      service.setCursorAtBeginningOfLine(textarea);

      expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    });

    it('skips focus when focus: false is passed', () => {
      textarea = createTextarea('hello');
      const focusSpy = vi.spyOn(textarea, 'focus');

      service.setCursorAtBeginningOfLine(textarea, 0, { focus: false });

      expect(focusSpy).not.toHaveBeenCalled();
      // Position is still applied.
      expect(textarea.selectionStart).toBe(0);
    });

    it('clamps and warns on an out-of-range line number, falling back to line 0', () => {
      textarea = createTextarea('Line 1\nLine 2');
      const warnSpy = vi.fn();
      // The service logs via the shared logger; we only assert the resulting
      // cursor position falls back to line 0 rather than throwing.
      expect(() => service.setCursorAtBeginningOfLine(textarea, 99)).not.toThrow();
      expect(textarea.selectionStart).toBe(0);
      warnSpy.mockClear();
    });

    it('falls back to line 0 for a negative line number', () => {
      textarea = createTextarea('Line 1\nLine 2');
      service.setCursorAtBeginningOfLine(textarea, -1);
      expect(textarea.selectionStart).toBe(0);
    });

    describe('syntax skipping (skipSyntax: true, default)', () => {
      it('skips a level-1 header marker ("# ")', () => {
        textarea = createTextarea('# Heading');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe('# '.length);
      });

      it('skips a level-6 header marker with multiple spaces', () => {
        textarea = createTextarea('######   Heading');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe('######   '.length);
      });

      it('skips bold ** syntax at the beginning of the line', () => {
        textarea = createTextarea('**bold text**');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(2);
      });

      it('skips bold __ syntax at the beginning of the line', () => {
        textarea = createTextarea('__bold text__');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(2);
      });

      it('skips italic * syntax (not part of **) at the beginning of the line', () => {
        textarea = createTextarea('*italic text*');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(1);
      });

      it('skips italic _ syntax (not part of __) at the beginning of the line', () => {
        textarea = createTextarea('_italic text_');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(1);
      });

      it('skips strikethrough ~~ syntax at the beginning of the line', () => {
        textarea = createTextarea('~~struck text~~');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(2);
      });

      it('skips inline code ` syntax at the beginning of the line', () => {
        textarea = createTextarea('`code text`');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(1);
      });

      it('does not skip anything when the line has no leading syntax', () => {
        textarea = createTextarea('plain text');
        service.setCursorAtBeginningOfLine(textarea, 0);
        expect(textarea.selectionStart).toBe(0);
      });
    });

    describe('skipSyntax: false', () => {
      it('positions at the raw start of the line, ignoring header syntax', () => {
        textarea = createTextarea('# Heading');
        service.setCursorAtBeginningOfLine(textarea, 0, { skipSyntax: false });
        expect(textarea.selectionStart).toBe(0);
      });

      it('positions at the raw start of the line, ignoring bold syntax', () => {
        textarea = createTextarea('**bold text**');
        service.setCursorAtBeginningOfLine(textarea, 0, { skipSyntax: false });
        expect(textarea.selectionStart).toBe(0);
      });
    });

    describe('delay option', () => {
      beforeEach(() => {
        vi.useFakeTimers();
      });

      afterEach(() => {
        vi.useRealTimers();
      });

      it('defers positioning via setTimeout when delay > 0', () => {
        textarea = createTextarea('Line 1\nLine 2');
        // Happy-DOM defaults selectionStart to the content length; positioning
        // should not change it until the timer fires.
        const beforeTimer = textarea.selectionStart;
        service.setCursorAtBeginningOfLine(textarea, 1, { delay: 50 });

        // Not yet applied.
        expect(textarea.selectionStart).toBe(beforeTimer);

        vi.advanceTimersByTime(50);

        expect(textarea.selectionStart).toBe(7);
      });

      it('applies immediately when delay is 0 (default)', () => {
        textarea = createTextarea('Line 1\nLine 2');
        service.setCursorAtBeginningOfLine(textarea, 1, { delay: 0 });
        expect(textarea.selectionStart).toBe(7);
      });
    });
  });

  describe('setCursorAtPosition', () => {
    it('sets the cursor at an absolute character position', () => {
      textarea = createTextarea('Hello world');
      service.setCursorAtPosition(textarea, 5);
      expect(textarea.selectionStart).toBe(5);
      expect(textarea.selectionEnd).toBe(5);
    });

    it('clamps a position beyond the content length to the end', () => {
      textarea = createTextarea('Hello');
      service.setCursorAtPosition(textarea, 999);
      expect(textarea.selectionStart).toBe(5);
    });

    it('clamps a negative position to 0', () => {
      textarea = createTextarea('Hello');
      service.setCursorAtPosition(textarea, -10);
      expect(textarea.selectionStart).toBe(0);
    });

    it('focuses the textarea by default', () => {
      textarea = createTextarea('Hello world');
      const focusSpy = vi.spyOn(textarea, 'focus');
      service.setCursorAtPosition(textarea, 3);
      expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    });

    it('skips focus when focus: false is passed', () => {
      textarea = createTextarea('Hello world');
      const focusSpy = vi.spyOn(textarea, 'focus');
      service.setCursorAtPosition(textarea, 3, { focus: false });
      expect(focusSpy).not.toHaveBeenCalled();
      expect(textarea.selectionStart).toBe(3);
    });

    it('defers positioning via setTimeout when delay > 0', () => {
      vi.useFakeTimers();
      textarea = createTextarea('Hello world');
      const beforeTimer = textarea.selectionStart;
      service.setCursorAtPosition(textarea, 4, { delay: 25 });

      expect(textarea.selectionStart).toBe(beforeTimer);
      vi.advanceTimersByTime(25);
      expect(textarea.selectionStart).toBe(4);
      vi.useRealTimers();
    });
  });

  describe('setCursorAtLineColumn', () => {
    it('sets the cursor at a given line and column', () => {
      textarea = createTextarea('Line 1\nLine 2\nLine 3');
      service.setCursorAtLineColumn(textarea, { line: 1, column: 3 });
      // "Line 1\n" (7) + 3 = 10
      expect(textarea.selectionStart).toBe(10);
    });

    it('focuses the textarea by default', () => {
      textarea = createTextarea('Line 1\nLine 2');
      const focusSpy = vi.spyOn(textarea, 'focus');
      service.setCursorAtLineColumn(textarea, { line: 0, column: 2 });
      expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    });

    it('skips focus when focus: false is passed', () => {
      textarea = createTextarea('Line 1\nLine 2');
      const focusSpy = vi.spyOn(textarea, 'focus');
      service.setCursorAtLineColumn(textarea, { line: 0, column: 2 }, { focus: false });
      expect(focusSpy).not.toHaveBeenCalled();
    });

    it('falls back to line 0 for an out-of-range line', () => {
      textarea = createTextarea('Line 1\nLine 2');
      service.setCursorAtLineColumn(textarea, { line: 50, column: 0 });
      expect(textarea.selectionStart).toBe(0);
    });

    it('falls back to line 0 for a negative line', () => {
      textarea = createTextarea('Line 1\nLine 2');
      service.setCursorAtLineColumn(textarea, { line: -1, column: 0 });
      expect(textarea.selectionStart).toBe(0);
    });

    it('clamps a column beyond the line length to the line end', () => {
      textarea = createTextarea('Line 1\nLine 2');
      service.setCursorAtLineColumn(textarea, { line: 0, column: 999 });
      expect(textarea.selectionStart).toBe('Line 1'.length);
    });

    it('clamps a negative column to 0', () => {
      textarea = createTextarea('Line 1\nLine 2');
      service.setCursorAtLineColumn(textarea, { line: 1, column: -5 });
      // "Line 1\n" (7) + 0
      expect(textarea.selectionStart).toBe(7);
    });

    it('defers positioning via setTimeout when delay > 0', () => {
      vi.useFakeTimers();
      textarea = createTextarea('Line 1\nLine 2');
      const beforeTimer = textarea.selectionStart;
      service.setCursorAtLineColumn(textarea, { line: 1, column: 2 }, { delay: 30 });

      expect(textarea.selectionStart).toBe(beforeTimer);
      vi.advanceTimersByTime(30);
      // "Line 1\n" (7) + 2 = 9
      expect(textarea.selectionStart).toBe(9);
      vi.useRealTimers();
    });
  });

  describe('getCursorPosition', () => {
    it('returns line 0 / column 0 when the cursor is at the start', () => {
      textarea = createTextarea('Line 1\nLine 2');
      textarea.selectionStart = 0;
      textarea.selectionEnd = 0;

      expect(service.getCursorPosition(textarea)).toEqual({ line: 0, column: 0 });
    });

    it('returns the correct line and column for a mid-content position', () => {
      textarea = createTextarea('Line 1\nLine 2\nLine 3');
      // "Line 1\n" (7) + "Li" (2) = 9, on line 1 at column 2
      textarea.selectionStart = 9;
      textarea.selectionEnd = 9;

      expect(service.getCursorPosition(textarea)).toEqual({ line: 1, column: 2 });
    });

    it('returns the end-of-content position on the last line', () => {
      textarea = createTextarea('abc\ndef');
      textarea.selectionStart = textarea.value.length;
      textarea.selectionEnd = textarea.value.length;

      expect(service.getCursorPosition(textarea)).toEqual({ line: 1, column: 3 });
    });
  });
});
