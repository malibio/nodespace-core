/**
 * SSE Event Types for Browser Mode Real-Time Sync
 *
 * These types define the structure of Server-Sent Events received from
 * the dev-proxy's `/api/events` endpoint. They mirror the Rust SseEvent
 * enum in dev-proxy.rs.
 */

import type { Node } from '$lib/types';

/**
 * Base interface for all SSE events
 */
export interface SseEventBase {
  type: string;
}

/**
 * Event sent when a new node is created
 */
export interface NodeCreatedEvent extends SseEventBase {
  type: 'nodeCreated';
  nodeId: string;
  nodeData: Node;
}

/**
 * Event sent when an existing node is updated
 */
export interface NodeUpdatedEvent extends SseEventBase {
  type: 'nodeUpdated';
  nodeId: string;
  nodeData: Node;
}

/**
 * Event sent when a node is deleted
 */
export interface NodeDeletedEvent extends SseEventBase {
  type: 'nodeDeleted';
  nodeId: string;
}

/**
 * Event sent when a parent-child edge is created
 */
export interface EdgeCreatedEvent extends SseEventBase {
  type: 'edgeCreated';
  parentId: string;
  childId: string;
}

/**
 * Event sent when a parent-child edge is deleted
 */
export interface EdgeDeletedEvent extends SseEventBase {
  type: 'edgeDeleted';
  parentId: string;
  childId: string;
}

/**
 * Union type of all SSE events
 */
export type SseEvent =
  | NodeCreatedEvent
  | NodeUpdatedEvent
  | NodeDeletedEvent
  | EdgeCreatedEvent
  | EdgeDeletedEvent;

/**
 * Type guard for NodeCreatedEvent
 */
export function isNodeCreatedEvent(event: SseEvent): event is NodeCreatedEvent {
  return event.type === 'nodeCreated';
}

/**
 * Type guard for NodeUpdatedEvent
 */
export function isNodeUpdatedEvent(event: SseEvent): event is NodeUpdatedEvent {
  return event.type === 'nodeUpdated';
}

/**
 * Type guard for NodeDeletedEvent
 */
export function isNodeDeletedEvent(event: SseEvent): event is NodeDeletedEvent {
  return event.type === 'nodeDeleted';
}

/**
 * Type guard for EdgeCreatedEvent
 */
export function isEdgeCreatedEvent(event: SseEvent): event is EdgeCreatedEvent {
  return event.type === 'edgeCreated';
}

/**
 * Type guard for EdgeDeletedEvent
 */
export function isEdgeDeletedEvent(event: SseEvent): event is EdgeDeletedEvent {
  return event.type === 'edgeDeleted';
}
