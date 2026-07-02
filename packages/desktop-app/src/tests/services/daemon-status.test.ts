/**
 * Daemon status service (#1470) — shared daemon-status listener that drives
 * AppShell's connecting/unreachable banner and fans out a reconnect signal
 * to daemon-dependent stores (schemas, collections, children-tree) so they
 * retry a failed load once the daemon becomes healthy, instead of staying
 * failed until a manual app restart.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockIsTauri = vi.fn(() => true);
vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => mockIsTauri()
}));

// Capture the `daemon-status` handler so tests can drive it.
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

function emitDaemonStatus(payload: string) {
  daemonStatusHandler?.({ payload });
}

describe('daemon-status service (#1470)', () => {
  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockIsTauri.mockReturnValue(true);
    daemonStatusHandler = null;
  });

  it('clears connecting via the grace-period timer if no daemon-status event ever arrives', async () => {
    vi.useFakeTimers();
    try {
      const { daemonStatus, startDaemonStatusListener } = await import(
        '$lib/services/daemon-status'
      );
      startDaemonStatusListener();
      // Allow the async listen() registration to resolve without advancing timers.
      await vi.advanceTimersByTimeAsync(0);

      expect(get(daemonStatus).connecting).toBe(true);

      await vi.advanceTimersByTimeAsync(1500);

      expect(get(daemonStatus).connecting).toBe(false);
      expect(get(daemonStatus).unreachable).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('starts in a connecting state and clears it once a healthy event arrives', async () => {
    const { daemonStatus, startDaemonStatusListener } = await import('$lib/services/daemon-status');
    startDaemonStatusListener();
    // Allow the async listen() registration to resolve
    await Promise.resolve();
    await Promise.resolve();

    expect(get(daemonStatus).connecting).toBe(true);
    expect(get(daemonStatus).unreachable).toBe(false);

    emitDaemonStatus('healthy');

    expect(get(daemonStatus).connecting).toBe(false);
    expect(get(daemonStatus).unreachable).toBe(false);
  });

  it('marks unreachable on a not_running event and clears connecting', async () => {
    const { daemonStatus, startDaemonStatusListener } = await import('$lib/services/daemon-status');
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    emitDaemonStatus('not_running');

    expect(get(daemonStatus).connecting).toBe(false);
    expect(get(daemonStatus).unreachable).toBe(true);
  });

  it('clears unreachable once a later healthy event arrives', async () => {
    const { daemonStatus, startDaemonStatusListener } = await import('$lib/services/daemon-status');
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    emitDaemonStatus('not_running');
    expect(get(daemonStatus).unreachable).toBe(true);

    emitDaemonStatus('healthy');
    expect(get(daemonStatus).unreachable).toBe(false);
  });

  it('fires onDaemonReconnect callbacks when the daemon transitions to healthy', async () => {
    const { startDaemonStatusListener, onDaemonReconnect } = await import(
      '$lib/services/daemon-status'
    );
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    const callback = vi.fn();
    onDaemonReconnect(callback);

    emitDaemonStatus('healthy');

    expect(callback).toHaveBeenCalledTimes(1);
  });

  it('does not fire reconnect callbacks for a not_running event', async () => {
    const { startDaemonStatusListener, onDaemonReconnect } = await import(
      '$lib/services/daemon-status'
    );
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    const callback = vi.fn();
    onDaemonReconnect(callback);

    emitDaemonStatus('not_running');

    expect(callback).not.toHaveBeenCalled();
  });

  it('does not re-fire reconnect callbacks for consecutive healthy events', async () => {
    const { startDaemonStatusListener, onDaemonReconnect } = await import(
      '$lib/services/daemon-status'
    );
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    const callback = vi.fn();
    onDaemonReconnect(callback);

    emitDaemonStatus('healthy');
    emitDaemonStatus('healthy');

    expect(callback).toHaveBeenCalledTimes(1);
  });

  it('fires reconnect callbacks again after a healthy -> not_running -> healthy cycle', async () => {
    const { startDaemonStatusListener, onDaemonReconnect } = await import(
      '$lib/services/daemon-status'
    );
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    const callback = vi.fn();
    onDaemonReconnect(callback);

    emitDaemonStatus('healthy');
    emitDaemonStatus('not_running');
    emitDaemonStatus('healthy');

    expect(callback).toHaveBeenCalledTimes(2);
  });

  it('an unsubscribe function stops further reconnect callbacks', async () => {
    const { startDaemonStatusListener, onDaemonReconnect } = await import(
      '$lib/services/daemon-status'
    );
    startDaemonStatusListener();
    await Promise.resolve();
    await Promise.resolve();

    const callback = vi.fn();
    const unsubscribe = onDaemonReconnect(callback);
    unsubscribe();

    emitDaemonStatus('healthy');

    expect(callback).not.toHaveBeenCalled();
  });

  it('does nothing outside Tauri (browser mode)', async () => {
    mockIsTauri.mockReturnValue(false);
    const { daemonStatus, startDaemonStatusListener } = await import('$lib/services/daemon-status');
    startDaemonStatusListener();
    await Promise.resolve();

    // Listener never registers, so state stays at its initial default.
    expect(get(daemonStatus).connecting).toBe(true);
    expect(daemonStatusHandler).toBeNull();
  });
});
