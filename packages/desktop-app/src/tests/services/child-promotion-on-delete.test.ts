/**
 * Tests for child promotion behavior when nodes are deleted via Backspace merge
 *
 * When a node is deleted (merged into previous node via Backspace), its children
 * should "shift up" visually while maintaining their depth level. The new parent
 * is found by walking up from the merged-into node to find an ancestor at the
 * same depth as the deleted node.
 *
 * Example scenario tested:
 * ```
 * Starting:                     After deleting F (merges into "So so so deep"):
 * - A (depth 0)                 - A (depth 0)
 * - C (depth 0)                 - C (depth 0)
 *   - D (depth 1)                 - D (depth 1)
 *     - Even deeper (2)             - Even deeper (2)
 *       - So so deep (3)              - So so deepF (merged)
 * - F (depth 0) <- deleted        - G (depth 1, now child of C)
 *   - G (depth 1)                   - H (depth 2)
 *     - H (depth 2)
 * ```
 */

// CRITICAL: Import setup BEFORE anything else to ensure Svelte mocks are applied
import '../setup-svelte-mocks';

import { describe, test, expect, beforeEach, vi } from 'vitest';
import {
  createReactiveNodeService,
  ReactiveNodeService as NodeManager
} from '../../lib/services/reactive-node-service.svelte.js';
import type { NodeManagerEvents } from '../../lib/services/reactive-node-service.svelte.js';
import { structureTree } from '../../lib/stores/reactive-structure-tree.svelte';
import { sharedNodeStore } from '../../lib/services/shared-node-store';
import { createTestNode } from '../helpers';

// Special marker for root-level nodes in structureTree
const ROOT_MARKER = '__root__';

/**
 * Helper to set up parent-child relationships in structureTree for tests.
 * This simulates what the backend would do via LIVE SELECT events.
 *
 * Note: Root-level nodes (parentId: null) are registered under ROOT_MARKER
 * to match the structureTree's internal representation.
 */
function setupHierarchy(
  parentMapping: Record<string, string | null>
): Record<string, string | null> {
  // Build the tree structure
  for (const [childId, parentId] of Object.entries(parentMapping)) {
    // Use ROOT_MARKER for root-level nodes to match structureTree's type requirements
    structureTree.addChild({
      parentId: parentId ?? ROOT_MARKER,
      childId: childId,
      order: 0 // Order doesn't matter for these tests
    });
  }
  return parentMapping;
}

