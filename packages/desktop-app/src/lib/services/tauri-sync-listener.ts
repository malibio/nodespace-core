/**
 * Tauri LIVE SELECT Event Listener
 *
 * Listens for real-time synchronization events emitted from the Rust backend
 * via SurrealDB LIVE SELECT subscriptions. Converts Tauri events to frontend
 * EventBus events for consumption by UI components and services.
 *
 * This bridge connects the Tauri backend (LiveQueryService) with the frontend
 * event system, enabling real-time multi-client synchronization.
 */

import { listen } from '@tauri-apps/api/event';
import { eventBus } from './event-bus';
import type {
  RealtimeSyncNodeChangedEvent,
  RealtimeSyncEdgeChangedEvent,
  RealtimeSyncErrorEvent,
  RealtimeSyncStatusEvent
} from './event-types';

/**
 * Payload structure for Tauri sync events emitted by LiveQueryService
 */
interface TauriSyncEventPayload extends Record<string, unknown> {
  event_type: 'node-changed' | 'edge-changed' | 'error' | 'status';
  payload: Record<string, unknown>;
}

/**
 * Initialize Tauri LIVE SELECT event listeners
 *
 * Sets up listeners for all real-time synchronization events and bridges them
 * to the frontend EventBus. Should be called once during app initialization.
 *
 * @returns Promise resolving when all listeners are registered
 */
export async function initializeTauriSyncListeners(): Promise<void> {
  if (!isRunningInTauri()) {
    console.debug('Not running in Tauri environment, skipping sync listener initialization');
    return;
  }

  console.info('Initializing Tauri LIVE SELECT sync listeners');

  try {
    // Listen for node changes
    await listen<TauriSyncEventPayload>('sync:node-changed', (event) => {
      handleNodeChangedEvent(event.payload);
    });

    // Listen for edge changes
    await listen<TauriSyncEventPayload>('sync:edge-changed', (event) => {
      handleEdgeChangedEvent(event.payload);
    });

    // Listen for synchronization errors
    await listen<TauriSyncEventPayload>('sync:error', (event) => {
      handleSyncErrorEvent(event.payload);
    });

    // Listen for synchronization status changes
    await listen<TauriSyncEventPayload>('sync:status', (event) => {
      handleSyncStatusEvent(event.payload);
    });

    console.info('✅ Tauri LIVE SELECT sync listeners initialized successfully');
  } catch (error) {
    console.error('❌ Failed to initialize Tauri sync listeners', error);
    throw new Error(`Failed to initialize LIVE SELECT listeners: ${error}`);
  }
}

/**
 * Handle node change events from Tauri backend
 */
function handleNodeChangedEvent(payload: Record<string, unknown>): void {
  try {
    const nodeId = String(payload.nodeId);
    const changeType = String(payload.changeType) as 'create' | 'update' | 'delete';

    const event: Omit<RealtimeSyncNodeChangedEvent, 'timestamp'> = {
      type: 'sync:node-changed',
      namespace: 'sync',
      source: 'tauri-live-query',
      nodeId,
      changeType,
      nodeData: payload.nodeData ? (payload.nodeData as Record<string, unknown>) : undefined,
      previousData: payload.previousData ? (payload.previousData as Record<string, unknown>) : undefined,
      metadata: {
        receivedAt: new Date().toISOString()
      }
    };

    eventBus.emit(event);
    console.debug(`🔄 Node ${changeType}: ${nodeId}`);
  } catch (error) {
    console.error('Error processing node-changed event', error);
  }
}

/**
 * Handle edge change events from Tauri backend
 */
function handleEdgeChangedEvent(payload: Record<string, unknown>): void {
  try {
    const edgeId = String(payload.edgeId);
    const sourceNodeId = String(payload.sourceNodeId);
    const targetNodeId = String(payload.targetNodeId);
    const changeType = String(payload.changeType) as 'create' | 'update' | 'delete';

    const event: Omit<RealtimeSyncEdgeChangedEvent, 'timestamp'> = {
      type: 'sync:edge-changed',
      namespace: 'sync',
      source: 'tauri-live-query',
      edgeId,
      sourceNodeId,
      targetNodeId,
      changeType,
      edgeData: payload.edgeData ? (payload.edgeData as Record<string, unknown>) : undefined,
      previousData: payload.previousData ? (payload.previousData as Record<string, unknown>) : undefined,
      metadata: {
        receivedAt: new Date().toISOString()
      }
    };

    eventBus.emit(event);
    console.debug(`🔗 Edge ${changeType}: ${sourceNodeId} → ${targetNodeId}`);
  } catch (error) {
    console.error('Error processing edge-changed event', error);
  }
}

/**
 * Handle synchronization error events
 */
function handleSyncErrorEvent(payload: Record<string, unknown>): void {
  try {
    const message = String(payload.message);
    const errorType = String(payload.errorType) as
      | 'subscription-failed'
      | 'stream-interrupted'
      | 'parse-error'
      | 'unknown';
    const retryable = Boolean(payload.retryable);

    const event: Omit<RealtimeSyncErrorEvent, 'timestamp'> = {
      type: 'error:sync-failed',
      namespace: 'error',
      source: 'tauri-live-query',
      message,
      errorType,
      retryable,
      metadata: {
        timestamp: new Date().toISOString()
      }
    };

    eventBus.emit(event);
    console.error(`❌ Sync error (${errorType}): ${message}`);
  } catch (error) {
    console.error('Error processing sync-error event', error);
  }
}

/**
 * Handle synchronization status change events
 */
function handleSyncStatusEvent(payload: Record<string, unknown>): void {
  try {
    const status = String(payload.status) as 'connected' | 'disconnected' | 'reconnecting';
    const reason = payload.reason ? String(payload.reason) : undefined;

    const event: Omit<RealtimeSyncStatusEvent, 'timestamp'> = {
      type: 'sync:status-changed',
      namespace: 'sync',
      source: 'tauri-live-query',
      status,
      reason,
      metadata: {
        timestamp: new Date().toISOString()
      }
    };

    eventBus.emit(event);
    console.info(`🔌 Sync status: ${status}${reason ? ` (${reason})` : ''}`);
  } catch (error) {
    console.error('Error processing sync-status event', error);
  }
}

/**
 * Check if running in Tauri environment
 */
function isRunningInTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window;
}
