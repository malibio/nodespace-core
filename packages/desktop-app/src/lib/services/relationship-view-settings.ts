/**
 * Relationship viewer — per-group VIEW SETTINGS model (pure logic).
 *
 * The relationship modal renders each table-layout group as "Target" plus a set
 * of columns. This module is the pure, framework-free core that decides:
 *  - which columns a group CAN show (candidate resolution across three sources),
 *  - which columns it currently shows (applying a persisted selection),
 *  - and how a group's rows read/sort/filter for a chosen column.
 *
 * Kept free of Svelte/DOM/adapter imports so it is unit-testable in isolation
 * (see `src/tests/unit/relationship-view-settings.test.ts`) and can be shared by
 * the modal and the persistence service without pulling either into the other.
 *
 * ## Column model
 *
 * A displayable column is identified by a stable string TOKEN combining a
 * source with a key, so a selection survives round-tripping through
 * localStorage and never collides across sources:
 *  - `target:label` — the related node's title (always shown, pinned first).
 *  - `target:type`  — the related node's type (intrinsic).
 *  - `edge:<name>`  — an edge-attribute value (from the connecting edge).
 *  - `field:<name>` — a field of the TARGET node's own schema, whose value comes
 *                     from the related node's properties.
 */

import { humanizeName } from './relationship-grouping';

export type ColumnSource = 'target' | 'edge' | 'field';
export type SortDirection = 'asc' | 'desc';

/** The always-present first column: the related node's title/label. */
export const LABEL_COLUMN = 'target:label';
/** The intrinsic target node-type column. */
export const TYPE_COLUMN = 'target:type';

/** Build the token for an edge-attribute column. */
export function edgeColumnToken(name: string): string {
  return `edge:${name}`;
}

/** Build the token for a target-schema-field column. */
export function fieldColumnToken(name: string): string {
  return `field:${name}`;
}

interface ParsedColumn {
  source: ColumnSource;
  key: string;
}

/**
 * Split a column token into its source + key. Splits on the FIRST colon so a
 * key that itself contains a colon is preserved. An unrecognized/prefixless
 * token is treated as an edge column (the historical, most common source).
 */
export function parseColumnToken(token: string): ParsedColumn {
  const idx = token.indexOf(':');
  if (idx === -1) return { source: 'edge', key: token };
  const prefix = token.slice(0, idx);
  const key = token.slice(idx + 1);
  if (prefix === 'target') return { source: 'target', key };
  if (prefix === 'field') return { source: 'field', key };
  return { source: 'edge', key };
}

/** Human-readable heading for a column token. */
export function columnLabel(token: string): string {
  const { source, key } = parseColumnToken(token);
  if (source === 'target' && key === 'label') return 'Target';
  if (source === 'target' && key === 'type') return 'Type';
  return humanizeName(key);
}

/** A column the group offers, ready to render as a header/picker entry. */
export interface ColumnCandidate {
  /** Stable id, e.g. `edge:role`, `target:type`, `field:capacity`. */
  token: string;
  source: ColumnSource;
  /** The underlying field name, or `label`/`type` for intrinsic columns. */
  key: string;
  /** Humanized display label. */
  label: string;
  /** True for `LABEL_COLUMN`: always shown, first, and not removable. */
  pinned?: boolean;
}

/** The candidate column set spans edge attributes, intrinsics, and target fields. */
export interface ColumnCandidateSources {
  /** Edge-attribute column names (declared + ad-hoc), in display order. */
  edgeColumns: string[];
  /** Target node's own schema field names (values read from row properties). */
  targetFieldNames?: string[] | null;
}

/**
 * Ordered candidate columns for a group:
 * `Target` (pinned) → `Type` → edge attributes → target schema fields. A target
 * field whose name collides with an edge column is skipped, since they would
 * render an indistinguishable duplicate heading.
 */
export function resolveColumnCandidates(sources: ColumnCandidateSources): ColumnCandidate[] {
  const candidates: ColumnCandidate[] = [];
  const seen = new Set<string>();
  const push = (token: string, source: ColumnSource, key: string, pinned?: boolean) => {
    if (seen.has(token)) return;
    seen.add(token);
    candidates.push({ token, source, key, label: columnLabel(token), ...(pinned ? { pinned } : {}) });
  };

  push(LABEL_COLUMN, 'target', 'label', true);
  push(TYPE_COLUMN, 'target', 'type');

  const edgeNames = new Set<string>();
  for (const name of sources.edgeColumns) {
    edgeNames.add(name);
    push(edgeColumnToken(name), 'edge', name);
  }
  for (const name of sources.targetFieldNames ?? []) {
    if (edgeNames.has(name)) continue;
    // `type`/`label` would duplicate the intrinsic target columns (same heading).
    if (name === 'type' || name === 'label') continue;
    push(fieldColumnToken(name), 'field', name);
  }
  return candidates;
}

/** A group's persisted presentation preference. */
export interface RelationshipViewSort {
  /** A column token to sort by. */
  column: string;
  direction: SortDirection;
}

export interface RelationshipViewFilter {
  /** A column token to filter on. */
  column: string;
  /** Raw filter text (contains for strings, equals for numbers). */
  value: string;
}

