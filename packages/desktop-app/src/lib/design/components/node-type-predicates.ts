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
 * The value a viewer header shows while it is NOT focused.
 *
 * Which field wins depends on whether the node's schema is title_template-driven:
 *
 * - `hasTitleTemplate` — `title` is computed from the node's properties and is a genuinely
 *   different value from `content`, so it is the only valid display value. There is no
 *   fallback to `content`: an unresolved title renders empty and the read-only header shows
 *   the titleTemplate placeholder instead. A title with no word characters (e.g. the `" "`
 *   left by a `"{first_name} {last_name}"` template with both fields empty) counts as
 *   unresolved, matching the `/\w/` guard node-row.svelte applies when rendering these
 *   nodes inline — otherwise the header would render blank with no placeholder.
 * - otherwise — `content` is the source of truth. The store's cached `title` is only
 *   refreshed by a backend round-trip, while optimistic content updates leave it untouched,
 *   so preferring `title` here would surface a stale value (e.g. a title captured mid-slash
 *   command, before the node's type conversion) whenever the header loses focus.
 *
 * Markdown header syntax is stripped from content only (`## Foo` → `Foo`), for header nodes
 * whose content carries the `#` markers. A template-computed title is built from property
 * values and has no such syntax to strip.
 */
export function computeHeaderDisplayValue(
  node: { title?: string | null; content?: string | null } | null | undefined,
  hasTitleTemplate: boolean
): string {
  if (hasTitleTemplate) {
    const title = node?.title ?? '';
    return /\w/.test(title) ? title : '';
  }
  return (node?.content ?? '').replace(/^#+\s*/, '');
}
