/**
 * Unit tests for link navigation decision logic.
 *
 * Tests the decision logic from app-shell.svelte handleLinkClick:
 * - Click = in-place, Cmd+Click = new tab, Cmd+Shift+Click = other pane
 *
 * (The former chat-tab override was removed alongside the ephemeral ChatPanel /
 *  chatStore in a later follow-up — every conversation is now an ai-chat node,
 *  so links behave the same in every tab.)
 */

import { describe, it, expect } from 'vitest';

/** Replicates the navigation decision logic from app-shell.svelte. */
function computeNavigation(modifierPressed: boolean, shiftPressed: boolean) {
  const openInOtherPane = modifierPressed && shiftPressed;
  const openInNewTab = modifierPressed && !shiftPressed;
  return { openInOtherPane, openInNewTab };
}

describe('Link Navigation', () => {
  it('regular click navigates in-place', () => {
    const result = computeNavigation(false, false);
    expect(result.openInNewTab).toBe(false);
    expect(result.openInOtherPane).toBe(false);
  });

  it('Cmd+Click opens in new tab', () => {
    const result = computeNavigation(true, false);
    expect(result.openInNewTab).toBe(true);
    expect(result.openInOtherPane).toBe(false);
  });

  it('Cmd+Shift+Click opens in other pane', () => {
    const result = computeNavigation(true, true);
    expect(result.openInOtherPane).toBe(true);
    expect(result.openInNewTab).toBe(false);
  });
});
