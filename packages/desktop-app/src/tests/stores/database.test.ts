import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

// Collaborators exercised by switchTo — stubbed so we can assert the flush →
// switch → clear → reset → reload orchestration without their real behavior.
const flushAllPendingSaves = vi.fn((..._a: unknown[]) => Promise.resolve(new Set<string>()));
const clearAll = vi.fn((..._a: unknown[]) => undefined);
vi.mock('$lib/services/shared-node-store.svelte', () => ({
  sharedNodeStore: {
    flushAllPendingSaves: (...a: unknown[]) => flushAllPendingSaves(...a),
    clearAll: (...a: unknown[]) => clearAll(...a)
  }
}));

const structureTreeClear = vi.fn((..._a: unknown[]) => undefined);
vi.mock('$lib/stores/reactive-structure-tree.svelte', () => ({
  structureTree: { clear: (...a: unknown[]) => structureTreeClear(...a) }
}));

const loadCollections = vi.fn((..._a: unknown[]) => undefined);
vi.mock('$lib/stores/collections.svelte', () => ({
  collectionsData: { loadCollections: (...a: unknown[]) => loadCollections(...a) }
}));

const loadSchemas = vi.fn((..._a: unknown[]) => undefined);
vi.mock('$lib/stores/schemas.svelte', () => ({
  schemasData: { loadSchemas: (...a: unknown[]) => loadSchemas(...a) }
}));

const clearAllTabs = vi.fn((..._a: unknown[]) => undefined);
const addTab = vi.fn((..._a: unknown[]) => undefined);
vi.mock('$lib/stores/navigation.svelte', () => ({
  clearAllTabs: (...a: unknown[]) => clearAllTabs(...a),
  addTab: (...a: unknown[]) => addTab(...a),
  DAILY_JOURNAL_TAB_ID: 'daily-journal',
  DEFAULT_PANE_ID: 'pane-1'
}));

vi.mock('$lib/utils/date-formatting', () => ({
  formatDateISO: () => '2026-07-09'
}));

import {
  databaseStore,
  isActiveDatabaseEvent,
  type DatabaseInfo
} from '$lib/stores/database.svelte';

function db(id: string, overrides: Partial<DatabaseInfo> = {}): DatabaseInfo {
  return {
    id,
    name: `db-${id}`,
    path: `/tmp/${id}.db`,
    isDefault: false,
    status: 'closed',
    createdAt: '2026-01-01T00:00:00Z',
    lastOpenedAt: null,
    ...overrides
  };
}

