/**
 * Shared date-field helpers (core#2132): extracted out of schema-field-leaf.svelte
 * so TaskSchemaForm's collapsed-header "Due: ..." text parses/formats a date
 * value identically to the SchemaFieldLeaf date control itself, instead of
 * keeping its own copy with divergent error handling.
 */
import { describe, it, expect } from 'vitest';
import { parseScalarDate, formatDateDisplay, formatDateForStorage } from '$lib/utils/schema-date-values';

describe('parseScalarDate', () => {
  it('returns undefined for null/undefined/empty', () => {
    expect(parseScalarDate(null)).toBeUndefined();
    expect(parseScalarDate(undefined)).toBeUndefined();
    expect(parseScalarDate('')).toBeUndefined();
  });

  it('parses a plain YYYY-MM-DD date', () => {
    const date = parseScalarDate('2026-12-31');
    expect(date?.toString()).toBe('2026-12-31');
  });

  it('extracts the date-only portion from a full ISO8601 timestamp', () => {
    const date = parseScalarDate('2026-12-31T10:00:00Z');
    expect(date?.toString()).toBe('2026-12-31');
  });

  it('returns undefined (not a throw) for an unparseable value', () => {
    expect(parseScalarDate('not-a-date')).toBeUndefined();
  });
});

describe('formatDateDisplay', () => {
  it('returns a placeholder for null/undefined', () => {
    expect(formatDateDisplay(null)).toBe('Pick a date');
    expect(formatDateDisplay(undefined)).toBe('Pick a date');
  });

  it('formats a valid date', () => {
    expect(formatDateDisplay('2026-01-05')).toBe('2026-01-05');
  });

  it('returns the raw value as-is when it fails to parse, rather than throwing', () => {
    expect(formatDateDisplay('garbage')).toBe('garbage');
  });
});

describe('formatDateForStorage', () => {
  it('returns null for undefined', () => {
    expect(formatDateForStorage(undefined)).toBeNull();
  });

  it('formats a DateValue to YYYY-MM-DD, zero-padded', () => {
    const date = parseScalarDate('2026-01-05');
    expect(formatDateForStorage(date)).toBe('2026-01-05');
  });
});
