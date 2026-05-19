/**
 * Bun HTTP dev-proxy for browser mode
 *
 * Forwards REST calls from the Svelte frontend to nodespaced via gRPC,
 * and bridges the WatchNodes gRPC stream to browser SSE clients.
 *
 * Required flow:
 *   Playwright/Browser → dev-proxy (Bun/HTTP :3001) → gRPC → nodespaced → RocksDB
 *
 * No SurrealDB required — only nodespaced must be running.
 */

import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PROTO_PATH = path.resolve(__dirname, '../../daemon/proto/node_service.proto');
const PORT = parseInt(process.env.DEV_PROXY_PORT ?? '3001', 10);

// ============================================================================
// gRPC client setup
// ============================================================================

function resolveSocketAddress(): string {
  const sock =
    process.env.NODESPACED_SOCKET ?? `${process.env.HOME}/.nodespace/daemon.sock`;
  return `unix:${sock}`;
}

const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true
});

const proto = grpc.loadPackageDefinition(packageDefinition) as unknown as {
  nodespace: {
    NodeService: grpc.ServiceClientConstructor;
  };
};

const address = resolveSocketAddress();
const nodeClient = new proto.nodespace.NodeService(
  address,
  grpc.credentials.createInsecure()
);

// Promisify a unary gRPC call
function call<TReq, TRes>(method: Function, request: TReq): Promise<TRes> {
  return new Promise((resolve, reject) => {
    method.call(nodeClient, request, (err: grpc.ServiceError | null, res: TRes) => {
      if (err) reject(err);
      else resolve(res);
    });
  });
}

// ============================================================================
// SSE broadcast
// ============================================================================

interface SseClient {
  id: string;
  controller: ReadableStreamDefaultController;
}

const sseClients = new Set<SseClient>();

function broadcast(event: Record<string, unknown>): void {
  const data = `data: ${JSON.stringify(event)}\n\n`;
  const encoded = new TextEncoder().encode(data);
  for (const client of sseClients) {
    try {
      client.controller.enqueue(encoded);
    } catch {
      sseClients.delete(client);
    }
  }
}

// ============================================================================
// WatchNodes → SSE bridge
// ============================================================================

// Proto NodeEvent oneof field names after camelCase conversion
interface ProtoNodeData {
  id: string;
  nodeType: string;
  content: string;
  parentId?: string;
  properties: string;
  version: string;
  lifecycleStatus: string;
  createdAt: string;
  modifiedAt: string;
  collectionId: string;
}

interface ProtoNodeEvent {
  created?: ProtoNodeData;
  updated?: ProtoNodeData;
  deleted?: { nodeId: string; nodeType: string };
}

function startWatchBridge(): void {
  function connect(): void {
    const stream = (nodeClient as unknown as Record<string, Function>).watchNodes({
      nodeType: '',
      rootId: ''
    }) as grpc.ClientReadableStream<ProtoNodeEvent>;

    stream.on('data', (event: ProtoNodeEvent) => {
      if (event.created) {
        broadcast({
          type: 'nodeCreated',
          nodeId: event.created.id,
          nodeType: event.created.nodeType
        });
      } else if (event.updated) {
        broadcast({
          type: 'nodeUpdated',
          nodeId: event.updated.id
        });
      } else if (event.deleted) {
        broadcast({
          type: 'nodeDeleted',
          nodeId: event.deleted.nodeId,
          nodeType: event.deleted.nodeType
        });
      }
    });

    stream.on('error', (err: Error) => {
      console.error('[dev-proxy] WatchNodes stream error, reconnecting in 2s:', err.message);
      setTimeout(connect, 2000);
    });

    stream.on('end', () => {
      console.log('[dev-proxy] WatchNodes stream ended, reconnecting in 1s');
      setTimeout(connect, 1000);
    });
  }

  connect();
}

// ============================================================================
// Response helpers
// ============================================================================

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json', ...corsHeaders }
  });
}

function error(code: string, message: string, status = 500): Response {
  return json({ code, message, details: message }, status);
}

