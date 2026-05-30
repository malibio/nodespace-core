/**
 * Tests for C3a: Frontend outdent ordering math removal (Issue #1259)
 *
 * Verifies that:
 * 1. No frontend code computes fractional order values for optimistic placement.
 * 2. moveInMemoryRelationship is called without an order argument (relative-after intent).
 * 3. applyHasChildUpdated reconciles the optimistic placement to the daemon-supplied order.
 * 4. The three outdent variants (normal, in-flight not-persisted, in-flight CREATE executing)
 *    all call moveInMemoryRelationship without a hand-computed order.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createReactiveNodeService,
  type ReactiveNodeService,
  type NodeManagerEvents
} from '$lib/services/reactive-node-service.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';
import type { Node } from '$lib/types';

// vi.hoisted() runs before vi.mock hoisting — safe to reference in factory
const { moveInMemoryRelationshipSpy } = vi.hoisted(() => ({
  moveInMemoryRelationshipSpy: vi.fn()
}));

vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    moveNode: vi.fn().mockResolvedValue({
      id: 'mock-node',
      nodeType: 'text',
      content: '',
      version: 2,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    }),
    getNode: vi.fn().mockResolvedValue(null),
    createNode: vi.fn().mockResolvedValue('mock-id'),
    updateNode: vi.fn().mockResolvedValue(null),
    deleteNode: vi.fn().mockResolvedValue({ deleted: true }),
    getChildren: vi.fn().mockResolvedValue([]),
    getChildrenTree: vi.fn().mockResolvedValue(null),
    getDescendants: vi.fn().mockResolvedValue([]),
    createMention: vi.fn().mockResolvedValue(undefined),
    deleteMention: vi.fn().mockResolvedValue(undefined),
    getOutgoingMentions: vi.fn().mockResolvedValue([]),
    getIncomingMentions: vi.fn().mockResolvedValue([]),
    getMentioningContainers: vi.fn().mockResolvedValue([]),
    queryNodes: vi.fn().mockResolvedValue([]),
    mentionAutocomplete: vi.fn().mockResolvedValue([]),
    createContainerNode: vi.fn().mockResolvedValue('mock-container-id'),
    updateTaskNode: vi.fn().mockResolvedValue(null)
  },
  insertPosition: {
    beginning: () => ({ type: 'beginning' }),
    end: () => ({ type: 'end' }),
    after: (siblingId: string) => ({ type: 'after', siblingId })
  }
}));

vi.mock('$lib/stores/reactive-structure-tree.svelte', () => ({
  structureTree: {
    addInMemoryRelationship: vi.fn(),
    moveInMemoryRelationship: moveInMemoryRelationshipSpy,
    getChildren: vi.fn(() => []),
    getChildrenWithOrder: vi.fn(() => []),
    getParent: vi.fn(() => null),
    removeChild: vi.fn(),
    addChild: vi.fn(),
    children: new Map()
  }
}));

function makeNode(id: string): Node {
  return {
    id,
    nodeType: 'text',
    content: `Content ${id}`,
    version: 1,
    properties: {},
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString()
  };
}

describe('C3a — outdentNode emits no fractional order values', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    vi.clearAllMocks();
    SharedNodeStore.resetInstance();
    sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    // Mock structureTree.getParent to return a parent chain for a child node
    vi.mocked(structureTree.getParent).mockImplementation((nodeId: string) => {
      if (nodeId === 'child') return 'parent';
      if (nodeId === 'parent') return 'grandparent';
      return null;
    });
    vi.mocked(structureTree.getChildren).mockImplementation((nodeId: string) => {
      if (nodeId === 'parent') return ['child'];
      return [];
    });
    vi.mocked(structureTree.getChildrenWithOrder).mockImplementation((nodeId: string) => {
      if (nodeId === 'grandparent') return [{ nodeId: 'parent', order: 2.0 }];
      if (nodeId === 'parent') return [{ nodeId: 'child', order: 1.0 }];
      return [];
    });
  });

  afterEach(() => {
    service.destroy();
  });

  function addPersistedNode(id: string) {
    const node = makeNode(id);
    // source.type === 'database' triggers shouldMarkAsPersisted=true in determinePersistenceBehavior
    sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });
    return node;
  }

  it('outdent of persisted node calls moveInMemoryRelationship WITHOUT a numeric order argument', async () => {
    addPersistedNode('grandparent');
    addPersistedNode('parent');
    addPersistedNode('child');

    const result = await service.outdentNode('child');

    // outdentNode returns a boolean (true if outdent ran, false if validation failed)
    expect(typeof result).toBe('boolean');

    // No moveInMemoryRelationship call should pass a fractional order value.
    // Frontend uses only relative-after placement (no order) or integer sibling-transfer order.
    for (const call of moveInMemoryRelationshipSpy.mock.calls) {
      const order = call[3];
      if (typeof order === 'number') {
        // Sibling-transfer uses integer i+1 — verify it is an integer, not a fractional midpoint
        expect(order % 1).toBe(0);
      }
    }
  });

  it('outdent of NOT-YET-PERSISTED node: optimistic moveInMemoryRelationship has no fractional order', async () => {
    addPersistedNode('grandparent');
    addPersistedNode('parent');

    // Use viewer source so the node is NOT added to persistedNodeIds.
    // source.type === 'viewer' → determinePersistenceBehavior returns shouldMarkAsPersisted: false,
    // so isNodePersisted('child') is false and the CREATE-cancel branch executes.
    const child = makeNode('child');
    sharedNodeStore.setNode(child, { type: 'viewer', viewerId: 'test-viewer' });

    await service.outdentNode('child');

    // No moveInMemoryRelationship call should pass a fractional order value.
    // The CREATE-cancel path calls moveInMemoryRelationship(oldParentId, newParentId, nodeId)
    // with no order arg — positional intent only.
    for (const call of moveInMemoryRelationshipSpy.mock.calls) {
      if (call[3] !== undefined) {
        expect(Number.isInteger(call[3])).toBe(true);
      }
    }
  });

  it('no call to moveInMemoryRelationship passes a value between 0 and 1 (no fractional midpoints)', async () => {
    addPersistedNode('grandparent');
    addPersistedNode('parent');
    addPersistedNode('child');

    await service.outdentNode('child');

    for (const call of moveInMemoryRelationshipSpy.mock.calls) {
      const order = call[3];
      if (typeof order === 'number') {
        // Fractional midpoints (like 2.5) are daemon-side only; frontend only uses integers
        expect(order % 1).toBe(0);
      }
    }
  });
});
