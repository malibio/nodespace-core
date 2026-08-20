import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import { backendAdapter } from '$lib/services/backend-adapter';
import { onDaemonReconnect } from '$lib/services/daemon-status';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';
import { collectionsData } from '$lib/stores/collections.svelte';
import { schemasData } from '$lib/stores/schemas.svelte';
import { aiChatsData } from '$lib/stores/ai-chats.svelte';
import {
  clearAllTabs,
  addTab,
  DAILY_JOURNAL_TAB_ID,
  DEFAULT_PANE_ID
} from '$lib/stores/navigation.svelte';
import { formatDateISO } from '$lib/utils/date-formatting';
import { DATABASE_SETTINGS_NODE_ID } from '$lib/plugins/ui-extensions';

const log = createLogger('DatabaseStore');

/**
 * A registered local database as surfaced by the `list_databases` command
 * (ADR-053: "One Daemon, Multiple Local Databases"). Mirrors the Rust
 * `DatabaseEntry` (camelCase).
 */
export interface DatabaseInfo {
  id: string;
  name: string;
  path: string;
  isDefault: boolean;
  /** "closed" | "open" | "missing" | "unknown". */
  status: string;
  createdAt: string;
  lastOpenedAt: string | null;
  /** Cloud tenant schema this database syncs to (ADR-053); null/empty when the
   * database is local-only (not bound to any tenant). */
  boundTenantSchema: string | null;
  /** The bound tenant's default (landing) collection id (ADR-053 / sync#297
   * per-install root). Used as the tree root to hide from the sidebar so
   * top-level collections render as peers rather than nested under the root.
   * null on the public/legacy tenant, where the collections store falls back to
   * the well-known root id. */
  boundTenantCollection: string | null;
}

interface DatabaseListing {
  databases: DatabaseInfo[];
  defaultDatabaseId: string;
}

/**
 * Whether the Tauri IPC bridge is present. Absent under `dev:browser`, where the
 * app runs in a plain browser against the dev-proxy — which forwards NodeService
 * but has no DatabaseService, so calling the `list_databases` invoke throws
 * `Cannot read properties of undefined (reading 'invoke')`. Same probe the model,
 * agent, and external-link surfaces use.
 */
function isTauriBridgePresent(): boolean {
  return (
    typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  );
}

/**
 * The single implicit database presented in browser dev mode: the dev-proxy is
 * bound to one daemon/database and exposes no registry, so the switcher shows one
 * concrete, non-switchable entry instead of erroring at boot.
 */
const IMPLICIT_BROWSER_DATABASE: DatabaseInfo = {
  id: 'default',
  name: 'Default',
  path: '',
  isDefault: true,
  status: 'open',
  createdAt: '',
  lastOpenedAt: null,
  boundTenantSchema: null,
  boundTenantCollection: null
};

/**
 * The active-database selection is desktop-local (not a daemon concept), so it
 * is persisted in the webview's localStorage. This survives a reload (Cmd+R) and
 * an app restart, so the user stays on the database they switched to instead of
 * snapping back to the daemon's registry default.
 */
const ACTIVE_DB_STORAGE_KEY = 'nodespace.activeDatabaseId';

function rememberActiveDatabaseId(id: string): void {
  try {
    localStorage.setItem(ACTIVE_DB_STORAGE_KEY, id);
  } catch {
    // localStorage unavailable (e.g. some sandboxed contexts) — persistence is
    // best-effort; the selection simply won't survive a reload.
  }
}

function readRememberedActiveDatabaseId(): string | null {
  try {
    return localStorage.getItem(ACTIVE_DB_STORAGE_KEY);
  } catch {
    return null;
  }
}

/**
 * Manages the daemon's registry of local databases and the desktop-local
 * "which database am I viewing" selection.
 *
 * Switching flushes the frontend's per-database caches and resets the
 * workspace so open viewers remount against the newly-active database; the
 * daemon-side re-subscribe (driven by the `set_active_database` command) makes
 * live node events stream from the new database.
 */