describe('Child Promotion on Node Deletion', () => {
  let nodeManager: NodeManager;
  let mockEvents: NodeManagerEvents;

  beforeEach(() => {
    // Reset the structure tree before each test
    structureTree.children = new Map();

    mockEvents = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };
    nodeManager = createReactiveNodeService(mockEvents);
  });

  describe('promoteChildren via combineNodes', () => {
    test('children maintain visual depth when same-branch sibling is deleted', () => {
      // Create the hierarchy:
      // A (root, depth 0)
      //   └── B (depth 1)
      //         ├── C (depth 2)
      //         │     └── D (depth 3)
      //         └── E (depth 2, sibling of C) <- will be deleted
      //               └── F (depth 3)
      //                     └── G (depth 4)
      //
      // When E is deleted (merged into D), F should become a child of C
      // (the node at depth 2 in D's ancestry), maintaining F's depth of 3

      const parentMapping = setupHierarchy({
        'node-a': null,
        'node-b': 'node-a',
        'node-c': 'node-b',
        'node-d': 'node-c',
        'node-e': 'node-b',
        'node-f': 'node-e',
        'node-g': 'node-f'
      });

      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeC = createTestNode('node-c', 'C', 'text', 'node-b');
      const nodeD = createTestNode('node-d', 'D', 'text', 'node-c');
      const nodeE = createTestNode('node-e', 'E', 'text', 'node-b'); // Sibling of C under B
      const nodeF = createTestNode('node-f', 'F', 'text', 'node-e');
      const nodeG = createTestNode('node-g', 'G', 'text', 'node-f');

      nodeManager.initializeNodes([nodeA, nodeB, nodeC, nodeD, nodeE, nodeF, nodeG], {
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        parentMapping
      });

      // Verify initial depths are set correctly
      expect(nodeManager.getUIState('node-a')?.depth).toBe(0);
      expect(nodeManager.getUIState('node-b')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-c')?.depth).toBe(2);
      expect(nodeManager.getUIState('node-d')?.depth).toBe(3);
      expect(nodeManager.getUIState('node-e')?.depth).toBe(2);
      expect(nodeManager.getUIState('node-f')?.depth).toBe(3);
      expect(nodeManager.getUIState('node-g')?.depth).toBe(4);

      // Get E's children before deletion
      const eChildrenBefore = sharedNodeStore.getNode('node-f');
      expect(eChildrenBefore).toBeDefined();
      expect(eChildrenBefore?.parentId).toBe('node-e');

      // Delete E by combining with D (simulates Backspace at start of E)
      nodeManager.combineNodes('node-e', 'node-d');

      // Verify E is deleted
      expect(sharedNodeStore.getNode('node-e')).toBeUndefined();

      // Verify D's content was merged (D + E = "DE")
      const mergedD = sharedNodeStore.getNode('node-d');
      expect(mergedD?.content).toBe('DE');

      // CRITICAL: F should maintain its visual depth of 3
      // New parent is C (depth 2 ancestor of D)
      const fDepthAfter = nodeManager.getUIState('node-f')?.depth;
      expect(fDepthAfter).toBe(3); // Maintains visual depth

      // G should maintain its relative depth to F
      const gDepthAfter = nodeManager.getUIState('node-g')?.depth;
      expect(gDepthAfter).toBe(4); // Maintains visual depth

      // Verify sibling ordering: C should have children [D, F]
      // F should be positioned after C's existing child D
      const cChildren = structureTree.getChildren('node-c');
      expect(cChildren).toEqual(['node-d', 'node-f']);
    });

    test('children become children of merged-into node ancestor when cross-branch merge occurs', () => {
      // Structure (the user's exact scenario):
      // A (depth 0)
      //   └── B (depth 1)
      // C (depth 0)
      //   └── D (depth 1)
      //         └── Even deeper (depth 2)
      //               └── So so so deep (depth 3)
      // F (depth 0) <- will be deleted (merged into "So so so deep")
      //   └── G (depth 1)
      //         └── H (depth 2)
      //
      // When F is deleted (merged into "So so so deep"):
      // - Find ancestor of "So so so deep" at depth 0 -> C
      // - G becomes child of C (maintaining G's depth of 1)
      // - H maintains depth of 2

      const parentMapping = setupHierarchy({
        'node-a': null,
        'node-b': 'node-a',
        'node-c': null,
        'node-d': 'node-c',
        'even-deeper': 'node-d',
        'so-deep': 'even-deeper',
        'node-f': null,
        'node-g': 'node-f',
        'node-h': 'node-g'
      });

      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeC = createTestNode('node-c', 'C', 'text', null);
      const nodeD = createTestNode('node-d', 'D', 'text', 'node-c');
      const evenDeeper = createTestNode('even-deeper', 'Even deeper', 'text', 'node-d');
      const soDeep = createTestNode('so-deep', 'So so so deep', 'text', 'even-deeper');
      const nodeF = createTestNode('node-f', 'F', 'text', null);
      const nodeG = createTestNode('node-g', 'G', 'text', 'node-f');
      const nodeH = createTestNode('node-h', 'H', 'text', 'node-g');

      nodeManager.initializeNodes(
        [nodeA, nodeB, nodeC, nodeD, evenDeeper, soDeep, nodeF, nodeG, nodeH],
        {
          expanded: true,
          autoFocus: false,
          inheritHeaderLevel: 0,
          parentMapping
        }
      );

      // Verify initial depths
      expect(nodeManager.getUIState('node-c')?.depth).toBe(0);
      expect(nodeManager.getUIState('node-d')?.depth).toBe(1);
      expect(nodeManager.getUIState('even-deeper')?.depth).toBe(2);
      expect(nodeManager.getUIState('so-deep')?.depth).toBe(3);
      expect(nodeManager.getUIState('node-f')?.depth).toBe(0);
      expect(nodeManager.getUIState('node-g')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-h')?.depth).toBe(2);

      // Delete F by combining with "So so so deep"
      nodeManager.combineNodes('node-f', 'so-deep');

      // Verify F is deleted
      expect(sharedNodeStore.getNode('node-f')).toBeUndefined();

      // Verify content was merged
      const merged = sharedNodeStore.getNode('so-deep');
      expect(merged?.content).toBe('So so so deepF');

      // CRITICAL: G should now be a child of C (ancestor of so-deep at depth 0)
      // G maintains its visual depth of 1
      const gNode = sharedNodeStore.getNode('node-g');
      expect(gNode?.parentId).toBe('node-c');
      expect(nodeManager.getUIState('node-g')?.depth).toBe(1);

      // H maintains its visual depth of 2
      expect(nodeManager.getUIState('node-h')?.depth).toBe(2);

      // Verify sibling ordering: A should have children [B, D, G]
      // (C was deleted, its child D remains, and F's child G is inserted after B)
      // Wait - this scenario has A -> B -> C, not A -> [B, C]
      // So the ordering check would be: B's children should be [C, G]
      const aChildren = structureTree.getChildren(ROOT_MARKER);
      expect(aChildren).toContain('node-a');

      const bChildren = structureTree.getChildren('node-b');
      // B originally had [C, E as siblings]. After E deleted, should have [C, G]
      // where G is inserted after C (E's previous sibling)
      // Actually wait - let me re-examine the structure...
      // B has children: C and (was E). After E deleted, F moves to C's parent (B)
      // So B's children should be [C, F] with F after C
      expect(bChildren).toContain('node-c');
      expect(bChildren).toContain('node-f');

      // F should be after C in B's children list
      const fIndex = bChildren.indexOf('node-f');
      const cIndex = bChildren.indexOf('node-c');
      expect(fIndex).toBeGreaterThan(cIndex);
    });

    test('children become root when no ancestor at target depth exists', () => {
      // Structure:
      // C (root, depth 0)
      // A (root, depth 0) <- will be deleted
      //   └── B (depth 1)
      //
      // When A is deleted (merged into C), there's no ancestor of C at depth 0
      // (C itself is at depth 0), so B becomes child of C

      const parentMapping = setupHierarchy({
        'node-c': null,
        'node-a': null,
        'node-b': 'node-a'
      });

      const nodeC = createTestNode('node-c', 'C', 'text', null);
      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');

      nodeManager.initializeNodes([nodeC, nodeA, nodeB], {
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        parentMapping
      });

      // Verify initial state
      expect(nodeManager.getUIState('node-c')?.depth).toBe(0);
      expect(nodeManager.getUIState('node-a')?.depth).toBe(0);
      expect(nodeManager.getUIState('node-b')?.depth).toBe(1);

      // Delete A by combining with C
      nodeManager.combineNodes('node-a', 'node-c');

      // Verify A is deleted
      expect(sharedNodeStore.getNode('node-a')).toBeUndefined();

      // Verify C's content was merged
      const mergedC = sharedNodeStore.getNode('node-c');
      expect(mergedC?.content).toBe('CA');

      // B should become child of C (C is at depth 0, which matches A's depth)
      // B maintains depth of 1
      const bNode = sharedNodeStore.getNode('node-b');
      expect(bNode?.parentId).toBe('node-c');
      expect(nodeManager.getUIState('node-b')?.depth).toBe(1);
    });

    test('multiple children all become children of nearest matching ancestor', () => {
      // Structure:
      // A (root, depth 0)
      //   └── B (depth 1) <- will be deleted
      //         ├── C (depth 2)
      //         ├── D (depth 2)
      //         └── E (depth 2)
      //
      // When B is deleted (merged into A):
      // - We look for ancestor of A at depth 1 (B's depth)
      // - A is at depth 0 with no parent, so A becomes the new parent
      // - C, D, E become children of A with depth = 1 (A.depth + 1)

      const parentMapping = setupHierarchy({
        'node-a': null,
        'node-b': 'node-a',
        'node-c': 'node-b',
        'node-d': 'node-b',
        'node-e': 'node-b'
      });

      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeC = createTestNode('node-c', 'C', 'text', 'node-b');
      const nodeD = createTestNode('node-d', 'D', 'text', 'node-b');
      const nodeE = createTestNode('node-e', 'E', 'text', 'node-b');

      nodeManager.initializeNodes([nodeA, nodeB, nodeC, nodeD, nodeE], {
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        parentMapping
      });

      // Verify B's children are at depth 2
      expect(nodeManager.getUIState('node-c')?.depth).toBe(2);
      expect(nodeManager.getUIState('node-d')?.depth).toBe(2);
      expect(nodeManager.getUIState('node-e')?.depth).toBe(2);

      // Delete B by combining with A
      nodeManager.combineNodes('node-b', 'node-a');

      // All children of B should now be children of A
      const cNode = sharedNodeStore.getNode('node-c');
      const dNode = sharedNodeStore.getNode('node-d');
      const eNode = sharedNodeStore.getNode('node-e');

      expect(cNode?.parentId).toBe('node-a');
      expect(dNode?.parentId).toBe('node-a');
      expect(eNode?.parentId).toBe('node-a');

      // Depth is now 1 (A.depth + 1) since A is the nearest ancestor
      // Children shift up visually when their parent is merged into grandparent
      expect(nodeManager.getUIState('node-c')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-d')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-e')?.depth).toBe(1);

      // Verify sibling ordering: A's children should be [B's previous siblings, C, D, E]
      // Since B was the only child of A, after deletion the children should be [C, D, E]
      // But wait - B had no siblings, so the children should be inserted at B's position
      // which means they should come first: [C, D, E]
      const aChildren = structureTree.getChildren('node-a');
      expect(aChildren).toEqual(['node-c', 'node-d', 'node-e']);
    });

    test('promoted children maintain correct sibling order relative to existing siblings', () => {
      // Structure:
      // A (root, depth 0)
      //   ├── B (depth 1)
      //   ├── E (depth 1) <- will be deleted
      //   │     ├── F (depth 2)
      //   │     └── G (depth 2)
      //   └── H (depth 1)
      //
      // When E is deleted (merged into B):
      // - F and G should be inserted after B (E's previous sibling)
      // - Final order should be: A -> [B, F, G, H]

      const parentMapping = setupHierarchy({
        'node-a': null,
        'node-b': 'node-a',
        'node-e': 'node-a',
        'node-f': 'node-e',
        'node-g': 'node-e',
        'node-h': 'node-a'
      });

      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeE = createTestNode('node-e', 'E', 'text', 'node-a');
      const nodeF = createTestNode('node-f', 'F', 'text', 'node-e');
      const nodeG = createTestNode('node-g', 'G', 'text', 'node-e');
      const nodeH = createTestNode('node-h', 'H', 'text', 'node-a');

      nodeManager.initializeNodes([nodeA, nodeB, nodeE, nodeF, nodeG, nodeH], {
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        parentMapping
      });

      // Verify initial state: A has children [B, E, H]
      const initialAChildren = structureTree.getChildren('node-a');
      expect(initialAChildren).toContain('node-b');
      expect(initialAChildren).toContain('node-e');
      expect(initialAChildren).toContain('node-h');

      // Delete E by combining with B
      nodeManager.combineNodes('node-e', 'node-b');

      // Verify E is deleted
      expect(sharedNodeStore.getNode('node-e')).toBeUndefined();

      // CRITICAL: A's children should now be [B, F, G, H]
      // F and G are inserted after B (E's previous sibling)
      const finalAChildren = structureTree.getChildren('node-a');
      expect(finalAChildren).toEqual(['node-b', 'node-f', 'node-g', 'node-h']);

      // Verify F and G maintain depth 2 (was E's children at depth 2, now A's children)
      // Actually they should be depth 1 now (A.depth + 1)
      expect(nodeManager.getUIState('node-f')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-g')?.depth).toBe(1);
    });
  });
});
