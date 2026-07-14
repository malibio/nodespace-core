/**
 * first-pro-consent-slot component.
 *
 * Registry wrapper for the consent modal. On merge it records the consent
 * (pro_enable_sync → flips sync_enabled) and then starts sign-in; on decline it
 * closes and shares nothing.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import FirstProConsentSlot from '$lib/components/first-pro-consent-slot.svelte';
import { proSync } from '$lib/stores/pro-sync.svelte';

describe('FirstProConsentSlot', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
    proSync.tier = 'pro';
    proSync.consentPromptOpen = true;
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    proSync.tier = 'unknown';
    proSync.consentPromptOpen = false;
  });

  it('merges: enables sync, then starts sign-in, then closes', async () => {
    const { getByRole, findByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');

    await fireEvent.click(getByRole('checkbox'));
    await fireEvent.click(getByRole('button', { name: /merge into public workspace/i }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(2));
    expect(mockInvoke).toHaveBeenNthCalledWith(1, 'pro_enable_sync');
    expect(mockInvoke).toHaveBeenNthCalledWith(2, 'pro_initiate_oauth');
    await waitFor(() => expect(proSync.consentPromptOpen).toBe(false));
  });

  it('keep-local closes without any daemon call', async () => {
    const { getByRole, findByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');

    await fireEvent.click(getByRole('button', { name: /keep this database local-only/i }));

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(proSync.consentPromptOpen).toBe(false);
  });
});
