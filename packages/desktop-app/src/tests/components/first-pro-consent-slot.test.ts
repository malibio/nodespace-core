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
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import FirstProConsentSlot from '$lib/components/first-pro-consent-slot.svelte';
import { proSync } from '$lib/stores/pro-sync.svelte';
import { databaseStore } from '$lib/stores/database.svelte';

/** In-memory localStorage stub (happy-dom's isn't guaranteed available). */
function stubLocalStorage(): Map<string, string> {
  const backing = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => (backing.has(k) ? backing.get(k)! : null),
    setItem: (k: string, v: string) => void backing.set(k, v),
    removeItem: (k: string) => void backing.delete(k),
    clear: () => backing.clear()
  });
  return backing;
}

describe('FirstProConsentSlot', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
    proSync.tier = 'pro';
    proSync.signedInEpisode = 0;
    proSync.consentDeclinedEpisode = -1;
    proSync.consentPromptOpen = true;
    databaseStore.activeDatabaseId = null;
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    proSync.tier = 'unknown';
    proSync.signedInEpisode = 0;
    proSync.consentDeclinedEpisode = -1;
    proSync.consentPromptOpen = false;
    databaseStore.activeDatabaseId = null;
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

  it('merge re-pulls the settings node so the consent → connected flip does not depend on a watch event', async () => {
    const refreshSpy = vi
      .spyOn(databaseStore, 'refreshDatabaseSettings')
      .mockImplementation(() => {});
    const { getByRole, findByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');

    await fireEvent.click(getByRole('checkbox'));
    await fireEvent.click(getByRole('button', { name: /merge into public workspace/i }));

    await waitFor(() => expect(refreshSpy).toHaveBeenCalled());
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

  it('keep-local persists the decline per database', async () => {
    const store = stubLocalStorage();
    databaseStore.activeDatabaseId = 'db-alpha';

    const { getByRole, findByRole } = render(FirstProConsentSlot);
    await findByRole('dialog');
    await fireEvent.click(getByRole('button', { name: /keep this database local-only/i }));

    expect(store.get('ns:consent-declined:db-alpha')).toBe('1');
  });

  it('does not re-pop after a reload once the decline is persisted for this database', async () => {
    const store = stubLocalStorage();
    store.set('ns:consent-declined:db-alpha', '1');
    databaseStore.activeDatabaseId = 'db-alpha';
    // Simulate a fresh store on reload: no manual open, but the resumed session
    // re-bumps signedInEpisode.
    proSync.consentPromptOpen = false;
    proSync.consentDeclinedEpisode = -1;
    proSync.signedInEpisode = 1;

    const { queryByRole } = render(FirstProConsentSlot);
    // The persisted decline suppresses the auto-open, so no dialog appears.
    await waitFor(() => expect(queryByRole('dialog')).toBeNull());
  });

  it('a persisted decline for a different database does not suppress this one', async () => {
    const store = stubLocalStorage();
    store.set('ns:consent-declined:db-other', '1');
    databaseStore.activeDatabaseId = 'db-alpha';
    proSync.consentPromptOpen = false;
    proSync.consentDeclinedEpisode = -1;
    proSync.signedInEpisode = 1;

    const { findByRole } = render(FirstProConsentSlot);
    // db-alpha has no decline of its own, so the consent still auto-opens.
    await findByRole('dialog');
  });
});
