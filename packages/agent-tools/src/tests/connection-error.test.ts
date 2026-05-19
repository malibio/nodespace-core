import { describe, it, expect, afterAll } from 'vitest';
import { getNode } from '../tools.js';
import { resetClient } from '../client.js';
import { ToolError } from '../types.js';

// Integration-style check: with no daemon running at the bogus socket path, a
// real gRPC call should surface as a structured ToolError instead of an
// uncaught exception. We point at a nonexistent socket via env override.

const originalSocket = process.env.NODESPACED_SOCKET;

afterAll(() => {
  resetClient();
  if (originalSocket === undefined) {
    delete process.env.NODESPACED_SOCKET;
  } else {
    process.env.NODESPACED_SOCKET = originalSocket;
  }
});

describe('connection error handling', () => {
  it('returns a ToolError when nodespaced is unreachable', async () => {
    // Use a socket path that is almost certainly not listening.
    process.env.NODESPACED_SOCKET = '/tmp/nodespace-no-such-daemon.sock';
    resetClient();

    try {
      await getNode('any-id');
      throw new Error('expected getNode to reject');
    } catch (err) {
      expect(err).toBeInstanceOf(ToolError);
      expect((err as ToolError).code).toBeTruthy();
    }
  }, 10_000);
});
