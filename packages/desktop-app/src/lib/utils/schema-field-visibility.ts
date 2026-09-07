/**
 * Single shared predicate for "should this schema field be shown to a user?".
 *
 * `protection: 'system'` marks a field the backend owns end to end — a
 * convergence marker like person's `_possible_duplicate`, an ai-chat session
 * bookkeeping field like `capture:transcript`. Nothing a user types, and on a
 * local-only install several of them are empty by construction. They must not
 * render as an editable control, and equally must not render as a table column
 * that can never be filled.
 *
 * Every UI surface that iterates `SchemaNode.fields` filters through this, so
 * the detail form and the table view cannot drift apart on what "visible"
 * means.
 */
import type { SchemaField } from '$lib/types/schema-node';

export function isUserVisibleField(field: SchemaField): boolean {
  return field.protection !== 'system';
}