class DatabaseStore {
  databases = $state<DatabaseInfo[]>([]);
  /** The database the app is currently viewing. `null` until `load()` runs. */
  activeDatabaseId = $state<string | null>(null);
  /** The daemon-wide default (the header-less routing target). */
  defaultDatabaseId = $state<string | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  /**
   * Monotonic token bumped on every `switchTo`. A switch awaits (flush, then the
   * `set_active_database` command), so a second switch fired before the first
   * finishes would otherwise race — leaving `activeDatabaseId` and the routed
   * client transiently pointed at different databases. Each continuation checks
   * its captured token and bails if a newer switch superseded it, so the latest
   * switch always wins cleanly.
   */
  private switchSeq = 0;

  /** The database currently being viewed, or `null` if none is selected. */
  get activeDatabase(): DatabaseInfo | null {
    return this.databases.find((db) => db.id === this.activeDatabaseId) ?? null;
  }

  /**
   * Load the registry. Initializes `activeDatabaseId` to the daemon default the
   * first time (later loads preserve the current selection so a background
   * refresh never yanks the user back to the default).
   */
  async load(): Promise<void> {
    this.loading = true;
    this.error = null;

    // Browser dev mode (dev:browser): no Tauri bridge, so there is no database
    // registry to query. Present a single implicit database rather than logging a
    // boot error and rendering the switcher's failure fallback. The Tauri app,
    // where the bridge is present, is unaffected.
    if (!isTauriBridgePresent()) {
      this.databases = [IMPLICIT_BROWSER_DATABASE];
      this.defaultDatabaseId = IMPLICIT_BROWSER_DATABASE.id;
      if (this.activeDatabaseId === null) {
        this.activeDatabaseId = IMPLICIT_BROWSER_DATABASE.id;
        this.refreshDatabaseSettings();
      }
      this.loading = false;
      return;
    }

    try {
      const listing = await invoke<DatabaseListing>('list_databases');
      this.databases = listing.databases;
      this.defaultDatabaseId = listing.defaultDatabaseId || null;

      if (this.activeDatabaseId === null) {
        // Restore the last-active database across webview reloads / restarts.
        // Fall back to the daemon default, then the first registered database, so
        // the switcher always shows a concrete selection. Ignore ids that are no
        // longer registered (e.g. the database was deleted).
        //
        // A database named for *this launch* wins over the remembered one: it is
        // set only when the user picked that database from the tray, which is a
        // more specific instruction than "whatever you had open last time".
        const registered = (id: string | null): string | null =>
          id !== null && this.databases.some((db) => db.id === id) ? id : null;

        const requested = registered(await this.readInitialDatabaseId());
        const remembered = registered(readRememberedActiveDatabaseId());
        const resolved =
          requested ?? remembered ?? this.defaultDatabaseId ?? this.databases[0]?.id ?? null;

        // Re-check after the awaits above. A second `load()` runs on every
        // launch — the daemon-reconnect listener fires one — and both can pass
        // the outer check before either assigns. Assigning unconditionally lets
        // whichever finishes last overwrite a selection already made.
        if (this.activeDatabaseId === null) {
          this.activeDatabaseId = resolved;
          // Hydrate the active database's DatabaseSettingsNode so the Pro-sync
          // variant machine can read sync_enabled/auth_status.
          this.refreshDatabaseSettings();
        }
      }
    } catch (err) {
      this.error = String(err);
      log.error('Failed to load databases', err);
    } finally {
      this.loading = false;
    }
  }

  /**
   * The database this launch was told to open, if any.
   *
   * Set by the daemon tray when the user picks a database from its submenu.
   * A failure here is not worth surfacing — it only means we fall through to
   * the remembered/default selection, which is the normal path anyway.
   */
  private async readInitialDatabaseId(): Promise<string | null> {
    try {
      return (await invoke<string | null>('initial_database_id')) ?? null;
    } catch (err) {
      log.debug('No launch-time database selection available', err);
      return null;
    }
  }

