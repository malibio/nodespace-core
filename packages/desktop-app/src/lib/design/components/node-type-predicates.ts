/**
 * Node type predicates
 *
 * Answers what the UI actually needs to know about a node type: **which frontend
 * integration does it have?** Every predicate here delegates to the plugin registry, so
 * they stay correct as types are registered and unregistered at runtime.
 *
 * This deliberately replaces an older hand-maintained "core node types" list that was
 * used as a proxy for "has a dedicated frontend integration". The two are unrelated:
 * `project` is a core type with no plugin registration at all, while `person`,
 * `document`, `user` and `ai-chat` are registered plugins that were never in the list.
 * Ask the registry, not a list.
 */

import { pluginRegistry } from '$lib/plugins/plugin-registry';

/**
 * True when a plugin registers an inline node component for this type.
 *
 * Types with one (text, task, header, query, …) are edited in place in the outline.
 * Types without one render through the BaseNode fallback as a read-only entity row.
 */
export function hasInlineNodeComponent(nodeType: string): boolean {
  return pluginRegistry.hasNodeComponent(nodeType);
}

/**
 * True when this type renders in the outline as a read-only entity row rather than an
 * inline-editable node — i.e. no plugin registered an inline node component for it.
 *
 * Entity rows get an "open in other pane" affordance, are skipped by arrow navigation
 * (there is nothing to put a caret into), and open directly rather than resolving to a
 * parent viewer.
 */
export function rendersAsEntityRow(nodeType: string): boolean {
  return !hasInlineNodeComponent(nodeType);
}

/**
 * True when this type needs the generic, schema-driven properties form — no plugin
 * registered a hardcoded, type-specific schema form for it.
 *
 * `task` and `person` have hardcoded forms; `project` and every user-defined type fall
 * back to the generic one.
 */
export function needsGenericSchemaForm(nodeType: string): boolean {
  return !pluginRegistry.hasSchemaForm(nodeType);
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
