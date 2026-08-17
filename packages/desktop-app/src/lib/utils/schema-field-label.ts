/**
 * Single shared accessor for a schema field's UI display label.
 *
 * `friendlyName` is always populated in storage — derived from `name` at the
 * write boundary (create_schema/update_schema) when the caller omits it — so
 * every UI surface reads it unconditionally through this helper. No fallback
 * to `description` (which is LLM-facing prose, not label text — see
 * `SchemaField.description`) and no per-component regex humanization of
 * `name`.
 */
import type { SchemaField } from '$lib/types/schema-node';

export function labelForField(field: SchemaField): string {
  return field.friendlyName;
}
