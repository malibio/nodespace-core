/**
 * Relationship View Settings Persistence Service
 *
 * Persists the relationship-viewer modal's per-group presentation preferences
 * (which columns show, in what order, plus sort and filter) to localStorage,
 * mirroring the versioned-envelope pattern of `layout-persistence-service.ts`
 * and `tab-persistence-service.ts`.
 *
 * ## Why keyed by (nodeType, relationshipName, direction) — not the node instance
 *
 * A view setting is a per-USER PRESENTATION preference for a schema-declared
 * relationship: "how I like to see a task's outbound `assigned_to` laid out".
 * That preference is stable across every instance of the node type — a user who
 * arranges the columns for `assigned_to` on one task expects the same layout on
 * every task. So the key is the relationship's IDENTITY (node type + relationship
 * name + direction), not the id of the node the modal happens to be centered on.
 *
 * ## Why localStorage — not the schema node
 *
 * The setting is deliberately NOT stored on the schema node. The schema is shared
 * graph data that syncs between users, whereas a column layout / sort / filter is
 * a local, personal viewing choice. Storing it on the schema would broadcast one
 * user's presentation to everyone and turn a UI tweak into a graph write. Keeping
 * it in localStorage keeps it local and per-user, exactly like layout and tab
 * state.
 */

import { createLogger } from '$lib/utils/logger';
import type { RelationshipDirection } from './relationship-grouping';
import {
  defaultViewSettings,
  type RelationshipViewSettings,
  type SortDirection
} from './relationship-view-settings';

const log = createLogger('RelationshipViewSettings');

/** Versioned localStorage envelope holding every relationship's settings. */
interface PersistedEnvelope {
  version: number;
  entries: Record<string, RelationshipViewSettings>;
}

export class RelationshipViewSettingsService {
  private static readonly STORAGE_KEY = 'ns:rel-view-settings';
  private static readonly VERSION = 1;

  /**
   * Composite key identifying a schema-declared relationship's view. Includes
   * `targetType` because a node can render two distinct inbound groups that share
   * a (name, direction) but differ by the declaring source type (the aggregation
   * emits one inbound group per source_type); without it their view settings
   * would collide. `*` stands in for an untyped target.
   */
  static buildKey(
    nodeType: string,
    relationshipName: string,
    direction: RelationshipDirection,
    targetType: string | null
  ): string {
    return `${nodeType}::${relationshipName}::${direction}::${targetType ?? '*'}`;
  }

  /**
   * Load the settings for one relationship, returning the neutral default when
   * none are stored (or storage is unreadable/corrupt).
   */
  static get(
    nodeType: string,
    relationshipName: string,
    direction: RelationshipDirection,
    targetType: string | null
  ): RelationshipViewSettings {
    const envelope = this.readEnvelope();
    const stored = envelope.entries[this.buildKey(nodeType, relationshipName, direction, targetType)];
    return stored ? this.normalize(stored) : defaultViewSettings();
  }

  /** Save the settings for one relationship, leaving every other entry intact. */
  static set(
    nodeType: string,
    relationshipName: string,
    direction: RelationshipDirection,
    targetType: string | null,
    settings: RelationshipViewSettings
  ): void {
    try {
      const envelope = this.readEnvelope();
      envelope.entries[this.buildKey(nodeType, relationshipName, direction, targetType)] =
        this.normalize(settings);
      localStorage.setItem(this.STORAGE_KEY, JSON.stringify(envelope));
    } catch (error) {
      log.error('Failed to save view settings:', error);
    }
  }

  /** Remove all persisted relationship view settings. */
  static clear(): void {
    try {
      localStorage.removeItem(this.STORAGE_KEY);
    } catch (error) {
      log.error('Failed to clear view settings:', error);
    }
  }

  private static readEnvelope(): PersistedEnvelope {
    const empty: PersistedEnvelope = { version: this.VERSION, entries: {} };
    try {
      const raw = localStorage.getItem(this.STORAGE_KEY);
      if (!raw) return empty;
      const parsed = JSON.parse(raw) as unknown;
      if (!this.isValidEnvelope(parsed)) {
        log.warn('Invalid view-settings envelope, ignoring');
        return empty;
      }
      return parsed;
    } catch (error) {
      log.error('Failed to load view settings:', error);
      return empty;
    }
  }

  private static isValidEnvelope(value: unknown): value is PersistedEnvelope {
    if (!value || typeof value !== 'object') return false;
    const e = value as Record<string, unknown>;
    return (
      typeof e.version === 'number' &&
      typeof e.entries === 'object' &&
      e.entries !== null &&
      !Array.isArray(e.entries)
    );
  }

  /**
   * Coerce (possibly corrupt) stored data into the known settings shape so a
   * malformed entry degrades to sensible values rather than propagating.
   */
  private static normalize(settings: RelationshipViewSettings): RelationshipViewSettings {
    const columns = Array.isArray(settings.columns)
      ? settings.columns.filter((c): c is string => typeof c === 'string')
      : null;

    const sort =
      settings.sort &&
      typeof settings.sort.column === 'string' &&
      (settings.sort.direction === 'asc' || settings.sort.direction === 'desc')
        ? { column: settings.sort.column, direction: settings.sort.direction as SortDirection }
        : null;

    const filter =
      settings.filter &&
      typeof settings.filter.column === 'string' &&
      typeof settings.filter.value === 'string'
        ? { column: settings.filter.column, value: settings.filter.value }
        : null;

    return { columns, sort, filter };
  }
}
