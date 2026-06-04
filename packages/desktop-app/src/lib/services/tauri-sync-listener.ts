/**
 * Tauri Domain Event Listener
 *
 * Listens for real-time synchronization events emitted from the Rust backend
 * via domain events. The backend's DomainEventForwarder service subscribes
 * to NodeService domain events and forwards them to the frontend via Tauri events.
 *
 * This module handles:
 * - Node events (created, updated, deleted) → updates SharedNodeStore
 * - Relationship events (has_child, mentions, member_of) → updates ReactiveStructureTree
 *
 * This enables real-time sync when external sources (MCP, other windows) modify data.
 *
 * Issue #724: Events now send only node_id (not full payload) for efficiency.
 * Frontend fetches full node data via getNode() API only when the node is in the active view.
 *
 * Issue #811: All relationship types use unified RelationshipCreated/Updated/Deleted events.
 */

import { listen } from '@tauri-apps/api/event';
import type {
  NodeEventData,
  RelationshipEvent,
  RelationshipDeletedPayload
} from '$lib/types/event-types';
import { sharedNodeStore } from './shared-node-store.svelte';
import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';
import { backendAdapter } from './backend-adapter';
import { createLogger } from '$lib/utils/logger';
import { scheduleCollectionRefresh, scheduleSchemaRefresh } from '$lib/utils/collection-refresh';
import { registerSchemaPlugin, unregisterSchemaPlugin } from '$lib/plugins/schema-plugin-loader';
import { applyHasChildCreated, applyHasChildUpdated, applyHasChildDeleted } from './hierarchy-sync';
import { normalizeNodeData } from './node-normalize';

const log = createLogger('TauriSync');

/**
 * Strip the `node:` table prefix from a stored record id so it
 * matches the bare-id key shape `reactiveStructureTree` uses
 * elsewhere in the app (the date-page route, the outliner's
 * local-action `addChild` path, and `sharedNodeStore` all key by
 * bare ids). Backend `RelationshipEvent` payloads carry the
 * prefixed form per the serialization contract; the frontend's
 * tree-keyspace is historically bare, so normalize at the boundary.
 */
function stripNodePrefix(id: string): string {
  return id.startsWith('node:') ? id.slice('node:'.length) : id;
}

/**
 * Fetch full node data from API and update SharedNodeStore
 *
 * Issue #724: Events now send only node_id. This function fetches the full
 * node data and updates the store.
 */
async function fetchAndUpdateNode(nodeId: string, eventType: string): Promise<void> {
  try {
    const node = await backendAdapter.getNode(nodeId);
    if (node) {
      const normalizedNode = normalizeNodeData(node);

      // Guard: never overwrite an ai-chat node with a snapshot that has fewer
      // messages than the current store. This prevents a stale echo (arriving
      // after hasPending clears) from wiping an optimistically-appended user
      // message before the daemon's next write delivers the assistant reply.
      const currentNode = sharedNodeStore.getNode(nodeId);
      if (currentNode?.nodeType === 'ai-chat') {
        const currentMsgs = (currentNode.properties?.['ai-chat'] as Record<string, unknown> | undefined)?.['messages'];
        const fetchedMsgs = (normalizedNode.properties?.['ai-chat'] as Record<string, unknown> | undefined)?.['messages'];
        const fetchedStatus = (normalizedNode.properties?.['ai-chat'] as Record<string, unknown> | undefined)?.['status'];
        const currentCount = Array.isArray(currentMsgs) ? currentMsgs.length : 0;
        const fetchedCount = Array.isArray(fetchedMsgs) ? fetchedMsgs.length : 0;
        log.info(`${eventType}: ai-chat node fetch complete`, { nodeId, fetchedCount, currentCount, fetchedStatus });
        if (fetchedCount < currentCount) {
          log.warn(`${eventType}: skipping stale ai-chat snapshot (fetched ${fetchedCount} msgs < current ${currentCount} msgs)`, nodeId);
          return;
        }
      }

      sharedNodeStore.setNode(normalizedNode, { type: 'database', reason: 'domain-event' }, true);
      log.info(`${eventType}: store updated for node`, nodeId);
    } else {
      log.warn(`${eventType}: node not found`, nodeId);
    }
  } catch (error) {
    log.error(`${eventType}: failed to fetch node`, { nodeId, error });
  }
}

/**
 * Initialize Tauri real-time synchronization event listeners
 *
 * Sets up listeners for logging/debugging sync events.
 * Should be called once during app initialization.
 *
 * @returns Promise resolving when all listeners are registered
 */
