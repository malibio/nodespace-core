/**
 * Shared enum-field helpers (core#2132): merging coreValues/userValues, and
 * resolving a stored value to its display label. Extracted out of
 * schema-field-leaf.svelte (the enum <Select> case) so TaskSchemaForm's
 * collapsed-header status badge can reuse the exact same lookup instead of
 * re-deriving its own copy — see task-schema-form.test.ts for the
 * component-level assertion that the header agrees with the field control.
 */
import { describe, it, expect } from 'vitest';
import type { SchemaField } from '$lib/types/schema-node';
import { getEnumValues, formatEnumFallbackLabel, enumValueLabel } from '$lib/utils/schema-enum-values';

function enumField(overrides: Partial<SchemaField> = {}): SchemaField {
  return {
    name: 'status',
    friendlyName: 'Status',
    type: 'enum',
    protection: 'user',
    indexed: true,
    ...overrides
  };
}

describe('getEnumValues', () => {
  it('returns an empty array when neither coreValues nor userValues are set', () => {
    expect(getEnumValues(enumField())).toEqual([]);
  });

  it('merges coreValues followed by userValues, in that order', () => {
    const field = enumField({
      coreValues: [{ value: 'open', label: 'Open' }],
      userValues: [{ value: 'blocked', label: 'Blocked' }]
    });
    expect(getEnumValues(field)).toEqual([
      { value: 'open', label: 'Open' },
      { value: 'blocked', label: 'Blocked' }
    ]);
  });

  it('tolerates an empty userValues array (the shape core_schemas.rs actually sends)', () => {
    const field = enumField({ coreValues: [{ value: 'open', label: 'Open' }], userValues: [] });
    expect(getEnumValues(field)).toEqual([{ value: 'open', label: 'Open' }]);
  });

  it('drops a userValue whose value collides with an existing coreValue, keeping the core label', () => {
    // Matches the pre-#2132 TaskSchemaForm behavior (statusOptionsWithExtensions'
    // `if (!coreValues.has(uv.value))` guard) — an extension adds choices, it
    // doesn't shadow one that already exists.
    const field = enumField({
      coreValues: [{ value: 'open', label: 'Open' }],
      userValues: [
        { value: 'open', label: 'Open (duplicate)' },
        { value: 'blocked', label: 'Blocked' }
      ]
    });
    const values = getEnumValues(field);
    expect(values).toEqual([
      { value: 'open', label: 'Open' },
      { value: 'blocked', label: 'Blocked' }
    ]);
  });

  it('drops a later userValue that collides with an earlier one, keeping the first', () => {
    const field = enumField({
      userValues: [
        { value: 'blocked', label: 'Blocked' },
        { value: 'blocked', label: 'Blocked (dup)' }
      ]
    });
    expect(getEnumValues(field)).toEqual([{ value: 'blocked', label: 'Blocked' }]);
  });
});

describe('formatEnumFallbackLabel', () => {
  it('capitalizes a single word', () => {
    expect(formatEnumFallbackLabel('open')).toBe('Open');
  });

  it('humanizes snake_case', () => {
    expect(formatEnumFallbackLabel('in_progress')).toBe('In Progress');
  });

  it('humanizes multi-word snake_case', () => {
    expect(formatEnumFallbackLabel('blocked_by_external')).toBe('Blocked By External');
  });

  it('normalizes an already-uppercase word', () => {
    expect(formatEnumFallbackLabel('TODO')).toBe('Todo');
  });
});

describe('enumValueLabel', () => {
  it('returns undefined for a null/undefined/empty value', () => {
    const field = enumField({ coreValues: [{ value: 'open', label: 'Open' }] });
    expect(enumValueLabel(field, null)).toBeUndefined();
    expect(enumValueLabel(field, undefined)).toBeUndefined();
    expect(enumValueLabel(field, '')).toBeUndefined();
  });

  it("resolves to the schema's declared label for a current option", () => {
    const field = enumField({ coreValues: [{ value: 'in_progress', label: 'In Progress' }] });
    expect(enumValueLabel(field, 'in_progress')).toBe('In Progress');
  });

  it('falls back to a humanized version of the raw value when the schema no longer declares it', () => {
    const field = enumField({ coreValues: [{ value: 'open', label: 'Open' }] });
    expect(enumValueLabel(field, 'archived_legacy')).toBe('Archived Legacy');
  });

  it('falls back to a humanized label when the matching option has a blank declared label', () => {
    // `||`, not `??`: a malformed `label: ''` is exactly as unhelpful as no match at
    // all — must not surface a blank where SchemaFieldLeaf's own Select trigger (which
    // now calls this same helper) would otherwise show something.
    const field = enumField({ coreValues: [{ value: 'in_progress', label: '' }] });
    expect(enumValueLabel(field, 'in_progress')).toBe('In Progress');
  });
});