describe('Database Store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    databaseStore.databases = [];
    databaseStore.activeDatabaseId = null;
    databaseStore.defaultDatabaseId = null;
    databaseStore.error = null;
  });

  describe('load', () => {
    it('lists databases and initializes the active id to the default', async () => {
      mockInvoke.mockResolvedValueOnce({
        databases: [db('a'), db('b', { isDefault: true })],
        defaultDatabaseId: 'b'
      });

      await databaseStore.load();

      expect(mockInvoke).toHaveBeenCalledWith('list_databases');
      expect(databaseStore.databases).toHaveLength(2);
      expect(databaseStore.activeDatabaseId).toBe('b');
      expect(databaseStore.activeDatabase?.id).toBe('b');
    });

    it('falls back to the first database when no default is set', async () => {
      mockInvoke.mockResolvedValueOnce({
        databases: [db('a'), db('b')],
        defaultDatabaseId: ''
      });

      await databaseStore.load();

      expect(databaseStore.activeDatabaseId).toBe('a');
    });

    it('preserves an existing selection across a refresh', async () => {
      databaseStore.activeDatabaseId = 'a';
      mockInvoke.mockResolvedValueOnce({
        databases: [db('a'), db('b', { isDefault: true })],
        defaultDatabaseId: 'b'
      });

      await databaseStore.load();

      expect(databaseStore.activeDatabaseId).toBe('a');
    });

    it('records an error when the list fails', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('boom'));
      await databaseStore.load();
      expect(databaseStore.error).toContain('boom');
    });
  });

  describe('switchTo', () => {
    beforeEach(() => {
      databaseStore.databases = [db('a'), db('b')];
      databaseStore.activeDatabaseId = 'a';
    });

    it('flushes, switches, clears caches, resets tabs, and reloads', async () => {
      mockInvoke.mockResolvedValueOnce(undefined); // set_active_database

      await databaseStore.switchTo('b');

      expect(flushAllPendingSaves).toHaveBeenCalledOnce();
      expect(mockInvoke).toHaveBeenCalledWith('set_active_database', { id: 'b' });
      expect(databaseStore.activeDatabaseId).toBe('b');
      expect(clearAll).toHaveBeenCalledOnce();
      expect(structureTreeClear).toHaveBeenCalledOnce();
      expect(clearAllTabs).toHaveBeenCalledOnce();
      expect(addTab).toHaveBeenCalledOnce();
      expect(loadCollections).toHaveBeenCalledOnce();
      expect(loadSchemas).toHaveBeenCalledOnce();
    });

    it('no-ops when switching to the already-active database', async () => {
      await databaseStore.switchTo('a');
      expect(mockInvoke).not.toHaveBeenCalled();
      expect(clearAll).not.toHaveBeenCalled();
    });

    it('flushes pending saves BEFORE re-pointing the routed clients', async () => {
      const order: string[] = [];
      flushAllPendingSaves.mockImplementationOnce(async () => {
        order.push('flush');
        return new Set<string>();
      });
      mockInvoke.mockImplementationOnce(async () => {
        order.push('switch');
      });

      await databaseStore.switchTo('b');

      expect(order).toEqual(['flush', 'switch']);
    });

    it('lets the latest of two concurrent switches win and bails the superseded one', async () => {
      databaseStore.databases = [db('a'), db('b'), db('c')];
      databaseStore.activeDatabaseId = 'a';
      mockInvoke.mockResolvedValue(undefined); // set_active_database (winner only)

      // Fire both without awaiting: each captures its switch token synchronously
      // before the first await, so the later call supersedes the earlier one.
      const first = databaseStore.switchTo('b');
      const second = databaseStore.switchTo('c');
      await Promise.all([first, second]);

      expect(databaseStore.activeDatabaseId).toBe('c');
      // The superseded switch bailed before re-pointing the routed clients.
      expect(mockInvoke).not.toHaveBeenCalledWith('set_active_database', { id: 'b' });
      expect(mockInvoke).toHaveBeenCalledWith('set_active_database', { id: 'c' });
      // Only the winner cleared caches and reloaded — no stale mid-switch state.
      expect(clearAll).toHaveBeenCalledOnce();
      expect(loadCollections).toHaveBeenCalledOnce();
    });
  });

  describe('registry mutations', () => {
    it('create refreshes the list and returns the new entry', async () => {
      const created = db('c', { name: 'Work' });
      mockInvoke
        .mockResolvedValueOnce(created) // create_database
        .mockResolvedValueOnce({ databases: [created], defaultDatabaseId: '' }); // load

      const result = await databaseStore.create('Work');

      expect(mockInvoke).toHaveBeenCalledWith('create_database', { name: 'Work', path: null });
      expect(result?.id).toBe('c');
      expect(databaseStore.databases).toHaveLength(1);
    });

    it('register passes the path through', async () => {
      const registered = db('d');
      mockInvoke
        .mockResolvedValueOnce(registered)
        .mockResolvedValueOnce({ databases: [registered], defaultDatabaseId: '' });

      await databaseStore.register('/tmp/d.db');

      expect(mockInvoke).toHaveBeenCalledWith('register_database', { path: '/tmp/d.db' });
    });

    it('remove unregisters and refreshes', async () => {
      databaseStore.databases = [db('a'), db('b')];
      databaseStore.activeDatabaseId = 'a';
      mockInvoke
        .mockResolvedValueOnce('b') // remove_database
        .mockResolvedValueOnce({ databases: [db('a')], defaultDatabaseId: '' }); // load

      await databaseStore.remove('b');

      expect(mockInvoke).toHaveBeenCalledWith('remove_database', { id: 'b' });
      expect(databaseStore.databases).toHaveLength(1);
    });
  });

  describe('isActiveDatabaseEvent', () => {
    it('passes events with no database id (single-database / Pro daemon)', () => {
      databaseStore.activeDatabaseId = 'a';
      expect(isActiveDatabaseEvent(undefined)).toBe(true);
      expect(isActiveDatabaseEvent('')).toBe(true);
    });

    it('passes any event before a selection is loaded', () => {
      databaseStore.activeDatabaseId = null;
      expect(isActiveDatabaseEvent('a')).toBe(true);
    });

    it('drops events tagged for a different database', () => {
      databaseStore.activeDatabaseId = 'a';
      expect(isActiveDatabaseEvent('a')).toBe(true);
      expect(isActiveDatabaseEvent('b')).toBe(false);
    });
  });
});