  /**
   * Switch the active database. Flushes pending writes to the current database
   * first (so they never land in the target), then re-points the routed
   * clients, clears the frontend caches, and resets the workspace so viewers
   * reload from the newly-active database.
   */
  async switchTo(id: string): Promise<void> {
    if (id === this.activeDatabaseId) return;
    this.error = null;
    const seq = ++this.switchSeq;
    try {
      // Land any in-flight debounced saves in the database they were made
      // against before the routed clients re-point.
      await sharedNodeStore.flushAllPendingSaves();
      // A newer switch superseded this one while we flushed — let it win.
      if (seq !== this.switchSeq) return;

      await invoke('set_active_database', { id });
      if (seq !== this.switchSeq) return;
      this.activeDatabaseId = id;
      // Remember the selection so a webview reload / app restart restores it
      // instead of snapping back to the daemon's registry default.
      rememberActiveDatabaseId(id);

      // Re-target Pro cloud-sync to follow the newly-active database (ADR-053
      // single-active sync): switching database in the app switches which tenant
      // syncs, not just which one is read/written. No-ops in community mode (the
      // Tauri command returns early without a ProClient). Best-effort — a
      // re-target failure must not abort the already-committed routing switch.
      try {
        await invoke('pro_activate_database', { databaseId: id });
      } catch (err) {
        log.warn('Failed to re-target sync to the switched database', { id, error: err });
      }

      // Evict the previous database's cached data. `clearAll()` also bumps the
      // store's database epoch, which closes the in-flight-read window: a read
      // (e.g. loadChildren/getNode) dispatched against the previous database
      // *before* this switch whose response resolves after this clear captured
      // the old epoch and is dropped instead of writing the previous
      // database's rows into the now-active store (see
      // `sharedNodeStore.currentEpoch()`).
      sharedNodeStore.clearAll();
      structureTree.clear();

      // Reset the workspace: open tabs referenced the previous database's
      // nodes, so drop them and land on the new database's daily journal
      // (a date page exists in every database). Remounting the viewer reloads
      // its content from the now-active database.
      clearAllTabs();
      addTab(
        {
          id: DAILY_JOURNAL_TAB_ID,
          title: 'Daily Journal',
          type: 'node',
          content: { nodeId: formatDateISO(new Date()), nodeType: 'date' },
          closeable: true,
          paneId: DEFAULT_PANE_ID
        },
        true
      );

      // Reload the sidebar from the new database. The locally-created
      // exemptions belong to the database being left — collection ids are
      // derived from the name, so keeping them would wrongly un-hide a
      // same-named empty collection in the new one.
      collectionsData.forgetLocallyCreated();
      collectionsData.loadCollections();
      schemasData.loadSchemas();
      // As with collectionsData.forgetLocallyCreated() above: invalidate any
      // in-flight "+ New chat" create before reloading, so its result can't
      // land in a store that now represents a different database.
      aiChatsData.invalidateForDatabaseSwitch();
      aiChatsData.loadAiChats();
      // Re-hydrate the new database's DatabaseSettingsNode (the previous one was
      // evicted by clearAll) so the Pro-sync variant re-resolves for it.
      this.refreshDatabaseSettings();
    } catch (err) {
      this.error = String(err);
      log.error('Failed to switch database', { id, error: err });
    }
  }

