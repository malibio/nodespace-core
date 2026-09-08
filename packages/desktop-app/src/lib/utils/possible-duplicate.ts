/**
 * Read-side accessor for the convergence "possible duplicate" marker
 * (ADR-065 §4).
 *
 * `NodeService::mark_possible_duplicates` (core) and `nodespace-sync`'s
 * pulled-write handler both stamp `properties.<node_type>._possible_duplicate
 * = true` on a node when it collides with another active node on a
 * schema-declared `unique` field, after both copies land in the same
 * database (typically via sync convergence). Nothing strips this property
 * before it reaches the frontend — `node.properties` arrives unfiltered on
 * every existing read path (getNode, queryNodes, the WatchNodes push, a
 * findDuplicateFor match, …) — so this is a pure, local read, never a fresh
 * fetch of its own.
 *
 * This is the frontend's single source of truth for the marker's location,
 * mirroring `NodeService::is_possible_duplicate` (core,
 * `services/node_service/schema.rs`) on the Rust side: both read the same
 * `properties.<node_type>._possible_duplicate` path via the same field-name
 * constant, so a scattered `node.properties.person._possible_duplicate`
 * check at each UI call site can never drift from what the backend actually
 * writes.
 */
import type { Node } from '$lib/types';

/** Property key the marker is stored under, namespaced by node type. */
export const POSSIBLE_DUPLICATE_FIELD = '_possible_duplicate';

/**
 * Does `node` carry the convergence "possible duplicate" marker?
 *
 * Generic across node types (matches `mark_possible_duplicates`'s own
 * generic-by-construction design): reads `properties.<node.nodeType>.
 * _possible_duplicate`, not a hardcoded `person` lookup. Returns `false` for
 * a missing node, a missing/non-boolean marker, or a marker explicitly set
 * to `false` — the marker is only ever meaningfully `true` (nothing clears
 * it once written).
 */
export function isPossibleDuplicate(node: Node | null | undefined): boolean {
  if (!node) return false;
  const typeProps = node.properties?.[node.nodeType];
  if (typeProps === null || typeof typeProps !== 'object') return false;
  return (typeProps as Record<string, unknown>)[POSSIBLE_DUPLICATE_FIELD] === true;
}
