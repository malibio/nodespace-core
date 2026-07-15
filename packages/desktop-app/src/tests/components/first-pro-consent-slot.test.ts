/**
 * first-pro-consent-slot component.
 *
 * Registry wrapper for the consent modal in the sign-in-first flow. The user has
 * already signed in when this mounts, so merge only records the publish consent
 * (pro_enable_sync → flips sync_enabled). It auto-opens once per fresh sign-in
 * episode; decline ("Keep local") records the episode, shows a confirmation, and
 * shares nothing.
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
    proSync.signedInEpisode = 0;
    proSync.consentDeclinedEpisode = -1;
    proSync.consentPromptOpen = true;
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    proSync.tier = 'unknown';
    proSync.signedInEpisode = 0;
    proSync.consentDeclinedEpisode = -1;
    proSync.consentPromptOpen = false;
  });

  it('merges: records the publish consent (already signed in) and closes', async () => {
    const { getByRole, findByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');

    await fireEvent.click(getByRole('checkbox'));
    await fireEvent.click(getByRole('button', { name: /merge into public workspace/i }));

    // Sign-in already happened before consent, so merge is a single enable call —
    // no pro_initiate_oauth from here.
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(1));
    expect(mockInvoke).toHaveBeenCalledWith('pro_enable_sync');
    await waitFor(() => expect(proSync.consentPromptOpen).toBe(false));
  });

  it('keep-local closes with no daemon call, records the decline, and confirms', async () => {
    const { getByRole, findByRole, findByText } = render(FirstProConsentSlot);
    await findByRole('dialog');

    await fireEvent.click(getByRole('button', { name: /keep this database local-only/i }));

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(proSync.consentPromptOpen).toBe(false);
    // The decline is recorded against the current sign-in episode and confirmed.
    expect(proSync.consentDeclinedEpisode).toBe(proSync.signedInEpisode);
    await findByText(/kept local — sync stays off/i);
  });

  it('auto-opens once per sign-in episode; a decline keeps it closed for that episode', async () => {
    // No manual open — a fresh sign-in episode should surface the consent modal.
    proSync.consentPromptOpen = false;
    proSync.signedInEpisode = 1;

    const { getByRole, findByRole, queryByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');

    // Decline records episode 1 and closes the modal.
    await fireEvent.click(getByRole('button', { name: /keep this database local-only/i }));
    expect(proSync.consentDeclinedEpisode).toBe(1);
    await waitFor(() => expect(queryByRole('dialog')).toBeNull());
  });
});
