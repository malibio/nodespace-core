/**
 * TableView — column header labels (issue #2100).
 *
 * table-view.svelte previously used field.description (when present) as the
 * column header, so schemas with genuine help-text prose in `description`
 * (e.g. the person schema's `name`/`email` fields) leaked that prose into
 * the header instead of a short label derived from `name`. These tests cover
 * the fix: headers are always derived from `field.name` via the shared
 * `labelForField()` helper, and `description` (when present) survives only
 * as the header's tooltip (`title` attribute), not the visible label.
 */
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import TableView from '$lib/components/query/table-view.svelte';

function field(partial: Partial<SchemaField> & { name: string; type: string }): SchemaField {
  return { protection: 'user', indexed: false, ...partial } as SchemaField;
}

function schema(fields: SchemaField[]): SchemaNode {
  return {
    id: 'person',
    content: 'Person',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    isCore: false,
    schemaVersion: 1,
    fields,
  };
}

describe('TableView — column header labels', () => {
  afterEach(() => cleanup());

  it('derives person schema headers from field name, not the long description prose', () => {
    const s = schema([
      field({
        name: 'name',
        type: 'string',
        description: 'Display name; optional — a person may exist before a name is set',
      }),
      field({
        name: 'email',
        type: 'string',
        description: 'Email address; optional at schema level, required in practice for invited teammates',
      }),
    ]);

    const { getByText, queryByText } = render(TableView, {
      props: { nodeIds: [], schema: s, fieldSchemaMap: new Map(), onRowClick: () => {} },
    });

    expect(getByText('Name')).toBeTruthy();
    expect(getByText('Email')).toBeTruthy();
    expect(queryByText(/Display name; optional/)).toBeNull();
    expect(queryByText(/Email address; optional/)).toBeNull();
  });

  it('leaves task-like schema headers unchanged', () => {
    const s = schema([
      field({ name: 'status', type: 'enum', description: 'Status' }),
      field({ name: 'priority', type: 'enum', description: 'Priority' }),
      field({ name: 'due_date', type: 'date', description: 'Due date' }),
    ]);

    const { getByText } = render(TableView, {
      props: { nodeIds: [], schema: s, fieldSchemaMap: new Map(), onRowClick: () => {} },
    });

    expect(getByText('Status')).toBeTruthy();
    expect(getByText('Priority')).toBeTruthy();
    expect(getByText('Due date')).toBeTruthy();
  });

  it('formats snake_case and camelCase names, and namespaced names sensibly', () => {
    const s = schema([
      field({ name: 'due_date', type: 'date' }),
      field({ name: 'firstName', type: 'string' }),
      field({ name: 'custom:capacity', type: 'number' }),
    ]);

    const { getByText } = render(TableView, {
      props: { nodeIds: [], schema: s, fieldSchemaMap: new Map(), onRowClick: () => {} },
    });

    expect(getByText('Due date')).toBeTruthy();
    expect(getByText('First Name')).toBeTruthy();
    expect(getByText('Capacity')).toBeTruthy();
  });

  it('carries description through as the header title tooltip, not the label', () => {
    const s = schema([
      field({ name: 'email', type: 'string', description: 'Email address; optional at schema level' }),
    ]);

    const { container } = render(TableView, {
      props: { nodeIds: [], schema: s, fieldSchemaMap: new Map(), onRowClick: () => {} },
    });

    const headers = Array.from(container.querySelectorAll('th'));
    const emailHeader = headers.find((th) => th.textContent?.trim() === 'Email');
    expect(emailHeader?.getAttribute('title')).toBe('Email address; optional at schema level');
  });

  it('leaves the header title attribute empty when there is no description', () => {
    const s = schema([field({ name: 'status', type: 'enum' })]);

    const { container } = render(TableView, {
      props: { nodeIds: [], schema: s, fieldSchemaMap: new Map(), onRowClick: () => {} },
    });

    const headers = Array.from(container.querySelectorAll('th'));
    const statusHeader = headers.find((th) => th.textContent?.trim() === 'Status');
    expect(statusHeader?.getAttribute('title')).toBeFalsy();
  });
});