export async function initializeTauriSyncListeners(): Promise<void> {
  if (!isRunningInTauri()) {
    log.debug('Not running in Tauri environment, skipping sync listener initialization');
    return;
  }

  log.info('Initializing Tauri real-time sync listeners');

  try {
    // Listen for node events and update SharedNodeStore
    // Issue #724: Events now send only node_id, fetch full data if needed
    // Issue #832: node:created includes nodeType for reactive UI updates
    await listen<NodeEventData>('node:created', (event) => {
      log.debug(`Node created: ${event.payload.id} (type: ${event.payload.nodeType})`);

      // Issue #832: If a collection node is created, refresh collections sidebar
      if (event.payload.nodeType === 'collection') {
        scheduleCollectionRefresh();
      }

      // If a schema node is created, refresh the schema types sidebar
      if (event.payload.nodeType === 'schema') {
        scheduleSchemaRefresh();
        registerSchemaPlugin(event.payload.id).catch((err) =>
          log.error('Failed to register schema plugin:', err)
        );
      }

      // Fetch full node data since the node might be in the current view
      fetchAndUpdateNode(event.payload.id, 'node:created');
    });

    await listen<NodeEventData>('node:updated', (event) => {
      const nodeId = event.payload.id;
      const inStore = sharedNodeStore.hasNode(nodeId);
      log.info(`node:updated received`, { nodeId, inStore });
      if (inStore) {
        fetchAndUpdateNode(nodeId, 'node:updated');
      } else {
        log.warn('node:updated: node not in store, skipping fetch', nodeId);
      }
    });

    await listen<{ id: string }>('node:deleted', (event) => {
      log.debug(`Node deleted: ${event.payload.id}`);
      sharedNodeStore.deleteNode(event.payload.id, { type: 'database', reason: 'domain-event' }, true);

      // Issue #832: We don't know if deleted node was a collection without fetching,
      // but if we have it cached in collectionsData, we should refresh
      // For simplicity, we rely on the UI to handle stale data gracefully
      // A more robust solution would cache node types or include type in delete events
      unregisterSchemaPlugin(event.payload.id);
    });

    // ========================================================================
    // Unified Relationship Events (Issue #811)
    // All relationship types (has_child, member_of, mentions, custom) use these events.
    // ========================================================================

    await listen<RelationshipEvent>('relationship:created', (event) => {
      const rel = event.payload;
      log.debug(`Relationship created: ${rel.relationshipType} (${rel.fromId} -> ${rel.toId})`);

      // Handle different relationship types
      if (rel.relationshipType === 'has_child') {
        const parentBare = stripNodePrefix(rel.fromId);
        const childBare = stripNodePrefix(rel.toId);
        applyHasChildCreated(structureTree, {
          parentId: parentBare,
          childId: childBare,
          order: (rel.properties as { order?: unknown } | undefined)?.order
        });
      } else if (rel.relationshipType === 'member_of') {
        // Collection membership changed - refresh collections sidebar.
        // `scheduleCollectionRefresh` compares the passed id against
        // `state.selectedCollectionId`, which is keyed by bare ids
        // elsewhere in the app — strip the `node:` prefix the
        // serialization contract requires.
        const toId = stripNodePrefix(rel.toId);
        log.debug(`Member added: ${rel.fromId} to collection ${toId}`);
        scheduleCollectionRefresh(toId);
      } else if (rel.relationshipType === 'mentions') {
        // Mention relationship created - target node's mentionedIn needs refresh
        // mentionedIn is populated by get_children_tree, so we need to refetch the tree
        // for the target node to get updated backlinks. Strip prefix for log clarity;
        // when this branch grows to call `loadChildrenTree`, normalization will be
        // necessary for the lookup to hit the bare-id keyspace.
        log.debug(
          `Mention created: ${stripNodePrefix(rel.fromId)} mentions ${stripNodePrefix(rel.toId)}`
        );

        // If the target node is currently displayed, its mentionedIn will update
        // on next tree load. For immediate reactivity, the user can refresh the view.
        // Future enhancement: call loadChildrenTree for toId if it's the current view.
      } else {
        // Custom relationship type
        log.debug(`Custom relationship created: ${rel.relationshipType}`);
      }
    });

    await listen<RelationshipEvent>('relationship:updated', (event) => {
      const rel = event.payload;
      log.debug(`Relationship updated: ${rel.relationshipType} (${rel.fromId} -> ${rel.toId})`);
      if (rel.relationshipType === 'has_child') {
        applyHasChildUpdated(structureTree, {
          parentId: stripNodePrefix(rel.fromId),
          childId: stripNodePrefix(rel.toId),
          order: (rel.properties?.order as unknown)
        });
      }
    });

    await listen<RelationshipDeletedPayload>('relationship:deleted', (event) => {
      const { id, fromId, toId, relationshipType } = event.payload;
      log.debug(`Relationship deleted: ${relationshipType} (${id}) from ${fromId} to ${toId}`);

      if (relationshipType === 'has_child') {
        applyHasChildDeleted(structureTree, {
          parentId: stripNodePrefix(fromId),
          childId: stripNodePrefix(toId)
        });
      } else if (relationshipType === 'member_of') {
        // Collection membership removed - refresh collections sidebar.
        // Bare-id keyspace, same rationale as `relationship:created`
        // above.
        const bareToId = stripNodePrefix(toId);
        log.debug(`Member removed from collection: ${id}`);
        scheduleCollectionRefresh(bareToId);
      } else if (relationshipType === 'mentions') {
        // Mention relationship deleted - target node's mentionedIn needs refresh.
        log.debug(
          `Mention deleted: ${id} (${stripNodePrefix(fromId)} -> ${stripNodePrefix(toId)})`
        );

        // Same as creation: mentionedIn updates on next tree load for toId.
        // Future enhancement: call loadChildrenTree for toId if it's the current view.
      }
    });

    // Listen for synchronization errors
    await listen<Record<string, unknown>>('sync:error', (event) => {
      const message = String(event.payload.message);
      const errorType = String(event.payload.errorType);
      log.error(`Sync error (${errorType}): ${message}`);
    });

    // Listen for synchronization status changes
    await listen<Record<string, unknown>>('sync:status', (event) => {
      const status = String(event.payload.status);
      const reason = event.payload.reason ? String(event.payload.reason) : '';
      log.info(`Sync status: ${status}${reason ? ` (${reason})` : ''}`);
    });

    log.info('Real-time sync listeners initialized successfully');
  } catch (error) {
    log.error('Failed to initialize sync listeners', error);
    throw new Error(`Failed to initialize sync listeners: ${error}`);
  }
}

/**
 * Check if running in Tauri environment
 */
function isRunningInTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window;
}
