/**
 * Pure helpers for the Kanban view (query-node-viewer).
 *
 * Kept DOM-free and side-effect-free so the grouping / eligibility / write-shape
 * / per-column reveal-set rules can be unit-tested directly, following the
 * project convention of testing extracted logic rather than rendering Svelte
 * components.
 */

import type { Node } from '$lib/types';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';

/** Column key used for nodes whose group-by value is unset or unrecognized. */
export const UNASSIGNED = '__unassigned__';

/** A Kanban column derived from an enum field value. */
export interface KanbanColumn {
  /** The stored enum value that defines this column. */
  value: string;
  /** Human-readable label for the column header. */
  label: string;
}

/**
 * Convert a snake_case schema field name to the camelCase key the API uses for
 * typed core fields (e.g. `due_date` → `dueDate`). Mirrors `table-row.svelte`.
 */
function toCamelCase(name: string): string {
  return name.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

/**
 * Fields a Kanban board can group by.
 *
 * Enum-only: enum fields carry `coreValues`/`userValues`, which give both the
 * complete column set and the display labels. Non-enum fields have unbounded
 * value sets, so columns could only be inferred from values that happen to
 * exist and dragging could not offer a valid target set.
 *
 * Also excludes an enum field if any of its values is literally `UNASSIGNED`
 * (`"__unassigned__"`) — the internal sentinel `groupByColumn` below uses for
 * the "no value" bucket. Nothing stops a schema author from choosing that
 * exact string as a real enum value, but if one did, that value and every
 * genuinely-unset node would collide into the same bucket (indistinguishable,
 * and unreachable by name — the display column is always labeled
 * "Unassigned", never that value's own label) and `displayColumns` would carry
 * two entries with the same key. Rather than a bucketing scheme that has to
 * reconcile that collision, the simpler and safer rule is: a field that could
 * produce it isn't offered as a Kanban grouping choice at all — this field's
 * other values are still visible via List/Table, just not this board.
 */
export function eligibleGroupByFields(schema: SchemaNode | null): SchemaField[] {
  return (schema?.fields ?? []).filter(
    (f) =>
      f.type === 'enum' &&
      !(f.coreValues ?? []).some((v) => v.value === UNASSIGNED) &&
      !(f.userValues ?? []).some((v) => v.value === UNASSIGNED)
  );
}

/**
 * The ordered set of columns for an enum field: its core values followed by its
 * user-extensible values, each mapped to `{ value, label }`.
 */
export function enumColumns(field: SchemaField | undefined | null): KanbanColumn[] {
  if (!field) return [];
  const all = [...(field.coreValues ?? []), ...(field.userValues ?? [])];
  return all.map((ev) => ({ value: ev.value, label: ev.label }));
}

/**
 * Read a node's value for the given field, mirroring `table-row.svelte`'s
 * resolution order: camelCase top-level (typed core fields) → snake_case
 * top-level → `properties[field]` (user-defined schema fields). Returns `null`
 * for unset/empty values.
 *
 * Precedence here must exactly mirror `resolveFieldWrite`'s below — both
 * gate the top-level branches on the field NOT also being present in
 * `properties`, not just on the top-level slot being defined. A user-defined
 * type is free to use a bare field name that shadows a core property (e.g.
 * "status" — CLAUDE.md documents this as discouraged but not forbidden); for
 * such a node, an unconditional `rec[camel] ?? …` would read a stale/unset
 * top-level slot while every write still lands in `properties[field]` (the
 * slot `resolveFieldWrite` actually targets, since `field in props` there),
 * so the board would never reflect its own writes — reads and writes must
 * agree on which slot is authoritative for a given node, not just default to
 * the same *kind* of slot independently.
 */
export function readGroupValue(node: Node, field: string): string | null {
  const rec = node as unknown as Record<string, unknown>;
  const camel = toCamelCase(field);
  const props = (node.properties ?? {}) as Record<string, unknown>;
  const raw =
    rec[camel] !== undefined && !(camel in props)
      ? rec[camel]
      : rec[field] !== undefined && !(field in props)
        ? rec[field]
        : props[field];
  if (raw === null || raw === undefined || raw === '') return null;
  return String(raw);
}

/**
 * Build the `updateNode` change-set that moves a node into the column identified
 * by `value`, writing the value back to wherever it is *read* from (see
 * `readGroupValue`) so the card re-groups consistently after the store update.
 *
 * `value` is `null` for a move to Unassigned — written through as a genuine
 * `null`/absent value, not an empty string. An empty string is a real
 * (if unusual) enum value some backends would accept and persist as a
 * `TaskStatus::User("")` rather than clearing the field, so callers moving a
 * card to Unassigned must pass `null` here, not `''`.
 *
 * Not every field this can be called for actually HAS clear semantics on the
 * backend, though: task's `status` is a required, non-nullable `TaskStatus`
 * with no "cleared" state at all (unlike its siblings `priority`/`dueDate`/
 * `assignee`, which are genuinely optional). A `null` write to such a field
 * round-trips as an HTTP success while silently changing nothing server-side
 * — the caller is responsible for never offering Unassigned as a target for
 * a `required: true` schema field in the first place (kanban-view.svelte's
 * `displayColumns` and `moveCard` both guard on `activeField?.required`).
 *
 * Both shapes this produces persist through the store's viewer-write rule: a
 * user-defined schema field — the common Kanban case — is written under
 * `properties[field]` (property changes always persist, matching
 * `generic-schema-form`), and a typed core field stays a top-level field, which
 * persists via that type's registered updater for the mutable core enums
 * (`status`/`priority`/…). Grouping a core type by a non-standard top-level enum
 * field is out of scope — its board would move cards but not persist them.
 */
export function resolveFieldWrite(
  node: Node,
  field: string,
  value: string | null
): Partial<Node> {
  const rec = node as unknown as Record<string, unknown>;
  const camel = toCamelCase(field);
  const props = (node.properties ?? {}) as Record<string, unknown>;

  if (rec[camel] !== undefined && !(camel in props)) {
    return { [camel]: value } as unknown as Partial<Node>;
  }
  if (rec[field] !== undefined && !(field in props)) {
    return { [field]: value } as unknown as Partial<Node>;
  }
  // Default and user-defined-schema case: the bare property.
  return { properties: { ...props, [field]: value } };
}

/**
 * Bucket `{ id, value }` items into columns. Every column value gets an entry
 * (even if empty), plus a trailing `UNASSIGNED` bucket for items whose value is
 * `null` or does not match any column. Column order is preserved.
 */
export function groupByColumn(
  items: Array<{ id: string; value: string | null }>,
  columnValues: string[]
): Map<string, string[]> {
  const valid = new Set(columnValues);
  const buckets = new Map<string, string[]>();
  for (const cv of columnValues) buckets.set(cv, []);
  buckets.set(UNASSIGNED, []);

  for (const item of items) {
    const key = item.value !== null && valid.has(item.value) ? item.value : UNASSIGNED;
    buckets.get(key)!.push(item.id);
  }
  return buckets;
}

/**
 * Pick the group-by field to use: the stored selection if it is still an
 * eligible field, otherwise the first eligible field, otherwise `null` (the
 * schema has no enum field and the view should show an empty state).
 */
export function resolveActiveGroupBy(
  eligible: SchemaField[],
  stored: string | undefined
): string | null {
  if (stored && eligible.some((f) => f.name === stored)) return stored;
  return eligible[0]?.name ?? null;
}

/**
 * Grow a column's revealed-id set by up to `batch` more ids, in `ids` order,
 * preserving every id already in `revealed` regardless of where it now sits
 * in `ids`. Used to bound Kanban's per-column render (a "+N more" control)
 * without List/Table's flip-page pagination: capping by *position* alone
 * (`ids.slice(0, n)`) can't guarantee an already-shown card stays shown,
 * because a different card joining the column ahead of it in `ids` order
 * would push it past a plain positional cutoff — exactly the "card vanishes
 * out from under an in-progress drag" failure this exists to avoid. Tracking
 * by id instead means a card, once revealed, stays revealed for as long as
 * it remains in this column, independent of how the column's order churns.
 */
export function growRevealed(
  revealed: ReadonlySet<string>,
  ids: string[],
  batch: number
): Set<string> {
  const next = new Set(revealed);
  let added = 0;
  for (const id of ids) {
    if (added >= batch) break;
    if (!next.has(id)) {
      next.add(id);
      added++;
    }
  }
  return next;
}
