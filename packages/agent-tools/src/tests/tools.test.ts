import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as grpc from '@grpc/grpc-js';
import * as clientModule from '../client.js';

type UnaryCallback = (err: grpc.ServiceError | null, response?: unknown) => void;

const mockClient = {
  createNode: vi.fn(),
  getNode: vi.fn(),
  updateNode: vi.fn(),
  getChildren: vi.fn(),
  searchNodes: vi.fn(),
  close: vi.fn()
};

vi.spyOn(clientModule, 'getClient').mockReturnValue(
  mockClient as unknown as ReturnType<typeof clientModule.getClient>
);

const sampleNodeData = {
  id: 'node-1',
  nodeType: 'text',
  content: 'hello world',
  parentId: 'parent-1',
  properties: '{}',
  version: '1',
  lifecycleStatus: 'active',
  createdAt: '2026-05-18T00:00:00Z',
  modifiedAt: '2026-05-18T00:00:00Z',
  collectionId: ''
};

const sampleNodeResponse = {
  nodeId: 'node-1',
  nodeType: 'text',
  parentId: 'parent-1',
  collectionId: '',
  nodeData: sampleNodeData
};

function unavailableError(): grpc.ServiceError {
  const err = new Error('connect ECONNREFUSED 127.0.0.1:50051') as grpc.ServiceError;
  err.code = grpc.status.UNAVAILABLE;
  err.details = 'connect ECONNREFUSED 127.0.0.1:50051';
  err.metadata = new grpc.Metadata();
  err.name = 'Error';
  return err;
}

function notFoundError(): grpc.ServiceError {
  const err = new Error('node missing') as grpc.ServiceError;
  err.code = grpc.status.NOT_FOUND;
  err.details = 'node missing';
  err.metadata = new grpc.Metadata();
  err.name = 'Error';
  return err;
}

beforeEach(() => {
  for (const fn of Object.values(mockClient)) {
    fn.mockReset();
  }
});

describe('searchSemantic', () => {
  it('returns mapped nodes and echoes the query', async () => {
    mockClient.searchNodes.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, { nodes: [sampleNodeData], count: 1, collectionId: '' });
      }
    );
    const { searchSemantic } = await import('../tools.js');

    const result = await searchSemantic('hello', 5);

    expect(result.query).toBe('hello');
    expect(result.nodes).toHaveLength(1);
    expect(result.nodes[0]).toEqual({
      id: 'node-1',
      nodeType: 'text',
      content: 'hello world',
      parentId: 'parent-1'
    });
    const callArg = mockClient.searchNodes.mock.calls[0][0];
    expect(callArg).toMatchObject({ query: 'hello', limit: 5, semantic: true });
  });

  it('defaults limit to 0 when omitted', async () => {
    mockClient.searchNodes.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, { nodes: [], count: 0, collectionId: '' });
      }
    );
    const { searchSemantic } = await import('../tools.js');

    await searchSemantic('q');

    expect(mockClient.searchNodes.mock.calls[0][0]).toMatchObject({ limit: 0 });
  });

  it('wraps UNAVAILABLE errors as ToolError', async () => {
    mockClient.searchNodes.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(unavailableError());
      }
    );
    const { searchSemantic } = await import('../tools.js');
    const { ToolError } = await import('../types.js');

    await expect(searchSemantic('q')).rejects.toBeInstanceOf(ToolError);
    await expect(searchSemantic('q')).rejects.toMatchObject({
      code: 'UNAVAILABLE'
    });
  });
});

describe('getNode', () => {
  it('returns a mapped node', async () => {
    mockClient.getNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, sampleNodeResponse);
      }
    );
    const { getNode } = await import('../tools.js');

    const result = await getNode('node-1');

    expect(result).toEqual({
      id: 'node-1',
      nodeType: 'text',
      content: 'hello world',
      parentId: 'parent-1'
    });
    expect(mockClient.getNode.mock.calls[0][0]).toEqual({ nodeId: 'node-1' });
  });

  it('wraps NOT_FOUND as ToolError', async () => {
    mockClient.getNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(notFoundError());
      }
    );
    const { getNode } = await import('../tools.js');
    const { ToolError } = await import('../types.js');

    await expect(getNode('missing')).rejects.toBeInstanceOf(ToolError);
    await expect(getNode('missing')).rejects.toMatchObject({ code: 'NOT_FOUND' });
  });
});

describe('createNode', () => {
  it('returns the created node and forwards the request', async () => {
    mockClient.createNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, sampleNodeResponse);
      }
    );
    const { createNode } = await import('../tools.js');

    const result = await createNode('text', 'hello world', 'parent-1');

    expect(result.id).toBe('node-1');
    expect(mockClient.createNode.mock.calls[0][0]).toMatchObject({
      nodeType: 'text',
      content: 'hello world',
      parentId: 'parent-1'
    });
  });

  it('defaults parentId to empty string for root nodes', async () => {
    mockClient.createNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, sampleNodeResponse);
      }
    );
    const { createNode } = await import('../tools.js');

    await createNode('text', 'hello');

    expect(mockClient.createNode.mock.calls[0][0]).toMatchObject({ parentId: '' });
  });
});

describe('updateNode', () => {
  it('forwards content update and returns mapped node', async () => {
    mockClient.updateNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, sampleNodeResponse);
      }
    );
    const { updateNode } = await import('../tools.js');

    const result = await updateNode('node-1', 'new content');

    expect(result.id).toBe('node-1');
    expect(mockClient.updateNode.mock.calls[0][0]).toMatchObject({
      nodeId: 'node-1',
      content: 'new content'
    });
  });
});

describe('getChildren', () => {
  it('returns the mapped child list', async () => {
    mockClient.getChildren.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, {
          nodes: [sampleNodeData, { ...sampleNodeData, id: 'node-2' }],
          count: 2,
          collectionId: ''
        });
      }
    );
    const { getChildren } = await import('../tools.js');

    const result = await getChildren('parent-1');

    expect(result).toHaveLength(2);
    expect(result[0].id).toBe('node-1');
    expect(result[1].id).toBe('node-2');
    expect(mockClient.getChildren.mock.calls[0][0]).toEqual({ nodeId: 'parent-1' });
  });

  it('returns empty array when no children', async () => {
    mockClient.getChildren.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, { nodes: [], count: 0, collectionId: '' });
      }
    );
    const { getChildren } = await import('../tools.js');

    expect(await getChildren('leaf')).toEqual([]);
  });
});

describe('parentId normalization', () => {
  it('treats empty parentId as undefined', async () => {
    mockClient.getNode.mockImplementation(
      (_req: unknown, cb: UnaryCallback) => {
        cb(null, {
          ...sampleNodeResponse,
          parentId: '',
          nodeData: { ...sampleNodeData, parentId: '' }
        });
      }
    );
    const { getNode } = await import('../tools.js');

    const result = await getNode('root');

    expect(result.parentId).toBeUndefined();
  });
});
