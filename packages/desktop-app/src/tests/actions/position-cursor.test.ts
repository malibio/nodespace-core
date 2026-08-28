/**
 * Tests for positionCursor Svelte action
 *
 * Verifies reactive cursor positioning behavior with different cursor position types.
 *
 * The action defers its work with Svelte's tick() (not requestAnimationFrame — see the
 * "Why tick(), not requestAnimationFrame" doc comment in position-cursor.ts for why that
 * changed) so every test that observes a positioning side effect awaits tick() itself
 * after invoking the action, giving the action's own internal tick() a chance to resolve
 * and run its switch statement before assertions run.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { tick } from 'svelte';
import { positionCursor, type CursorPosition } from '$lib/actions/position-cursor';
import { TextareaController } from '$lib/design/components/textarea-controller';

describe('positionCursor action', () => {
  let textarea: HTMLTextAreaElement;
  let controller: TextareaController;

  beforeEach(() => {
    // Create textarea element
    textarea = document.createElement('textarea');
    textarea.value = 'Line 1\nLine 2\nLine 3';
    document.body.appendChild(textarea);

    // Create controller
    controller = new TextareaController(textarea, 'test-node', 'text', 'default', {
      contentChanged: vi.fn(),
      focus: vi.fn(),
      blur: vi.fn(),
      createNewNode: vi.fn(),
      indentNode: vi.fn(),
      outdentNode: vi.fn(),
      navigateArrow: vi.fn(),
      combineWithPrevious: vi.fn(),
      deleteNode: vi.fn(),
      directSlashCommand: vi.fn(),
      triggerDetected: vi.fn(),
      triggerHidden: vi.fn(),
      nodeReferenceSelected: vi.fn(),
      slashCommandDetected: vi.fn(),
      slashCommandHidden: vi.fn(),
      slashCommandSelected: vi.fn(),
      nodeTypeConversionDetected: vi.fn()
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    controller.destroy();
    document.body.removeChild(textarea);
  });

  it('should apply default cursor position', async () => {
    const spy = vi.spyOn(controller, 'positionCursorAtLineBeginning');

    const data: CursorPosition = { type: 'default', skipSyntax: true };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith(0, true);
  });

  it('should apply absolute cursor position', async () => {
    const spy = vi.spyOn(controller, 'setCursorPosition');

    const data: CursorPosition = { type: 'absolute', position: 10 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith(10);
  });

  it('should apply arrow navigation cursor position', async () => {
    const spy = vi.spyOn(controller, 'enterFromArrowNavigation');

    const data: CursorPosition = { type: 'arrow-navigation', direction: 'up', pixelOffset: 50 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith('up', 50);
  });

  it('should apply line-column cursor position', async () => {
    const spy = vi.spyOn(controller, 'positionCursorAtLineBeginning');

    const data: CursorPosition = { type: 'line-column', line: 2, skipSyntax: false };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith(2, false);
  });

  it('should skip positioning when data is null', async () => {
    const spy = vi.spyOn(controller, 'setCursorPosition');

    positionCursor(textarea, { data: null, controller });
    await tick();

    expect(spy).not.toHaveBeenCalled();
  });

  it('should skip positioning when controller is null', async () => {
    const data: CursorPosition = { type: 'absolute', position: 10 };

    // Should not throw
    expect(() => {
      positionCursor(textarea, { data, controller: null });
    }).not.toThrow();
    await tick();
  });

  it('should not re-apply same position (duplicate prevention)', async () => {
    const spy = vi.spyOn(controller, 'setCursorPosition');
    const data: CursorPosition = { type: 'absolute', position: 10 };

    const action = positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledTimes(1);

    // Update with same data
    action.update({ data, controller });
    await tick();

    // Should still be called only once (no duplicate)
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('should re-apply position after data becomes null', async () => {
    const spy = vi.spyOn(controller, 'setCursorPosition');
    const data: CursorPosition = { type: 'absolute', position: 10 };

    const action = positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledTimes(1);

    // Clear data
    action.update({ data: null, controller });
    await tick();

    // Re-apply same position
    action.update({ data, controller });
    await tick();

    // Should be called twice (re-application allowed after null)
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('should handle different position types in updates', async () => {
    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');
    const setLineSpy = vi.spyOn(controller, 'positionCursorAtLineBeginning');
    const arrowNavSpy = vi.spyOn(controller, 'enterFromArrowNavigation');

    const data1: CursorPosition = { type: 'absolute', position: 5 };
    const action = positionCursor(textarea, { data: data1, controller });
    await tick();

    expect(setCursorSpy).toHaveBeenCalledWith(5);

    // Update to different type
    const data2: CursorPosition = { type: 'line-column', line: 1, skipSyntax: true };
    action.update({ data: data2, controller });
    await tick();

    expect(setLineSpy).toHaveBeenCalledWith(1, true);

    // Update to arrow navigation
    const data3: CursorPosition = { type: 'arrow-navigation', direction: 'down', pixelOffset: 100 };
    action.update({ data: data3, controller });
    await tick();

    expect(arrowNavSpy).toHaveBeenCalledWith('down', 100);
  });

  it('should apply node-type-conversion cursor position', async () => {
    const focusSpy = vi.spyOn(controller, 'focus');
    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');

    const data: CursorPosition = { type: 'node-type-conversion', position: 15 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(focusSpy).toHaveBeenCalled();
    expect(setCursorSpy).toHaveBeenCalledWith(15);
  });

  it('should retry node-type-conversion if cursor position changes', async () => {
    // Fake ONLY the timer the retry logic uses. tick()'s own Promise.resolve() microtask
    // is unaffected by faking setTimeout/clearTimeout.
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    // Mock setCursorPosition to NOT actually change the cursor position
    // This simulates the scenario where a component switch resets the cursor
    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition').mockImplementation(() => {
      // Do nothing - simulates cursor being reset by component
    });

    const data: CursorPosition = { type: 'node-type-conversion', position: 20 };

    // Simulate textarea being focused with wrong cursor position
    textarea.focus();
    textarea.selectionStart = 5; // Different from target position
    textarea.selectionEnd = 5;

    positionCursor(textarea, { data, controller });
    await tick();

    // First call happens once the action's own tick() resolves
    expect(setCursorSpy).toHaveBeenCalledWith(20);

    // Wait for retry timeout (10ms)
    vi.advanceTimersByTime(10);

    // Should retry because selectionStart (5) !== data.position (20)
    expect(setCursorSpy).toHaveBeenCalledTimes(2);
    expect(setCursorSpy).toHaveBeenCalledWith(20);
  });

  it('should not retry node-type-conversion if cursor position is correct', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');
    const data: CursorPosition = { type: 'node-type-conversion', position: 20 };

    // Simulate textarea being focused with correct cursor position
    textarea.focus();
    textarea.selectionStart = 20; // Same as target position
    textarea.selectionEnd = 20;

    positionCursor(textarea, { data, controller });
    await tick();

    expect(setCursorSpy).toHaveBeenCalledWith(20);

    // Wait for retry timeout (10ms)
    vi.advanceTimersByTime(10);

    // Should NOT retry because position is already correct
    expect(setCursorSpy).toHaveBeenCalledTimes(1);
  });

  it('should not retry node-type-conversion if element is not a textarea', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');
    const data: CursorPosition = { type: 'node-type-conversion', position: 20 };

    // Simulate a non-textarea element being focused
    const div = document.createElement('div');
    div.focus();

    positionCursor(textarea, { data, controller });
    await tick();

    expect(setCursorSpy).toHaveBeenCalledWith(20);

    // Wait for retry timeout (10ms)
    vi.advanceTimersByTime(10);

    // Should NOT retry because activeElement is not a textarea
    expect(setCursorSpy).toHaveBeenCalledTimes(1);
  });

  it('should apply inherited-type cursor position', async () => {
    const focusSpy = vi.spyOn(controller, 'focus');
    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');

    const data: CursorPosition = { type: 'inherited-type', position: 8 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(focusSpy).toHaveBeenCalled();
    expect(setCursorSpy).toHaveBeenCalledWith(8);
  });

  it('should retry inherited-type if cursor position changes', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    // Mock setCursorPosition to NOT actually change the cursor position
    // This simulates the scenario where a component switch resets the cursor
    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition').mockImplementation(() => {
      // Do nothing - simulates cursor being reset by component
    });

    const data: CursorPosition = { type: 'inherited-type', position: 12 };

    // Simulate textarea being focused with wrong cursor position
    textarea.focus();
    textarea.selectionStart = 3; // Different from target position
    textarea.selectionEnd = 3;

    positionCursor(textarea, { data, controller });
    await tick();

    expect(setCursorSpy).toHaveBeenCalledWith(12);

    // Wait for retry timeout (10ms)
    vi.advanceTimersByTime(10);

    // Should retry because selectionStart (3) !== data.position (12)
    expect(setCursorSpy).toHaveBeenCalledTimes(2);
    expect(setCursorSpy).toHaveBeenCalledWith(12);
  });

  it('should not retry inherited-type if cursor position is correct', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });

    const setCursorSpy = vi.spyOn(controller, 'setCursorPosition');
    const data: CursorPosition = { type: 'inherited-type', position: 12 };

    // Simulate textarea being focused with correct cursor position
    textarea.focus();
    textarea.selectionStart = 12; // Same as target position
    textarea.selectionEnd = 12;

    positionCursor(textarea, { data, controller });
    await tick();

    expect(setCursorSpy).toHaveBeenCalledWith(12);

    // Wait for retry timeout (10ms)
    vi.advanceTimersByTime(10);

    // Should NOT retry because position is already correct
    expect(setCursorSpy).toHaveBeenCalledTimes(1);
  });

  it('should use skipSyntax default value (true) for default position', async () => {
    const spy = vi.spyOn(controller, 'positionCursorAtLineBeginning');

    // Omit skipSyntax to test default
    const data: CursorPosition = { type: 'default' };
    positionCursor(textarea, { data, controller });
    await tick();

    // Should default to true
    expect(spy).toHaveBeenCalledWith(0, true);
  });

  it('should use skipSyntax default value (true) for line-column position', async () => {
    const spy = vi.spyOn(controller, 'positionCursorAtLineBeginning');

    // Omit skipSyntax to test default
    const data: CursorPosition = { type: 'line-column', line: 1 };
    positionCursor(textarea, { data, controller });
    await tick();

    // Should default to true
    expect(spy).toHaveBeenCalledWith(1, true);
  });

  it('should handle skipSyntax: false explicitly for default position', async () => {
    const spy = vi.spyOn(controller, 'positionCursorAtLineBeginning');

    const data: CursorPosition = { type: 'default', skipSyntax: false };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith(0, false);
  });

  it('should handle arrow navigation with down direction', async () => {
    const spy = vi.spyOn(controller, 'enterFromArrowNavigation');

    const data: CursorPosition = { type: 'arrow-navigation', direction: 'down', pixelOffset: 75 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(spy).toHaveBeenCalledWith('down', 75);
  });
});

describe('positionCursor action - focus-drop race regression', () => {
  let textarea: HTMLTextAreaElement;
  let controller: TextareaController;

  beforeEach(() => {
    textarea = document.createElement('textarea');
    document.body.appendChild(textarea);

    controller = new TextareaController(textarea, 'test-node', 'text', 'default', {
      contentChanged: vi.fn(),
      focus: vi.fn(),
      blur: vi.fn(),
      createNewNode: vi.fn(),
      indentNode: vi.fn(),
      outdentNode: vi.fn(),
      navigateArrow: vi.fn(),
      combineWithPrevious: vi.fn(),
      deleteNode: vi.fn(),
      directSlashCommand: vi.fn(),
      triggerDetected: vi.fn(),
      triggerHidden: vi.fn(),
      nodeReferenceSelected: vi.fn(),
      slashCommandDetected: vi.fn(),
      slashCommandHidden: vi.fn(),
      slashCommandSelected: vi.fn(),
      nodeTypeConversionDetected: vi.fn()
    });
  });

  afterEach(() => {
    controller.destroy();
    document.body.removeChild(textarea);
  });

  // Regression test for a dropped-first-keystroke bug: focus used to be deferred via
  // requestAnimationFrame, which waits up to ~16ms for the next paint. A keystroke
  // dispatched before that callback ran had nothing focused to land on and was silently
  // lost. tick() resolves on the next microtask instead, which always completes before
  // the event loop hands control back to dispatch another keydown — so a keystroke
  // "typed" as soon as this promise-based action call resolves must never be able to
  // observe an unfocused target.
  it('focuses the textarea before a same-microtask-turn caller can observe it unfocused', async () => {
    expect(document.activeElement).not.toBe(textarea);

    const data: CursorPosition = { type: 'absolute', position: 0 };
    positionCursor(textarea, { data, controller });

    // No macrotask (setTimeout/rAF) is awaited here on purpose: only a microtask-level
    // tick(), the same primitive real keydown dispatch would already have run past by
    // the time it fires. If focus were still deferred to rAF, activeElement would still
    // be unfocused at this point and the assertion below would fail.
    await tick();

    expect(document.activeElement).toBe(textarea);
  });

  it('positions the cursor at the requested absolute offset synchronously relative to a following microtask', async () => {
    textarea.value = 'hello world';

    const data: CursorPosition = { type: 'absolute', position: 5 };
    positionCursor(textarea, { data, controller });
    await tick();

    expect(document.activeElement).toBe(textarea);
    expect(textarea.selectionStart).toBe(5);
    expect(textarea.selectionEnd).toBe(5);
  });
});
