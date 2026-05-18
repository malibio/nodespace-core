import * as grpc from '@grpc/grpc-js';
import { getClient, type NodeServiceClient } from './client.js';
import { type NodeResult, type SearchResult, ToolError } from './types.js';

// Wire-format mirrors of the proto messages in
// packages/daemon/proto/node_service.proto. The proto is the source of
// truth; these interfaces only exist to give the response handlers a typed
// shape after JS-style camelCase conversion by @grpc/proto-loader.
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

interface ProtoNodeResponse {
  nodeId: string;
  nodeType: string;
  parentId: string;
  collectionId: string;
  nodeData?: ProtoNodeData;
}

interface ProtoNodeListResponse {
  nodes: ProtoNodeData[];
  count: number;
  collectionId: string;
}

function fromNodeData(data: ProtoNodeData): NodeResult {
  return {
    id: data.id,
    nodeType: data.nodeType,
    content: data.content,
    parentId: data.parentId === undefined || data.parentId === '' ? undefined : data.parentId
  };
}

function fromNodeResponse(response: ProtoNodeResponse): NodeResult {
  // The proto guarantees node_data is populated for GetNode/CreateNode/UpdateNode.
  // If it's missing, that's a server-side bug — surface it loudly rather than
  // returning a NodeResult with empty content that callers might quietly persist.
  if (response.nodeData === undefined) {
    throw new ToolError('INTERNAL', 'NodeResponse missing node_data');
  }
  return fromNodeData(response.nodeData);
}

type UnaryCall<TRequest, TResponse> = (
  request: TRequest,
  callback: (err: grpc.ServiceError | null, response: TResponse) => void
) => grpc.ClientUnaryCall;

function promisify<TRequest, TResponse>(
  client: NodeServiceClient,
  method: UnaryCall<TRequest, TResponse>,
  request: TRequest
): Promise<TResponse> {
  return new Promise((resolve, reject) => {
    method.call(client, request, (err, response) => {
      if (err !== null && err !== undefined) {
        reject(toToolError(err));
        return;
      }
      resolve(response);
    });
  });
}

function toToolError(err: grpc.ServiceError): ToolError {
  const codeName = grpc.status[err.code] ?? 'UNKNOWN';
  if (err.code === grpc.status.UNAVAILABLE) {
    return new ToolError(
      codeName,
      `nodespaced is not reachable (${err.details ?? err.message})`
    );
  }
  return new ToolError(codeName, err.details ?? err.message);
}

export async function searchSemantic(query: string, limit?: number): Promise<SearchResult> {
  const client = getClient();
  const request = {
    query,
    nodeTypes: [],
    collection: '',
    collectionId: '',
    limit: limit ?? 0,
    offset: 0,
    threshold: 0,
    semantic: true,
    filters: ''
  };
  const response = await promisify<typeof request, ProtoNodeListResponse>(
    client,
    client.searchNodes as UnaryCall<typeof request, ProtoNodeListResponse>,
    request
  );
  return {
    nodes: response.nodes.map(fromNodeData),
    query
  };
}

export async function getNode(nodeId: string): Promise<NodeResult> {
  const client = getClient();
  const response = await promisify<{ nodeId: string }, ProtoNodeResponse>(
    client,
    client.getNode as UnaryCall<{ nodeId: string }, ProtoNodeResponse>,
    { nodeId }
  );
  return fromNodeResponse(response);
}

export async function createNode(
  type: string,
  content: string,
  parentId?: string
): Promise<NodeResult> {
  const client = getClient();
  const request = {
    nodeType: type,
    content,
    parentId: parentId ?? '',
    properties: '',
    collection: '',
    lifecycleStatus: ''
  };
  const response = await promisify<typeof request, ProtoNodeResponse>(
    client,
    client.createNode as UnaryCall<typeof request, ProtoNodeResponse>,
    request
  );
  return fromNodeResponse(response);
}

export async function updateNode(nodeId: string, content: string): Promise<NodeResult> {
  const client = getClient();
  // The proto marks `content` as optional (so callers can express "no change"),
  // but for the agent-tools contract `updateNode` always sets content. The
  // remaining non-optional enum-like fields (nodeType, lifecycleStatus,
  // add/remove collection) are sent as empty strings — proto3 server-side
  // treats empty here as "no change", matching the proto's inline comments.
  const request = {
    nodeId,
    content,
    nodeType: '',
    addToCollection: '',
    removeFromCollection: '',
    lifecycleStatus: ''
  };
  const response = await promisify<typeof request, ProtoNodeResponse>(
    client,
    client.updateNode as UnaryCall<typeof request, ProtoNodeResponse>,
    request
  );
  return fromNodeResponse(response);
}

export async function getChildren(nodeId: string): Promise<NodeResult[]> {
  const client = getClient();
  const response = await promisify<{ nodeId: string }, ProtoNodeListResponse>(
    client,
    client.getChildren as UnaryCall<{ nodeId: string }, ProtoNodeListResponse>,
    { nodeId }
  );
  return response.nodes.map(fromNodeData);
}