  /**
   * Force-refetch the active database's `DatabaseSettingsNode` into the shared
   * store (bypassing the cache-first `ensureNode` path) so the Pro-sync variant
   * machine re-resolves from fresh `sync_enabled`/`auth_status` values.
   *
   * The node is otherwise read once per app life and then kept fresh only by
   * `node:updated` watch events, which have unrecoverable loss modes (watcher
   * reconnect backoff, broadcast lag drops, failed coalescer refetch) — miss one
   * and the variant is stuck stale, e.g. at `sign-in` after the daemon already
   * flipped `auth_status` to connected (#1674). Callers re-pull on the
   * transitions that matter: sync-status edges, the consent decision, database
   * switch, and initial load.
   *
   * Fire-and-forget: callers must not block on it, and a missing node (older
   * database not yet backfilled) simply leaves the variant at its default.
   */
  refreshDatabaseSettings(): void {
    const epoch = sharedNodeStore.currentEpoch();
    backendAdapter
      .getNode(DATABASE_SETTINGS_NODE_ID)
      .then((node) => {
        // The active database switched while the read was in flight — the row
        // belongs to the previous database, so drop it (see `currentEpoch()`).
        if (!node || sharedNodeStore.currentEpoch() !== epoch) return;
        sharedNodeStore.setNode(
          node,
          { type: 'database', reason: 'refresh-database-settings' },
          true
        );
      })
      .catch((err: unknown) => {
        log.debug('Could not refresh DatabaseSettingsNode', { error: err });
      });
  }

  /**
   * Create a brand-new database and register it, then refresh the list. When
   * `path` is omitted the daemon places the file under its managed directory.
   * Returns the new entry so callers can switch to it.
   */
  async create(name: string, path?: string): Promise<DatabaseInfo | null> {
    return this.mutate(() => invoke<DatabaseInfo>('create_database', { name, path: path ?? null }));
  }

  /** Register an existing database file already on disk, then refresh the list. */
  async register(path: string): Promise<DatabaseInfo | null> {
    return this.mutate(() => invoke<DatabaseInfo>('register_database', { path }));
  }

  /** Rename a registered database's label, then refresh the list. */
  async rename(id: string, name: string): Promise<DatabaseInfo | null> {
    return this.mutate(() => invoke<DatabaseInfo>('rename_database', { id, name }));
  }

  /** Set the daemon-wide default database, then refresh the list. */
  async setDefault(id: string): Promise<DatabaseInfo | null> {
    return this.mutate(() => invoke<DatabaseInfo>('set_default_database', { id }));
  }

  /**
   * Unregister a database (never deletes the file), then refresh the list. If
   * the removed database was the active one, fall back to the daemon default.
   */
  async remove(id: string): Promise<void> {
    this.error = null;
    try {
      await invoke<string>('remove_database', { id });
      await this.load();
      if (this.activeDatabaseId === id) {
        const fallback = this.defaultDatabaseId ?? this.databases[0]?.id ?? null;
        if (fallback) {
          await this.switchTo(fallback);
        } else {
          this.activeDatabaseId = null;
        }
      }
    } catch (err) {
      this.error = String(err);
      log.error('Failed to remove database', { id, error: err });
    }
  }

  /** Run a registry mutation, refresh the list, and return the mutation result. */
  private async mutate(op: () => Promise<DatabaseInfo>): Promise<DatabaseInfo | null> {
    this.error = null;
    try {
      const result = await op();
      await this.load();
      return result;
    } catch (err) {
      this.error = String(err);
      log.error('Database registry operation failed', err);
      return null;
    }
  }
}

export const databaseStore = new DatabaseStore();

// Retry the initial registry load once the daemon becomes reachable, mirroring
// the collections/schemas stores: a `load()` that ran while the daemon was
// still starting fails and leaves `activeDatabaseId` null (and the settings
// node unhydrated) until a manual reload. Guarded on the not-yet-loaded state
// so a background reconnect never re-runs against an established selection.
onDaemonReconnect(() => {
  if (databaseStore.activeDatabaseId === null) {
    void databaseStore.load();
  }
});

/**
 * True when a `database_id` event envelope belongs to the active database.
 * An empty id (single-database / Pro daemon) or an as-yet-unloaded selection
 * always applies — the guard only drops events explicitly tagged for a
 * different database, closing the race where a watch stream open across a
 * switch delivers the previous database's events.
 */
export function isActiveDatabaseEvent(databaseId: string | undefined): boolean {
  if (!databaseId) return true;
  if (databaseStore.activeDatabaseId === null) return true;
  return databaseId === databaseStore.activeDatabaseId;
}
