/**
 * pro-teaser-pill component.
 *
 * The community/upsell surface. It must render a static CTA and never touch a Pro
 * daemon — clicking only opens a marketing URL, so the community build stays inert
 * with respect to sync.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import ProTeaserPill from '$lib/components/pro-teaser-pill.svelte';

describe('ProTeaserPill', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders a static upgrade CTA', () => {
    const { getByRole, getByText } = render(ProTeaserPill);
    expect(getByText('Upgrade to Pro')).toBeTruthy();
    expect(getByRole('button', { name: /upgrade to nodespace pro/i })).toBeTruthy();
  });

  it('never invokes a daemon command, even on click', async () => {
    const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);
    const { getByRole } = render(ProTeaserPill);

    await fireEvent.click(getByRole('button', { name: /upgrade to nodespace pro/i }));

    // The CTA opens a marketing URL, never a Tauri/daemon command.
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(openSpy).toHaveBeenCalledTimes(1);
    expect(openSpy.mock.calls[0][0]).toContain('nodespace.ai');
  });
});
