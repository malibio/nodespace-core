import { describe, it, expect } from 'vitest';
import {
  LABEL_COLUMN,
  TYPE_COLUMN,
  applyViewSettings,
  cellValue,
  columnLabel,
  defaultColumnTokens,
  defaultViewSettings,
  edgeColumnToken,
  fieldColumnToken,
  parseColumnToken,
  resolveColumnCandidates,
  resolveDisplayedColumns,
  type RelationshipViewRow,
  type RelationshipViewSettings
} from '$lib/services/relationship-view-settings';

function row(overrides: Partial<RelationshipViewRow> = {}): RelationshipViewRow {
  return {
    id: 'n1',
    nodeType: 'person',
    label: 'Sarah',
    edgeValues: {},
    ...overrides
  };
}

describe('relationship-view-settings: tokens', () => {
  it('parses each source from its token', () => {
    expect(parseColumnToken(LABEL_COLUMN)).toEqual({ source: 'target', key: 'label' });
    expect(parseColumnToken(TYPE_COLUMN)).toEqual({ source: 'target', key: 'type' });
    expect(parseColumnToken(edgeColumnToken('role'))).toEqual({ source: 'edge', key: 'role' });
    expect(parseColumnToken(fieldColumnToken('capacity'))).toEqual({
      source: 'field',
      key: 'capacity'
    });
  });

  it('treats a prefixless token as an edge column and preserves inner colons', () => {
    expect(parseColumnToken('role')).toEqual({ source: 'edge', key: 'role' });
    expect(parseColumnToken('edge:a:b')).toEqual({ source: 'edge', key: 'a:b' });
  });

  it('humanizes labels, with friendly names for intrinsic columns', () => {
    expect(columnLabel(LABEL_COLUMN)).toBe('Target');
    expect(columnLabel(TYPE_COLUMN)).toBe('Type');
    expect(columnLabel(edgeColumnToken('assigned_at'))).toBe('Assigned At');
    expect(columnLabel(fieldColumnToken('seat_count'))).toBe('Seat Count');
  });
});

describe('relationship-view-settings: resolveColumnCandidates', () => {
  it('orders label (pinned) → type → edge columns → target fields', () => {
    const candidates = resolveColumnCandidates({
      edgeColumns: ['role', 'weight'],
      targetFieldNames: ['email', 'department']
    });
    expect(candidates.map((c) => c.token)).toEqual([
      LABEL_COLUMN,
      TYPE_COLUMN,
      edgeColumnToken('role'),
      edgeColumnToken('weight'),
      fieldColumnToken('email'),
      fieldColumnToken('department')
    ]);
    expect(candidates[0].pinned).toBe(true);
    expect(candidates[1].pinned).toBeUndefined();
  });

  it('skips a target field whose name collides with an edge column', () => {
    const candidates = resolveColumnCandidates({
      edgeColumns: ['role'],
      targetFieldNames: ['role', 'email']
    });
    const tokens = candidates.map((c) => c.token);
    expect(tokens).toContain(edgeColumnToken('role'));
    expect(tokens).not.toContain(fieldColumnToken('role'));
    expect(tokens).toContain(fieldColumnToken('email'));
  });

  it('handles absent target fields', () => {
    const candidates = resolveColumnCandidates({ edgeColumns: ['role'], targetFieldNames: null });
    expect(candidates.map((c) => c.token)).toEqual([
      LABEL_COLUMN,
      TYPE_COLUMN,
      edgeColumnToken('role')
    ]);
  });
});

describe('relationship-view-settings: defaultColumnTokens', () => {
  it('returns only the edge columns (preserving the pre-settings look)', () => {
    const candidates = resolveColumnCandidates({
      edgeColumns: ['role', 'weight'],
      targetFieldNames: ['email']
    });
    expect(defaultColumnTokens(candidates)).toEqual([
      edgeColumnToken('role'),
      edgeColumnToken('weight')
    ]);
  });
});

