import { describe, it, expect, afterEach } from 'vitest';
import * as fs from 'node:fs';
import { getClient, resetClient, PROTO_PATH } from '../client.js';

afterEach(() => {
  resetClient();
});

describe('client', () => {
  it('resolves PROTO_PATH to a file that exists on disk', () => {
    // Guards against silent breakage if packages/daemon is ever relocated.
    expect(fs.existsSync(PROTO_PATH)).toBe(true);
  });

  it('loads node_service.proto and constructs a NodeService client', () => {
    const client = getClient();
    expect(client).toBeDefined();
    expect(typeof (client as unknown as Record<string, unknown>).searchNodes).toBe('function');
    expect(typeof (client as unknown as Record<string, unknown>).getNode).toBe('function');
    expect(typeof (client as unknown as Record<string, unknown>).createNode).toBe('function');
    expect(typeof (client as unknown as Record<string, unknown>).updateNode).toBe('function');
    expect(typeof (client as unknown as Record<string, unknown>).getChildren).toBe('function');
  });

  it('caches the client across calls', () => {
    const first = getClient();
    const second = getClient();
    expect(second).toBe(first);
  });

  it('returns a fresh client after resetClient', () => {
    const first = getClient();
    resetClient();
    const second = getClient();
    expect(second).not.toBe(first);
  });
});
