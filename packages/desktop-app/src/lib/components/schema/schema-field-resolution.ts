/**
 * Field-value resolution for the generic, schema-driven properties form.
 *
 * Two property storage shapes are in play across node types:
 *
 * - **Namespaced** — schema-defined fields live under `properties[nodeType]` (e.g.
 *   `properties.project.status`). NodeService namespaces on create for every type except
 *   `schema`, so this is the normal shape for user-defined types as well as core ones.
 * - **Flat** — fields stored directly as `properties[fieldName]`. Reached by nodes written
 *   outside the create path, and by older rows predating namespacing.
 *
 * Core behaviors read nested-first with a flat fallback (`project`, `task`), though not
 * universally — `person` reads nested-only. Reads here mirror the nested-first order so one
 * generic form renders both shapes correctly, and writes preserve whichever shape the node
 * already uses (see `buildFieldWrite`).
 *
 * Extracted from generic-schema-form.svelte so both are unit-testable without rendering
 * the component.
 */

export interface FieldValueSource {
  nodeType: string;
  properties?: Record<string, unknown>;
}

/**
 * Read a schema field's value, preferring the type's property namespace over a flat key.
 *
 * @returns the stored value, or `null` when the field is unset in both shapes
 */
export function resolveFieldValue(node: FieldValueSource, fieldName: string): unknown {
  const namespace = node.properties?.[node.nodeType];
  if (namespace && typeof namespace === 'object' && fieldName in namespace) {
    return (namespace as Record<string, unknown>)[fieldName] ?? null;
  }
  return node.properties?.[fieldName] ?? null;
}

/**
 * Build the `properties` payload that writes `fieldName = value` in the shape the node
 * already stores — mirroring `resolveFieldValue`'s precedence so a field round-trips.
 *
 * The namespaced branch is load-bearing **because this payload spreads the node's existing
 * properties**. The backend's normalize step returns the payload untouched when it already
 * carries a `properties[nodeType]` key, so a spread payload plus a flat `fieldName` merges
 * as two siblings — the namespaced copy wins on read and the edit is silently discarded.
 * (A payload carrying *only* the bare field would be namespaced and merged correctly; it is
 * the spread that reintroduces the key and defeats that.) Re-nesting here keeps the edit in
 * the branch the read path actually consults. `task-schema-form` re-nests likewise.
 *
 * Anyone tempted to drop the spread should note it is what makes both branches necessary —
 * sending only the changed field would let a single flat write serve both shapes.
 */
export function buildFieldWrite(
  node: FieldValueSource,
  fieldName: string,
  value: unknown
): Record<string, unknown> {
  const properties = node.properties ?? {};
  const namespace = properties[node.nodeType];
  if (namespace && typeof namespace === 'object') {
    return {
      ...properties,
      [node.nodeType]: { ...(namespace as Record<string, unknown>), [fieldName]: value }
    };
  }
  return { ...properties, [fieldName]: value };
}
