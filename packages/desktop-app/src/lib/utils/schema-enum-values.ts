/**
 * Shared enum-field helpers: merging a schema field's coreValues + userValues
 * into one selectable list, and resolving a stored value to its display label.
 *
 * Single source of truth for "what are field X's options" and "what label
 * does value Y map to" — every property-form surface that needs to humanize
 * a raw enum value (a field control, a collapsed-header summary badge, …)
 * reads it from here instead of re-deriving its own copy of the same lookup.
 */
import type { SchemaField, EnumValue } from '$lib/types/schema-node';

/** Merge a field's protected core values with its user-extensible ones, in that order. */
export function getEnumValues(field: SchemaField): EnumValue[] {
  const values: EnumValue[] = [];
  if (field.coreValues) values.push(...field.coreValues);
  if (field.userValues) values.push(...field.userValues);
  return values;
}

/**
 * Humanize a raw value the schema doesn't (or no longer) declare a label for,
 * e.g. "in_progress" -> "In Progress". Used as the fallback when a stored
 * value isn't found among a field's current enum options.
 */
export function formatEnumFallbackLabel(value: string): string {
  return value
    .replace(/[_-]/g, ' ')
    .split(' ')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ');
}

/**
 * Resolve a stored enum value to its display label: the schema's declared
 * label when the value is still a current option, else a humanized fallback
 * of the raw value — never the raw key verbatim — so every surface that
 * displays this field (the control itself, a collapsed-header summary, …)
 * agrees on how a value renders. Returns undefined for an absent value.
 */
export function enumValueLabel(field: SchemaField, value: string | null | undefined): string | undefined {
  if (!value) return undefined;
  return getEnumValues(field).find((ev) => ev.value === value)?.label ?? formatEnumFallbackLabel(value);
}
