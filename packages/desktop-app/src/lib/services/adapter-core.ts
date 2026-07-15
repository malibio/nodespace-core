/**
 * Transport-agnostic backend adapter core.
 *
 * The daemon's NodeService contract (packages/proto/proto/node_service.proto) is
 * consumed by three independent client paths — TauriAdapter (IPC), HttpAdapter
 * (fetch), and the dev-proxy (REST→gRPC translation) — plus a fourth copy that
 * used to live in the e2e harness. This module is the single place that encodes
 * *what the wire contract is*: which fields are required vs. optional-omit vs.
 * tri-state-clearable, and what each logical operation is named per transport.
 * TauriAdapter/HttpAdapter/dev-proxy call these builders instead of re-deriving
 * the encoding by hand, so a field/shape change can no longer drift between them
 * silently — it becomes one function to update, used everywhere.
 */

// This file is imported directly by packages/dev-tools/src/dev-proxy.ts via a
// relative path (a separate workspace package with no SvelteKit/$lib alias
// resolution). Keep every `$lib`-aliased import type-only — TypeScript erases
// type-only imports before Bun ever needs to resolve the specifier, but a
// value-level `$lib` import here would break dev-proxy at runtime.
import type { Node, NodeWithChildren, TaskNode, TaskNodeUpdate } from '$lib/types';
import type { SchemaNode } from '$lib/types/schema-node';

// ============================================================================
// Shared types (public BackendAdapter surface)
// ============================================================================

/** Explicit insertion position for new or moved nodes. */
export type InsertPosition =
  | { type: 'beginning' }
  | { type: 'end' }
  | { type: 'after'; siblingId: string };

/** Factory helpers for building InsertPosition values. */
export const insertPosition = {
  beginning: (): InsertPosition => ({ type: 'beginning' }),
  end: (): InsertPosition => ({ type: 'end' }),
  after: (siblingId: string): InsertPosition => ({ type: 'after', siblingId }),
} as const;

export interface CreateNodeInput {
  id: string;
  nodeType: string;
  content: string;
  properties?: Record<string, unknown>;
  mentions?: string[];
  parentId?: string | null;
  /** Where to insert among siblings. Omit for End (default). */
  insertPosition?: InsertPosition | null;
}

export interface UpdateNodeInput {
  content?: string;
  nodeType?: string;
  properties?: Record<string, unknown>;
  mentions?: string[];
}

export interface DeleteResult {
  existed: boolean;
  deletedCount: number;
}

export interface EdgeRecord {
  id: string;
  in: string;
  out: string;
  order: number;
}

export interface NodeQuery {
  id?: string;
  mentionedBy?: string;
  contentContains?: string;
  nodeType?: string;
  limit?: number;
}

export interface CreateContainerInput {
  content: string;
  nodeType: string;
  properties?: Record<string, unknown>;
  mentionedBy?: string;
}

export interface BackendAdapter {
  // Node CRUD
  createNode(input: CreateNodeInput | Node): Promise<string>;
  getNode(id: string): Promise<Node | null>;
  updateNode(id: string, version: number, update: UpdateNodeInput): Promise<Node>;
  updateTaskNode(id: string, version: number, update: TaskNodeUpdate): Promise<TaskNode>;
  deleteNode(id: string, version: number): Promise<DeleteResult>;

  // Hierarchy
  getChildren(parentId: string): Promise<Node[]>;
  getDescendants(rootNodeId: string): Promise<Node[]>;
  getChildrenTree(parentId: string): Promise<NodeWithChildren | null>;
  moveNode(nodeId: string, version: number, newParentId: string | null, insertPosition: InsertPosition | null): Promise<Node>;
  moveChildrenToParent(newParentId: string, children: Array<{ id: string; version: number }>): Promise<Node[]>;

