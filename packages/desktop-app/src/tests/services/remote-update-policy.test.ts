/**
 * Unit tests for the extracted remote-update policy (issue: extract explicit
 * remote-update policy from the skip-while-editing logic previously inlined
 * in SharedNodeStore.setNode / batchSetNodes).
 *
 * End-to-end behavior through the store is covered by
 * `shared-node-store-skip-while-editing.test.ts`; these tests pin the pure
 * decision function's contract in isolation.
 */

import { describe, it, expect } from 'vitest';
import {
  decideRemoteUpdate,
  isPlausibleOwnEcho,
  shouldSkipStaleAiChatUpdate
} from '$lib/services/remote-update-policy';
import type { Node } from '$lib/types';
import type { UpdateSource } from '$lib/types/update-protocol';

function makeNode(overrides: Partial<Node> = {}): Node {
  return {
    id: 'n1',
    nodeType: 'text',
    content: 'content',
    createdAt: '2024-01-01T00:00:00Z',
    modifiedAt: '2024-01-01T00:00:00Z',
    version: 1,
    properties: {},
    mentions: [],
    ...overrides
  } as Node;
}

const viewerSource: UpdateSource = { type: 'viewer', viewerId: 'v1' };
const databaseSource: UpdateSource = { type: 'database', reason: 'domain-event' };
const notEditing = { isFocused: false, hasPending: false };

describe('isPlausibleOwnEcho', () => {
  it('returns false when there is no last-sent record', () => {
    expect(isPlausibleOwnEcho(makeNode({ content: 'x' }), undefined)).toBe(false);
  });

  it('returns true when incoming content matches the last-sent content exactly', () => {
    expect(isPlausibleOwnEcho(makeNode({ content: 'hello world' }), 'hello world')).toBe(true);
  });

  it('returns false for a prefix-compatible but different foreign write (not a startsWith heuristic)', () => {
    // Regression: an older `local.startsWith(incoming)` heuristic mis-classified
    // this as an echo. The content-equality contract must not do that.
    expect(isPlausibleOwnEcho(makeNode({ content: 'hello' }), 'hello world')).toBe(false);
  });

  it('treats missing content as empty string', () => {
    expect(isPlausibleOwnEcho(makeNode({ content: undefined }), '')).toBe(true);
  });
});

describe('decideRemoteUpdate', () => {
  it('applies when source is not database', () => {
    const decision = decideRemoteUpdate(
      makeNode(),
      makeNode(),
      viewerSource,
      { isFocused: true, hasPending: false },
      undefined
    );
    expect(decision.apply).toBe(true);
  });

  it('applies when there is no existing node (first sighting)', () => {
    const decision = decideRemoteUpdate(
      makeNode(),
      undefined,
      databaseSource,
      { isFocused: true, hasPending: false },
      undefined
    );
    expect(decision.apply).toBe(true);
  });

  it('applies when the node is not actively edited (not focused, no pending write)', () => {
    const decision = decideRemoteUpdate(
      makeNode(),
      makeNode(),
      databaseSource,
      notEditing,
      undefined
    );
    expect(decision.apply).toBe(true);
  });

  it('skips (does not apply) a database update to a focused node', () => {
    const decision = decideRemoteUpdate(
      makeNode({ content: 'incoming' }),
      makeNode({ content: 'local' }),
      databaseSource,
      { isFocused: true, hasPending: false },
      undefined
    );
    expect(decision.apply).toBe(false);
  });

  it('skips a database update to a node with a pending write even if unfocused', () => {
    const decision = decideRemoteUpdate(
      makeNode({ content: 'incoming' }),
      makeNode({ content: 'local' }),
      databaseSource,
      { isFocused: false, hasPending: true },
      undefined
    );
    expect(decision.apply).toBe(false);
  });

  it('stashes the version and does not notify when the broadcast is a plausible own-echo', () => {
    const decision = decideRemoteUpdate(
      makeNode({ content: 'hello world', version: 6 }),
      makeNode({ content: 'hello world', version: 3 }),
      databaseSource,
      { isFocused: true, hasPending: false },
      'hello world'
    );
    expect(decision.apply).toBe(false);
    if (decision.apply) throw new Error('unreachable');
    expect(decision.stashVersion).toBe(true);
    expect(decision.notifyConflict).toBe(false);
  });

  it('does not stash the version and notifies when the broadcast looks like a foreign write', () => {
    const decision = decideRemoteUpdate(
      makeNode({ content: 'bob wrote this', version: 9 }),
      makeNode({ content: 'alice typed this', version: 3 }),
      databaseSource,
      { isFocused: true, hasPending: false },
      'alice typed this'
    );
    expect(decision.apply).toBe(false);
    if (decision.apply) throw new Error('unreachable');
    expect(decision.stashVersion).toBe(false);
    expect(decision.notifyConflict).toBe(true);
  });

  it('treats an incoming node with no numeric version as never stash-eligible', () => {
    const decision = decideRemoteUpdate(
      makeNode({ content: 'hello world', version: undefined as unknown as number }),
      makeNode({ content: 'hello world', version: 3 }),
      databaseSource,
      { isFocused: true, hasPending: false },
      'hello world'
    );
    expect(decision.apply).toBe(false);
    if (decision.apply) throw new Error('unreachable');
    expect(decision.stashVersion).toBe(false);
    expect(decision.notifyConflict).toBe(true);
  });
});

describe('shouldSkipStaleAiChatUpdate', () => {
  it('returns false for non-ai-chat nodes', () => {
    expect(
      shouldSkipStaleAiChatUpdate(
        makeNode({ nodeType: 'text' }),
        makeNode({ nodeType: 'text' }),
        databaseSource
      )
    ).toBe(false);
  });

  it('returns false for viewer-sourced updates', () => {
    const existing = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1, 2, 3] } as Node;
    const incoming = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1] } as Node;
    expect(shouldSkipStaleAiChatUpdate(incoming, existing, viewerSource)).toBe(false);
  });

  it('returns false when there is no existing node', () => {
    const incoming = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1] } as Node;
    expect(shouldSkipStaleAiChatUpdate(incoming, undefined, databaseSource)).toBe(false);
  });

  it('returns true when the incoming snapshot has fewer messages than the existing one', () => {
    const existing = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1, 2, 3] } as Node;
    const incoming = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1] } as Node;
    expect(shouldSkipStaleAiChatUpdate(incoming, existing, databaseSource)).toBe(true);
  });

  it('returns false when the incoming snapshot has the same or more messages', () => {
    const existing = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1, 2] } as Node;
    const incoming = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1, 2, 3] } as Node;
    expect(shouldSkipStaleAiChatUpdate(incoming, existing, databaseSource)).toBe(false);
  });

  it('treats a missing messages array as zero-length', () => {
    const existing = { ...makeNode({ nodeType: 'ai-chat' }), messages: [1] } as Node;
    const incoming = makeNode({ nodeType: 'ai-chat' });
    expect(shouldSkipStaleAiChatUpdate(incoming, existing, databaseSource)).toBe(true);
  });
});
