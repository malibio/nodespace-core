/**
 * enable-sync-pill component.
 *
 * Rendered only for a Pro daemon whose active database has sync disabled. Clicking
 * starts the interactive sign-in — the concrete "turn sync on" action available
 * today (a dedicated per-database enable command is backend follow-up).
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import EnableSyncPill from '$lib/components/enable-sync-pill.svelte';

describe('EnableSyncPill', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue('attempt-1');
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders the enable-sync CTA', () => {
    const { getByRole, getByText } = render(EnableSyncPill);
    expect(getByText('Enable sync')).toBeTruthy();
    expect(getByRole('button', { name: /enable cloud sync/i })).toBeTruthy();
  });

  it('starts sign-in on click via pro_initiate_oauth', async () => {
    const { getByRole } = render(EnableSyncPill);
    await fireEvent.click(getByRole('button', { name: /enable cloud sync/i }));
    expect(mockInvoke).toHaveBeenCalledWith('pro_initiate_oauth');
  });
});
