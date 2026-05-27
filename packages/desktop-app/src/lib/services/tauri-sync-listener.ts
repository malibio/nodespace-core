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
import type { Node } from '$lib/types/node';
import { nodeToTaskNode } from '$lib/types/task-node';
import { backendAdapter } from './backend-adapter';
import { createLogger } from '$lib/utils/logger';
import { scheduleCollectionRefresh, scheduleSchemaRefresh } from '$lib/utils/collection-refresh';
import { registerSchemaPlugin, unregisterSchemaPlugin } from '$lib/plugins/schema-plugin-loader';

const log = createLogger('TauriSync');

/**
 * Strip the `node:` table prefix from a SurrealDB Thing id so it
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
 * Normalize node data from domain events to type-specific format
 *
 * Domain events send generic Node objects where type-specific fields (like task status)
 * are stored in `properties`. This function converts them to the flat format
 * expected by the frontend stores and components.
 *
 * @param nodeData - Raw node data from domain event
 * @returns Normalized node with flat type-specific fields for typed nodes
 */
function normalizeNodeData(nodeData: Node): Node {
  if (nodeData.nodeType === 'task') {
    return nodeToTaskNode(nodeData) as unknown as Node;
  }
  // Add other type-specific conversions here as needed (e.g., SchemaNode)
  return nodeData;
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
      // Normalize node data to type-specific format (e.g., TaskNode with flat status)
      const normalizedNode = normalizeNodeData(node);
      // Use database source with domain-event reason to indicate external change
      sharedNodeStore.setNode(normalizedNode, { type: 'database', reason: 'domain-event' }, true);
      log.debug(`${eventType}: updated store for node`, nodeId);
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
      log.debug(`Node updated: ${event.payload.id}`);
      // Issue #724: Only fetch if node is already in the store (visible to user)
      if (sharedNodeStore.hasNode(event.payload.id)) {
        fetchAndUpdateNode(event.payload.id, 'node:updated');
      } else {
        log.debug('Node not in store, skipping fetch:', event.payload.id);
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
        // Hierarchy relationship — always call addChild so the backend's authoritative
        // fractional order overwrites any optimistic order set during creation.
        // addChild handles deduplication internally: if the child already exists it
        // updates the order and re-sorts rather than inserting a duplicate.
        //
        // **Order-fallback contract** (nodespace-sync#77 root cause):
        //   - When the local daemon emits this event, `properties.order`
        //     carries the fractional order from `move_node` — that's the
        //     authoritative position and we use it directly.
        //   - When the **cloud LIVE-SELECT echo** re-emits the same edge,
        //     `properties.order` is `undefined` because the sync layer's
        //     `RELATE … SET rel_type = $rt` (cloud_writer.rs:430) does NOT
        //     push the order field to cloud. Before the fix this fell back
        //     to `Date.now()`, which silently overwrote the correct local
        //     order with a value 11 orders of magnitude larger — relocating
        //     the just-created sibling.
        //
        // Fix tier 1: when the incoming order is missing, look up the
        // existing entry in the structureTree and preserve its order.
        //
        // Fix tier 2: if the child is genuinely new to this client, append
        // it after the parent's current last child — order = lastChild.order
        // + 1 + tiny-jitter. This matches the backend's append-at-end
        // semantic and keeps brand-new sync-echoed children sorted at the
        // tail. (Previously `Date.now()` was used here, which dropped the
        // child to a position far below any sibling — a "stable" but
        // wrong-looking location.)
        //
        // **Performance note**: `getChildrenWithOrder(...).find(...)` is
        // O(n) per echo. Hot enough to matter for parents with hundreds
        // of children; consider exposing a Map-backed `getOrderOf(parent,
        // child)` primitive on structureTree if profiling shows a hotspot.
        if (structureTree) {
          const parentBare = stripNodePrefix(rel.fromId);
          const childBare = stripNodePrefix(rel.toId);
          // `typeof === 'number'` (not `??`) so 0 / negative orders count
          // as a real value. `rel.properties?.order` is typed `unknown` on
          // the wire; the `as number | undefined` is unchecked — the
          // typeof gate IS the runtime check.
          const incomingOrderRaw = (rel.properties as { order?: unknown } | undefined)?.order;
          const incomingOrder =
            typeof incomingOrderRaw === 'number' ? incomingOrderRaw : undefined;

          const siblings = structureTree.getChildrenWithOrder(parentBare);
          let order: number;
          if (typeof incomingOrder === 'number') {
            order = incomingOrder;
          } else {
            const existing = siblings.find((c) => c.nodeId === childBare);
            if (existing) {
              // Local optimistic path or an earlier authoritative event
              // already placed this child; keep that order.
              order = existing.order;
            } else {
              // Brand-new child via cloud echo with no order. Land at end
              // (lastSibling.order + 1) with a tiny jitter so concurrent
              // appends from different windows don't collide on order.
              const lastOrder = siblings[siblings.length - 1]?.order ?? 0;
              order = lastOrder + 1 + Math.random() * 0.001;
            }
          }
          structureTree.addChild({
            parentId: parentBare,
            childId: childBare,
            order
          });
        }
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
      if (rel.relationshipType === 'has_child' && structureTree) {
        // Date.now() is a defensive fallback only — a millisecond timestamp (~1.7e12) is
        // far outside the normal fractional order range and will sort the node to the end.
        // In practice, relationship:updated events from the backend always include order.
        const order = (rel.properties?.order as number) ?? Date.now();
        structureTree.updateChildOrder(
          stripNodePrefix(rel.fromId),
          stripNodePrefix(rel.toId),
          order
        );
      }
    });

    await listen<RelationshipDeletedPayload>('relationship:deleted', (event) => {
      const { id, fromId, toId, relationshipType } = event.payload;
      log.debug(`Relationship deleted: ${relationshipType} (${id}) from ${fromId} to ${toId}`);

      if (relationshipType === 'has_child') {
        // Hierarchy deletion - update ReactiveStructureTree
        if (structureTree) {
          structureTree.removeChild({
            parentId: stripNodePrefix(fromId),
            childId: stripNodePrefix(toId),
            order: 0 // Order doesn't matter for removal
          });
        }
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
