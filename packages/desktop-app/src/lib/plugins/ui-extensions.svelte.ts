/**
 * Reactive wrapper over {@link UiExtensionRegistry}
 * ========================================================
 *
 * The registry (`ui-extensions.ts`) holds declarative, non-reactive data. This
 * module layers reactivity on top (ADR-049): it resolves the active Pro-sync
 * variant from the two signals and returns only the contributions matching it.
 *
 * Both source reads are reactive when these functions are called inside a
 * `$derived`/template:
 *   - `proSync.tier` is `$state`.
 *   - `SharedNodeStore.getInstance().getNode(...)` reads a `SvelteMap`, so a
 *     later `setNode` for the `DatabaseSettingsNode` re-runs the derivation.
 *
 * Importing this module also registers the built-in Pro UI extension (side-effect
 * import of `./pro-plugin`), so any consumer of the wrapper sees the contributions
 * without a separate init call.
 */

import './pro-plugin';

import { proSync } from '$lib/stores/pro-sync.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import {
  uiExtensionRegistry,
  DATABASE_SETTINGS_NODE_ID,
  type ChromeSlot,
  type ChromeContribution,
  type ViewerExtension,
  type ProSyncVariant
} from './ui-extensions';

/** The `database-settings`-namespaced properties held on the settings singleton. */
interface DatabaseSettings {
  sync_enabled?: boolean;
  auth_status?: string;
}

/**
 * Read the active database's `DatabaseSettingsNode` properties (the
 * `database-settings` sub-object), or `undefined` when the node isn't hydrated
 * yet. Reads the reactive `SharedNodeStore` map, so callers in a reactive context
 * re-run when the node lands or changes.
 */
export function activeDatabaseSettings(): DatabaseSettings | undefined {
  const node = SharedNodeStore.getInstance().getNode(DATABASE_SETTINGS_NODE_ID);
  const settings = (node?.properties as Record<string, unknown> | undefined)?.['database-settings'];
  return settings && typeof settings === 'object' ? (settings as DatabaseSettings) : undefined;
}

/**
 * Resolve the current Pro-sync variant from the two independent signals:
 *   - axis 1: `proSync.tier` (daemon build variant) — not Pro ⇒ `teaser`.
 *   - axis 2: the active database's `auth_status` (signed in?) and `sync_enabled`
 *     (opted into the public-workspace publish?).
 *
 * The flow is sign-in-first: an un-opted-in database shows `sign-in` until the
 * user authenticates, then `consent` (the publish decision) — nothing is enabled
 * until they merge. An opted-in database is `connected` while authenticated, or
 * `relogin` when its session has lapsed.
 *
 * A plain function (not a `$derived`) so it composes into any consumer's own
 * derivation; its reads are reactive because the underlying sources are.
 */
export function resolveProSyncVariant(): ProSyncVariant {
  if (proSync.tier !== 'pro') return 'teaser';
  const settings = activeDatabaseSettings();
  const authed = settings?.auth_status === 'connected';
  const enabled = settings?.sync_enabled === true;
  // Not yet opted in: sign in first, then present the publish consent.
  if (!enabled) return authed ? 'consent' : 'sign-in';
  // Opted in: connected once authenticated, else a re-login is needed.
  return authed ? 'connected' : 'relogin';
}

/**
 * True when sync is actually active for the current build + active database —
 * i.e. axis 1 is Pro AND axis 2 has `sync_enabled: true` (the `relogin` and
 * `connected` variants). This is the two-axis gate dependent stores (membership,
 * recovered-items) use in place of the raw `proSync.isPro` (which is axis 1 only).
 */
export function isProSyncActive(): boolean {
  const variant = resolveProSyncVariant();
  return variant === 'relogin' || variant === 'connected';
}

/** Chrome contributions for `slot` that match the currently-resolved variant. */
export function getActiveChromeContributions(slot: ChromeSlot): ChromeContribution[] {
  const variant = resolveProSyncVariant();
  return uiExtensionRegistry.chromeFor(slot).filter((c) => c.variant === variant);
}

/** Viewer extensions for `nodeType` that match the currently-resolved variant. */
export function getActiveViewerExtensions(nodeType: string): ViewerExtension[] {
  const variant = resolveProSyncVariant();
  return uiExtensionRegistry.viewersFor(nodeType).filter((e) => e.variant === variant);
}
