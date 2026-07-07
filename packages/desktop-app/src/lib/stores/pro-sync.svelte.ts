/**
 * Pro-tier sync state — populated by the Tauri capability probe and
 * the WatchSyncStatus stream from the Pro daemon (nodespaced-pro).
 *
 * Driven by two Tauri events:
 *   - `pro:tier-detected` fired once at startup with
 *     `{ tier, initial_status }`.
 *   - `sync:status` fired repeatedly while subscribed to
 *     CloudSyncService.WatchSyncStatus, payload `{ state, detail }`.
 *
 * In community mode, `tier` stays `'community'` and the UI surfaces
 * gated on `isPro` never render.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('ProSync');

export type ProTier = 'pro' | 'community' | 'unknown';

/**
 * SyncStatusEvent.State proto enum mirrored as a TS type. Numbers
 * match `nodespace.pro.v1.SyncStatusEvent.State` defined in
 * `nodespace-sync/nodespaced-pro/proto/nodespace_pro.proto`
 * (vendored at `packages/desktop-app/src-tauri/proto/`).
 */
export type SyncState =
  | 'unspecified'
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'auth-required'
  | 'syncing'
  | 'connected'
  | 'error';

/**
 * Decode `nodespace.pro.v1.SyncStatusEvent.State` numeric enum to the
 * TS string variant. Update both this switch and the proto file if a
 * new state is added.
 */
function decodeState(n: number): SyncState {
  switch (n) {
    case 1:
      return 'disconnected';
    case 2:
      return 'connecting';
    case 3:
      return 'authenticating';
    case 4:
      return 'auth-required';
    case 5:
      return 'syncing';
    case 6:
      return 'connected';
    case 7:
      return 'error';
    default:
      return 'unspecified';
  }
}

/** Reactive Pro-sync state — Svelte 5 runes via class-based pattern. */
class ProSyncStore {
  tier = $state<ProTier>('unknown');
  state = $state<SyncState>('unspecified');
  detail = $state<string>('');
  /**
   * Email of the signed-in user, from the daemon's `SyncStatusEvent.user_email`
   * (decoded from the session JWT). Empty when signed out / not yet authenticated.
   * Drives the "signed in as <email>" affordance (#199 S6) — needed because on the
   * silent-resume path the frontend never sees an OAuth response.
   */
  userEmail = $state<string>('');

  /**
   * Bumped each time `state` transitions into `'auth-required'` from some other
   * state. Lets consumers (e.g. the re-login modal's dismissal) distinguish a
   * fresh auth-required episode from the one already dismissed, without an
   * effect that reads and writes its own dismissal flag.
   */
  authRequiredEpisode = $state(0);

  /**
   * Bumped each time `userEmail` transitions from empty to non-empty — i.e. a fresh
   * sign-in. Lets consumers (the sync pill's first-run onboarding) react to the
   * transition via a derived comparison instead of an effect that reads and writes
   * its own "seen" flag. Same pattern as authRequiredEpisode.
   */
  signedInEpisode = $state(0);

  isPro = $derived(this.tier === 'pro');

  /**
   * Callbacks fired once when `tier` first resolves to 'pro'. Lets consumers run a
   * Pro-confirmed one-shot (e.g. loading the recovered-items log) from this state
   * transition rather than an $effect that guards on isPro (ADR-049). A callback
   * registered after tier is already 'pro' fires immediately.
   *
   * `proConfirmed` is a deliberate one-way latch: Pro tier is a per-process capability
   * probe result, not a session-lifetime flag, so it never resets on sign-out (unlike
   * `signedInEpisode`, which tracks the per-session sign-in edge and re-bumps). A
   * consumer needing to re-run on a genuine tier downgrade→upgrade would need a
   * different, episode-style signal — none exists today because tier does not change
   * within a running app.
   */
  private proConfirmedCallbacks = new Set<() => void>();
  private proConfirmed = false;

  /** Register a one-shot callback for Pro-tier confirmation. Returns an unregister fn. */
  onProConfirmed(callback: () => void): () => void {
    if (this.proConfirmed) {
      callback();
      return () => {};
    }
    this.proConfirmedCallbacks.add(callback);
    return () => this.proConfirmedCallbacks.delete(callback);
  }