describe('relationship-view-settings: resolveDisplayedColumns', () => {
  const candidates = resolveColumnCandidates({
    edgeColumns: ['role', 'weight'],
    targetFieldNames: ['email']
  });

  it('defaults to label + edge columns when unconfigured', () => {
    const cols = resolveDisplayedColumns(defaultViewSettings(), candidates);
    expect(cols.map((c) => c.token)).toEqual([
      LABEL_COLUMN,
      edgeColumnToken('role'),
      edgeColumnToken('weight')
    ]);
  });

  it('honors an explicit selection and keeps the label pinned first', () => {
    const settings: RelationshipViewSettings = {
      columns: [fieldColumnToken('email'), TYPE_COLUMN],
      sort: null,
      filter: null
    };
    const cols = resolveDisplayedColumns(settings, candidates);
    expect(cols.map((c) => c.token)).toEqual([
      LABEL_COLUMN,
      fieldColumnToken('email'),
      TYPE_COLUMN
    ]);
  });

  it('renders a selected token even when it is not a current candidate', () => {
    // e.g. a persisted target-field column whose schema has not loaded yet.
    const settings: RelationshipViewSettings = {
      columns: [fieldColumnToken('unloaded')],
      sort: null,
      filter: null
    };
    const cols = resolveDisplayedColumns(settings, [
      { token: LABEL_COLUMN, source: 'target', key: 'label', label: 'Target', pinned: true }
    ]);
    expect(cols.map((c) => c.token)).toEqual([LABEL_COLUMN, fieldColumnToken('unloaded')]);
    expect(cols[1].label).toBe('Unloaded');
  });

  it('never duplicates the label even if the selection includes it', () => {
    const settings: RelationshipViewSettings = {
      columns: [LABEL_COLUMN, edgeColumnToken('role')],
      sort: null,
      filter: null
    };
    const cols = resolveDisplayedColumns(settings, candidates);
    expect(cols.map((c) => c.token)).toEqual([LABEL_COLUMN, edgeColumnToken('role')]);
  });
});

describe('relationship-view-settings: cellValue', () => {
  const r = row({
    label: 'Sarah',
    nodeType: 'person',
    edgeValues: { role: 'lead' },
    targetProperties: { email: 'sarah@example.com' }
  });

  it('resolves each source', () => {
    expect(cellValue(r, LABEL_COLUMN)).toBe('Sarah');
    expect(cellValue(r, TYPE_COLUMN)).toBe('person');
    expect(cellValue(r, edgeColumnToken('role'))).toBe('lead');
    expect(cellValue(r, fieldColumnToken('email'))).toBe('sarah@example.com');
  });

  it('is undefined for missing keys / absent target properties', () => {
    expect(cellValue(r, edgeColumnToken('nope'))).toBeUndefined();
    expect(cellValue(row(), fieldColumnToken('email'))).toBeUndefined();
  });
});

