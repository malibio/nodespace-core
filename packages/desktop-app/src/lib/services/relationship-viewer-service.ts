/**
 * Relationship viewer service (issue #1918, read-only slice).
 *
 * Thin Tauri transport for the `get_node_relationships` command. Kept separate
 * from `relationship-grouping.ts` so the pure grouping logic stays free of the
 * `@tauri-apps/api/core` import and remains unit-testable.
 */

import { invoke } from '@tauri-apps/api/core';
import { createLogger } from '$lib/utils/logger';
import {
  buildRelationshipsView,
  type NodeRelationshipsView,
  type RawNodeRelationships
} from './relationship-grouping';

const log = createLogger('RelationshipViewer');

/** Fetch the raw aggregate for a node (grouped relationships + edge data). */
export async function fetchNodeRelationships(nodeId: string): Promise<RawNodeRelationships> {
  log.debug('Fetching relationships', { nodeId });
  return await invoke<RawNodeRelationships>('get_node_relationships', { nodeId });
}

/** Fetch and normalize a node's relationships into the modal's view model. */
export async function loadNodeRelationshipsView(nodeId: string): Promise<NodeRelationshipsView> {
  const raw = await fetchNodeRelationships(nodeId);
  return buildRelationshipsView(raw);
}
