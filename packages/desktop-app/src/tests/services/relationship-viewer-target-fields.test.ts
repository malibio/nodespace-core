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
 * the same shared `isUserVisibleField` predicate. Fixtures below are copied
 * verbatim from core_schemas.rs.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';

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

const getSchema = backendAdapter.getSchema as unknown as ReturnType<typeof vi.fn>;

function field(partial: Partial<SchemaField> & { name: string; type: string }): SchemaField {
  return { protection: 'user', indexed: false, friendlyName: partial.name, ...partial };
}

function schemaWith(id: string, isCore: boolean, fields: SchemaField[]): SchemaNode {
  return {
    id,
    content: id,
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    isCore,
    schemaVersion: 1,
    fields
  };
}

/** person, verbatim from core_schemas.rs. */
const PERSON_FIELDS: SchemaField[] = [
  field({ name: 'name', friendlyName: 'Name', type: 'string', protection: 'core' }),
  field({ name: 'email', friendlyName: 'Email', type: 'string', protection: 'core' }),
  field({
    name: '_possible_duplicate',
    friendlyName: 'Possible duplicate',
    type: 'boolean',
    protection: 'system',
    default: false
  })
];

/** ai-chat's top-level fields, verbatim from core_schemas.rs: 4 visible, 6 system. */
const AI_CHAT_FIELDS: SchemaField[] = [
  field({ name: 'provider', friendlyName: 'Provider', type: 'string', protection: 'core' }),
  field({ name: 'model', friendlyName: 'Model', type: 'string', protection: 'core' }),
  field({ name: 'status', friendlyName: 'Conversation status', type: 'enum', protection: 'core' }),
  field({ name: 'last_active', friendlyName: 'Last active', type: 'date', protection: 'system' }),
  field({
    name: 'context_tokens',
    friendlyName: 'Context tokens',
    type: 'number',
    protection: 'system'
  }),
  field({
    name: 'created_nodes',
    friendlyName: 'Created nodes',
    type: 'array',
    protection: 'system'
  }),
  field({ name: 'messages', friendlyName: 'Messages', type: 'array', protection: 'core' }),
  field({
    name: 'capture:session_id',
    friendlyName: 'Session id',
    type: 'string',
    protection: 'system'
  }),
  field({
    name: 'capture:transcript',
    friendlyName: 'Transcript',
    type: 'text',
    protection: 'system'
  }),
  field({ name: 'capture:summary', friendlyName: 'Summary', type: 'text', protection: 'system' })
];

beforeEach(() => {
  getSchema.mockReset();
});

describe('fetchTargetSchemaFields — protection-level filtering', () => {
  it('omits person’s system-managed _possible_duplicate', async () => {
    getSchema.mockResolvedValue(schemaWith('person', true, PERSON_FIELDS));

    expect(await fetchTargetSchemaFields('person')).toEqual(['name', 'email']);
  });

  it('omits all six of ai-chat’s system fields, transcript included', async () => {
    getSchema.mockResolvedValue(schemaWith('ai-chat', true, AI_CHAT_FIELDS));

    const names = await fetchTargetSchemaFields('ai-chat');

    for (const name of [
      'last_active',
      'context_tokens',
      'created_nodes',
      'capture:session_id',
      'capture:transcript',
      'capture:summary'
    ]) {
      expect(names).not.toContain(name);
    }
    // Non-system names survive in schema order — `messages` still follows
    // `status` despite three system fields removed from between them.
    expect(names).toEqual(['provider', 'model', 'status', 'messages']);
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
