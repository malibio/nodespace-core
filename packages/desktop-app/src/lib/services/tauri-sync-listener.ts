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
 * Events send only node_id (not full payload) for efficiency.
 * Frontend fetches full node data via getNode() API only when the node is in the active view.
 *
 * All relationship types use unified RelationshipCreated/Updated/Deleted events.
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
import { proSync } from '$lib/stores/pro-sync.svelte';
import { isActiveDatabaseEvent } from '$lib/stores/database.svelte';

const log = createLogger('TauriSync');

// ---------------------------------------------------------------------------
// Pro-only reconnect-replay render coalescing
//
// On reconnect the Pro daemon now flushes a caught-up batch of node events as a
// single burst. Applied one-by-one, each `node:created/updated`
// triggers its own async fetch + `setNode` → one re-render per node. To render
// the burst in one pass, when sync is active we collect the node ids over a tiny
// window, fetch them together, then apply them in a SYNCHRONOUS `setNode` loop —
// Svelte batches synchronous store mutations into a single render.
//
// Gated on `proSync.isPro`: reconnect replay is a Pro-only flow, so the community
// build never enters this path and its per-event behavior is byte-for-byte
// unchanged.
// ---------------------------------------------------------------------------

/** How long to gather a burst before flushing. One frame (~16ms) is enough to
 *  collect a daemon-flushed batch without adding perceptible latency. */
const REPLAY_COALESCE_WINDOW_MS = 16;

const pendingNodeIds = new Set<string>();
let coalesceTimer: ReturnType<typeof setTimeout> | null = null;
// Ids deleted while a flush is mid-fetch (between the snapshot and the apply
// loop). The flush skips these so a delete that lands during the fetch wins over
// the now-stale upsert it raced — without this, the re-fetch could resurrect a
// just-deleted node (a failure class opened anew by coalescing).
let flushInProgress = false;
const tombstonedDuringFlush = new Set<string>();

/** Drop any queued/in-flight re-fetch for a node that was just deleted, so the
 *  delete wins over a racing upsert in the same coalescing window. */
function cancelPendingNodeFetch(nodeId: string): void {
  pendingNodeIds.delete(nodeId);
  if (flushInProgress) tombstonedDuringFlush.add(nodeId);
}

/** Reset all coalescer state. Called on (re)init so a stale in-flight timer from
 *  a prior listener registration can't fire into fresh state. */
function resetNodeFetchCoalescer(): void {
  if (coalesceTimer !== null) {
    clearTimeout(coalesceTimer);
    coalesceTimer = null;
  }
  pendingNodeIds.clear();
  tombstonedDuringFlush.clear();
  flushInProgress = false;
}

/** Queue a node id for the next coalesced flush (Pro path). */
function enqueueNodeFetch(nodeId: string): void {
  pendingNodeIds.add(nodeId);
  if (coalesceTimer !== null) return;
  coalesceTimer = setTimeout(flushPendingNodeFetches, REPLAY_COALESCE_WINDOW_MS);
}

/** Fetch all queued nodes, then apply them in one synchronous pass so the burst
 *  renders once. A failed fetch for a single node is skipped, never fatal; a node
 *  tombstoned (deleted) during the fetch is skipped so the delete wins. */
