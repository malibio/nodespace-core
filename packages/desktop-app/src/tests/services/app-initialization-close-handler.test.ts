/**
 * Regression coverage for core#2347: `destroy()` was only called inside the
 * `hasPendingWrites()` branch of the Tauri close handler. Tauri's own
 * manager calls `prevent_close()` on every `CloseRequested` whenever a JS
 * listener is registered for it -- which this one always is -- before the
 * callback even runs, and holds the window open regardless of what the
 * callback does. With no pending writes (the common case), the old handler
 * never called `destroy()`, so the window -- and therefore the app, since
 * this is also what the "Quit" tray menu item's close request drives --
 * silently never closed at all.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const onCloseRequestedHandlers: Array<(event: { preventDefault: () => void }) => Promise<void>> =
  [];
const destroyMock = vi.fn().mockResolvedValue(undefined);

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (handler: (event: { preventDefault: () => void }) => Promise<void>) => {
      onCloseRequestedHandlers.push(handler);
      return Promise.resolve(() => {});
    },
    destroy: destroyMock
  })
}));

interface WindowWithTauriInternals extends Window {
  __TAURI_INTERNALS__?: unknown;
}

describe('registerShutdownHandlers — Tauri close handler', () => {
  beforeEach(() => {
    vi.resetModules();
    onCloseRequestedHandlers.length = 0;
    destroyMock.mockClear();
    (window as WindowWithTauriInternals).__TAURI_INTERNALS__ = {};
  });

  afterEach(() => {
    delete (window as WindowWithTauriInternals).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
  });

  /**
   * Imports `sharedNodeStore` and `registerShutdownHandlers` together, in the
   * same post-`resetModules()` "epoch", so a spy set on the returned
   * `sharedNodeStore` is guaranteed to be the exact same instance the
   * handler's own internal import resolves to -- `vi.resetModules()` clears
   * Vitest's module cache, so a `sharedNodeStore` imported at this file's
   * top level (once, before any `resetModules()` call) would otherwise be a
   * different singleton instance than the one a freshly re-imported
   * `app-initialization` module pulls in internally, and a spy on the wrong
   * instance silently no-ops instead of failing loudly.
   */
  async function registerAndCapture() {
    const { sharedNodeStore } = await import('$lib/services/shared-node-store.svelte');
    const { registerShutdownHandlers } = await import('$lib/services/app-initialization');
    registerShutdownHandlers();
    // registerShutdownHandlers is sync but fires off registerTauriCloseHandler
    // (async, unawaited) in the background -- wait for its dynamic import and
    // onCloseRequested registration to actually land rather than assuming a
    // fixed number of microtask ticks is enough.
    await vi.waitFor(() => {
      expect(onCloseRequestedHandlers).toHaveLength(1);
    });
    return { handler: onCloseRequestedHandlers[0], sharedNodeStore };
  }

  it('closes the window even when there are no pending writes', async () => {
    const { handler, sharedNodeStore } = await registerAndCapture();
    vi.spyOn(sharedNodeStore, 'hasPendingWrites').mockReturnValue(false);
    const flushSpy = vi.spyOn(sharedNodeStore, 'flushAllPending');

    await handler({ preventDefault: vi.fn() });

    expect(destroyMock).toHaveBeenCalledTimes(1);
    expect(flushSpy).not.toHaveBeenCalled();
  });

  it('flushes pending writes, then still closes the window', async () => {
    const { handler, sharedNodeStore } = await registerAndCapture();
    vi.spyOn(sharedNodeStore, 'hasPendingWrites').mockReturnValue(true);
    const flushSpy = vi.spyOn(sharedNodeStore, 'flushAllPending').mockResolvedValue(undefined);
    const preventDefault = vi.fn();

    await handler({ preventDefault });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(flushSpy).toHaveBeenCalledTimes(1);
    expect(destroyMock).toHaveBeenCalledTimes(1);
  });

  it('still closes the window even if the flush itself fails', async () => {
    const { handler, sharedNodeStore } = await registerAndCapture();
    vi.spyOn(sharedNodeStore, 'hasPendingWrites').mockReturnValue(true);
    vi.spyOn(sharedNodeStore, 'flushAllPending').mockRejectedValue(new Error('flush failed'));

    await handler({ preventDefault: vi.fn() });

    expect(destroyMock).toHaveBeenCalledTimes(1);
  });

  it('does not propagate if destroy() itself throws', async () => {
    const { handler, sharedNodeStore } = await registerAndCapture();
    vi.spyOn(sharedNodeStore, 'hasPendingWrites').mockReturnValue(false);
    destroyMock.mockRejectedValueOnce(new Error('destroy failed'));

    await expect(handler({ preventDefault: vi.fn() })).resolves.toBeUndefined();

    expect(destroyMock).toHaveBeenCalledTimes(1);
  });
});
