/**
 * Shared date-field helpers: parsing a backend date string (YYYY-MM-DD or a
 * full ISO8601 timestamp) into a `DateValue`, formatting one for display, and
 * formatting a picked `DateValue` back into storage form.
 *
 * Single source of truth so every surface that shows a date field's value (a
 * SchemaFieldLeaf date control, a collapsed-header summary, …) parses and
 * displays it identically instead of each keeping its own copy.
 */
import { parseDate, type DateValue } from '@internationalized/date';
import { createLogger } from './logger';

const log = createLogger('SchemaDateValues');

/** Parse a backend date value (handles both YYYY-MM-DD and full ISO8601 strings). */
export function parseScalarDate(raw: string | null | undefined): DateValue | undefined {
  if (!raw) return undefined;
  try {
    // Extract just the date part (YYYY-MM-DD) if it's a full ISO8601 string
    const dateOnly = raw.includes('T') ? raw.split('T')[0] : raw;
    return parseDate(dateOnly);
  } catch (error) {
    log.warn(`Failed to parse date value "${raw}":`, error);
    return undefined;
  }
}

/** Human-readable display for a date field's current value, or a "Pick a date" placeholder. */
export function formatDateDisplay(raw: string | null | undefined): string {
  if (!raw) return 'Pick a date';
  const date = parseScalarDate(raw);
  return date ? date.toString() : raw;
}

/** Format a picked DateValue back into the YYYY-MM-DD storage form the backend expects. */
export function formatDateForStorage(dateValue: DateValue | undefined): string | null {
  if (!dateValue) return null;
  return `${dateValue.year}-${String(dateValue.month).padStart(2, '0')}-${String(dateValue.day).padStart(2, '0')}`;
}
