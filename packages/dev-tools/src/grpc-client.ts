/**
 * gRPC-js client factory for the Bun dev-proxy (browser mode).
 *
 * This module owns the single seam where the dev-proxy talks to nodespaced
 * over a Unix-domain socket. It exists as its own module (rather than inline
 * in dev-proxy.ts) so the not-ready -> recovered behavior below can be
 * regression-tested against a stub gRPC server without spawning the whole
 * proxy or the real daemon.
 *
 * ---------------------------------------------------------------------------
 * Why this module reshapes gRPC-js's reconnect behavior
 * ---------------------------------------------------------------------------
 * The dev-proxy constructs its gRPC-js client ONCE, at startup, against the
 * daemon socket. When the proxy starts BEFORE the daemon binds that socket
 * (which the e2e readiness harness does deliberately), the gRPC-js channel's
 * subchannel enters TRANSIENT_FAILURE and starts its OWN reconnect backoff —
 * a timer completely independent of the daemon socket actually becoming
 * reachable moments later. Two properties of stock gRPC-js make that a real
 * bug for the dev path:
 *
 *   1. The reconnect backoff GROWS unboundedly while the peer is down
 *      (grpc-js defaults: 1s initial, x1.6 each attempt, up to 120s). By the
 *      time the daemon binds, the channel may be sitting out a multi-second
 *      backoff before it even retries the transport — so a raw socket probe
 *      can report the daemon up while the channel is still mid-backoff.
 *      `getConnectivityState(true)` / `client.waitForReady()` / an LB
 *      `resetBackoff()` do NOT cut this short: in grpc-js the backoff lives on
 *      the subchannel, the pick-first balancer's `resetBackoff` is a no-op,
 *      and `startConnecting()` on a TRANSIENT_FAILURE subchannel only marks
 *      "reconnect once the current timer ends" — it does not restart the
 *      timer. The only robust lever the public API exposes is bounding the
 *      backoff via channel options, which we do here: a fixed ~100ms retry
 *      interval means the channel re-probes the transport ~10x/second and
 *      recovers within ~100ms of the socket appearing, regardless of how long
 *      the daemon was down.
 *
 *   2. A unary call issued while the channel is NOT READY fails IMMEDIATELY
 *      with UNAVAILABLE by default (gRPC "wait for ready" is off by default),
 *      so callers that catch-and-log (e.g. loadSchemas) silently get nothing.
 *      `call`/`agentCall`/`agentStream` gate every RPC on
 *      `client.waitForReady(deadline)` first,
 *      so the call is issued only once the channel has actually reached READY.
 *      Combined with (1), that resolves within ~100ms of the daemon binding.
 *
 * The production Tauri path (Rust/tonic `connect_with_connector_lazy`) has no
 * equivalent cached-backoff state — tonic retries the connector fresh per
 * call — so this shaping is dev-proxy-specific and does not exist there.
 */

import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PROTO_PATH = path.resolve(__dirname, '../../proto/proto/node_service.proto');
const AGENT_PROTO_PATH = path.resolve(__dirname, '../../proto/proto/local_agent_service.proto');

/**
 * Bound the gRPC-js reconnect backoff to a short, FIXED interval. This is the
 * root-cause fix: it caps grpc-js's default 1s->120s exponential backoff so a
 * channel whose peer was unreachable re-probes the transport roughly every
 * 100ms and recovers promptly once the socket appears — instead of sitting
 * out an ever-growing backoff the caller cannot control. Applies only to the
 * dev-proxy's gRPC-js client, never to the production tonic path.
 */
export const RECONNECT_CHANNEL_OPTIONS: grpc.ClientOptions = {
  'grpc.initial_reconnect_backoff_ms': 100,
  'grpc.max_reconnect_backoff_ms': 100
};

export function resolveSocketAddress(): string {
  const sock =
    process.env.NODESPACED_SOCKET ?? `${process.env.HOME}/.nodespace/daemon.sock`;
  return `unix:${sock}`;
}

