import * as grpc from '@grpc/grpc-js';
import { getClient, type NodeServiceClient } from './client.js';
import { type NodeResult, type SearchResult, ToolError } from './types.js';

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
  if (response.nodeData !== undefined) {
    return fromNodeData(response.nodeData);
  }
  return {
    id: response.nodeId,
    nodeType: response.nodeType,
    content: '',
    parentId: response.parentId === '' ? undefined : response.parentId
  };
}

function callUnary<TRequest, TResponse>(
  client: NodeServiceClient,
  method: string,
  request: TRequest
): Promise<TResponse> {
  return new Promise((resolve, reject) => {
    const fn = (client as unknown as Record<string, Function>)[method];
    if (typeof fn !== 'function') {
      reject(new ToolError('INTERNAL', `gRPC method "${method}" not found on client`));
      return;
    }
    fn.call(client, request, (err: grpc.ServiceError | null, response: TResponse) => {
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
  const response = await callUnary<typeof request, ProtoNodeListResponse>(
    client,
    'searchNodes',
    request
  );
  return {
    nodes: response.nodes.map(fromNodeData),
    query
  };
}

export async function getNode(nodeId: string): Promise<NodeResult> {
  const client = getClient();
  const response = await callUnary<{ nodeId: string }, ProtoNodeResponse>(
    client,
    'getNode',
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
  const response = await callUnary<typeof request, ProtoNodeResponse>(
    client,
    'createNode',
    request
  );
  return fromNodeResponse(response);
}

export async function updateNode(nodeId: string, content: string): Promise<NodeResult> {
  const client = getClient();
  const request = {
    nodeId,
    content,
    nodeType: '',
    addToCollection: '',
    removeFromCollection: '',
    lifecycleStatus: ''
  };
  const response = await callUnary<typeof request, ProtoNodeResponse>(
    client,
    'updateNode',
    request
  );
  return fromNodeResponse(response);
}

export async function getChildren(nodeId: string): Promise<NodeResult[]> {
  const client = getClient();
  const response = await callUnary<{ nodeId: string }, ProtoNodeListResponse>(
    client,
    'getChildren',
    { nodeId }
  );
  return response.nodes.map(fromNodeData);
}