function grpcError(err: grpc.ServiceError): Response {
  const status = grpcStatusToHttp(err.code);
  return error(grpc.status[err.code] ?? 'UNKNOWN', err.details ?? err.message, status);
}

function grpcStatusToHttp(code: grpc.status): number {
  switch (code) {
    case grpc.status.NOT_FOUND:
      return 404;
    case grpc.status.ALREADY_EXISTS:
      return 409;
    case grpc.status.INVALID_ARGUMENT:
      return 400;
    case grpc.status.PERMISSION_DENIED:
      return 403;
    case grpc.status.UNAUTHENTICATED:
      return 401;
    case grpc.status.RESOURCE_EXHAUSTED:
      return 429;
    case grpc.status.ABORTED:
      return 409;
    default:
      return 500;
  }
}

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, PATCH, DELETE, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization'
};

// ============================================================================
// Node shape helpers
// ============================================================================

function nodeDataToApiNode(n: ProtoNodeData): Record<string, unknown> {
  let properties: Record<string, unknown> = {};
  try {
    properties = n.properties ? (JSON.parse(n.properties) as Record<string, unknown>) : {};
  } catch {
    properties = {};
  }
  return {
    id: n.id,
    nodeType: n.nodeType,
    content: n.content,
    parentId: n.parentId && n.parentId !== '' ? n.parentId : null,
    properties,
    version: parseInt(n.version, 10),
    lifecycleStatus: n.lifecycleStatus,
    createdAt: n.createdAt,
    modifiedAt: n.modifiedAt,
    collectionId: n.collectionId && n.collectionId !== '' ? n.collectionId : null
  };
}

// ============================================================================
// Request router
// ============================================================================

