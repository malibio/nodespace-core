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
  resolveActiveGroupBy,
  growRevealed
} from '$lib/components/query/kanban-grouping';

function field(name: string, type: string, extra: Partial<SchemaField> = {}): SchemaField {
  return { name, type, friendlyName: name, protection: 'user', indexed: false, ...extra };
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

  it('excludes an enum field whose values include the UNASSIGNED sentinel', () => {
    // Nothing in the schema system forbids an enum value literally named
    // "__unassigned__" — groupByColumn's bucketing can't tell that value
    // apart from a genuinely-unset node if it were allowed through, so the
    // field itself is excluded rather than ever reaching that bucketing.
    const collidingCore = field('stage', 'enum', {
      coreValues: [{ value: UNASSIGNED, label: 'Somehow Unassigned' }]
    });
    const collidingUser = field('phase', 'enum', {
      userValues: [{ value: UNASSIGNED, label: 'Also Unassigned' }]
    });
    const schema = {
      fields: [statusField, collidingCore, collidingUser]
    } as SchemaNode;
    expect(eligibleGroupByFields(schema).map((f) => f.name)).toEqual(['status']);
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

  it('prefers properties over a same-named top-level field, mirroring resolveFieldWrite (shadowed core property name)', () => {
    // A user-defined type is allowed a bare field name that shadows a core
    // property (CLAUDE.md: discouraged, not forbidden). resolveFieldWrite
    // already treats `field in props` as authoritative over a same-named
    // top-level slot when deciding where to WRITE — this asserts the READ
    // side agrees, so the board reflects its own writes instead of reading a
    // stale/unrelated top-level value forever.
    const n = node('n1', { status: 'stale-top-level', properties: { status: 'current' } });
    expect(readGroupValue(n, 'status')).toBe('current');
    expect(resolveFieldWrite(n, 'status', 'next')).toEqual({
      properties: { status: 'next' }
    });
  });

  it('still reads a genuine typed top-level field when properties has no same-named key', () => {
    const n = node('n1', { stage: 'lead', properties: { note: 'hi' } });
    expect(readGroupValue(n, 'stage')).toBe('lead');
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

describe('growRevealed', () => {
  const ids = Array.from({ length: 10 }, (_, i) => `n${i}`);

  it('reveals the first batch from an empty set, in ids order', () => {
    const revealed = growRevealed(new Set(), ids, 3);
    expect([...revealed]).toEqual(['n0', 'n1', 'n2']);
  });

  it('grows by one more batch of NEW ids, keeping everything already revealed', () => {
    const first = growRevealed(new Set(), ids, 3);
    const second = growRevealed(first, ids, 3);
    expect([...second]).toEqual(['n0', 'n1', 'n2', 'n3', 'n4', 'n5']);
  });

  it('stops at the end of ids even if the batch would overshoot', () => {
    const revealed = growRevealed(new Set(), ids.slice(0, 4), 25);
    expect([...revealed]).toEqual(['n0', 'n1', 'n2', 'n3']);
  });

  it('is a no-op once every id is already revealed', () => {
    const all = new Set(ids);
    expect(growRevealed(all, ids, 3)).toEqual(all);
  });

  it('keeps an already-revealed id even when it now sits later in ids order (position-independent)', () => {
    // n0 was revealed while it was first in the column; the column's order
    // has since shifted (e.g. another card was reassigned ahead of it), but
    // n0's membership hasn't changed, so it must stay revealed.
    const revealed = new Set(['n0']);
    const reordered = ['nNew1', 'nNew2', 'n0', ...ids.slice(1)];
    const grown = growRevealed(revealed, reordered, 3);
    expect(grown.has('n0')).toBe(true);
  });

  it('does not mutate the input set', () => {
    const revealed = new Set(['n0']);
    growRevealed(revealed, ids, 3);
    expect(revealed).toEqual(new Set(['n0']));
  });
});
