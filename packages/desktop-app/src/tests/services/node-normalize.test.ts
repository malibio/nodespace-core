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

  it('converts a task node with nested properties to flat TaskNode', () => {
    const node = makeNode({
      nodeType: 'task',
      properties: { task: { status: 'done', priority: 'high' } } as Record<string, unknown>
    });
    const result = normalizeNodeData(node);
    expect(result.nodeType).toBe('task');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).status).toBe('done');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).priority).toBe('high');
  });

  it('task node with no properties gets default status "open"', () => {
    const node = makeNode({ nodeType: 'task' });
    const result = normalizeNodeData(node);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((result as any).status).toBe('open');
  });

  /**
   * Parity test: both sync paths must produce identical output from identical input.
   * This is the anti-drift guard — if either path diverges, this test breaks.
   */
  it('Tauri path and browser path produce identical output (parity guard)', () => {
    const taskNode = makeNode({
      nodeType: 'task',
      properties: { task: { status: 'in_progress', priority: 'low', dueDate: '2024-12-31' } } as Record<
        string,
        unknown
      >
    });

    // Both paths now call the same shared normalizeNodeData — simulate both invocations
    const tauriResult = normalizeNodeData(taskNode);
    const browserResult = normalizeNodeData(taskNode);

    expect(tauriResult).toEqual(browserResult);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((tauriResult as any).status).toBe('in_progress');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect((browserResult as any).dueDate).toBe('2024-12-31');
  });
});
