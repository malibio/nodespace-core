/**
 * QueryEditor filter-builder model tests (issue #1920)
 *
 * The editor was replaced with a structured filter builder. These tests exercise
 * the actual pure logic the component delegates to
 * (`query-editor-model.ts`) — operator narrowing, value coercion, seeding rows
 * from a definition, and building a validated definition that preserves the
 * inherited targetType/sorting/limit.
 */

import { describe, it, expect } from 'vitest';
import {
  operatorsForType,
  enumOptions,
  coerceRowValue,
  rowsFromDefinition,
  buildDefinition,
  type FilterRow,
} from '$lib/components/query/query-editor-model';
import type { SchemaField } from '$lib/types/schema-node';
import type { QueryDefinition } from '$lib/types/query';

function field(partial: Partial<SchemaField> & { name: string; type: string }): SchemaField {
  return { protection: 'user', indexed: false, ...partial } as SchemaField;
}

describe('operatorsForType', () => {
  it('offers comparison operators for number and date', () => {
    for (const t of ['number', 'date']) {
      const ops = operatorsForType(t);
      expect(ops).toContain('gt');
      expect(ops).toContain('lte');
      expect(ops).not.toContain('contains');
    }
  });

  it('offers equals/in/exists for enum', () => {
    expect(operatorsForType('enum')).toEqual(['equals', 'in', 'exists']);
  });

  it('offers equals/exists for boolean', () => {
    expect(operatorsForType('boolean')).toEqual(['equals', 'exists']);
  });

  it('offers contains for strings and unknown types', () => {
    expect(operatorsForType('string')).toContain('contains');
    expect(operatorsForType(undefined)).toContain('contains');
  });
});

describe('enumOptions', () => {
  it('merges core and user enum values', () => {
    const f = field({
      name: 'status',
      type: 'enum',
      coreValues: [{ value: 'open', label: 'Open' }],
      userValues: [{ value: 'blocked', label: 'Blocked' }],
    });
    expect(enumOptions(f).map((v) => v.value)).toEqual(['open', 'blocked']);
  });

  it('returns [] for non-enum fields', () => {
    expect(enumOptions(field({ name: 'title', type: 'string' }))).toEqual([]);
    expect(enumOptions(undefined)).toEqual([]);
  });
});

describe('coerceRowValue', () => {
  const num = field({ name: 'points', type: 'number' });
  it('returns undefined for exists (no value)', () => {
    expect(coerceRowValue({ property: 'x', operator: 'exists', value: '' }, undefined)).toBeUndefined();
  });
  it('splits an "in" value into a trimmed array', () => {
    expect(coerceRowValue({ property: 'status', operator: 'in', value: 'open, done ,' }, field({ name: 'status', type: 'enum' }))).toEqual(['open', 'done']);
  });
  it('coerces "in" values to numbers for number fields', () => {
    expect(coerceRowValue({ property: 'points', operator: 'in', value: '1, 2' }, num)).toEqual([1, 2]);
  });
  it('coerces a number field value to a number', () => {
    expect(coerceRowValue({ property: 'points', operator: 'gt', value: '3' }, num)).toBe(3);
  });
  it('coerces a boolean field value to a boolean', () => {
    const b = field({ name: 'done', type: 'boolean' });
    expect(coerceRowValue({ property: 'done', operator: 'equals', value: 'true' }, b)).toBe(true);
    expect(coerceRowValue({ property: 'done', operator: 'equals', value: 'false' }, b)).toBe(false);
  });
});

describe('rowsFromDefinition', () => {
  it('seeds rows from property filters, stringifying array values', () => {
    const def: QueryDefinition = {
      targetType: 'task',
      filters: [
        { type: 'property', operator: 'equals', property: 'status', value: 'open' },
        { type: 'property', operator: 'in', property: 'tags', value: ['a', 'b'] },
        { type: 'content', operator: 'contains', value: 'x' }, // dropped (not a property filter)
      ],
    };
    const rows = rowsFromDefinition(def);
    expect(rows).toEqual([
      { property: 'status', operator: 'equals', value: 'open' },
      { property: 'tags', operator: 'in', value: 'a, b' },
    ]);
  });

  it('returns [] for a null definition', () => {
    expect(rowsFromDefinition(null)).toEqual([]);
  });
});

describe('buildDefinition', () => {
  const fields = [
    field({ name: 'status', type: 'enum', coreValues: [{ value: 'open', label: 'Open' }] }),
    field({ name: 'points', type: 'number' }),
  ];

  it('preserves the inherited targetType, sorting, and limit', () => {
    const rows: FilterRow[] = [{ property: 'status', operator: 'equals', value: 'open' }];
    const result = buildDefinition(rows, fields, { targetType: 'task', sorting: [{ field: 'points', direction: 'asc' }], limit: 25 });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.definition.targetType).toBe('task');
      expect(result.definition.limit).toBe(25);
      expect(result.definition.sorting).toEqual([{ field: 'points', direction: 'asc' }]);
      expect(result.definition.filters).toEqual([
        { type: 'property', operator: 'equals', property: 'status', value: 'open' },
      ]);
    }
  });

  it('errors when a value-bearing filter has an empty value', () => {
    const rows: FilterRow[] = [{ property: 'status', operator: 'equals', value: '' }];
    const result = buildDefinition(rows, fields, { targetType: 'task' });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('status');
  });

  it('omits the value for an exists filter', () => {
    const rows: FilterRow[] = [{ property: 'status', operator: 'exists', value: '' }];
    const result = buildDefinition(rows, fields, { targetType: 'task' });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.definition.filters[0]).toEqual({ type: 'property', operator: 'exists', property: 'status' });
    }
  });

  it('coerces a number filter value', () => {
    const rows: FilterRow[] = [{ property: 'points', operator: 'gte', value: '5' }];
    const result = buildDefinition(rows, fields, { targetType: 'task' });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.definition.filters[0].value).toBe(5);
  });

  it('builds an empty filter list for no rows', () => {
    const result = buildDefinition([], fields, { targetType: 'task' });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.definition.filters).toEqual([]);
  });
});
