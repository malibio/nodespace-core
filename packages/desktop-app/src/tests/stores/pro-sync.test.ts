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


describe('ProSync store — signedInEpisode transition counter (#1566)', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue('pro');
    await proSync.start();
    // Reset to a signed-out baseline so the empty→non-empty edge fires freshly.
    emit('sync:status', { state: 4, detail: '', user_email: '' });
    expect(proSync.userEmail).toBe('');
  });

  it('bumps the episode when userEmail transitions from empty to non-empty', () => {
    const before = proSync.signedInEpisode;

    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });

    expect(proSync.userEmail).toBe('mayank@nodespace.dev');
    expect(proSync.signedInEpisode).toBe(before + 1);
  });

  it('does not bump while already signed in (non-empty → non-empty)', () => {
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    const afterSignIn = proSync.signedInEpisode;

    // A subsequent status update for the same signed-in user must not re-bump.
    emit('sync:status', { state: 5, detail: 'syncing', user_email: 'mayank@nodespace.dev' });

    expect(proSync.signedInEpisode).toBe(afterSignIn);
  });

  it('bumps again on a fresh sign-in after signing out', () => {
    emit('sync:status', { state: 6, detail: '', user_email: 'mayank@nodespace.dev' });
    const firstEpisode = proSync.signedInEpisode;

    emit('sync:status', { state: 4, detail: '', user_email: '' }); // sign out
    emit('sync:status', { state: 6, detail: '', user_email: 'other@nodespace.dev' }); // sign back in

    expect(proSync.signedInEpisode).toBe(firstEpisode + 1);
  });
});

describe('ProSync store — onProConfirmed (#1566)', () => {
  // proSync is a module-level singleton and proConfirmed is a one-way latch, so once
  // any prior test has confirmed Pro tier it stays confirmed. These assertions are
  // therefore written to be order-independent: they exercise the "already pro" path,
  // which is the steady state after tier detection.
  beforeEach(async () => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue('pro');
    await proSync.start();
    emit('pro:tier-detected', { tier: 'pro', initial_status: null });
  });

  it('fires a callback registered after tier is already pro immediately', () => {
    const cb = vi.fn();
    proSync.onProConfirmed(cb);
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it('fires each registered callback exactly once, not again on re-confirmation', () => {
    const cb = vi.fn();
    proSync.onProConfirmed(cb);
    expect(cb).toHaveBeenCalledTimes(1);

    // A subsequent pro re-confirmation must not re-fire it.
    emit('pro:tier-detected', { tier: 'pro', initial_status: null });
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it('returns an unregister function', () => {
    const cb = vi.fn();
    const unregister = proSync.onProConfirmed(cb);
    expect(typeof unregister).toBe('function');
    expect(() => unregister()).not.toThrow();
  });
});
