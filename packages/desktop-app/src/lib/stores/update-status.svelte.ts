/**
 * App-update status store.
 *
 * The Rust side (`update_check.rs`) checks GitHub Releases at startup and emits
 * `update://available` only when a newer NodeSpace version exists; it also exposes
 * the `check_for_update_command` for an on-demand / post-reload check. This store
 * surfaces that as a dismissible, non-blocking banner (see `update-banner.svelte`).
 *
 * The app bundle is not code-signed and ships no auto-updater, so "update" means
 * "open the release download" — the user installs it and their data is untouched
 * (the local store lives in ~/.nodespace, outside the bundle; the daemon also
 * snapshots the DB before any release's migrations run). This store therefore only
 * NOTIFIES; it never mutates anything.
 *
 * Dismissal is per-version (persisted): dismissing 0.3.0 hides the banner for
 * 0.3.0 but it re-appears when 0.4.0 ships.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '$lib/utils/external-links';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('UpdateStatus');

/** Mirrors the Rust `UPDATE_AVAILABLE_EVENT`. */
export const UPDATE_AVAILABLE_EVENT = 'update://available';
/** Where a user gets the new build (both free and Pro releases are published here). */
export const RELEASES_URL = 'https://github.com/NodeSpaceAI/nodespace-core/releases/latest';
const DISMISSED_KEY = 'ns:update-dismissed-version';

/** Mirrors the Rust `UpdateStatus` payload. */
export interface UpdateStatus {
  current: string;
  latest: string | null;
  update_available: boolean;
}

function readDismissed(): string | null {
  try {
    return typeof localStorage !== 'undefined' ? localStorage.getItem(DISMISSED_KEY) : null;
  } catch {
    return null;
  }
}

class UpdateStore {
  current = $state('');
  latest = $state<string | null>(null);
  available = $state(false);
  /** The version the user last dismissed (persisted); banner stays hidden for it. */
  dismissedVersion = $state<string | null>(null);
  private unlisten: UnlistenFn | null = null;
  private started = false;

  /**
   * Show only when a newer version is available AND the user hasn't dismissed
   * THIS version. A newer `latest` than what was dismissed re-shows the banner.
   */
  get showBanner(): boolean {
    return this.available && this.latest !== null && this.dismissedVersion !== this.latest;
  }

  private apply(status: UpdateStatus): void {
    this.current = status.current;
    this.latest = status.latest;
    this.available = status.update_available && status.latest !== null;
  }

  /**
   * Subscribe to the startup event and run one on-demand check (so a webview
   * reload after the startup emit still learns of an available update).
   * Idempotent — only the first call wires up.
   */
  async init(): Promise<void> {
    if (this.started) return;
    this.started = true;
    this.dismissedVersion = readDismissed();
    try {
      this.unlisten = await listen<UpdateStatus>(UPDATE_AVAILABLE_EVENT, (e) => this.apply(e.payload));
    } catch (e) {
      log.warn('failed to subscribe to update event', { error: e });
    }
    try {
      this.apply(await invoke<UpdateStatus>('check_for_update_command'));
    } catch (e) {
      log.warn('on-demand update check failed', { error: e });
    }
  }

  /** Persist a per-version dismissal and hide the banner for the current `latest`. */
  dismiss(): void {
    if (!this.latest) return;
    try {
      localStorage?.setItem(DISMISSED_KEY, this.latest);
    } catch (e) {
      log.warn('failed to persist update dismissal', { error: e });
    }
    this.dismissedVersion = this.latest;
  }

  /** Open the release download page (no in-app install — see the module doc). */
  async download(): Promise<void> {
    await openUrl(RELEASES_URL);
  }

  stop(): void {
    this.unlisten?.();
    this.unlisten = null;
    this.started = false;
  }
}

export const updateStatus = new UpdateStore();