const loaderOptions: protoLoader.Options = {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true
};

const proto = grpc.loadPackageDefinition(
  protoLoader.loadSync(PROTO_PATH, loaderOptions)
) as unknown as {
  nodespace: { NodeService: grpc.ServiceClientConstructor };
};

const agentProto = grpc.loadPackageDefinition(
  protoLoader.loadSync(AGENT_PROTO_PATH, loaderOptions)
) as unknown as {
  nodespace: { LocalAgentService: grpc.ServiceClientConstructor };
};

export interface NodeSpaceGrpcClients {
  address: string;
  nodeClient: grpc.Client;
  agentClient: grpc.Client;
  /**
   * Wait for `client`'s channel to reach READY, actively driving a connection
   * attempt (and, thanks to RECONNECT_CHANNEL_OPTIONS, re-probing every
   * ~100ms). Resolves as soon as the transport is usable; rejects only if the
   * deadline passes with the channel still not READY.
   */
  ready: (client: grpc.Client, timeoutMs?: number) => Promise<void>;
  /** Promisified unary call on nodeClient, gated on the channel being READY. */
  call: <TReq, TRes>(method: Function, request: TReq) => Promise<TRes>;
  /** Promisified unary call on agentClient, gated on the channel being READY. */
  agentCall: <TReq, TRes>(method: Function, request: TReq) => Promise<TRes>;
  /** Server-streaming call on agentClient, gated on the channel being READY. */
  agentStream: <TReq, TEvent>(method: Function, request: TReq) => Promise<TEvent[]>;
}

/**
 * Build the dev-proxy's gRPC clients against `address` (a `unix:` target),
 * with the reconnect-backoff cap and the READY-gate wiring described above.
 */
export function createNodeSpaceClients(
  address: string = resolveSocketAddress()
): NodeSpaceGrpcClients {
  const nodeClient = new proto.nodespace.NodeService(
    address,
    grpc.credentials.createInsecure(),
    RECONNECT_CHANNEL_OPTIONS
  );
  const agentClient = new agentProto.nodespace.LocalAgentService(
    address,
    grpc.credentials.createInsecure(),
    RECONNECT_CHANNEL_OPTIONS
  );

  const ready = (client: grpc.Client, timeoutMs = 30_000): Promise<void> =>
    new Promise((resolve, reject) => {
      const deadline = new Date(Date.now() + timeoutMs);
      client.waitForReady(deadline, (err?: Error) => {
        if (err) reject(err);
        else resolve();
      });
    });

  const call = <TReq, TRes>(method: Function, request: TReq): Promise<TRes> =>
    ready(nodeClient).then(
      () =>
        new Promise<TRes>((resolve, reject) => {
          method.call(nodeClient, request, (err: grpc.ServiceError | null, res: TRes) => {
            if (err) reject(err);
            else resolve(res);
          });
        })
    );

  const agentCall = <TReq, TRes>(method: Function, request: TReq): Promise<TRes> =>
    ready(agentClient).then(
      () =>
        new Promise<TRes>((resolve, reject) => {
          method.call(agentClient, request, (err: grpc.ServiceError | null, res: TRes) => {
            if (err) reject(err);
            else resolve(res);
          });
        })
    );

  const agentStream = <TReq, TEvent>(method: Function, request: TReq): Promise<TEvent[]> =>
    ready(agentClient).then(
      () =>
        new Promise<TEvent[]>((resolve, reject) => {
          const stream = method.call(agentClient, request) as grpc.ClientReadableStream<TEvent>;
          const events: TEvent[] = [];
          stream.on('data', (evt: TEvent) => events.push(evt));
          stream.on('error', (err: grpc.ServiceError) => reject(err));
          stream.on('end', () => resolve(events));
        })
    );

  return { address, nodeClient, agentClient, ready, call, agentCall, agentStream };
}