export interface RelationshipViewSettings {
  /**
   * Ordered selected column tokens, EXCLUDING the always-shown `LABEL_COLUMN`.
   * `null` means "not configured" — the default set (edge columns) is used, so
   * an untouched group looks exactly as it did before view settings existed.
   */
  columns: string[] | null;
  sort: RelationshipViewSort | null;
  filter: RelationshipViewFilter | null;
}

/** The neutral settings: default columns, no sort, no filter. */
export function defaultViewSettings(): RelationshipViewSettings {
  return { columns: null, sort: null, filter: null };
}

/** The default displayed columns (excluding the pinned label): edge attributes. */
export function defaultColumnTokens(candidates: ColumnCandidate[]): string[] {
  return candidates.filter((c) => c.source === 'edge').map((c) => c.token);
}

function synthesizeCandidate(token: string): ColumnCandidate {
  const { source, key } = parseColumnToken(token);
  return { token, source, key, label: columnLabel(token) };
}

/**
 * The ordered columns to actually render for a group: the pinned label first,
 * then either the explicit selection or the default set. A selected token that
 * is not a current candidate (e.g. a persisted target-field column whose schema
 * has not loaded yet) is still rendered — a synthesized candidate is derived
 * from the token — so a saved layout never silently drops columns.
 */
export function resolveDisplayedColumns(
  settings: RelationshipViewSettings,
  candidates: ColumnCandidate[]
): ColumnCandidate[] {
  const byToken = new Map(candidates.map((c) => [c.token, c]));
  const labelCandidate =
    byToken.get(LABEL_COLUMN) ??
    ({ token: LABEL_COLUMN, source: 'target', key: 'label', label: 'Target', pinned: true } as ColumnCandidate);

  const tokens = settings.columns ?? defaultColumnTokens(candidates);
  const result: ColumnCandidate[] = [labelCandidate];
  const used = new Set<string>([LABEL_COLUMN]);
  for (const token of tokens) {
    if (used.has(token)) continue;
    used.add(token);
    result.push(byToken.get(token) ?? synthesizeCandidate(token));
  }
  return result;
}

/** A row the settings operate on. `targetProperties` backs `field:` columns. */
export interface RelationshipViewRow {
  id: string;
  nodeType: string;
  label: string;
  edgeValues: Record<string, unknown>;
  targetProperties?: Record<string, unknown>;
}

/** Resolve a row's value for a column token. */
export function cellValue(row: RelationshipViewRow, token: string): unknown {
  const { source, key } = parseColumnToken(token);
  if (source === 'target') {
    if (key === 'label') return row.label;
    if (key === 'type') return row.nodeType;
    return undefined;
  }
  if (source === 'edge') return row.edgeValues?.[key];
  return row.targetProperties?.[key];
}

function isMissing(value: unknown): boolean {
  return value === null || value === undefined || value === '';
}

/** A finite number if the value is numeric (number or numeric string), else null. */
function toNumber(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (trimmed === '') return null;
    const n = Number(trimmed);
    return Number.isNaN(n) ? null : n;
  }
  return null;
}

function normalizeString(value: unknown): string {
  if (typeof value === 'string') return value.toLowerCase();
  if (typeof value === 'number' || typeof value === 'boolean') return String(value).toLowerCase();
  return JSON.stringify(value).toLowerCase();
}

/**
 * Order two present (non-missing) cell values: numeric when both parse as
 * numbers, otherwise case-insensitive string comparison. Missing values are
 * handled by the caller (they always sort last, independent of direction).
 */
function comparePresent(a: unknown, b: unknown): number {
  const an = toNumber(a);
  const bn = toNumber(b);
  if (an !== null && bn !== null) return an < bn ? -1 : an > bn ? 1 : 0;
  const as = normalizeString(a);
  const bs = normalizeString(b);
  return as < bs ? -1 : as > bs ? 1 : 0;
}

/** Does a cell value match the filter? Contains for strings, equals for numbers. */
function matchesFilter(cell: unknown, filterValue: string): boolean {
  const needle = filterValue.trim().toLowerCase();
  if (needle === '') return true;
  if (isMissing(cell)) return false;
  const cellNum = toNumber(cell);
  const needleNum = toNumber(needle);
  if (cellNum !== null && needleNum !== null) return cellNum === needleNum;
  return normalizeString(cell).includes(needle);
}

/**
 * Apply a group's filter then sort to its rows, NON-DESTRUCTIVELY — the input
 * array and its rows are never mutated; a filtered/sorted copy is returned (the
 * same reference is returned only when neither a filter nor a sort applies).
 * Missing values always sort last, in both ascending and descending order.
 */
export function applyViewSettings(
  rows: RelationshipViewRow[],
  settings: RelationshipViewSettings
): RelationshipViewRow[] {
  let result = rows;

  if (settings.filter && settings.filter.value.trim() !== '') {
    const { column, value } = settings.filter;
    result = result.filter((row) => matchesFilter(cellValue(row, column), value));
  }

  if (settings.sort) {
    const { column, direction } = settings.sort;
    const dir = direction === 'desc' ? -1 : 1;
    result = [...result].sort((ra, rb) => {
      const a = cellValue(ra, column);
      const b = cellValue(rb, column);
      const aMissing = isMissing(a);
      const bMissing = isMissing(b);
      if (aMissing && bMissing) return 0;
      if (aMissing) return 1;
      if (bMissing) return -1;
      return dir * comparePresent(a, b);
    });
  }

  return result;
}
