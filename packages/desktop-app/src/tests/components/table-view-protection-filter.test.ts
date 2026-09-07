/**
 * TableView column derivation — protection-level filtering.
 *
 * TableView used to iterate `schema.fields` unconditionally, so every
 * `protection: 'system'` field became a user-facing column. That put a
 * "Possible duplicate" column on person (a local-only convergence marker that
 * is permanently empty on a local-only install) and would surface six columns
 * on ai-chat — including `capture:transcript`, raw PTY scrollback documented as
 * possibly containing secrets, tokens and absolute paths.
 *
 * The detail form already filtered these out (see
 * generic-schema-form-protection-filter.test.ts), so the two views disagreed.
 * These tests mirror that file and pin the table side, using the real field
 * shapes from core_schemas.rs.
 */
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';

import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () => mockTauriCore());

import TableView from '$lib/components/query/table-view.svelte';

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

function headerTexts(container: HTMLElement): string[] {
  return [...container.querySelectorAll('thead th')].map((th) => th.textContent?.trim() ?? '');
}

afterEach(() => {
  cleanup();
});

describe('TableView — protection-level filtering', () => {
  it('omits person’s system-managed "Possible duplicate" column', () => {
    const { container, getByText, queryByText } = render(TableView, {
      props: {
        nodeIds: [],
        schema: schemaWith('person', true, PERSON_FIELDS),
        fieldSchemaMap: new Map(),
        onRowClick: vi.fn()
      }
    });

    expect(getByText('Name')).toBeTruthy();
    expect(getByText('Email')).toBeTruthy();
    expect(queryByText('Possible duplicate')).toBeNull();
    // The leading '' column is the content/title link column.
    expect(headerTexts(container)).toEqual(['', 'Name', 'Email']);
  });

  it('omits all six of ai-chat’s system fields, transcript included', () => {
    const { container, queryByText } = render(TableView, {
      props: {
        nodeIds: [],
        schema: schemaWith('ai-chat', true, AI_CHAT_FIELDS),
        fieldSchemaMap: new Map(),
        onRowClick: vi.fn()
      }
    });

    for (const label of [
      'Last active',
      'Context tokens',
      'Created nodes',
      'Session id',
      'Transcript',
      'Summary'
    ]) {
      expect(queryByText(label)).toBeNull();
    }
    // Non-system fields survive, in schema order — note `messages` still follows
    // `status` even though three system fields were removed from between them.
    expect(headerTexts(container)).toEqual([
      '',
      'Provider',
      'Model',
      'Conversation status',
      'Messages'
    ]);
  });

  it('applies the same filter to a user-defined (schema-only) type', () => {
    const { container, queryByText } = render(TableView, {
      props: {
        nodeIds: [],
        schema: schemaWith('venue', false, [
          field({ name: 'capacity', friendlyName: 'Capacity', type: 'number' }),
          field({
            name: '_internal_marker',
            friendlyName: 'Internal marker',
            type: 'boolean',
            protection: 'system'
          }),
          field({ name: 'address', friendlyName: 'Address', type: 'string' })
        ]),
        fieldSchemaMap: new Map(),
        onRowClick: vi.fn()
      }
    });

    expect(queryByText('Internal marker')).toBeNull();
    expect(headerTexts(container)).toEqual(['', 'Capacity', 'Address']);
  });

  it('renders only the content column when every schema field is system-protected', () => {
    const { container } = render(TableView, {
      props: {
        nodeIds: [],
        schema: schemaWith('all-system', false, [
          field({ name: 'a', friendlyName: 'A', type: 'string', protection: 'system' }),
          field({ name: 'b', friendlyName: 'B', type: 'string', protection: 'system' })
        ]),
        fieldSchemaMap: new Map(),
        onRowClick: vi.fn()
      }
    });

    expect(headerTexts(container)).toEqual(['']);
  });

  it('leaves a schema with no system fields completely unchanged', () => {
    const { container } = render(TableView, {
      props: {
        nodeIds: [],
        schema: schemaWith('task', true, [
          field({ name: 'status', friendlyName: 'Status', type: 'enum', protection: 'core' }),
          field({ name: 'priority', friendlyName: 'Priority', type: 'enum', protection: 'core' }),
          field({ name: 'due_date', friendlyName: 'Due date', type: 'date', protection: 'core' })
        ]),
        fieldSchemaMap: new Map(),
        onRowClick: vi.fn()
      }
    });

    expect(headerTexts(container)).toEqual(['', 'Status', 'Priority', 'Due date']);
  });
});
