/**
 * Field-value resolution for the generic, schema-driven properties form.
 *
 * Two property storage shapes are in play across node types:
 *
 * - **Namespaced** — core types with hardcoded backend behavior keep schema-defined fields
 *   under `properties[nodeType]` (e.g. `properties.project.status`). NodeService hoists
 *   them there on write.
 * - **Flat** — user-defined schema types store fields directly as `properties[fieldName]`.
 *
 * The backend reads nested-first with a flat fallback; this mirrors that order so one
 * generic form renders both shapes correctly.
 *
 * Extracted from generic-schema-form.svelte so the resolution order is unit-testable
 * without rendering the component.
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
