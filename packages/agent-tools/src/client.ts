import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const PROTO_PATH = path.resolve(__dirname, '../../daemon/proto/node_service.proto');

function resolveAddress(): string {
  const sock = process.env.NODESPACED_SOCKET
    ?? `${process.env.HOME}/.nodespace/daemon.sock`;
  return `unix:${sock}`;
}

type UnaryCallback<TResponse> = (
  err: grpc.ServiceError | null,
  response: TResponse
) => void;

type UnaryMethod<TRequest, TResponse> = (
  request: TRequest,
  callback: UnaryCallback<TResponse>
) => grpc.ClientUnaryCall;

export interface NodeServiceClient extends grpc.Client {
  createNode: UnaryMethod<unknown, unknown>;
  getNode: UnaryMethod<unknown, unknown>;
  updateNode: UnaryMethod<unknown, unknown>;
  getChildren: UnaryMethod<unknown, unknown>;
  searchNodes: UnaryMethod<unknown, unknown>;
}

let cachedClient: NodeServiceClient | null = null;

function loadNodeServiceClient(address: string): NodeServiceClient {
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

  const ClientCtor = proto.nodespace.NodeService;
  return new ClientCtor(address, grpc.credentials.createInsecure()) as unknown as NodeServiceClient;
}

export function getClient(): NodeServiceClient {
  if (cachedClient === null) {
    cachedClient = loadNodeServiceClient(resolveAddress());
  }
  return cachedClient;
}

export function resetClient(): void {
  if (cachedClient !== null) {
    cachedClient.close();
    cachedClient = null;
  }
}
