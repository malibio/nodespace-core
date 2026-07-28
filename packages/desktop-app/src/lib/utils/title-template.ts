/**
 * Title template interpolation utility.
 *
 * Mirrors the behaviour of Rust's `interpolate_title_template` in
 * `packages/core/src/utils/markdown.rs`: replaces `{fieldName}` tokens,
 * then trims and collapses internal whitespace so the output is consistent
 * with what the backend will compute and return as `node.title`.
 *
 * Token syntax: `{fieldName}`, where fieldName is any run of characters up to
 * the next `}`. This matches the Rust scanner, which takes everything between
 * the braces rather than a restricted character class — so a namespaced field
 * name (`{custom:capacity}`) resolves here exactly as it does on the backend.
 * A narrower pattern would silently leave such tokens un-substituted in the UI
 * while the backend-computed title resolved them, and the two would disagree.
 */

import type { SchemaField } from '$lib/types/schema-node';

/**
 * Matches a `{token}` and captures the field name between the braces.
 *
 * Mirrors Rust's `interpolate_title_template_with_schema`: it scans to the next
 * `}`, so the name may contain any character except `}`. Kept in one place so
 * both template evaluators stay in sync.
 */
const TEMPLATE_TOKEN_RE = /\{([^}]*)\}/g;

/**
 * Interpolates a title template with the given field values.
 *
 * @param template - Template string with `{fieldName}` tokens
 * @param fieldValues - Flat map of field names to their current values
 * @returns Interpolated title with whitespace normalised and trimmed.
 *          Returns an empty string when all tokens resolve to empty values.
 */
export function evaluateTitleTemplate(
  template: string,
  fieldValues: Record<string, unknown>
): string {
  const interpolated = template.replace(TEMPLATE_TOKEN_RE, (_, fieldName) => {
    const val = fieldValues[fieldName];
    if (val === null || val === undefined) return '';
    return String(val);
  });
  // Normalize whitespace and trim (mirrors Rust WHITESPACE_RE + .trim())
  return interpolated.replace(/\s+/g, ' ').trim();
}

/**
 * Formats a date value (YYYY-MM-DD string) for human-readable display.
 * Returns the input unchanged if it cannot be parsed as a date.
 */
function formatDateValue(value: string): string {
  try {
    // YYYY-MM-DD → e.g. "Jun 30, 2026"
    const [year, month, day] = value.split('-').map(Number);
    if (!year || !month || !day) return value;
    return new Date(year, month - 1, day).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric'
    });
  } catch {
    return value;
  }
}

/**
 * Evaluates a `propertiesHeaderSummaryTemplate` with enum label resolution and date formatting.
 *
 * Unlike `evaluateTitleTemplate` (which is a plain token substitution used to optimistically
 * update the indexed title), this function applies schema-aware formatting:
 * - Enum field values are resolved to their human-readable labels.
 * - Date field values (YYYY-MM-DD strings) are formatted for display.
 *
 * Evaluated client-side only — the result is never persisted.
 *
 * @param template - Template string with `{fieldName}` tokens
 * @param fieldValues - Flat map of field names to their current values
 * @param fields - Schema field definitions (for enum label and date type lookup)
 * @returns Interpolated summary with whitespace normalised and trimmed.
 *          Returns an empty string when all tokens resolve to empty/whitespace values.
 */
export function evaluateSummaryTemplate(
  template: string,
  fieldValues: Record<string, unknown>,
  fields: SchemaField[]
): string {
  const fieldMap = new Map(fields.map((f) => [f.name, f]));

  const interpolated = template.replace(TEMPLATE_TOKEN_RE, (_, fieldName) => {
    const val = fieldValues[fieldName];
    if (val === null || val === undefined) return '';

    const field = fieldMap.get(fieldName);
    if (field) {
      if (field.type === 'enum') {
        const raw = String(val);
        const allValues = [...(field.coreValues ?? []), ...(field.userValues ?? [])];
        const enumEntry = allValues.find((ev) => ev.value === raw);
        return enumEntry?.label ?? raw;
      }
      if (field.type === 'date' && typeof val === 'string') {
        return formatDateValue(val);
      }
    }

    return String(val);
  });

  return interpolated.replace(/\s+/g, ' ').trim();
}
