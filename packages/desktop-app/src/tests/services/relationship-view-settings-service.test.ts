import { describe, it, expect, beforeEach } from 'vitest';
import { RelationshipViewSettingsService } from '$lib/services/relationship-view-settings-service';
import {
  defaultViewSettings,
  edgeColumnToken,
  LABEL_COLUMN,
  type RelationshipViewSettings
} from '$lib/services/relationship-view-settings';

const STORAGE_KEY = 'ns:rel-view-settings';

describe('RelationshipViewSettingsService', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('buildKey', () => {
    it('combines node type, relationship name, and direction', () => {
      expect(RelationshipViewSettingsService.buildKey('task', 'assigned_to', 'out', 'person')).toBe(
        'task::assigned_to::out::person'
      );
    });
  });

  describe('get', () => {
    it('returns the neutral default when nothing is stored', () => {
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual(
        defaultViewSettings()
      );
    });

    it('returns the default when storage holds invalid JSON', () => {
      localStorage.setItem(STORAGE_KEY, 'not-json{]');
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual(
        defaultViewSettings()
      );
    });

    it('returns the default when the envelope shape is invalid', () => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ version: 1, entries: [1, 2, 3] }));
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual(
        defaultViewSettings()
      );
    });
  });

  describe('set / get round-trip', () => {
    it('persists and retrieves settings for a relationship', () => {
      const settings: RelationshipViewSettings = {
        columns: [edgeColumnToken('role'), LABEL_COLUMN],
        sort: { column: edgeColumnToken('role'), direction: 'desc' },
        filter: { column: LABEL_COLUMN, value: 'sarah' }
      };
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', settings);
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual(settings);
    });

    it('overwrites the settings for the same key', () => {
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', {
        columns: [edgeColumnToken('role')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', {
        columns: null,
        sort: { column: LABEL_COLUMN, direction: 'asc' },
        filter: null
      });
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual({
        columns: null,
        sort: { column: LABEL_COLUMN, direction: 'asc' },
        filter: null
      });
    });
  });

  describe('per-key isolation', () => {
    it('keeps directions, relationships, and node types independent', () => {
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', {
        columns: [edgeColumnToken('role')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.set('task', 'assigned_to', 'in', 'person', {
        columns: [edgeColumnToken('since')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.set('project', 'assigned_to', 'out', 'person', {
        columns: null,
        sort: { column: LABEL_COLUMN, direction: 'desc' },
        filter: null
      });

      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person').columns).toEqual([
        edgeColumnToken('role')
      ]);
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'in', 'person').columns).toEqual([
        edgeColumnToken('since')
      ]);
      expect(RelationshipViewSettingsService.get('project', 'assigned_to', 'out', 'person').sort).toEqual({
        column: LABEL_COLUMN,
        direction: 'desc'
      });
      // An untouched relationship still reports the default.
      expect(RelationshipViewSettingsService.get('task', 'blocks', 'out', 'person')).toEqual(
        defaultViewSettings()
      );
    });

    it('keeps two inbound groups that share a name/direction but differ by target type independent', () => {
      // A person renders one inbound `assigned_to` group per declaring source
      // type (task, bug); their settings must not collide.
      RelationshipViewSettingsService.set('person', 'assigned_to', 'in', 'task', {
        columns: [edgeColumnToken('role')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.set('person', 'assigned_to', 'in', 'bug', {
        columns: [edgeColumnToken('severity')],
        sort: null,
        filter: null
      });
      expect(
        RelationshipViewSettingsService.get('person', 'assigned_to', 'in', 'task').columns
      ).toEqual([edgeColumnToken('role')]);
      expect(
        RelationshipViewSettingsService.get('person', 'assigned_to', 'in', 'bug').columns
      ).toEqual([edgeColumnToken('severity')]);
    });

    it('does not disturb existing entries when writing a new one', () => {
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', {
        columns: [edgeColumnToken('role')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.set('task', 'blocks', 'out', 'person', {
        columns: [edgeColumnToken('reason')],
        sort: null,
        filter: null
      });
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person').columns).toEqual([
        edgeColumnToken('role')
      ]);
    });
  });

  describe('normalize on load', () => {
    it('drops a corrupt sort/filter and coerces columns while keeping valid parts', () => {
      const key = RelationshipViewSettingsService.buildKey('task', 'assigned_to', 'out', 'person');
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify({
          version: 1,
          entries: {
            [key]: {
              columns: [edgeColumnToken('role'), 42, LABEL_COLUMN],
              sort: { column: edgeColumnToken('role'), direction: 'sideways' },
              filter: { column: LABEL_COLUMN, value: 'ok' }
            }
          }
        })
      );
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual({
        columns: [edgeColumnToken('role'), LABEL_COLUMN],
        sort: null,
        filter: { column: LABEL_COLUMN, value: 'ok' }
      });
    });
  });

  describe('clear', () => {
    it('removes all persisted settings', () => {
      RelationshipViewSettingsService.set('task', 'assigned_to', 'out', 'person', {
        columns: [edgeColumnToken('role')],
        sort: null,
        filter: null
      });
      RelationshipViewSettingsService.clear();
      expect(RelationshipViewSettingsService.get('task', 'assigned_to', 'out', 'person')).toEqual(
        defaultViewSettings()
      );
      expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
    });
  });
});
