/**
 * enable-sync-pill component.
 *
 * Rendered for the `consent` variant — a Pro daemon whose active database is
 * signed in but not yet opted into sync. Clicking (re)opens the first-Pro
 * data-sharing consent modal (which owns the irreversible publish choice) rather
 * than pushing anything itself.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () => mockTauriCore());
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import EnableSyncPill from '$lib/components/enable-sync-pill.svelte';
import { proSync } from '$lib/stores/pro-sync.svelte';

describe('EnableSyncPill', () => {
  beforeEach(() => {
    proSync.consentPromptOpen = false;
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders the turn-on-sync CTA', () => {
    const { getByRole, getByText } = render(EnableSyncPill);
    expect(getByText('Turn on sync')).toBeTruthy();
    expect(getByRole('button', { name: /turn on sync/i })).toBeTruthy();
  });

  it('opens the consent modal on click (does not push anything itself)', async () => {
    const { getByRole } = render(EnableSyncPill);
    expect(proSync.consentPromptOpen).toBe(false);
    await fireEvent.click(getByRole('button', { name: /turn on sync/i }));
    expect(proSync.consentPromptOpen).toBe(true);
  });
});
