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
  initialValueForField,
  partitionFilters,
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

describe('initialValueForField', () => {
  it('seeds a boolean field with a concrete "true" (its control has no empty state)', () => {
    expect(initialValueForField(field({ name: 'done', type: 'boolean' }))).toBe('true');
  });
  it('seeds other fields empty', () => {
    expect(initialValueForField(field({ name: 'title', type: 'string' }))).toBe('');
    expect(initialValueForField(undefined)).toBe('');
  });
});

describe('partitionFilters', () => {
  const fields = [
    field({ name: 'status', type: 'enum', coreValues: [{ value: 'open', label: 'Open' }] }),
    field({ name: 'tags', type: 'string' }),
    field({ name: 'points', type: 'number' }),
  ];

  it('makes editable rows only from property filters whose field exists', () => {
    const def: QueryDefinition = {
      targetType: 'task',
      filters: [
        { type: 'property', operator: 'equals', property: 'status', value: 'open' },
        { type: 'property', operator: 'in', property: 'tags', value: ['a', 'b'] },
      ],
    };
    const { rows, preserved } = partitionFilters(def, fields);
    expect(preserved).toEqual([]);
    expect(rows).toEqual([
      { property: 'status', operator: 'equals', value: 'open' },
      { property: 'tags', operator: 'in', value: 'a, b' },
    ]);
  });

  it('preserves content/relationship/metadata filters and unknown-field property filters', () => {
    const content = { type: 'content' as const, operator: 'contains' as const, value: 'x' };
    const unknown = { type: 'property' as const, operator: 'equals' as const, property: 'gone', value: 'y' };
    const def: QueryDefinition = {
      targetType: 'task',
      filters: [{ type: 'property', operator: 'equals', property: 'status', value: 'open' }, content, unknown],
    };
    const { rows, preserved } = partitionFilters(def, fields);
    expect(rows.map((r) => r.property)).toEqual(['status']);
    expect(preserved).toEqual([content, unknown]);
  });

  it('coerces a stored operator that is invalid for the field type', () => {
    // `contains` is not valid for a number field → coerced to the first allowed.
    const def: QueryDefinition = {
      targetType: 'task',
      filters: [{ type: 'property', operator: 'contains', property: 'points', value: '3' }],
    };
    const { rows } = partitionFilters(def, fields);
    expect(operatorsForType('number')).toContain(rows[0].operator);
    expect(rows[0].operator).not.toBe('contains');
  });

  it('returns empty for a null definition', () => {
    expect(partitionFilters(null, fields)).toEqual({ rows: [], preserved: [] });
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

  it('rejects a NaN number value (e.g. a bad entry in an in-list)', () => {
    const rows: FilterRow[] = [{ property: 'points', operator: 'in', value: '1, abc' }];
    const result = buildDefinition(rows, fields, { targetType: 'task' });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('valid number');
  });

  it('re-emits preserved filters ahead of the edited rows (no data loss)', () => {
    const preserved = [{ type: 'content' as const, operator: 'contains' as const, value: 'kw' }];
    const rows: FilterRow[] = [{ property: 'status', operator: 'equals', value: 'open' }];
    const result = buildDefinition(rows, fields, { targetType: 'task', preserved });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.definition.filters).toEqual([
        { type: 'content', operator: 'contains', value: 'kw' },
        { type: 'property', operator: 'equals', property: 'status', value: 'open' },
      ]);
    }
  });

  it('builds an empty filter list for no rows', () => {
    const result = buildDefinition([], fields, { targetType: 'task' });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.definition.filters).toEqual([]);
  });
});
