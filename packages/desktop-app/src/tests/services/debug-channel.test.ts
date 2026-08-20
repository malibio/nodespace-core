import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockInvoke = vi.fn();
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);

import {
  debugChannelWrite,
  isChannelEnabledSync,
  isChannelEnabled,
  captureDomSnapshot,
  captureStoreDump
} from '$lib/services/debug-channel';

describe('debug-channel', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe('debugChannelWrite', () => {
    it('never touches the Tauri invoke bridge under VITEST (isTest guard)', () => {
      debugChannelWrite({
        kind: 'console',
        timestamp: new Date().toISOString(),
        level: 'info',
        message: 'test message'
      });

      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it('does not throw for any DebugEvent kind', () => {
      const timestamp = new Date().toISOString();
      expect(() => {
        debugChannelWrite({ kind: 'console', timestamp, level: 'debug', message: 'm' });
        debugChannelWrite({ kind: 'console', timestamp, level: 'error', message: 'm', data: { a: 1 } });
        debugChannelWrite({
          kind: 'invoke',
          timestamp,
          method: 'createNode',
          args: ['a'],
          durationMs: 12,
          status: 'success',
          result: { id: '1' }
        });
        debugChannelWrite({
          kind: 'invoke',
          timestamp,
          method: 'createNode',
          args: ['a'],
          durationMs: 12,
          status: 'error',
          error: 'boom'
        });
        debugChannelWrite({ kind: 'dom_snapshot', timestamp, html: '<html></html>' });
        debugChannelWrite({ kind: 'store_dump', timestamp, stores: { foo: 'bar' } });
      }).not.toThrow();
    });
  });

  describe('isChannelEnabledSync', () => {
    it('reports false before the async probe resolves (default state)', () => {
      expect(isChannelEnabledSync()).toBe(false);
    });
  });

  describe('isChannelEnabled', () => {
    it('resolves based on the frontend_log_enabled probe', async () => {
      mockInvoke.mockResolvedValueOnce(false);
      const enabled = await isChannelEnabled();
      expect(enabled).toBe(false);
      expect(mockInvoke).toHaveBeenCalledWith('frontend_log_enabled');
    });
  });

  describe('captureDomSnapshot', () => {
    it('does not throw and does not invoke the Tauri bridge under VITEST', () => {
      expect(() => captureDomSnapshot()).not.toThrow();
      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });

  describe('captureStoreDump', () => {
    it('resolves without throwing and does not invoke the Tauri bridge under VITEST', async () => {
      await expect(captureStoreDump()).resolves.toBeUndefined();
      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });
});