  // Mentions
  createMention(mentioningNodeId: string, mentionedNodeId: string): Promise<void>;
  deleteMention(mentioningNodeId: string, mentionedNodeId: string): Promise<void>;
  getOutgoingMentions(nodeId: string): Promise<string[]>;
  getIncomingMentions(nodeId: string): Promise<string[]>;
  getMentioningContainers(nodeId: string): Promise<string[]>;

  // Queries
  queryNodes(query: NodeQuery): Promise<Node[]>;
  mentionAutocomplete(query: string, limit?: number): Promise<Node[]>;

  // Composite operations
  createContainerNode(input: CreateContainerInput): Promise<string>;

  // Schema operations (read-only - mutation commands removed)
  // Returns SchemaNode with typed top-level fields (isCore, schemaVersion, description, fields)
  getAllSchemas(): Promise<SchemaNode[]>;
  getSchema(schemaId: string): Promise<SchemaNode>;
}

// ============================================================================
// CreateNode — shared request shaping
// ============================================================================

/**
 * Normalized CreateNode wire fields, matching CreateNodeRequest in
 * node_service.proto: parentId/insertPosition are omittable (proto `optional`),
 * not merely nullable — a `null` sent over IPC/HTTP is a real "no parent" value
 * on the Rust side, not "field absent."
 */
export interface CreateNodeFields {
  id: string;
  nodeType: string;
  content: string;
  properties: Record<string, unknown>;
  mentions: string[];
  parentId: string | null;
  insertPosition: InsertPosition | null;
}

export function buildCreateNodeFields(input: CreateNodeInput | Node): CreateNodeFields {
  return {
    id: input.id,
    nodeType: input.nodeType,
    content: input.content,
    properties: input.properties ?? {},
    mentions: (input as CreateNodeInput).mentions ?? [],
    parentId: (input as CreateNodeInput).parentId ?? null,
    insertPosition: (input as CreateNodeInput).insertPosition ?? null,
  };
}

// ============================================================================
// UpdateTaskNode — tri-state clearable-field encoding
// ============================================================================

/**
 * Wire encoding for a single "optional, clearable" field, mirroring
 * OptionalStringClear / OptionalTimestampClear in node_service.proto:
 *   - field absent from the patch   → no change (outer None)
 *   - field present, value `null`   → clear the value (Some(None))
 *   - field present, value `T`      → set the value (Some(Some(T)))
 */
export type ClearableField<T> = { clear: true } | { clear: false; value: T } | undefined;

export interface TaskNodeUpdatePatch {
  status?: string;
  priority: ClearableField<string>;
  dueDate: ClearableField<string>;
  assignee: ClearableField<string>;
  startedAt: ClearableField<string>;
  completedAt: ClearableField<string>;
  content?: string;
}

function clearable(value: string | null | undefined): ClearableField<string> {
  if (value === undefined) return undefined;
  if (value === null) return { clear: true };
  return { clear: false, value };
}

/**
 * The single authoritative mapping from the frontend's `TaskNodeUpdate` shape
 * (plain `null` = clear, `undefined`/absent = no change) to the tri-state wire
 * patch the daemon expects. Both the dev-proxy's gRPC request and any future
 * Tauri-side equivalent must derive from this function, not re-implement it.
 */
export function buildTaskNodeUpdatePatch(update: TaskNodeUpdate): TaskNodeUpdatePatch {
  return {
    status: update.status,
    priority: clearable(update.priority),
    dueDate: clearable(update.dueDate),
    assignee: clearable(update.assignee),
    startedAt: clearable(update.startedAt),
    completedAt: clearable(update.completedAt),
    content: update.content,
  };
}

// ============================================================================
// MoveNode / CreateNode — InsertPosition wire encoding
// ============================================================================

/** Wire shape for InsertPosition, matching the `oneof position` in node_service.proto. */
export type InsertPositionWire =
  | { beginning: true }
  | { end: true }
  | { after: string }
  | Record<string, never>;

export function encodeInsertPosition(pos: InsertPosition | null | undefined): InsertPositionWire {
  if (!pos) return {};
  switch (pos.type) {
    case 'beginning':
      return { beginning: true };
    case 'end':
      return { end: true };
    case 'after':
      return { after: pos.siblingId };
  }
}

