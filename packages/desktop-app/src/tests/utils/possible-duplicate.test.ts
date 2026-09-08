/**
 * isPossibleDuplicate — read-side accessor for the convergence "possible
 * duplicate" marker (ADR-065 §4).
 *
 * Mirrors NodeService::is_possible_duplicate (core) on the Rust side: both
 * read properties.<nodeType>._possible_duplicate. These tests prove the
 * frontend accessor agrees with the backend's write shape (mark_possible_duplicates
 * always writes a JSON boolean `true`, never a string/number, and never sets
 * it on a node type other than the node's own).
 */
import { describe, it, expect } from 'vitest';
import type { Node } from '$lib/types';
import { isPossibleDuplicate, POSSIBLE_DUPLICATE_FIELD } from '$lib/utils/possible-duplicate';

function personNode(properties: Record<string, unknown> = {}): Node {
  return {
    id: 'person-1',
    nodeType: 'person',
    content: 'Alice',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties,
    ...({} as Partial<Node>)
  } as Node;
}

describe('isPossibleDuplicate', () => {
  it('is false when the node has no properties at all under its own type', () => {
    expect(isPossibleDuplicate(personNode({}))).toBe(false);
  });

  it('is false when the marker property is simply absent', () => {
    expect(isPossibleDuplicate(personNode({ person: { name: 'Alice', email: 'a@x.com' } }))).toBe(
      false
    );
  });

  it('is false when the marker is explicitly written as false', () => {
    expect(
      isPossibleDuplicate(personNode({ person: { name: 'Alice', [POSSIBLE_DUPLICATE_FIELD]: false } }))
    ).toBe(false);
  });

  it('is false for a truthy-but-non-boolean marker value (never coerces)', () => {
    expect(
      isPossibleDuplicate(personNode({ person: { [POSSIBLE_DUPLICATE_FIELD]: 'true' } }))
    ).toBe(false);
    expect(isPossibleDuplicate(personNode({ person: { [POSSIBLE_DUPLICATE_FIELD]: 1 } }))).toBe(
      false
    );
  });

  it('is true when the marker is set true under the node\'s own type namespace', () => {
    expect(
      isPossibleDuplicate(personNode({ person: { name: 'Alice', [POSSIBLE_DUPLICATE_FIELD]: true } }))
    ).toBe(true);
  });

  it('is generic across node types — reads properties.<nodeType>, not a hardcoded "person"', () => {
    const orgNode = {
      id: 'org-1',
      nodeType: 'organization',
      content: 'Acme',
      createdAt: '2026-01-01T00:00:00Z',
      modifiedAt: '2026-01-01T00:00:00Z',
      version: 1,
      properties: { organization: { [POSSIBLE_DUPLICATE_FIELD]: true } }
    } as Node;
    expect(isPossibleDuplicate(orgNode)).toBe(true);
  });

  it('does not match a marker stashed under the WRONG type namespace', () => {
    // A marker sitting under a different key than this node's own nodeType
    // must never count — it isn't this node's marker.
    expect(
      isPossibleDuplicate(personNode({ organization: { [POSSIBLE_DUPLICATE_FIELD]: true } }))
    ).toBe(false);
  });

  it('is false for a null or undefined node', () => {
    expect(isPossibleDuplicate(null)).toBe(false);
    expect(isPossibleDuplicate(undefined)).toBe(false);
  });
});
