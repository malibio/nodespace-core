/**
 * Daemon Status Service
 *
 * Single shared listener for the Rust-emitted `daemon-status` Tauri event
 * (`healthy` | `not_running`). Fans the signal out to:
 *   - a `connecting` grace-period flag for `AppShell`'s banner (starts true,
 *     flips false once a `daemon-status` event of either kind arrives, or
 *     after a short local timer — whichever comes first)
 *   - an `unreachable` flag for the existing "background service is not
 *     running" banner/retry button
 *   - a set of "on reconnect" callbacks that daemon-dependent stores
 *     (schemas, collections, children-tree) register to retry their load
 *     once the daemon transitions to healthy
 *
 * Generalizes the one-off `pro:tier-detected` reload pattern that used to
 * live only in app-shell.svelte into a single shared hook every
 * daemon-dependent store can use.
 */

import { writable } from 'svelte/store';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('DaemonStatus');

/** How long to show a "connecting" banner before any daemon-status event arrives. */
const CONNECTING_GRACE_PERIOD_MS = 1500;

export interface DaemonStatusState {
  /** True until the first daemon-status event arrives or the grace period elapses. */
  connecting: boolean;
  /** True once a `not_running` event has been received. */
  unreachable: boolean;
}

const _status = writable<DaemonStatusState>({ connecting: true, unreachable: false });

const reconnectListeners = new Set<() => void>();

let started = false;
let lastHealthy = false;

/**
 * Register a callback to run whenever the daemon transitions to healthy
 * (including the very first healthy event of the session). Returns an
 * unsubscribe function.
 */
export function onDaemonReconnect(callback: () => void): () => void {
  reconnectListeners.add(callback);
  return () => reconnectListeners.delete(callback);
}

export const daemonStatus = {
  subscribe: _status.subscribe
};

/**
 * Start the shared daemon-status listener. Safe to call multiple times —
 * only the first call registers the Tauri listener.
 */
export function startDaemonStatusListener(): void {
  if (started || !isTauri()) return;
  started = true;

  const graceTimer = setTimeout(() => {
    _status.update((s) => ({ ...s, connecting: false }));
  }, CONNECTING_GRACE_PERIOD_MS);

  listen<string>('daemon-status', (event) => {
    clearTimeout(graceTimer);

    const healthy = event.payload === 'healthy';
    _status.set({ connecting: false, unreachable: event.payload === 'not_running' });

    if (healthy && !lastHealthy) {
      lastHealthy = true;
      for (const cb of reconnectListeners) {
        try {
          cb();
        } catch (err) {
          log.error('Reconnect listener threw', err);
        }
      }
    } else if (!healthy) {
      lastHealthy = false;
    }
  }).catch((err) => {
    log.warn('Failed to register daemon-status listener', err);
  });
}