async function handleRequest(req: Request): Promise<Response> {
  const url = new URL(req.url);
  const pathname = url.pathname;
  const method = req.method;

  if (method === 'OPTIONS') {
    return new Response(null, { status: 204, headers: corsHeaders });
  }

  // GET /health
  if (method === 'GET' && pathname === '/health') {
    return new Response('ok', { status: 200, headers: corsHeaders });
  }

  // GET /api/events  (SSE)
  if (method === 'GET' && pathname === '/api/events') {
    const clientId = url.searchParams.get('clientId') ?? crypto.randomUUID();
    let clientRef: SseClient;

    const stream = new ReadableStream({
      start(controller) {
        clientRef = { id: clientId, controller };
        sseClients.add(clientRef);
        // Send initial keep-alive comment
        controller.enqueue(new TextEncoder().encode(': connected\n\n'));
      },
      cancel() {
        sseClients.delete(clientRef);
      }
    });

    return new Response(stream, {
      status: 200,
      headers: {
        ...corsHeaders,
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive'
      }
    });
  }

  // POST /api/nodes
  if (method === 'POST' && pathname === '/api/nodes') {
    try {
      const body = await req.json() as Record<string, unknown>;
      const request = {
        nodeType: body.nodeType ?? '',
        content: body.content ?? '',
        parentId: body.parentId ?? '',
        insertAfterNodeId: body.insertAfterNodeId ?? '',
        properties: body.properties ? JSON.stringify(body.properties) : '',
        collection: '',
        lifecycleStatus: '',
        id: body.id ?? ''
      };
      const res = await call<typeof request, { nodeId: string }>(
        (nodeClient as unknown as Record<string, Function>).createNode,
        request
      );
      return json(res.nodeId);
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id
  const getNodeMatch = pathname.match(/^\/api\/nodes\/([^/]+)$/);
  if (method === 'GET' && getNodeMatch) {
    const nodeId = decodeURIComponent(getNodeMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { nodeData?: ProtoNodeData }>(
        (nodeClient as unknown as Record<string, Function>).getNode,
        { nodeId }
      );
      if (!res.nodeData) return json(null);
      return json(nodeDataToApiNode(res.nodeData));
    } catch (err) {
      const grpcErr = err as grpc.ServiceError;
      if (grpcErr.code === grpc.status.NOT_FOUND) return json(null);
      return grpcError(grpcErr);
    }
  }

  // PATCH /api/nodes/:id
  if (method === 'PATCH' && getNodeMatch) {
    const nodeId = decodeURIComponent(getNodeMatch[1]);
    try {
      const body = await req.json() as Record<string, unknown>;
      const request = {
        nodeId,
        version: body.version ?? null,
        nodeType: body.nodeType ?? '',
        content: body.content !== undefined ? String(body.content) : null,
        properties: body.properties !== undefined ? JSON.stringify(body.properties) : null,
        addToCollection: body.addToCollection ?? '',
        removeFromCollection: body.removeFromCollection ?? '',
        lifecycleStatus: body.lifecycleStatus ?? ''
      };
      const res = await call<typeof request, { nodeData?: ProtoNodeData }>(
        (nodeClient as unknown as Record<string, Function>).updateNode,
        request
      );
      if (!res.nodeData) return error('NO_DATA', 'UpdateNode returned no data');
      return json(nodeDataToApiNode(res.nodeData));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // DELETE /api/nodes/:id
  if (method === 'DELETE' && getNodeMatch) {
    const nodeId = decodeURIComponent(getNodeMatch[1]);
    try {
      let body: Record<string, unknown> = {};
      try { body = await req.json() as Record<string, unknown>; } catch { /* no body */ }
      const request = {
        nodeId,
        version: body.version ?? null
      };
      await call<typeof request, unknown>(
        (nodeClient as unknown as Record<string, Function>).deleteNode,
        request
      );
      return new Response(null, { status: 204, headers: corsHeaders });
    } catch (err) {
      const grpcErr = err as grpc.ServiceError;
      if (grpcErr.code === grpc.status.NOT_FOUND) {
        return new Response(null, { status: 204, headers: corsHeaders });
      }
      return grpcError(grpcErr);
    }
  }

  // PATCH /api/tasks/:id
  const taskMatch = pathname.match(/^\/api\/tasks\/([^/]+)$/);
  if (method === 'PATCH' && taskMatch) {
    const nodeId = decodeURIComponent(taskMatch[1]);
    try {
      const body = await req.json() as Record<string, unknown>;
      const request = {
        nodeId,
        version: body.version ?? 0,
        status: body.status ?? null,
        priority: body.priority !== undefined
          ? { clear: body.priority === null, value: body.priority ?? '' }
          : null,
        dueDate: body.dueDate !== undefined
          ? { clear: body.dueDate === null, value: body.dueDate ?? '' }
          : null,
        assignee: body.assignee !== undefined
          ? { clear: body.assignee === null, value: body.assignee ?? '' }
          : null,
        startedAt: body.startedAt !== undefined
          ? { clear: body.startedAt === null, value: body.startedAt ?? '' }
          : null,
        completedAt: body.completedAt !== undefined
          ? { clear: body.completedAt === null, value: body.completedAt ?? '' }
          : null,
        content: body.content ?? null,
        properties: body.properties !== undefined ? JSON.stringify(body.properties) : null
      };
      const res = await call<typeof request, { nodeData?: ProtoNodeData }>(
        (nodeClient as unknown as Record<string, Function>).updateTaskNode,
        request
      );
      if (!res.nodeData) return error('NO_DATA', 'UpdateTaskNode returned no data');
      return json(nodeDataToApiNode(res.nodeData));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // POST /api/nodes/:id/parent  (move node)
  const parentMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/parent$/);
  if (method === 'POST' && parentMatch) {
    const nodeId = decodeURIComponent(parentMatch[1]);
    try {
      const body = await req.json() as Record<string, unknown>;
      const request = {
        nodeId,
        version: body.version ?? 0,
        newParentId: body.parentId ?? '',
        insertAfterNodeId: body.insertAfterNodeId ?? ''
      };
      const res = await call<typeof request, { nodeData?: ProtoNodeData }>(
        (nodeClient as unknown as Record<string, Function>).moveNode,
        request
      );
      if (!res.nodeData) return error('NO_DATA', 'MoveNode returned no data');
      return json(nodeDataToApiNode(res.nodeData));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id/children
  const childrenMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/children$/);
  if (method === 'GET' && childrenMatch) {
    const nodeId = decodeURIComponent(childrenMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { nodes: ProtoNodeData[] }>(
        (nodeClient as unknown as Record<string, Function>).getChildren,
        { nodeId }
      );
      return json((res.nodes ?? []).map(nodeDataToApiNode));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id/children-tree
  const childrenTreeMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/children-tree$/);
  if (method === 'GET' && childrenTreeMatch) {
    const nodeId = decodeURIComponent(childrenTreeMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { treeJson: string }>(
        (nodeClient as unknown as Record<string, Function>).getChildrenTree,
        { nodeId }
      );
      const tree = res.treeJson ? JSON.parse(res.treeJson) : {};
      return json(tree);
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // POST /api/query
  if (method === 'POST' && pathname === '/api/query') {
    try {
      const body = await req.json() as Record<string, unknown>;
      const request = {
        id: body.id ?? null,
        mentionedBy: body.mentionedBy ?? null,
        contentContains: body.contentContains ?? null,
        titleContains: body.titleContains ?? null,
        nodeType: body.nodeType ?? null,
        limit: body.limit ?? 0,
        offset: body.offset ?? 0
      };
      const res = await call<typeof request, { nodes: ProtoNodeData[] }>(
        (nodeClient as unknown as Record<string, Function>).queryNodesSimple,
        request
      );
      return json((res.nodes ?? []).map(nodeDataToApiNode));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // POST /api/mentions
  if (method === 'POST' && pathname === '/api/mentions') {
    try {
      const body = await req.json() as Record<string, unknown>;
      await call(
        (nodeClient as unknown as Record<string, Function>).createMention,
        { mentioningNodeId: body.sourceId, mentionedNodeId: body.targetId }
      );
      return new Response(null, { status: 204, headers: corsHeaders });
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // DELETE /api/mentions
  if (method === 'DELETE' && pathname === '/api/mentions') {
    try {
      const body = await req.json() as Record<string, unknown>;
      await call(
        (nodeClient as unknown as Record<string, Function>).deleteMention,
        { mentioningNodeId: body.sourceId, mentionedNodeId: body.targetId }
      );
      return new Response(null, { status: 204, headers: corsHeaders });
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // POST /api/mentions/autocomplete
  if (method === 'POST' && pathname === '/api/mentions/autocomplete') {
    try {
      const body = await req.json() as Record<string, unknown>;
      const res = await call<{ query: string; limit: number }, { nodes: ProtoNodeData[] }>(
        (nodeClient as unknown as Record<string, Function>).mentionAutocomplete,
        { query: String(body.query ?? ''), limit: Number(body.limit ?? 0) }
      );
      return json((res.nodes ?? []).map(nodeDataToApiNode));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id/mentions/outgoing
  const outgoingMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/mentions\/outgoing$/);
  if (method === 'GET' && outgoingMatch) {
    const nodeId = decodeURIComponent(outgoingMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { nodeIds: string[] }>(
        (nodeClient as unknown as Record<string, Function>).getOutgoingMentions,
        { nodeId }
      );
      return json(res.nodeIds ?? []);
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id/mentions/incoming
  const incomingMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/mentions\/incoming$/);
  if (method === 'GET' && incomingMatch) {
    const nodeId = decodeURIComponent(incomingMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { nodeIds: string[] }>(
        (nodeClient as unknown as Record<string, Function>).getIncomingMentions,
        { nodeId }
      );
      return json(res.nodeIds ?? []);
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/nodes/:id/mentions/roots
  const mentionRootsMatch = pathname.match(/^\/api\/nodes\/([^/]+)\/mentions\/roots$/);
  if (method === 'GET' && mentionRootsMatch) {
    const nodeId = decodeURIComponent(mentionRootsMatch[1]);
    try {
      const res = await call<{ nodeId: string }, { references: Array<{ id: string; title?: string; nodeType: string }> }>(
        (nodeClient as unknown as Record<string, Function>).getMentioningRoots,
        { nodeId }
      );
      // Backend adapter expects string[] (node IDs) for getMentioningContainers
      return json((res.references ?? []).map((r) => r.id));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/schemas
  if (method === 'GET' && pathname === '/api/schemas') {
    try {
      const res = await call<Record<string, never>, { nodes: ProtoNodeData[] }>(
        (nodeClient as unknown as Record<string, Function>).getAllSchemas,
        {}
      );
      return json((res.nodes ?? []).map(nodeDataToSchemaNode));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/schemas/:id
  const schemaMatch = pathname.match(/^\/api\/schemas\/([^/]+)$/);
  if (method === 'GET' && schemaMatch) {
    const schemaId = decodeURIComponent(schemaMatch[1]);
    try {
      const res = await call<{ schemaId: string }, { nodeData?: ProtoNodeData }>(
        (nodeClient as unknown as Record<string, Function>).getSchemaDefinition,
        { schemaId }
      );
      if (!res.nodeData) return error('SCHEMA_NOT_FOUND', `Schema '${schemaId}' not found`, 404);
      return json(nodeDataToSchemaNode(res.nodeData));
    } catch (err) {
      const grpcErr = err as grpc.ServiceError;
      if (grpcErr.code === grpc.status.NOT_FOUND) {
        return error('SCHEMA_NOT_FOUND', `Schema '${schemaId}' not found`, 404);
      }
      return grpcError(grpcErr);
    }
  }

  // GET /api/collections
  if (method === 'GET' && pathname === '/api/collections') {
    try {
      const res = await call<Record<string, never>, { collections: Array<{ node: ProtoNodeData; memberCount: number; parentCollectionIds: string[] }> }>(
        (nodeClient as unknown as Record<string, Function>).getAllCollections,
        {}
      );
      const result = (res.collections ?? []).map((c) => ({
        ...nodeDataToApiNode(c.node),
        memberCount: c.memberCount,
        parentCollectionIds: c.parentCollectionIds ?? []
      }));
      return json(result);
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  // GET /api/collections/:id/members
  const collectionMembersMatch = pathname.match(/^\/api\/collections\/([^/]+)\/members$/);
  if (method === 'GET' && collectionMembersMatch) {
    const collectionId = decodeURIComponent(collectionMembersMatch[1]);
    try {
      const res = await call<{ collectionId: string }, { nodes: ProtoNodeData[] }>(
        (nodeClient as unknown as Record<string, Function>).getCollectionMembers,
        { collectionId }
      );
      return json((res.nodes ?? []).map(nodeDataToApiNode));
    } catch (err) {
      return grpcError(err as grpc.ServiceError);
    }
  }

  return new Response('Not found', { status: 404, headers: corsHeaders });
}

// ============================================================================
// Schema node helpers
// ============================================================================

function nodeDataToSchemaNode(n: ProtoNodeData): Record<string, unknown> {
  const base = nodeDataToApiNode(n);
  const props = base.properties as Record<string, unknown>;
  return {
    ...base,
    isCore: props.isCore ?? false,
    schemaVersion: props.schemaVersion ?? 1,
    description: props.description ?? '',
    fields: props.fields ?? []
  };
}

// ============================================================================
// Server startup
// ============================================================================

startWatchBridge();

const server = Bun.serve({
  port: PORT,
  fetch: handleRequest,
  error(err: Error) {
    console.error('[dev-proxy] Unhandled error:', err);
    return new Response('Internal Server Error', { status: 500 });
  }
});

console.log(`[dev-proxy] Listening on http://localhost:${PORT}`);
console.log(`[dev-proxy] Connecting to nodespaced at ${address}`);
console.log(`[dev-proxy] SSE endpoint: http://localhost:${PORT}/api/events`);