// ============================================================================
// Response normalization
// ============================================================================

/** Backend returns {} for a non-existent parent's children-tree; normalize to null. */
export function normalizeChildrenTree(
  result: NodeWithChildren | Record<string, never> | null | undefined,
): NodeWithChildren | null {
  if (!result || Object.keys(result).length === 0) return null;
  return result as NodeWithChildren;
}

// ============================================================================
// Route table — single source of truth for the HTTP surface
// ============================================================================

/**
 * HTTP route templates used by both HttpAdapter (to build fetch URLs) and the
 * dev-proxy (to route incoming requests to the matching gRPC call). Keeping
 * these in one place means a path change is a single edit instead of two
 * hand-synced pattern literals.
 */
export const HTTP_ROUTES = {
  createNode: () => '/api/nodes',
  getNode: (id: string) => `/api/nodes/${encodeURIComponent(id)}`,
  updateNode: (id: string) => `/api/nodes/${encodeURIComponent(id)}`,
  deleteNode: (id: string) => `/api/nodes/${encodeURIComponent(id)}`,
  updateTaskNode: (id: string) => `/api/tasks/${encodeURIComponent(id)}`,
  moveNode: (id: string) => `/api/nodes/${encodeURIComponent(id)}/parent`,
  moveChildrenToParent: (parentId: string) => `/api/nodes/${encodeURIComponent(parentId)}/move-children`,
  getChildren: (parentId: string) => `/api/nodes/${encodeURIComponent(parentId)}/children`,
  getChildrenTree: (parentId: string) => `/api/nodes/${encodeURIComponent(parentId)}/children-tree`,
  createMention: () => '/api/mentions',
  deleteMention: () => '/api/mentions',
  getOutgoingMentions: (nodeId: string) => `/api/nodes/${encodeURIComponent(nodeId)}/mentions/outgoing`,
  getIncomingMentions: (nodeId: string) => `/api/nodes/${encodeURIComponent(nodeId)}/mentions/incoming`,
  getMentioningContainers: (nodeId: string) => `/api/nodes/${encodeURIComponent(nodeId)}/mentions/roots`,
  queryNodes: () => '/api/query',
  mentionAutocomplete: () => '/api/mentions/autocomplete',
  getAllSchemas: () => '/api/schemas',
  getSchema: (schemaId: string) => `/api/schemas/${encodeURIComponent(schemaId)}`,
} as const;

/**
 * Path-matching counterparts as RegExp, for the dev-proxy's router. Templates
 * with a dynamic segment expose the same key as HTTP_ROUTES so a new route
 * only needs one addition here plus one in the handler dispatch, not a
 * hand-copied regex that can drift from the URL the adapter actually builds.
 */
export const HTTP_ROUTE_PATTERNS = {
  getNode: /^\/api\/nodes\/([^/]+)$/,
  updateNode: /^\/api\/nodes\/([^/]+)$/,
  deleteNode: /^\/api\/nodes\/([^/]+)$/,
  updateTaskNode: /^\/api\/tasks\/([^/]+)$/,
  moveNode: /^\/api\/nodes\/([^/]+)\/parent$/,
  moveChildrenToParent: /^\/api\/nodes\/([^/]+)\/move-children$/,
  getChildren: /^\/api\/nodes\/([^/]+)\/children$/,
  getChildrenTree: /^\/api\/nodes\/([^/]+)\/children-tree$/,
  getOutgoingMentions: /^\/api\/nodes\/([^/]+)\/mentions\/outgoing$/,
  getIncomingMentions: /^\/api\/nodes\/([^/]+)\/mentions\/incoming$/,
  getMentioningContainers: /^\/api\/nodes\/([^/]+)\/mentions\/roots$/,
  getSchema: /^\/api\/schemas\/([^/]+)$/,
} as const;
