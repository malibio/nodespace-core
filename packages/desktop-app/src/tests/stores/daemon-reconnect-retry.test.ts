/**
 * Daemon-reconnect retry wiring — schemasData and collectionsData
 * both register with the shared daemon-status reconnect hook at module load
 * so a load that failed while the daemon was still starting up retries
 * automatically once the daemon becomes healthy, instead of staying failed
 * until a manual app restart.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockGetAllSchemas = vi.fn();
vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    getAllSchemas: (...args: unknown[]) => mockGetAllSchemas(...args)
  }
}));

const mockGetAllCollections = vi.fn();
vi.mock('$lib/services/collection-service', () => ({
  collectionService: {
    getAllCollections: (...args: unknown[]) => mockGetAllCollections(...args)
  }
}));

const mockIsTauri = vi.fn(() => true);
// Defaults to a promise that never resolves so the initial pull in
// startDaemonStatusListener() doesn't fire a spurious reconnect before
// goHealthy() drives the push-event path these tests exercise.
const mockInvoke = vi.fn((_cmd: string) => new Promise<string>(() => {}));
vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => mockIsTauri(),
  invoke: (cmd: string) => mockInvoke(cmd)
}));

let daemonStatusHandler: ((event: { payload: string }) => void) | null = null;
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: string }) => void) => {
    if (name === 'daemon-status') {
      daemonStatusHandler = handler;
    }
    return () => {
      if (name === 'daemon-status') daemonStatusHandler = null;
    };
  })
}));

async function goHealthy() {
  const { startDaemonStatusListener } = await import('$lib/services/daemon-status');
  startDaemonStatusListener();
  await Promise.resolve();
  await Promise.resolve();
  daemonStatusHandler?.({ payload: 'healthy' });
}

describe('daemon-reconnect retry wiring (#1470)', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mockIsTauri.mockReturnValue(true);
    mockInvoke.mockReturnValue(new Promise<string>(() => {}));
    daemonStatusHandler = null;
    mockGetAllSchemas.mockResolvedValue([]);
    mockGetAllCollections.mockResolvedValue([]);
  });

  it('schemasData retries loadSchemas once the daemon reconnects', async () => {
    await import('$lib/stores/schemas.svelte');
    expect(mockGetAllSchemas).not.toHaveBeenCalled();

    await goHealthy();

    expect(mockGetAllSchemas).toHaveBeenCalledTimes(1);
  });

  it('collectionsData retries loadCollections once the daemon reconnects', async () => {
    await import('$lib/stores/collections.svelte');
    expect(mockGetAllCollections).not.toHaveBeenCalled();

    await goHealthy();

    expect(mockGetAllCollections).toHaveBeenCalledTimes(1);
  });
});
