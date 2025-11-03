/**
 * Tab Persistence Service
 *
 * Manages persistence of tab state to localStorage/Tauri store.
 * Handles loading, saving, and migration of tab state across application restarts.
 */

import type { TabState } from '$lib/stores/navigation';

/**
 * Persisted tab state structure with versioning for future migrations
 */
export interface PersistedTabState {
  version: 1;
  tabs: TabState['tabs'];
  panes: TabState['panes'];
  activePaneId: string;
  activeTabIds: Record<string, string>;
}

/**
 * Service for persisting and loading tab state
 */
export class TabPersistenceService {
  private static STORAGE_KEY = 'nodespace:tab-state';
  private static readonly DEBOUNCE_MS = 500;

  private static saveTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * Save tab state to persistent storage (debounced)
   * @param state - The current tab state to save
   */
  static save(state: TabState): void {
    // Clear existing timer
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
    }

    // Debounce the save operation
    this.saveTimer = setTimeout(() => {
      this.saveImmediate(state);
    }, this.DEBOUNCE_MS);
  }

  /**
   * Save tab state immediately without debouncing
   * @param state - The current tab state to save
   */
  private static saveImmediate(state: TabState): void {
    try {
      const persisted: PersistedTabState = {
        version: 1,
        tabs: state.tabs,
        panes: state.panes,
        activePaneId: state.activePaneId,
        activeTabIds: state.activeTabIds
      };

      // For now, use localStorage (Tauri store integration can be added later)
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(persisted));

      console.log('[TabPersistence] State saved successfully');
    } catch (error) {
      console.error('[TabPersistence] Failed to save state:', error);
    }
  }

  /**
   * Load persisted tab state from storage
   * @returns The loaded tab state or null if no valid state exists
   */
  static load(): PersistedTabState | null {
    try {
      const stored = localStorage.getItem(this.STORAGE_KEY);

      if (!stored) {
        console.log('[TabPersistence] No saved state found');
        return null;
      }

      const parsed = JSON.parse(stored) as PersistedTabState;

      // Validate the structure
      if (!this.isValidState(parsed)) {
        console.warn('[TabPersistence] Invalid state structure, ignoring');
        return null;
      }

      // Handle version migrations
      const migrated = this.migrate(parsed);

      console.log('[TabPersistence] State loaded successfully');
      return migrated;
    } catch (error) {
      console.error('[TabPersistence] Failed to load state:', error);
      return null;
    }
  }

  /**
   * Validate that the loaded state has the expected structure
   * @param state - The state to validate
   * @returns True if the state is valid
   */
  private static isValidState(state: unknown): state is PersistedTabState {
    if (!state || typeof state !== 'object') {
      return false;
    }

    const s = state as Record<string, unknown>;

    return (
      typeof s.version === 'number' &&
      Array.isArray(s.tabs) &&
      Array.isArray(s.panes) &&
      typeof s.activePaneId === 'string' &&
      typeof s.activeTabIds === 'object' &&
      s.activeTabIds !== null
    );
  }

  /**
   * Migrate state from older versions to current version
   * @param state - The state to migrate
   * @returns The migrated state
   */
  private static migrate(state: PersistedTabState): PersistedTabState {
    // Currently only version 1 exists, so no migration needed
    // Future versions can add migration logic here:
    //
    // if (state.version === 1) {
    //   // Migrate from v1 to v2
    //   return { ...state, version: 2, newField: defaultValue };
    // }

    return state;
  }

  /**
   * Clear all persisted tab state
   */
  static clear(): void {
    try {
      localStorage.removeItem(this.STORAGE_KEY);
      console.log('[TabPersistence] State cleared');
    } catch (error) {
      console.error('[TabPersistence] Failed to clear state:', error);
    }
  }

  /**
   * Flush any pending saves immediately
   * Useful for testing or before app shutdown
   */
  static flush(): void {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
  }
}
