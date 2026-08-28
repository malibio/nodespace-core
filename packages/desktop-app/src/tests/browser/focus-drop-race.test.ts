/**
 * Focus-Drop Race Regression Tests (Browser Mode)
 *
 * Regression coverage for a bug where the first character typed immediately after
 * (a) clicking into an empty node's view-mode content, or (b) pressing Enter to create
 * a new sibling node, was silently dropped.
 *
 * Root cause: focus was deferred via `requestAnimationFrame` (in
 * `$lib/actions/position-cursor.ts`, driving both transitions -- click-to-edit-at-position
 * and Enter-creates-sibling both resolve through `focusManager.focusNodeAtPosition` /
 * `focusManager.focusNodeFromInheritedType`, both consumed by this one action) and, in a
 * secondary path, via a bare `setTimeout(..., 0)` (in base-node.svelte's Enter/Space
 * handler on the read-only view-mode div). Both defer focus by waiting for the *next
 * animation frame* or *next macrotask* -- up to ~16ms -- which a fast-following keystroke
 * (real typing, or Playwright's own 15ms per-keystroke delay) can arrive before. With
 * nothing focused yet, that keystroke has nowhere to land and is lost.
 *
 * The fix defers focus via Svelte's `tick()` instead, which resolves on the next
 * *microtask* -- always before the browser can dispatch another keydown, even one
 * typed as fast as physically/synthetically possible.
 *
 * Happy-DOM cannot be trusted to prove this: it doesn't reproduce a real engine's
 * relative scheduling of rAF vs. microtasks vs. keyboard event dispatch (the existing
 * Happy-DOM unit tests for this action have to install a *synchronous* rAF stand-in to
 * make assertions possible at all -- see position-cursor.test.ts -- which sidesteps the
 * exact timing question this bug is about). These tests run in real Chromium via
 * Playwright to measure the real scheduling relationship.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { tick } from 'svelte';
import { positionCursor, type CursorPosition } from '$lib/actions/position-cursor';
import { TextareaController } from '$lib/design/components/textarea-controller';

function makeController(textarea: HTMLTextAreaElement): TextareaController {
  return new TextareaController(textarea, 'test-node', 'text', 'default', {
    contentChanged: () => {},
    focus: () => {},
    blur: () => {},
    createNewNode: () => {},
    indentNode: () => {},
    outdentNode: () => {},
    navigateArrow: () => {},
    combineWithPrevious: () => {},
    deleteNode: () => {},
    directSlashCommand: () => {},
    triggerDetected: () => {},
    triggerHidden: () => {},
    nodeReferenceSelected: () => {},
    slashCommandDetected: () => {},
    slashCommandHidden: () => {},
    slashCommandSelected: () => {},
    nodeTypeConversionDetected: () => {}
  });
}

describe('Deferred-focus scheduling (Browser Mode)', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  it('a real requestAnimationFrame callback has NOT fired after one microtask turn', async () => {
    // This is the empirical basis for the whole fix: it proves that the deadline a
    // fast-following keystroke has to beat (one microtask turn -- the same wait
    // Svelte's tick() performs) is strictly shorter than requestAnimationFrame's
    // schedule in a real engine. If this ever stopped being true, rAF-deferred focus
    // would stop being racy -- but it is true, which is exactly why the original code
    // dropped keystrokes.
    let rafFired = false;
    requestAnimationFrame(() => {
      rafFired = true;
    });

    await Promise.resolve(); // one microtask turn

    expect(rafFired).toBe(false);
  });

  it('a microtask-deferred callback (what tick() resolves through) HAS run after one microtask turn', async () => {
    let ran = false;
    await Promise.resolve().then(() => {
      ran = true;
    });

    expect(ran).toBe(true);
  });

  it('a real setTimeout(fn, 0) callback has NOT fired after one microtask turn', async () => {
    // base-node.svelte's view-mode Enter/Space handler (the file:line the original issue
    // cited as the likely mechanism) deferred focus via a bare `setTimeout(fn, 0)`
    // rather than requestAnimationFrame -- a *macrotask*, not tied to the next paint at
    // all, but still strictly later than a single microtask turn. This is the empirical
    // basis for that half of the fix: it confirms the same deadline problem applies to
    // setTimeout(0), not just rAF, so deferring via tick() there is the correct fix and
    // not just an incidental match to the rAF case above.
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 0);

    await Promise.resolve(); // one microtask turn

    expect(fired).toBe(false);
  });
});

describe('positionCursor action - focus lands before the next keydown could arrive (Browser Mode)', () => {
  let textarea: HTMLTextAreaElement;
  let controller: TextareaController;

  beforeEach(() => {
    document.body.innerHTML = '';
    textarea = document.createElement('textarea');
    textarea.value = 'hello world';
    document.body.appendChild(textarea);
    controller = makeController(textarea);
  });

  it('focuses the textarea by the time a single tick() resolves (click-to-edit-at-position path)', async () => {
    // Nothing is focused yet -- simulates the outgoing view-mode element having just
    // been removed from the DOM as part of the view -> edit transition.
    expect(document.activeElement).not.toBe(textarea);

    const data: CursorPosition = { type: 'absolute', position: 5 };
    positionCursor(textarea, { data, controller });

    // Deliberately await only tick() -- not a macrotask, not requestAnimationFrame.
    // This is the real deadline a fast-following keystroke has to beat.
    await tick();

    expect(document.activeElement).toBe(textarea);
    expect(textarea.selectionStart).toBe(5);
  });

  it('focuses the textarea by the time a single tick() resolves (Enter-creates-sibling / inherited-type path)', async () => {
    expect(document.activeElement).not.toBe(textarea);

    const data: CursorPosition = { type: 'inherited-type', position: 0 };
    positionCursor(textarea, { data, controller });

    await tick();

    expect(document.activeElement).toBe(textarea);
  });

  it('a keystroke dispatched right after positionCursor() (no macrotask wait) is NOT silently lost: the textarea is already the dispatch target', async () => {
    const data: CursorPosition = { type: 'absolute', position: 0 };
    positionCursor(textarea, { data, controller });
    await tick();

    // Simulate "the next keystroke": a real KeyboardEvent + character insertion,
    // dispatched at the earliest possible moment (right after the microtask-based
    // focus lands, with no artificial delay). Because activeElement is already the
    // textarea, the browser routes this at the correct target instead of dropping it
    // on the floor (or landing it on whatever the previously-focused element was).
    expect(document.activeElement).toBe(textarea);

    textarea.dispatchEvent(new KeyboardEvent('keydown', { key: 'U', bubbles: true }));
    // Synthetic KeyboardEvents don't trigger the browser's native text-insertion
    // pipeline, so insertion is simulated explicitly here -- the assertion that
    // matters for this regression is the activeElement check above: an unfocused
    // target is the actual mechanism by which the real bug dropped a character.
    textarea.value = 'U' + textarea.value;
    textarea.setSelectionRange(1, 1);

    expect(textarea.value.startsWith('U')).toBe(true);
  });
});

