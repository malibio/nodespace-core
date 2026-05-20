/**
 * Pro-tier sync state — populated by the Tauri capability probe and
 * the WatchSyncStatus stream from the Pro daemon (nodespaced-pro).
 *
 * Driven by two Tauri events:
 *   - `pro:tier-detected` fired once at startup with { tier, addr,
 *     initial_status }.
 *   - `sync:status` fired repeatedly while subscribed to
 *     CloudSyncService.WatchSyncStatus, payload { state, detail }.
 *
 * In community mode (or when NODESPACED_ADDR is unset and the embedded
 * daemon is in use), `tier` stays `'community'` and the UI surfaces
 * gated on `isPro` never render.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('ProSync');

export type ProTier = 'pro' | 'community' | 'unknown';

/**
 * SyncStatusEvent.State proto enum mirrored as a TS type. Numbers
 * match nodespace.pro.v1.SyncStatusEvent.State.
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
  addr = $state<string>('');

  isPro = $derived(this.tier === 'pro');

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
      this.tier = t;
    } catch (e) {
      log.warn('pro_tier invoke failed', { error: e });
    }

    this.unlistenTier = await listen<{
      tier: ProTier;
      addr: string;
      initial_status: { state: number; detail: string } | null;
    }>('pro:tier-detected', async (event) => {
      const p = event.payload;
      log.info('tier detected', { tier: p.tier, addr: p.addr });
      this.tier = p.tier;
      this.addr = p.addr;
      if (p.initial_status) {
        this.state = decodeState(p.initial_status.state);
        this.detail = p.initial_status.detail;
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

    this.unlistenStatus = await listen<{ state: number; detail: string }>(
      'sync:status',
      (event) => {
        this.state = decodeState(event.payload.state);
        this.detail = event.payload.detail;
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