  private setState(next: SyncState) {
    if (next === 'auth-required' && this.state !== 'auth-required') {
      this.authRequiredEpisode++;
    }
    this.state = next;
  }

  /** Set tier, firing proConfirmed callbacks once on the first transition to 'pro'. */
  private setTier(next: ProTier) {
    this.tier = next;
    if (next === 'pro' && !this.proConfirmed) {
      this.proConfirmed = true;
      for (const cb of this.proConfirmedCallbacks) cb();
      this.proConfirmedCallbacks.clear();
    }
  }

  /** Set userEmail, bumping signedInEpisode on an empty→non-empty (fresh sign-in) edge. */
  private setUserEmail(next: string) {
    if (next !== '' && this.userEmail === '') {
      this.signedInEpisode++;
    }
    this.userEmail = next;
  }

  private unlistenTier: UnlistenFn | null = null;
  private unlistenStatus: UnlistenFn | null = null;
  private started = false;

  /**
   * Mount the listeners + kick off the streaming subscription. Safe
   * to call multiple times — only the first call wires anything up.
   * Returns a cleanup function for the caller to invoke on unmount.
   */
  async start(): Promise<() => void> {
    if (this.started) {
      return () => this.stop();
    }
    this.started = true;
    log.debug('mounting pro-sync listeners');

    // Pull the current tier in case `pro:tier-detected` already
    // fired before we subscribed (Tauri events are not buffered).
    try {
      const t = await invoke<ProTier>('pro_tier');
      this.setTier(t);
    } catch (e) {
      log.warn('pro_tier invoke failed', { error: e });
    }

    this.unlistenTier = await listen<{
      tier: ProTier;
      initial_status: { state: number; detail: string; user_email?: string } | null;
    }>('pro:tier-detected', async (event) => {
      const p = event.payload;
      log.info('tier detected', { tier: p.tier });
      this.setTier(p.tier);
      if (p.initial_status) {
        this.setState(decodeState(p.initial_status.state));
        this.detail = p.initial_status.detail;
        this.setUserEmail(p.initial_status.user_email ?? '');
      }
      // The first pro_subscribe_sync_status invoke below races the
      // backend's async init (Tauri setup spawns the connect on the
      // runtime and returns immediately). If ProClient wasn't yet in
      // managed state at first invoke, the command no-op'd. Re-invoke
      // now that we know ProClient is ready — backend is idempotent.
      if (p.tier === 'pro') {
        try {
          await invoke('pro_subscribe_sync_status');
        } catch (e) {
          log.warn('pro_subscribe_sync_status (post-tier) failed', { error: e });
        }
      }
    });

    this.unlistenStatus = await listen<{ state: number; detail: string; user_email?: string }>(
      'sync:status',
      (event) => {
        this.setState(decodeState(event.payload.state));
        this.detail = event.payload.detail;
        this.setUserEmail(event.payload.user_email ?? '');
      }
    );

    // Idempotent on the Rust side — subsequent calls return early.
    try {
      await invoke('pro_subscribe_sync_status');
    } catch (e) {
      log.warn('pro_subscribe_sync_status invoke failed', { error: e });
    }

    return () => this.stop();
  }

  /**
   * Manual sign-out (#199 S6). Tells the daemon to drop its session and wipe the
   * persisted refresh token from the keychain (so a restart won't auto-resume),
   * then optimistically reflects signed-out locally. The daemon's AUTH_REQUIRED
   * transition confirms via `sync:status`. No-op in community mode (the backend
   * command returns early with no `ProClient`).
   */
  async signOut(): Promise<void> {
    try {
      await invoke('pro_signout');
    } catch (e) {
      log.warn('pro_signout invoke failed', { error: e });
    }
    this.setState('auth-required');
    this.setUserEmail('');
  }

  stop() {
    if (!this.started) return;
    this.unlistenTier?.();
    this.unlistenStatus?.();
    this.unlistenTier = null;
    this.unlistenStatus = null;
    this.started = false;
  }
}

export const proSync = new ProSyncStore();
