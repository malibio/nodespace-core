import { describe, it, expect, afterAll } from 'vitest';
import { getNode } from '../tools.js';
import { resetClient } from '../client.js';
import { ToolError } from '../types.js';

// Integration-style check: with no daemon running on the bogus port, a real
// gRPC call should surface as a structured ToolError instead of an
// uncaught exception. We point at an unused loopback port via env override.

const originalAddr = process.env.NODESPACED_ADDR;

afterAll(() => {
  resetClient();
  if (originalAddr === undefined) {
    delete process.env.NODESPACED_ADDR;
  } else {
    process.env.NODESPACED_ADDR = originalAddr;
  }
});

describe('connection error handling', () => {
  it('returns a ToolError when nodespaced is unreachable', async () => {
    // Use a port that is almost certainly not listening.
    process.env.NODESPACED_ADDR = '127.0.0.1:1';
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
