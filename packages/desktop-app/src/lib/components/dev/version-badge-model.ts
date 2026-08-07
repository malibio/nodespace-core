/**
 * Pure logic for the dev-only build/version badge.
 *
 * Kept transport- and DOM-free so it can be unit-tested directly: the
 * `.svelte` component wires these to `localStorage`, the frontend's compiled
 * `__APP_VERSION__`, and the daemon version fetched via the backend adapter.
 */

/** localStorage flag that opts a build into showing the version badge. */
export const BADGE_STORAGE_KEY = 'ns:show-build-badge';

/** Value the flag must hold for the badge to be shown. */
export const BADGE_ENABLED_VALUE = '1';

/** The minimal slice of the Storage API the badge reads. */
export interface BadgeStorage {
  getItem(key: string): string | null;
}

/**
 * True only when the opt-in flag is explicitly set to `'1'`. Defensive against
 * a missing/partial storage object (some runtimes expose a Storage where
 * `getItem` is not a function) and against a throwing `getItem` — either case
 * keeps the badge hidden rather than crashing the caller.
 */
export function isBadgeEnabled(storage: BadgeStorage | null | undefined): boolean {
  if (!storage || typeof storage.getItem !== 'function') {
    return false;
  }
  try {
    return storage.getItem(BADGE_STORAGE_KEY) === BADGE_ENABLED_VALUE;
  } catch {
    return false;
  }
}

/**
 * True when both versions are known and they differ. When the daemon version
 * is unknown (fetch failed, or not yet loaded) there is nothing to compare, so
 * this returns false rather than flagging a false mismatch.
 */
export function versionMismatch(
  frontend: string | null | undefined,
  daemon: string | null | undefined
): boolean {
  if (!frontend || !daemon) {
    return false;
  }
  return frontend !== daemon;
}