describe('relationship-view-settings: applyViewSettings — filter', () => {
  const rows = [
    row({ id: 'a', label: 'Alice', edgeValues: { role: 'Lead' } }),
    row({ id: 'b', label: 'Bob', edgeValues: { role: 'member' } }),
    row({ id: 'c', label: 'Carol', edgeValues: {} })
  ];

  it('does a case-insensitive contains match on strings', () => {
    const out = applyViewSettings(rows, {
      columns: null,
      sort: null,
      filter: { column: edgeColumnToken('role'), value: 'lead' }
    });
    expect(out.map((r) => r.id)).toEqual(['a']);
  });

  it('excludes rows with a missing value for the filtered column', () => {
    const out = applyViewSettings(rows, {
      columns: null,
      sort: null,
      filter: { column: edgeColumnToken('role'), value: 'e' }
    });
    // Alice(Lead) + Bob(member) contain "e"; Carol has no role → excluded.
    expect(out.map((r) => r.id)).toEqual(['a', 'b']);
  });

  it('matches numbers by equality, not substring', () => {
    const numRows = [
      row({ id: 'a', edgeValues: { weight: 5 } }),
      row({ id: 'b', edgeValues: { weight: 50 } }),
      row({ id: 'c', edgeValues: { weight: 15 } })
    ];
    const out = applyViewSettings(numRows, {
      columns: null,
      sort: null,
      filter: { column: edgeColumnToken('weight'), value: '5' }
    });
    expect(out.map((r) => r.id)).toEqual(['a']);
  });

  it('treats a blank filter value as no filter', () => {
    const out = applyViewSettings(rows, {
      columns: null,
      sort: null,
      filter: { column: edgeColumnToken('role'), value: '   ' }
    });
    expect(out.map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });
});

describe('relationship-view-settings: applyViewSettings — sort', () => {
  it('sorts numbers numerically ascending and descending', () => {
    const rows = [
      row({ id: 'a', edgeValues: { weight: 10 } }),
      row({ id: 'b', edgeValues: { weight: 2 } }),
      row({ id: 'c', edgeValues: { weight: 100 } })
    ];
    const asc = applyViewSettings(rows, {
      columns: null,
      sort: { column: edgeColumnToken('weight'), direction: 'asc' },
      filter: null
    });
    expect(asc.map((r) => r.id)).toEqual(['b', 'a', 'c']);

    const desc = applyViewSettings(rows, {
      columns: null,
      sort: { column: edgeColumnToken('weight'), direction: 'desc' },
      filter: null
    });
    expect(desc.map((r) => r.id)).toEqual(['c', 'a', 'b']);
  });

  it('sorts strings case-insensitively', () => {
    const rows = [
      row({ id: 'a', label: 'bob' }),
      row({ id: 'b', label: 'Alice' }),
      row({ id: 'c', label: 'carol' })
    ];
    const out = applyViewSettings(rows, {
      columns: null,
      sort: { column: LABEL_COLUMN, direction: 'asc' },
      filter: null
    });
    expect(out.map((r) => r.id)).toEqual(['b', 'a', 'c']);
  });

  it('always sorts missing values last, in both directions', () => {
    const rows = [
      row({ id: 'a', edgeValues: { weight: 3 } }),
      row({ id: 'b', edgeValues: {} }),
      row({ id: 'c', edgeValues: { weight: 1 } })
    ];
    const asc = applyViewSettings(rows, {
      columns: null,
      sort: { column: edgeColumnToken('weight'), direction: 'asc' },
      filter: null
    });
    expect(asc.map((r) => r.id)).toEqual(['c', 'a', 'b']);

    const desc = applyViewSettings(rows, {
      columns: null,
      sort: { column: edgeColumnToken('weight'), direction: 'desc' },
      filter: null
    });
    expect(desc.map((r) => r.id)).toEqual(['a', 'c', 'b']);
  });

  it('sorts by a target-schema-field value', () => {
    const rows = [
      row({ id: 'a', targetProperties: { seats: 20 } }),
      row({ id: 'b', targetProperties: { seats: 5 } })
    ];
    const out = applyViewSettings(rows, {
      columns: null,
      sort: { column: fieldColumnToken('seats'), direction: 'asc' },
      filter: null
    });
    expect(out.map((r) => r.id)).toEqual(['b', 'a']);
  });
});

describe('relationship-view-settings: applyViewSettings — non-destructive', () => {
  it('does not mutate the input array or reorder it in place', () => {
    const rows = [
      row({ id: 'a', edgeValues: { weight: 2 } }),
      row({ id: 'b', edgeValues: { weight: 1 } })
    ];
    const snapshot = rows.map((r) => r.id);
    const out = applyViewSettings(rows, {
      columns: null,
      sort: { column: edgeColumnToken('weight'), direction: 'asc' },
      filter: null
    });
    expect(rows.map((r) => r.id)).toEqual(snapshot);
    expect(out).not.toBe(rows);
    expect(out.map((r) => r.id)).toEqual(['b', 'a']);
  });

  it('returns the same reference when neither sort nor filter applies', () => {
    const rows = [row({ id: 'a' })];
    expect(applyViewSettings(rows, defaultViewSettings())).toBe(rows);
  });
});
