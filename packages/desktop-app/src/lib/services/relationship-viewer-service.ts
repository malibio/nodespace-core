/**
 * Relationship viewer + editor service (issue #1918).
 *
 * Thin dual-mode transport for the typed-relationship commands, routed through
 * the `backendAdapter` (Tauri IPC in the desktop app, HTTP dev-proxy in browser
 * dev mode) exactly like the schema/query paths — so the relationship feature is
 * validatable in `dev:browser`, not just the real Tauri app. Kept separate from
 * `relationship-grouping.ts` so the pure grouping/orientation logic stays free of
 * the adapter import and remains unit-testable.
 */

import { createLogger } from '$lib/utils/logger';
import { backendAdapter } from '$lib/services/backend-adapter';
import { isUserVisibleField } from '$lib/utils/schema-field-visibility';
import type { Node } from '$lib/types';
import {
  buildRelationshipsView,
  resolveEdgeEndpoints,
  type NodeRelationshipsView,
  type RawNodeRelationships,
  type RelationshipGroupView
} from './relationship-grouping';

const log = createLogger('RelationshipViewer');

/** Fetch the raw aggregate for a node (grouped relationships + edge data). */
export async function fetchNodeRelationships(nodeId: string): Promise<RawNodeRelationships> {
  log.debug('Fetching relationships', { nodeId });
  return await backendAdapter.getNodeRelationships(nodeId);
}

/** Fetch and normalize a node's relationships into the modal's view model. */
export async function loadNodeRelationshipsView(nodeId: string): Promise<NodeRelationshipsView> {
  const raw = await fetchNodeRelationships(nodeId);
  return buildRelationshipsView(raw);
}

/**
 * Add a typed relationship edge from the modal's node to `targetId` within
 * `group`. Orientation follows the group's direction (see `resolveEdgeEndpoints`):
 * for an outbound group the modal's node is the source; for an inbound group it
 * is the target. `edgeData` carries declared edge-field values (omit for a bare
 * edge). The daemon validates target type and cardinality.
 */
export async function addEdge(
  nodeId: string,
  group: RelationshipGroupView,
  targetId: string,
  edgeData?: Record<string, unknown>
): Promise<void> {
  const { sourceId, targetId: resolvedTarget } = resolveEdgeEndpoints(
    nodeId,
    group.direction,
    targetId
  );
  log.debug('Adding edge', { sourceId, relationshipName: group.relationshipName, targetId: resolvedTarget });
  await backendAdapter.createRelationship(sourceId, group.relationshipName, resolvedTarget, edgeData);
}

/**
 * Remove the typed relationship edge between the modal's node and the related
 * row `rowId` within `group`. The daemon rejects removing the last edge of a
 * `required` relationship — that surfaces here as a thrown error the caller
 * should present.
 */
export async function removeEdge(
  nodeId: string,
  group: RelationshipGroupView,
  rowId: string
): Promise<void> {
  const { sourceId, targetId } = resolveEdgeEndpoints(nodeId, group.direction, rowId);
  log.debug('Removing edge', { sourceId, relationshipName: group.relationshipName, targetId });
  await backendAdapter.deleteRelationship(sourceId, group.relationshipName, targetId);
}

/**
 * Replace the edge attributes on the edge between the modal's node and the
 * related row `rowId` within `group`. Overwrites the stored edge properties
 * wholesale (matching the daemon's replace semantics).
 */
export async function updateEdgeProperties(
  nodeId: string,
  group: RelationshipGroupView,
  rowId: string,
  properties: Record<string, unknown>
): Promise<void> {
  const { sourceId, targetId } = resolveEdgeEndpoints(nodeId, group.direction, rowId);
  log.debug('Updating edge properties', { sourceId, relationshipName: group.relationshipName, targetId });
  await backendAdapter.updateRelationshipProperties(sourceId, group.relationshipName, targetId, properties);
}

/**
 * Type-ahead search for edge targets of the declared `targetType` by title.
 * `targetType` null means the relationship has no declared target type — search
 * across all node types.
 */
export async function searchTargets(targetType: string | null, query: string): Promise<Node[]> {
  return await backendAdapter.searchNodesByTitle(targetType, query, 10);
}

/**
 * Field names declared on the target type's schema — the candidate set for
 * offering target-node columns in the modal's per-group view settings. Values
 * for those columns come from the related node's own properties (see
 * `fetchNodesProperties`). Returns `[]` when the schema has no fields.
 *
 * System-protected fields are excluded: these names become user-facing column
 * offerings, and a backend-owned field (a convergence marker, an ai-chat
 * `capture:transcript`) is not something to offer as a column any more than as
 * an editable control. Same predicate the table and detail views use.
 */
export async function fetchTargetSchemaFields(targetType: string): Promise<string[]> {
  const schema = await backendAdapter.getSchema(targetType);
  return (schema.fields ?? []).filter(isUserVisibleField).map((field) => field.name);
}

/**
 * Fetch the `properties` bag of each given node, keyed by id, so target-schema
 * -field columns can read the related node's own values. Missing/unreadable
 * nodes resolve to an empty bag rather than rejecting the whole batch, so one
 * bad id never blanks the others.
 */
export async function fetchNodesProperties(
  ids: string[]
): Promise<Record<string, Record<string, unknown>>> {
  const result: Record<string, Record<string, unknown>> = {};
  await Promise.all(
    ids.map(async (id) => {
      try {
        const node = await backendAdapter.getNode(id);
        result[id] = node?.properties ?? {};
      } catch (error) {
        log.warn('Failed to fetch target node properties', { id, error });
        result[id] = {};
      }
    })
  );
  return result;
}
