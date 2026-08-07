/**
 * Pure logic for the structured query filter builder (issue #1920).
 *
 * Extracted from query-editor.svelte so the operator-narrowing, value-coercion,
 * and definition-building rules are unit-testable without rendering the Svelte
 * component. The component holds only the view state and delegates here.
 */

import type { QueryDefinition, QueryFilter } from '$lib/types/query';
import type { EnumValue, SchemaField } from '$lib/types/schema-node';

export type Operator = QueryFilter['operator'];

/** An editable property-filter row. The value is held as a string in the UI and
 *  coerced to the field's declared type when the definition is built. */
export interface FilterRow {
  property: string;
  operator: Operator;
  value: string;
}

export const OPERATOR_LABELS: Record<Operator, string> = {
  equals: 'equals',
  contains: 'contains',
  gt: 'greater than',
  lt: 'less than',
  gte: '≥',
  lte: '≤',
  in: 'is any of',
  exists: 'is set',
};

/** Operators offered for a field, narrowed by its declared type. */
export function operatorsForType(type: string | undefined): Operator[] {
  if (type === 'number' || type === 'date') return ['equals', 'gt', 'lt', 'gte', 'lte', 'exists'];
  if (type === 'enum') return ['equals', 'in', 'exists'];
  if (type === 'boolean') return ['equals', 'exists'];
  // strings and everything else
  return ['contains', 'equals', 'in', 'exists'];
}

export function labelForField(field: SchemaField): string {
  return field.description
    ? field.description
    : field.name
        .replace(/_/g, ' ')
        .replace(/([a-z])([A-Z])/g, '$1 $2')
        .replace(/^\w/, (c) => c.toUpperCase());
}

/** The selectable enum options for a field (core + user-extended), or [] when
 *  the field isn't an enum. */
export function enumOptions(field: SchemaField | undefined): EnumValue[] {
  if (!field || field.type !== 'enum') return [];
  return [...(field.coreValues ?? []), ...(field.userValues ?? [])];
}

/** Coerce a row's string value to the field's declared type. `exists` carries
 *  no value; `in` splits into an array. */
export function coerceRowValue(row: FilterRow, field: SchemaField | undefined): unknown {
  if (row.operator === 'exists') return undefined;
  if (row.operator === 'in') {
    const parts = row.value
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    return field?.type === 'number' ? parts.map((s) => Number(s)) : parts;
  }
  if (field?.type === 'number') return Number(row.value);
  if (field?.type === 'boolean') return row.value === 'true';
  return row.value;
}

/** Seed editable rows from a query definition. Only property filters become rows;
 *  the value is stringified for the text/number/enum controls. */
export function rowsFromDefinition(def: QueryDefinition | null): FilterRow[] {
  const source = def?.filters ?? [];
  return source
    .filter((f) => f.type === 'property' && typeof f.property === 'string')
    .map((f) => ({
      property: f.property as string,
      operator: f.operator,
      value: Array.isArray(f.value) ? f.value.join(', ') : f.value == null ? '' : String(f.value),
    }));
}

export type BuildResult =
  | { ok: true; definition: QueryDefinition }
  | { ok: false; error: string };

/** Build a validated QueryDefinition from the rows, preserving inherited fields
 *  (targetType, and any sorting/limit already on `base`). Returns an error when a
 *  row is missing its property or (for a value-bearing operator) its value. */
export function buildDefinition(
  rows: FilterRow[],
  fields: SchemaField[],
  base: { targetType: string; sorting?: QueryDefinition['sorting']; limit?: number },
): BuildResult {
  const byName = new Map(fields.map((f) => [f.name, f]));
  const filters: QueryFilter[] = [];
  for (const row of rows) {
    if (!row.property) {
      return { ok: false, error: 'Every filter needs a property.' };
    }
    if (row.operator !== 'exists' && row.value.trim() === '') {
      return { ok: false, error: `Filter on "${row.property}" needs a value.` };
    }
    const filter: QueryFilter = { type: 'property', operator: row.operator, property: row.property };
    const value = coerceRowValue(row, byName.get(row.property));
    if (value !== undefined) filter.value = value;
    filters.push(filter);
  }
  return {
    ok: true,
    definition: {
      targetType: base.targetType,
      filters,
      sorting: base.sorting,
      limit: base.limit,
    },
  };
}
