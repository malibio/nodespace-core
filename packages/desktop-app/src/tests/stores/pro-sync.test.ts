/**
 * ProSync store — signed-in identity + manual sign-out (#199 S6).
 *
 * Asserts the store surfaces `SyncStatusEvent.user_email` (the "signed in as
 * <email>" affordance) from the `sync:status` stream and clears it on sign-out,
 * and that `signOut()` drives the daemon command + optimistically resets state.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

// Capture the `sync:status` / `pro:tier-detected` handlers so tests can drive them.
const listeners = new Map<string, (event: { payload: unknown }) => void>();
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler);
    return () => listeners.delete(name);
  })
}));

import { proSync } from '$lib/stores/pro-sync.svelte';

function emit(name: string, payload: unknown) {
  listeners.get(name)?.({ payload });
}

describe('ProSync store — signed-in identity + sign-out (#199 S6)', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue('pro');
    await proSync.start();
    emit('pro:tier-detected', { tier: 'pro', initial_status: null });
  });

  it('surfaces user_email from sync:status and clears it when signed out', () => {
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    expect(proSync.state).toBe('connected');
    expect(proSync.userEmail).toBe('mayank@nodespace.dev');

    // A signed-out (AUTH_REQUIRED) event with no email clears the affordance.
    emit('sync:status', { state: 4, detail: 'signed out', user_email: '' });
    expect(proSync.state).toBe('auth-required');
    expect(proSync.userEmail).toBe('');
  });

  it('tolerates a missing user_email field (older payloads) as empty', () => {
    emit('sync:status', { state: 6, detail: '' });
    expect(proSync.userEmail).toBe('');
  });

  it('signOut() invokes pro_signout and optimistically resets', async () => {
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    mockInvoke.mockClear();
    mockInvoke.mockResolvedValue(undefined);

    await proSync.signOut();

    expect(mockInvoke).toHaveBeenCalledWith('pro_signout');
    expect(proSync.userEmail).toBe('');
    expect(proSync.state).toBe('auth-required');
  });
});

describe('ProSync store — authRequiredEpisode transition counter', () => {
  // proSync is a module-level singleton, so state carries over between tests.
  // Force a known non-auth-required starting state before each test so the
  // transition-into-auth-required edge is always exercised freshly.
  beforeEach(async () => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue('pro');
    await proSync.start();
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    expect(proSync.state).toBe('connected');
  });

  it('bumps the episode counter when transitioning into auth-required', () => {
    const before = proSync.authRequiredEpisode;

    emit('sync:status', { state: 4, detail: 'signed out', user_email: '' });

    expect(proSync.state).toBe('auth-required');
    expect(proSync.authRequiredEpisode).toBe(before + 1);
  });

  it('does not re-bump the counter while repeatedly re-entering auth-required via redundant events', () => {
    emit('sync:status', { state: 4, detail: '', user_email: '' });
    const afterFirst = proSync.authRequiredEpisode;

    // Same state delivered again (e.g. a duplicate/retried sync:status event).
    emit('sync:status', { state: 4, detail: '', user_email: '' });

    expect(proSync.authRequiredEpisode).toBe(afterFirst);
  });

  it('bumps the counter again on a distinct re-entry into auth-required', () => {
    emit('sync:status', { state: 4, detail: '', user_email: '' });
    const firstEpisode = proSync.authRequiredEpisode;

    // Leaves auth-required...
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    expect(proSync.state).toBe('connected');

    // ...then re-enters it. This is a new episode and must be distinguishable
    // from the first, so a dismissal recorded against firstEpisode doesn't
    // silently suppress the re-login prompt for the new episode.
    emit('sync:status', { state: 4, detail: '', user_email: '' });

    expect(proSync.authRequiredEpisode).toBe(firstEpisode + 1);
  });

  it('bumps the counter when the cold-start initial_status is already auth-required', async () => {
    // A fresh cold-start: tier-detected delivers initial_status directly,
    // without any prior sync:status event establishing a baseline state.
    const before = proSync.authRequiredEpisode;

    emit('pro:tier-detected', {
      tier: 'pro',
      initial_status: { state: 4, detail: '', user_email: '' }
    });

    expect(proSync.state).toBe('auth-required');
    expect(proSync.authRequiredEpisode).toBe(before + 1);
  });
});
