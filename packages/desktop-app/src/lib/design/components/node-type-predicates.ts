/**
 * Node type predicates
 *
 * Distinguishes NodeSpace's hardcoded core node types from custom, schema-driven
 * types. Custom schema node types are stored with a UUID as their `nodeType`;
 * everything in the core set ships built-in with hardcoded behavior.
 *
 * Single source of truth — shared by the viewer and the navigation service so the
 * core-type list can never drift between them.
 */

/** Core built-in node types that ship with NodeSpace — everything else is a custom schema type. */
export const CORE_NODE_TYPES = new Set([
  'text',
  'task',
  'project',
  'date',
  'header',
  'code-block',
  'quote-block',
  'ordered-list',
  'horizontal-line',
  'table',
  'checkbox',
  'collection',
  'query',
  'schema'
]);

/** True when the node type is a custom, schema-driven type rather than a core built-in. */
export function isCustomSchemaType(nodeType: string): boolean {
  return !CORE_NODE_TYPES.has(nodeType);
}

/**
 * The value a viewer header shows while it is NOT focused, with markdown header syntax
 * stripped (same rule as formatTabTitle).
 *
 * Which field wins depends on whether the node's schema is title_template-driven:
 *
 * - `hasTitleTemplate` — `title` is computed from the node's properties and is a genuinely
 *   different value from `content`, so it is the only valid display value. There is no
 *   fallback to `content`: an unresolved title renders empty and the read-only header shows
 *   the titleTemplate placeholder instead (matching node-row.svelte's inline rendering).
 * - otherwise — `content` is the source of truth. The store's cached `title` is only
 *   refreshed by a backend round-trip, while optimistic content updates leave it untouched,
 *   so preferring `title` here would surface a stale value (e.g. a title captured mid-slash
 *   command, before the node's type conversion) whenever the header loses focus.
 */
export function computeHeaderDisplayValue(
  node: { title?: string | null; content?: string | null } | null | undefined,
  hasTitleTemplate: boolean
): string {
  const raw = hasTitleTemplate ? (node?.title ?? '') : (node?.content ?? '');
  if (!raw) return '';
  return raw.replace(/^#+\s*/, '');
}
