import { describe, it, expect } from 'vitest';
import {
  normalizeNodeData,
  deepMergeProperties,
  promoteTypedFields,
  OPTIMISTIC_TYPED_FIELDS
} from '$lib/services/node-normalize';
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

describe('deepMergeProperties', () => {
  it('merges one level and keeps sibling keys', () => {
    const merged = deepMergeProperties(
      { 'capture:x': 'keep', provider: 'native' },
      { model: 'm1' },
      'ai-chat'
    );
    expect(merged).toEqual({ 'capture:x': 'keep', provider: 'native', model: 'm1' });
  });

  it('merges one level deeper into the type namespace', () => {
    const merged = deepMergeProperties(
      { task: { status: 'open', priority: 'high' } },
      { task: { status: 'done' } },
      'task'
    );
    expect(merged.task).toEqual({ status: 'done', priority: 'high' });
  });

  it('treats a missing existing bag as empty', () => {
    expect(deepMergeProperties(undefined, { model: 'm1' }, 'ai-chat')).toEqual({ model: 'm1' });
  });
});

describe('promoteTypedFields', () => {
  it('promotes only fields present in a flat ai-chat write', () => {
    const changes = { messages: [{ role: 'user', content: 'hi' }], turn_status: 'processing' };
    const promoted = promoteTypedFields('ai-chat', changes, changes);
    // provider/model/sessionStatus omitted → not promoted (guards against undefined-clobber)
    expect(promoted).toEqual({ messages: changes.messages, turnStatus: 'processing' });
    expect('provider' in promoted).toBe(false);
    expect('model' in promoted).toBe(false);
    expect('sessionStatus' in promoted).toBe(false);
  });

  it('promotes turnStatus and sessionStatus independently', () => {
    const changes = { session_status: 'archived' };
    const promoted = promoteTypedFields('ai-chat', changes, changes);
    expect(promoted).toEqual({ sessionStatus: 'archived' });
    expect('turnStatus' in promoted).toBe(false);
  });

  it('promotes nested task fields from the type namespace', () => {
    const changes = { task: { status: 'done' } };
    const merged = { task: { status: 'done', priority: 'high' } };
    const promoted = promoteTypedFields('task', changes, merged);
    expect(promoted).toEqual({ status: 'done' });
  });

  it('returns nothing for a node type with no typed-field map', () => {
    expect(promoteTypedFields('text', { foo: 'bar' }, { foo: 'bar' })).toEqual({});
  });

  it('promotes an explicit null value (present but null)', () => {
    const changes = { model: null };
    const promoted = promoteTypedFields('ai-chat', changes, changes);
    expect('model' in promoted).toBe(true);
    expect(promoted.model).toBeNull();
  });

  it('map stays aligned with the documented promoted types', () => {
    expect(Object.keys(OPTIMISTIC_TYPED_FIELDS).sort()).toEqual(['ai-chat', 'task']);
    expect(OPTIMISTIC_TYPED_FIELDS['ai-chat']).toEqual([
      { from: 'turn_status', to: 'turnStatus' },
      { from: 'session_status', to: 'sessionStatus' },
      { from: 'provider', to: 'provider' },
      { from: 'model', to: 'model' },
      { from: 'messages', to: 'messages' }
    ]);
  });

  it('promotes an ai-chat write using the real snake_case payload shape', () => {
    // Mirrors the actual write in ai-chat-node-viewer.svelte: canonical
    // snake_case property keys, promoted to camelCase top-level fields.
    const changes = {
      messages: [{ role: 'user', content: 'hi' }],
      turn_status: 'processing',
      session_status: 'active',
      provider: 'native',
      model: 'claude-sonnet-5'
    };
    const promoted = promoteTypedFields('ai-chat', changes, changes);
    expect(promoted).toEqual({
      messages: changes.messages,
      turnStatus: 'processing',
      sessionStatus: 'active',
      provider: 'native',
      model: 'claude-sonnet-5'
    });
  });
});
