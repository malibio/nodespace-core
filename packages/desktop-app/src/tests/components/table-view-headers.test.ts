/**
 * TableView column headers — friendly_name is the sole label source.
 *
 * Regression coverage for the friendly_name/description split (schema
 * property naming): headers must read `friendlyName` unconditionally, never
 * `description` (now LLM-facing prose) and never a name-derived regex label.
 * The person schema is the concrete acceptance-criteria example — its fields
 * carry deliberately verbose `description` text that must NOT leak into the
 * column header.
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

function schemaWith(fields: SchemaField[]): SchemaNode {
  return {
    id: 'person',
    content: 'Person',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    isCore: true,
    schemaVersion: 1,
    fields
  };
}

afterEach(() => {
  cleanup();
});

describe('TableView column headers', () => {
  it('reads friendlyName, not description, for the person schema (Name / Email)', () => {
    const schema = schemaWith([
      field({
        name: 'name',
        friendlyName: 'Name',
        type: 'string',
        description: 'Display name; optional — a person may exist before a name is set'
      }),
      field({
        name: 'email',
        friendlyName: 'Email',
        type: 'string',
        description:
          'Email address; optional at schema level, required in practice for invited teammates'
      })
    ]);

    const { getByText, queryByText } = render(TableView, {
      props: { nodeIds: [], schema, fieldSchemaMap: new Map(), onRowClick: vi.fn() }
    });

    expect(getByText('Name')).toBeTruthy();
    expect(getByText('Email')).toBeTruthy();
    // The verbose description prose must never leak into a header.
    expect(queryByText(/a person may exist before a name is set/)).toBeNull();
    expect(queryByText(/required in practice for invited teammates/)).toBeNull();
  });

  it('reads friendlyName for task-shaped fields unchanged from their prior label text', () => {
    const schema = schemaWith([
      field({
        name: 'status',
        friendlyName: 'Status',
        type: 'enum',
        description: 'Current workflow state of the task: open, in_progress, done, or cancelled.'
      }),
      field({
        name: 'due_date',
        friendlyName: 'Due date',
        type: 'date',
        description: 'Date by which the task should be completed.'
      })
    ]);

    const { getByText } = render(TableView, {
      props: { nodeIds: [], schema, fieldSchemaMap: new Map(), onRowClick: vi.fn() }
    });

    expect(getByText('Status')).toBeTruthy();
    expect(getByText('Due date')).toBeTruthy();
  });
});
