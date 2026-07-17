/**
 * Daemon Status Service
 *
 * Single shared source of daemon readiness. Fans the signal out to:
 *   - a `connecting` grace-period flag for `AppShell`'s banner (starts true,
 *     flips false once a status is known, or after a short local timer —
 *     whichever comes first)
 *   - an `unreachable` flag for the existing "background service is not
 *     running" banner/retry button
 *   - a set of "on reconnect" callbacks that daemon-dependent stores
 *     (schemas, collections, children-tree) register to retry their load
 *     once the daemon transitions to healthy
 *
 * Generalizes the one-off `pro:tier-detected` reload pattern that used to
 * live only in app-shell.svelte into a single shared hook every
 * daemon-dependent store can use.
 *
 * Readiness is pulled AND pushed, not push-only. A push-only design has its
 * own startup race: the backend emits `daemon-status` from its own setup
 * task, on its own clock, and if that emit fires before the webview has
 * registered its listener, the signal is lost with no way to recover it.
 * Pulling the current status on subscribe (via the `check_daemon_status`
 * command) closes that window, and gives the manual "Retry" affordance a
 * real path through this shared contract instead of bypassing it.
 */

import { writable } from 'svelte/store';
import { isTauri } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('DaemonStatus');

/** How long to show a "connecting" banner before any status is known. */
const CONNECTING_GRACE_PERIOD_MS = 1500;

/**
 * How often to re-probe daemon health once the listener is running.
 *
 * The backend pushes `daemon-status` only from its one-shot startup task, so a
 * daemon that dies and is relaunched mid-session emits nothing — every
 * reconnect consumer would stay wedged until an app restart. A low-frequency
 * pull observes the `healthy → not_running → healthy` cycle so `applyStatus`
 * re-fires `onDaemonReconnect`. Kept slow because the only work on a steady
 * healthy poll is a cheap socket probe; the reconnect callbacks fire only on
 * the not-healthy → healthy edge, never on repeated healthy polls.
 */
const STEADY_STATE_POLL_MS = 15000;

export interface DaemonStatusState {
  /** True until the first status is known or the grace period elapses. */
  connecting: boolean;
  /** True once a `not_running` status has been observed. */
  unreachable: boolean;
}

/**
 * A source of daemon readiness, decoupled from any particular transport.
 * Production binds a Tauri source (pull via `check_daemon_status` + push via
 * the `daemon-status` event); tests can bind any other source — e.g. one
 * backed by a real headless daemon in an integration harness — without the
 * shared core caring how the status arrived.
 */
export interface DaemonStatusSource {
  /** Pull the current status once. Resolves to `"healthy"`, `"starting"`, or `"not_running"`. */
  getCurrent(): Promise<string>;
  /** Subscribe to pushed status changes. Returns an unsubscribe function. */
  subscribe(callback: (status: string) => void): () => void;
}

const _status = writable<DaemonStatusState>({ connecting: true, unreachable: false });

const reconnectListeners = new Set<() => void>();

let started = false;
let lastHealthy = false;
let activeSource: DaemonStatusSource | null = null;
let steadyStatePoll: ReturnType<typeof setInterval> | null = null;
/** Guards against overlapping polls if a `getCurrent` probe runs long. */
let pollInFlight = false;

/**
 * Register a callback to run whenever the daemon transitions to healthy
 * (including the very first healthy status of the session). Returns an
 * unsubscribe function.
 */
export function onDaemonReconnect(callback: () => void): () => void {
  reconnectListeners.add(callback);
  return () => reconnectListeners.delete(callback);
}

export const daemonStatus = {
  subscribe: _status.subscribe
};

/** Apply a status string to shared state and fan out reconnect callbacks. Transport-agnostic. */
function applyStatus(payload: string): void {
  const healthy = payload === 'healthy';
  _status.set({ connecting: false, unreachable: payload === 'not_running' });

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
}

/** Tauri-backed source: pulls via the `check_daemon_status` command, pushes via the `daemon-status` event. */
function tauriSource(): DaemonStatusSource {
  return {
    getCurrent: () => invoke<string>('check_daemon_status'),
    subscribe(callback) {
      let unlistened = false;
      let unlisten: (() => void) | null = null;
      listen<string>('daemon-status', (event) => callback(event.payload))
        .then((fn) => {
          if (unlistened) {
            fn();
          } else {
            unlisten = fn;
          }
        })
        .catch((err) => {
          log.warn('Failed to register daemon-status listener', err);
        });
      return () => {
        unlistened = true;
        unlisten?.();
      };
    }
  };
}

/**
 * Start the shared daemon-status service against a given source (defaults to
 * the Tauri-backed source). Safe to call multiple times — only the first
 * call actually starts listening. Outside Tauri (browser dev mode) the
 * default source is a no-op, same as before.
 */
export function startDaemonStatusListener(source?: DaemonStatusSource): void {
  if (started) return;
  if (!source && !isTauri()) return;
  started = true;
  activeSource = source ?? tauriSource();

  const graceTimer = setTimeout(() => {
    _status.update((s) => ({ ...s, connecting: false }));
  }, CONNECTING_GRACE_PERIOD_MS);

  activeSource.subscribe((payload) => {
    clearTimeout(graceTimer);
    applyStatus(payload);
  });

  // Pull the current status immediately so a status emitted before this
  // subscribe call (or between app launch and listener registration) is
  // not lost — the push path above still applies later transitions.
  activeSource
    .getCurrent()
    .then((payload) => {
      clearTimeout(graceTimer);
      applyStatus(payload);
    })
    .catch((err) => {
      log.warn('Failed to pull initial daemon status', err);
    });

  // Steady-state poll: keep re-probing at a low frequency so a mid-session
  // daemon restart (healthy → not_running → healthy) is observed here. The
  // backend emits `daemon-status` only from its one-shot startup task, so a
  // relaunched daemon pushes nothing; without this poll every reconnect
  // consumer (schemas, collections, children-tree, pane hydration) stays
  // wedged until an app restart.
  steadyStatePoll = setInterval(() => {
    if (pollInFlight || !activeSource) return;
    pollInFlight = true;
    activeSource
      .getCurrent()
      .then(applyStatus)
      .catch((err) => log.warn('Steady-state daemon status poll failed', err))
      .finally(() => {
        pollInFlight = false;
      });
  }, STEADY_STATE_POLL_MS);
  // Never let the poll on its own keep a runtime (or a test worker) alive.
  (steadyStatePoll as unknown as { unref?: () => void })?.unref?.();
}

/**
 * Stop the steady-state poll and reset the listener so it can be started
 * again. Production never calls this (the service lives for the whole session);
 * it exists so tests can tear the singleton down between cases.
 */
export function stopDaemonStatusListener(): void {
  if (steadyStatePoll) {
    clearInterval(steadyStatePoll);
    steadyStatePoll = null;
  }
  pollInFlight = false;
  started = false;
  lastHealthy = false;
  activeSource = null;
}

/**
 * Re-pull the current daemon status through the shared contract. Used by the
 * manual "Retry" affordance so a recovered daemon is detected the same way
 * an automatic recovery would be — including firing `onDaemonReconnect`
 * listeners — instead of only clearing the banner locally.
 */
export async function refreshDaemonStatus(): Promise<void> {
  if (!activeSource) return;
  const payload = await activeSource.getCurrent();
  applyStatus(payload);
}
