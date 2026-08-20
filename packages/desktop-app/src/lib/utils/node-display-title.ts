/**
 * The single rule for choosing between a node's `title` and `content` as its current
 * display value.
 *
 * `sharedNodeStore`'s cached `title` is refreshed only by a backend round-trip. Optimistic
 * content edits patch `content` and leave `title` untouched, so `title` is stale for any
 * node whose schema doesn't compute it — e.g. a title captured mid-slash-command, before a
 * node's type conversion, or simply the last-synced value for a node whose content has since
 * changed. `node.title || node.content` (the pattern this replaces) surfaces that stale value
 * on every surface using it.
 *
 * The correct rule: prefer `title` only when the node's schema is genuinely title_template-
 * driven — there `title` is computed from properties and is a legitimately different value
 * from `content`, so it's the only valid display value (an unresolved template renders empty,
 * not a fallback to content). Every other type has `content` as its source of truth.
 *
 * Callers determine `hasTitleTemplate` themselves, since the answer comes from different
 * places depending on context (a `SchemaFormLoader` instance inside a node viewer;
 * `pluginRegistry.findSlashCommand(nodeType)?.hasTitleTemplate` outside one — see
 * `schema-form-loader.svelte.ts`'s `hasTitleTemplate` getter for why the two agree: the
 * frontend's notion of "title_template-driven" is custom-schema-scoped, while the backend's
 * `compute_title()` applies a title_template to any schema carrying one, core or not; they
 * agree only because no core type ships a template today).
 */
export function resolveTitleOrContent(
  node: { title?: string | null; content?: string | null } | null | undefined,
  hasTitleTemplate: boolean
): string {
  if (hasTitleTemplate) {
    const title = node?.title ?? '';
    // A title with no word characters (e.g. the `" "` left by a `"{first} {last}"`
    // template with both fields empty) counts as unresolved.
    return /\w/.test(title) ? title : '';
  }
  return node?.content ?? '';
}
