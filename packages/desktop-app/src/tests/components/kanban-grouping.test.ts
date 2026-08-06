/**
 * Unit tests for the Kanban grouping helpers (kanban-grouping.ts) — the
 * eligibility, column derivation, group-value read, write-shape, and bucketing
 * logic behind kanban-view.svelte.
 */

import { describe, it, expect } from 'vitest';
import type { Node } from '$lib/types';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';
import {
  UNASSIGNED,
  eligibleGroupByFields,
  enumColumns,
  readGroupValue,
  resolveFieldWrite,
  groupByColumn,
  resolveActiveGroupBy
} from '$lib/components/query/kanban-grouping';

function field(name: string, type: string, extra: Partial<SchemaField> = {}): SchemaField {
  return { name, type, protection: 'user', ...extra } as SchemaField;
}

function node(id: string, overrides: Partial<Node> & Record<string, unknown> = {}): Node {
  return {
    id,
    nodeType: 'invoice',
    content: '',
    createdAt: '2026-01-01T00:00:00.000Z',
    modifiedAt: '2026-01-01T00:00:00.000Z',
    version: 1,
    properties: {},
    ...overrides
  } as Node;
}

const statusField = field('status', 'enum', {
  coreValues: [
    { value: 'open', label: 'Open' },
    { value: 'closed', label: 'Closed' }
  ],
  userValues: [{ value: 'archived', label: 'Archived' }]
});

describe('eligibleGroupByFields', () => {
  it('returns only enum fields', () => {
    const schema = { fields: [field('name', 'string'), statusField, field('count', 'number')] } as SchemaNode;
    expect(eligibleGroupByFields(schema).map((f) => f.name)).toEqual(['status']);
  });

  it('returns [] for a null schema or a schema with no enum fields', () => {
    expect(eligibleGroupByFields(null)).toEqual([]);
    expect(eligibleGroupByFields({ fields: [field('name', 'string')] } as SchemaNode)).toEqual([]);
  });
});

describe('enumColumns', () => {
  it('flattens core then user values into { value, label }, preserving order', () => {
    expect(enumColumns(statusField)).toEqual([
      { value: 'open', label: 'Open' },
      { value: 'closed', label: 'Closed' },
      { value: 'archived', label: 'Archived' }
    ]);
  });

  it('returns [] for a missing field or a field with no values', () => {
    expect(enumColumns(null)).toEqual([]);
    expect(enumColumns(field('x', 'enum'))).toEqual([]);
  });
});

describe('readGroupValue', () => {
  it('reads a user-defined field from properties', () => {
    expect(readGroupValue(node('n1', { properties: { status: 'open' } }), 'status')).toBe('open');
  });

  it('prefers a camelCase top-level typed field (e.g. due_date → dueDate)', () => {
    expect(readGroupValue(node('n1', { dueDate: '2026-03-01' }), 'due_date')).toBe('2026-03-01');
  });

  it('falls back to a snake_case top-level field', () => {
    expect(readGroupValue(node('n1', { due_date: '2026-03-02' }), 'due_date')).toBe('2026-03-02');
  });

  it('returns null for unset, null, or empty-string values', () => {
    expect(readGroupValue(node('n1', { properties: {} }), 'status')).toBeNull();
    expect(readGroupValue(node('n1', { properties: { status: null } }), 'status')).toBeNull();
    expect(readGroupValue(node('n1', { properties: { status: '' } }), 'status')).toBeNull();
  });
});

describe('resolveFieldWrite', () => {
  it('writes a user-defined field into properties, preserving siblings', () => {
    const n = node('n1', { properties: { status: 'open', note: 'hi' } });
    expect(resolveFieldWrite(n, 'status', 'closed')).toEqual({
      properties: { status: 'closed', note: 'hi' }
    });
  });

  it('writes into properties when the field is currently unset (default path)', () => {
    const n = node('n1', { properties: {} });
    expect(resolveFieldWrite(n, 'status', 'open')).toEqual({ properties: { status: 'open' } });
  });

  it('writes a typed top-level field back to its camelCase key', () => {
    const n = node('n1', { stage: 'lead' });
    expect(resolveFieldWrite(n, 'stage', 'won')).toEqual({ stage: 'won' });
  });
});

describe('groupByColumn', () => {
  const columns = ['open', 'closed', 'archived'];

  it('buckets items by value and preserves column order with an Unassigned bucket', () => {
    const buckets = groupByColumn(
      [
        { id: 'a', value: 'open' },
        { id: 'b', value: 'closed' },
        { id: 'c', value: 'open' },
        { id: 'd', value: null }
      ],
      columns
    );
    expect([...buckets.keys()]).toEqual(['open', 'closed', 'archived', UNASSIGNED]);
    expect(buckets.get('open')).toEqual(['a', 'c']);
    expect(buckets.get('closed')).toEqual(['b']);
    expect(buckets.get('archived')).toEqual([]);
    expect(buckets.get(UNASSIGNED)).toEqual(['d']);
  });

  it('routes values not matching any column into Unassigned', () => {
    const buckets = groupByColumn([{ id: 'a', value: 'stale-value' }], columns);
    expect(buckets.get(UNASSIGNED)).toEqual(['a']);
  });
});

describe('resolveActiveGroupBy', () => {
  const eligible = [field('status', 'enum'), field('priority', 'enum')];

  it('keeps the stored selection when it is still eligible', () => {
    expect(resolveActiveGroupBy(eligible, 'priority')).toBe('priority');
  });

  it('falls back to the first eligible field when the stored one is gone', () => {
    expect(resolveActiveGroupBy(eligible, 'removed')).toBe('status');
  });

  it('returns null when there are no eligible fields', () => {
    expect(resolveActiveGroupBy([], 'status')).toBeNull();
  });
});
