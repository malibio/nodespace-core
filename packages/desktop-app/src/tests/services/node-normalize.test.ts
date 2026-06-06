import { describe, it, expect } from 'vitest';
import { normalizeNodeData } from '$lib/services/node-normalize';
import type { Node } from '$lib/types/node';

function makeNode(overrides: Partial<Node> = {}): Node {
  return {
    id: 'test-id',
    nodeType: 'text',
    content: 'test content',
    version: 1,
    createdAt: '2024-01-01T00:00:00Z',
    modifiedAt: '2024-01-01T00:00:00Z',
    ...overrides
  } as Node;
}

describe('normalizeNodeData', () => {
  it('returns non-task nodes unchanged', () => {
    const node = makeNode({ nodeType: 'text' });
    expect(normalizeNodeData(node)).toBe(node);
  });

  // The backend (`node_to_typed_value`) promotes type-specific fields to the TOP
  // LEVEL of the node for every transport — see the `wire_contract` tests in
  // `nodespace-types/src/convert.rs`. These tests pin that flat contract on the TS
  // side; the converters intentionally no longer accept the nested `properties.task`
  // shape, which no live producer emits.
  it('passes through a flat task node, preserving promoted fields', () => {
    const node = makeNode({
      nodeType: 'task',
      status: 'done',
      priority: 'high'
    } as Partial<Node>);
    const result = normalizeNodeData(node);
    expect(result.nodeType).toBe('task');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).status).toBe('done');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).priority).toBe('high');
  });

  it('task node with no status gets default "open"', () => {
    const node = makeNode({ nodeType: 'task' });
    const result = normalizeNodeData(node);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).status).toBe('open');
  });

  // Drift guard: both sync paths (Tauri + browser) call this single function, so the
  // transformation contract below applies identically to both runtime modes. Adding a
  // future type branch here is the one-place change that covers both paths.
  it('normalizes a full flat task node with status, priority, and dueDate', () => {
    const node = makeNode({
      nodeType: 'task',
      status: 'in_progress',
      priority: 'low',
      dueDate: '2024-12-31'
    } as Partial<Node>);
    const result = normalizeNodeData(node);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const r = result as any;
    expect(r.nodeType).toBe('task');
    expect(r.status).toBe('in_progress');
    expect(r.priority).toBe('low');
    expect(r.dueDate).toBe('2024-12-31');
    expect(r.id).toBe('test-id');
    expect(r.content).toBe('test content');
  });
});
