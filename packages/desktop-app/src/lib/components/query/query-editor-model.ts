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

/**
 * Derive a short, human-readable label from a field's `name` — the single
 * label helper shared by every query surface: table headers (table-view),
 * the group-by picker (kanban-view), and this module's own filter-builder
 * property list (query-editor).
 *
 * Deliberately ignores `description`: that's help text ("Human-readable
 * field description" per the schema doc comment), and schemas are free to
 * put arbitrarily long prose there (e.g. the person schema's `name`/`email`
 * fields) — prose that reads fine as a tooltip is unusable as a table header
 * or option label. `name` is the only field guaranteed to be short.
 *
 * A namespaced name (`custom:capacity`) is stripped to its local segment
 * before formatting — the namespace prefix disambiguates storage, not
 * display, so `custom:capacity` renders as `Capacity`, not `Custom:capacity`.
 */
export function labelForField(field: SchemaField): string {
  const localName = field.name.includes(':')
    ? field.name.slice(field.name.lastIndexOf(':') + 1)
    : field.name;
  return localName
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

/** The initial UI value for a freshly-added row on a given field. Boolean fields
 *  seed to a concrete `'true'` — their control is a two-option select with no
 *  empty state, so an empty seed would show "true" yet fail the value check. */
export function initialValueForField(field: SchemaField | undefined): string {
  return field?.type === 'boolean' ? 'true' : '';
}

/** Split a definition's filters into the ones the builder can edit as rows and
 *  the ones it must carry through untouched.
 *
 * A filter becomes an editable row only when it is a property filter whose
 * property still exists in the current schema — those are the ones the builder's
 * controls can faithfully represent. Everything else (content/relationship/
 * metadata filters, or property filters referencing a field the schema no longer
 * declares) is `preserved` verbatim so re-saving never silently drops a filter
 * the builder can't render. A stored operator that isn't valid for the field's
 * type is coerced to the first allowed operator so the control can't display one
 * value while holding another.
 */
export function partitionFilters(
  def: QueryDefinition | null,
  fields: SchemaField[],
): { rows: FilterRow[]; preserved: QueryFilter[] } {
  const byName = new Map(fields.map((f) => [f.name, f]));
  const rows: FilterRow[] = [];
  const preserved: QueryFilter[] = [];
  for (const f of def?.filters ?? []) {
    const field = f.type === 'property' && typeof f.property === 'string' ? byName.get(f.property) : undefined;
    if (!field) {
      preserved.push(f);
      continue;
    }
    const allowed = operatorsForType(field.type);
    rows.push({
      property: f.property as string,
      operator: allowed.includes(f.operator) ? f.operator : allowed[0],
      value: Array.isArray(f.value) ? f.value.join(', ') : f.value == null ? '' : String(f.value),
    });
  }
  return { rows, preserved };
}

export type BuildResult =
  | { ok: true; definition: QueryDefinition }
  | { ok: false; error: string };

/** Build a validated QueryDefinition from the rows, preserving inherited fields
 *  (targetType, sorting/limit) and any pass-through `preserved` filters the
 *  builder can't edit. Returns an error when a row is missing its property, is
 *  missing a value for a value-bearing operator, or coerces to `NaN` on a number
 *  field. `preserved` filters are re-emitted ahead of the edited rows so
 *  re-saving never drops a filter the builder couldn't render. */
export function buildDefinition(
  rows: FilterRow[],
  fields: SchemaField[],
  base: {
    targetType: string;
    sorting?: QueryDefinition['sorting'];
    limit?: number;
    preserved?: QueryFilter[];
  },
): BuildResult {
  const byName = new Map(fields.map((f) => [f.name, f]));
  const filters: QueryFilter[] = [...(base.preserved ?? [])];
  for (const row of rows) {
    if (!row.property) {
      return { ok: false, error: 'Every filter needs a property.' };
    }
    if (row.operator !== 'exists' && row.value.trim() === '') {
      return { ok: false, error: `Filter on "${row.property}" needs a value.` };
    }
    const field = byName.get(row.property);
    const filter: QueryFilter = { type: 'property', operator: row.operator, property: row.property };
    const value = coerceRowValue(row, field);
    if (value !== undefined) {
      if (field?.type === 'number') {
        const bad = Array.isArray(value)
          ? value.some((v) => Number.isNaN(v))
          : Number.isNaN(value as number);
        if (bad) {
          return { ok: false, error: `Filter on "${row.property}" needs a valid number.` };
        }
      }
      filter.value = value;
    }
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
