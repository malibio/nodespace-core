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