async function flushPendingNodeFetches(): Promise<void> {
  coalesceTimer = null;
  const ids = [...pendingNodeIds];
  pendingNodeIds.clear();
  if (ids.length === 0) return;

  flushInProgress = true;
  tombstonedDuringFlush.clear();
  try {
    // ADR-053: capture the database generation before the reads so a switch
    // mid-flush drops the whole burst rather than writing the previous
    // database's rows into the now-active store. isActiveDatabaseEvent gates on
    // event arrival, before these async fetches dispatch, so it cannot close
    // this in-flight window on its own.
    const epoch = sharedNodeStore.currentEpoch();
    const fetched = await Promise.all(
      ids.map((id) =>
        backendAdapter.getNode(id).catch((error) => {
          log.error('replay-coalesce: failed to fetch node', { nodeId: id, error });
          return null;
        })
      )
    );

    // The active database switched while these reads were in flight — the rows
    // belong to the previous database, so apply none of them.
    if (sharedNodeStore.currentEpoch() !== epoch) return;

    // Synchronous apply loop — no `await` between setNode calls, so Svelte
    // coalesces the resulting reactive updates into a single render. A node
    // deleted while we were fetching is skipped so the delete is not clobbered.
    let applied = 0;
    for (let i = 0; i < fetched.length; i++) {
      const node = fetched[i];
      if (!node || tombstonedDuringFlush.has(ids[i])) continue;
      sharedNodeStore.setNode(
        normalizeNodeData(node),
        { type: 'database', reason: 'domain-event' },
        true
      );
      applied++;
    }
    log.info('replay-coalesce: applied node burst in one pass', {
      requested: ids.length,
      applied
    });
  } finally {
    flushInProgress = false;
    tombstonedDuringFlush.clear();
  }
}

/** Route a node fetch through the Pro coalescer, or apply immediately in the
 *  community build (unchanged behavior). */
function queueOrFetchNode(nodeId: string, eventType: string): void {
  if (proSync.isPro) {
    enqueueNodeFetch(nodeId);
  } else {
    fetchAndUpdateNode(nodeId, eventType);
  }
}

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
 * Events send only node_id. This function fetches the full
 * node data and updates the store.
 */
async function fetchAndUpdateNode(nodeId: string, eventType: string): Promise<void> {
  try {
    // ADR-053: capture the database generation before the read so a switch
    // mid-fetch drops the write rather than writing the previous database's row
    // into the now-active store (isActiveDatabaseEvent gates before this async
    // fetch dispatches, so it does not cover the in-flight window).
    const epoch = sharedNodeStore.currentEpoch();
    const node = await backendAdapter.getNode(nodeId);
    if (sharedNodeStore.currentEpoch() !== epoch) return;
    if (node) {
      const normalizedNode = normalizeNodeData(node);
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

  // Clear any coalescer state (and a stale in-flight timer) from a prior init.
  resetNodeFetchCoalescer();

  try {
    // Listen for node events and update SharedNodeStore
    // Events send only node_id, fetch full data if needed
    // node:created includes nodeType for reactive UI updates
    await listen<NodeEventData>('node:created', (event) => {
      // ADR-053: drop events from a database we are no longer viewing (guards
      // the race where a watch stream open across a switch delivers stale events).
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
      log.debug(`Node created: ${event.payload.id} (type: ${event.payload.nodeType})`);

      // If a collection node is created, refresh collections sidebar
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

      // Fetch full node data since the node might be in the current view.
      // Pro: coalesce a reconnect-replay burst into one render; community:
      // apply immediately (unchanged).
      queueOrFetchNode(event.payload.id, 'node:created');
    });

    await listen<NodeEventData>('node:updated', (event) => {
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
      const nodeId = event.payload.id;
      log.debug(`node:updated received`, { nodeId });
      queueOrFetchNode(nodeId, 'node:updated');
    });

    await listen<NodeEventData>('node:deleted', (event) => {
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
      log.debug(`Node deleted: ${event.payload.id}`);
      // Evict any coalesced re-fetch first so a delete racing an upsert in the
      // same window can't be clobbered by the queued fetch re-adding the node.
      cancelPendingNodeFetch(event.payload.id);
      sharedNodeStore.deleteNode(event.payload.id, { type: 'database', reason: 'domain-event' }, true);

      // We don't know if deleted node was a collection without fetching,
      // but if we have it cached in collectionsData, we should refresh
      // For simplicity, we rely on the UI to handle stale data gracefully
      // A more robust solution would cache node types or include type in delete events
      unregisterSchemaPlugin(event.payload.id);
    });

    // ========================================================================
    // Unified Relationship Events
    // All relationship types (has_child, member_of, mentions, custom) use these events.
    // ========================================================================

    await listen<RelationshipEvent>('relationship:created', (event) => {
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
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
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
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
      if (!isActiveDatabaseEvent(event.payload.databaseId)) return;
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
  return (
    typeof window !== 'undefined' &&
    ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  );
}
