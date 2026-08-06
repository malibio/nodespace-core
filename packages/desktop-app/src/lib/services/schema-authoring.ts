/**
 * Schema-instance authoring helper.
 *
 * Backs the "+ New" action in `QueryNodeViewer`: mint a fresh, empty node of a
 * given schema type so the caller can open it immediately for schema-driven
 * field entry. Unlike `collection-authoring`'s `createNodeInCollection`, there
 * is no membership edge — a schema-type query is a flat `nodeType` filter, not
 * a `member_of` grouping — so this helper only creates the node.
 */

import { v4 as uuidv4 } from 'uuid';
import { backendAdapter } from '$lib/services/backend-adapter';
import type { Node } from '$lib/types';

/**
 * Mint a fresh, empty instance of the given schema type and return the created
 * node.
 *
 * The node is created as a root (`parentId: null`) with empty content and no
 * properties; the schema-driven form UI fills in the fields once the node is
 * opened. `nodeType` is the schema's id — the same key `QueryNodeViewer` queries
 * on — so the new node matches that type's result list.
 */
export async function createSchemaInstance(typeId: string): Promise<Node> {
  const newId = uuidv4();
  await backendAdapter.createNode({
    id: newId,
    nodeType: typeId,
    content: '',
    properties: {},
    mentions: [],
    parentId: null,
  });
  // Deliberate second round-trip: createNode returns only the id, but the caller
  // needs the full hydrated Node to seed the shared store — so load it back.
  const created = await backendAdapter.getNode(newId);
  if (!created) {
    throw new Error(`Newly created node ${newId} could not be loaded`);
  }
  return created;
}

/**
 * Decide whether a just-created instance should still be integrated into a
 * viewer's current results.
 *
 * Returns `false` when the active query generation (`loadId`) or database epoch
 * changed while the create was in flight — the node is still persisted, it just
 * must not be injected into a now-stale or switched-away view (ADR-053, the same
 * discipline the viewer's load path applies to its own writes).
 */
export function shouldIntegrateInstance(
  captured: { loadId: number; epoch: number },
  current: { loadId: number; epoch: number }
): boolean {
  return captured.loadId === current.loadId && captured.epoch === current.epoch;
}
