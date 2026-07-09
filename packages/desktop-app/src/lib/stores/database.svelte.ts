import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';
import { collectionsData } from '$lib/stores/collections.svelte';
import { schemasData } from '$lib/stores/schemas.svelte';
import {
  clearAllTabs,
  addTab,
  DAILY_JOURNAL_TAB_ID,
  DEFAULT_PANE_ID
} from '$lib/stores/navigation.svelte';
import { formatDateISO } from '$lib/utils/date-formatting';

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
}

interface DatabaseListing {
  databases: DatabaseInfo[];
  defaultDatabaseId: string;
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
    try {
      const listing = await invoke<DatabaseListing>('list_databases');
      this.databases = listing.databases;
      this.defaultDatabaseId = listing.defaultDatabaseId || null;

      if (this.activeDatabaseId === null) {
        // Prefer the daemon default; fall back to the first registered database
        // so the switcher always shows a concrete selection.
        this.activeDatabaseId =
          this.defaultDatabaseId ?? this.databases[0]?.id ?? null;
      }
    } catch (err) {
      this.error = String(err);
      log.error('Failed to load databases', err);
    } finally {
      this.loading = false;
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

      // Evict the previous database's cached data. NOTE: a read (e.g.
      // loadChildren) dispatched against the previous database *before* this
      // switch whose response resolves after this clear can still setNode its
      // rows into the now-active store; such orphans are unreferenced by the
      // new database's tree but may surface via global search until the next
      // reload. Fully closing this needs per-request database tagging — tracked
      // as follow-on hardening.
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

      // Reload the sidebar from the new database.
      collectionsData.loadCollections();
      schemasData.loadSchemas();
    } catch (err) {
      this.error = String(err);
      log.error('Failed to switch database', { id, error: err });
    }
  }

  /**
   * Create a brand-new database and register it, then refresh the list. When
   * `path` is omitted the daemon places the file under its managed directory.
   * Returns the new entry so callers can switch to it.
   */
  async create(name: string, path?: string): Promise<DatabaseInfo | null> {
    return this.mutate(() =>
      invoke<DatabaseInfo>('create_database', { name, path: path ?? null })
    );
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
  private async mutate(
    op: () => Promise<DatabaseInfo>
  ): Promise<DatabaseInfo | null> {
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
