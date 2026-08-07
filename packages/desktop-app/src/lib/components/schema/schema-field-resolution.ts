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
 * The backend reads nested-first with a flat fallback; reads here mirror that order so one
 * generic form renders both shapes correctly. Writes must match the shape the node already
 * uses — a flat write into a namespaced node is silently dropped (see `buildFieldWrite`).
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
 * Writing flat into a node that already has a namespace does NOT work: the backend's
 * hoisting step returns early when `properties[nodeType]` is already an object, so the
 * flat key is stored as a top-level sibling, and the read path then returns only the
 * namespace contents — silently discarding the edit. This mirrors `task-schema-form`,
 * which re-nests for the same reason.
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
