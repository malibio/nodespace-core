import { beforeEach, describe, expect, it, vi } from 'vitest';

const mockExecFile = vi.hoisted(() => vi.fn());
vi.mock('node:child_process', () => ({ execFile: mockExecFile }));

import { NodespaceCLIError, createNode, deleteNode, getNode, listNodes, searchNodes, updateNode } from '../tools.js';

function resolveExec(stdout: string) {
  mockExecFile.mockImplementation(
    (
      _cmd: string,
      _args: string[],
      _opts: object,
      cb: (err: null, result: { stdout: string }) => void
    ) => {
      cb(null, { stdout });
    }
  );
}

function rejectExec(err: object) {
  mockExecFile.mockImplementation(
    (_cmd: string, _args: string[], _opts: object, cb: (err: object) => void) => {
      cb(err);
    }
  );
}

beforeEach(() => {
  mockExecFile.mockReset();
});

describe('tools', () => {
  it('getNode passes correct args and returns stdout', async () => {
    resolveExec('{"id":"abc"}');
    const result = await getNode('abc');
    expect(result).toBe('{"id":"abc"}');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'node', 'get', 'abc'],
      expect.objectContaining({ timeout: 30_000 }),
      expect.any(Function)
    );
  });

  it('createNode passes type, content, and parent', async () => {
    resolveExec('{"id":"new"}');
    await createNode('text', 'hello', 'parent-1');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'node', 'create', '--type', 'text', '--content', 'hello', '--parent', 'parent-1'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('createNode omits --parent when not provided', async () => {
    resolveExec('{"id":"new"}');
    await createNode('text', 'hello');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'node', 'create', '--type', 'text', '--content', 'hello'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('searchNodes passes limit when provided', async () => {
    resolveExec('{"nodes":[]}');
    await searchNodes('test query', 5);
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'search', 'test query', '--limit', '5'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('listNodes uses --type filter not semantic search', async () => {
    resolveExec('{"nodes":[]}');
    await listNodes('task', 20);
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'search', '--type', 'task', '--limit', '20'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('listNodes defaults to limit 50', async () => {
    resolveExec('{"nodes":[]}');
    await listNodes('task');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'search', '--type', 'task', '--limit', '50'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('updateNode passes correct args', async () => {
    resolveExec('{"id":"abc"}');
    await updateNode('abc', 'new content');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'node', 'update', 'abc', '--content', 'new content'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('deleteNode passes correct args', async () => {
    resolveExec('{"existed":true}');
    await deleteNode('abc');
    expect(mockExecFile).toHaveBeenCalledWith(
      'nodespace',
      ['--json', 'node', 'delete', 'abc'],
      expect.any(Object),
      expect.any(Function)
    );
  });

  it('throws NodespaceCLIError with friendly message when nodespace not found', async () => {
    rejectExec({ code: 'ENOENT', message: 'not found', stderr: '' });
    await expect(getNode('abc')).rejects.toThrow(NodespaceCLIError);
    await expect(getNode('abc')).rejects.toThrow('nodespace CLI not found on $PATH');
  });

  it('throws NodespaceCLIError with exit code on CLI failure', async () => {
    rejectExec({ code: 1, message: 'command failed', stderr: 'some error' });
    await expect(getNode('abc')).rejects.toThrow(NodespaceCLIError);
  });
});
