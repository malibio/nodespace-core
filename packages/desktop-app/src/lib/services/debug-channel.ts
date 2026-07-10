/**
 * Structured NDJSON debug channel for the packaged Tauri (WKWebView) app.
 *
 * When `NS_FRONTEND_LOG` is set (a file path), every call below appends one
 * JSON object per line to that file via the `frontend_log` Tauri command —
 * console messages, backend invoke/network events, on-demand DOM snapshots,
 * and on-demand store dumps all share this one channel. Gated by a one-time
 * `frontend_log_enabled` probe, so normal builds, the browser, and tests pay
 * no cost. An agent (or human) queries the channel by tailing the file the
 * env var names — no daemon or HTTP endpoint involved.
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export type DebugEvent =
  | { kind: 'console'; timestamp: string; level: LogLevel; message: string; data?: unknown }
  | {
      kind: 'invoke';
      timestamp: string;
      method: string;
      args: unknown[];
      durationMs: number;
      status: 'success' | 'error';
      result?: unknown;
      error?: string;
    }
  | { kind: 'dom_snapshot'; timestamp: string; html: string }
  | { kind: 'store_dump'; timestamp: string; stores: Record<string, unknown> };

const isTest =
  (typeof import.meta !== 'undefined' && import.meta.env?.VITEST === 'true') ||
  (typeof process !== 'undefined' && process.env?.VITEST === 'true');

let channelState: 'unknown' | 'on' | 'off' = 'unknown';
let channelInit: Promise<void> | null = null;

/**
 * Synchronous best-effort read of the channel's enabled state. Returns
 * `false` until the async probe (triggered by the first `debugChannelWrite`
 * call) resolves — callers that need the definitive answer should await
 * `debugChannelWrite`'s internal probe indirectly by checking again after a
 * microtask, or just call `debugChannelWrite` unconditionally since it's
 * already a no-op when disabled.
 */
export function isChannelEnabledSync(): boolean {
  return channelState === 'on';
}

/** Resolves once the channel-enabled probe completes. Use to gate one-time setup (e.g. `window.__ns_debug`). */
export async function isChannelEnabled(): Promise<boolean> {
  if (channelState === 'unknown') {
    if (!channelInit) {
      channelInit = (async () => {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          channelState = (await invoke<boolean>('frontend_log_enabled')) ? 'on' : 'off';
        } catch {
          channelState = 'off';
        }
      })();
    }
    await channelInit;
  }
  return channelState === 'on';
}

/**
 * Serialize and append a debug event to the NDJSON channel. Best-effort —
 * never throws. Tests pay no cost: never touch the Tauri `invoke` bridge
 * under VITEST, or the one-time probe pollutes invoke-call assertions
 * elsewhere.
 */
export function debugChannelWrite(event: DebugEvent): void {
  if (isTest) return;
  void (async () => {
    try {
      if (!(await isChannelEnabled())) return;
      const { invoke } = await import('@tauri-apps/api/core');
      const line = JSON.stringify(event);
      await invoke('frontend_log', { line });
    } catch {
      /* best-effort diagnostic only */
    }
  })();
}

/** Capture the current DOM as one debug-channel event. On-demand only. */
export function captureDomSnapshot(): void {
  debugChannelWrite({
    kind: 'dom_snapshot',
    timestamp: new Date().toISOString(),
    html: document.documentElement.outerHTML
  });
}

/**
 * Serialize a value for the store dump, converting Map/Set (common in
 * Svelte 5 reactive stores) to plain JSON-representable shapes.
 */
function toDumpable(value: unknown): unknown {
  if (value instanceof Map) return Object.fromEntries(value);
  if (value instanceof Set) return Array.from(value);
  return value;
}

/**
 * Capture a snapshot of the highest-value global stores as one debug-channel
 * event. On-demand only — dynamic imports keep this from pulling every store
 * into the eagerly-loaded module graph.
 */
export async function captureStoreDump(): Promise<void> {
  const stores: Record<string, unknown> = {};
  try {
    const { sharedNodeStore } = await import('./shared-node-store.svelte');
    stores.sharedNodeStore = {
      nodes: toDumpable(sharedNodeStore.nodes)
    };
  } catch {
    /* store unavailable in this context */
  }
  try {
    const { databaseStore } = await import('$lib/stores/database.svelte');
    stores.databaseStore = JSON.parse(JSON.stringify(databaseStore, (_k, v) => toDumpable(v)));
  } catch {
    /* store unavailable in this context */
  }
  try {
    const { navigationStore } = await import('$lib/stores/navigation.svelte');
    stores.navigationStore = JSON.parse(JSON.stringify(navigationStore, (_k, v) => toDumpable(v)));
  } catch {
    /* store unavailable in this context */
  }
  try {
    const { agentStore } = await import('$lib/stores/agent-store.svelte');
    stores.agentStore = JSON.parse(JSON.stringify(agentStore, (_k, v) => toDumpable(v)));
  } catch {
    /* store unavailable in this context */
  }
  try {
    const { proSync } = await import('$lib/stores/pro-sync.svelte');
    stores.proSync = JSON.parse(JSON.stringify(proSync, (_k, v) => toDumpable(v)));
  } catch {
    /* store unavailable in this context */
  }

  debugChannelWrite({
    kind: 'store_dump',
    timestamp: new Date().toISOString(),
    stores
  });
}
