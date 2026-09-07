/**
 * fetchTargetSchemaFields — protection-level filtering.
 *
 * The returned names become user-facing column offerings in the relationship
 * modal's per-group "Columns" picker (resolveColumnCandidates → candidatesFor).
 * Deriving them from `schema.fields` unfiltered offered a column for every
 * `protection: 'system'` field on the target type: "Possible duplicate" for a
 * group targeting person, and six for ai-chat including `capture:transcript`,
 * raw PTY scrollback documented as possibly containing secrets and tokens.
 *
 * Same class of bug the table view had, on a different surface — so the fix is
 * the same shared `isUserVisibleField` predicate, and these cases mirror
 * `table-view-protection-filter.test.ts`. Both files draw their core-schema
 * shapes from the shared fixtures in `../helpers/schema-fixtures`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () => mockTauriCore());

vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: { getSchema: vi.fn() }
}));

import { backendAdapter } from '$lib/services/backend-adapter';
import { fetchTargetSchemaFields } from '$lib/services/relationship-viewer-service';
import {
  field,
  schemaWith,
  PERSON_FIELDS,
  PERSON_VISIBLE_NAMES,
  AI_CHAT_FIELDS,
  AI_CHAT_VISIBLE_NAMES,
  AI_CHAT_SYSTEM_NAMES
} from '../helpers/schema-fixtures';

const getSchema = vi.mocked(backendAdapter.getSchema);

beforeEach(() => {
  getSchema.mockReset();
});

describe('fetchTargetSchemaFields — protection-level filtering', () => {
  it('omits person’s system-managed _possible_duplicate', async () => {
    getSchema.mockResolvedValue(schemaWith('person', true, PERSON_FIELDS));

    expect(await fetchTargetSchemaFields('person')).toEqual(PERSON_VISIBLE_NAMES);
  });

  it('omits all six of ai-chat’s system fields, transcript included', async () => {
    getSchema.mockResolvedValue(schemaWith('ai-chat', true, AI_CHAT_FIELDS));

    const names = await fetchTargetSchemaFields('ai-chat');

    for (const name of AI_CHAT_SYSTEM_NAMES) {
      expect(names).not.toContain(name);
    }
    // Non-system names survive in schema order — `messages` still follows
    // `status` despite three system fields removed from between them.
    expect(names).toEqual(AI_CHAT_VISIBLE_NAMES);
  });

  it('applies the same filter to a user-defined (schema-only) target type', async () => {
    getSchema.mockResolvedValue(
      schemaWith('venue', false, [
        field({ name: 'capacity', friendlyName: 'Capacity', type: 'number' }),
        field({
          name: '_internal_marker',
          friendlyName: 'Internal marker',
          type: 'boolean',
          protection: 'system'
        }),
        field({ name: 'address', friendlyName: 'Address', type: 'string' })
      ])
    );

    expect(await fetchTargetSchemaFields('venue')).toEqual(['capacity', 'address']);
  });

  it('returns [] when every field on the target schema is system-protected', async () => {
    getSchema.mockResolvedValue(
      schemaWith('all-system', false, [
        field({ name: 'a', friendlyName: 'A', type: 'string', protection: 'system' }),
        field({ name: 'b', friendlyName: 'B', type: 'string', protection: 'system' })
      ])
    );

    expect(await fetchTargetSchemaFields('all-system')).toEqual([]);
  });

  it('leaves a target schema with no system fields completely unchanged', async () => {
    getSchema.mockResolvedValue(
      schemaWith('task', true, [
        field({ name: 'status', friendlyName: 'Status', type: 'enum', protection: 'core' }),
        field({ name: 'priority', friendlyName: 'Priority', type: 'enum', protection: 'core' }),
        field({ name: 'due_date', friendlyName: 'Due date', type: 'date', protection: 'core' })
      ])
    );

    expect(await fetchTargetSchemaFields('task')).toEqual(['status', 'priority', 'due_date']);
  });

  it('still returns [] for a schema with no fields at all', async () => {
    getSchema.mockResolvedValue(schemaWith('bare', false, []));

    expect(await fetchTargetSchemaFields('bare')).toEqual([]);
  });
});
