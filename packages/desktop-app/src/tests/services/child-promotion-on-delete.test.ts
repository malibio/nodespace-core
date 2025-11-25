/**
 * Tests for child promotion behavior when nodes are deleted via Backspace merge
 *
 * When a node is deleted (merged into previous node via Backspace), its children
 * should be promoted to the deleted node's parent level, NOT transferred to the
 * merged-into node. This maintains the document's depth structure.
 *
 * Example scenario tested:
 * ```
 * Starting:                After deleting E (merges into D):
 * - A                      - A
 *   - B                      - B
 *     - C                      - C
 *       - D                      - DE (content merged)
 *     - E <- delete            - F  (promoted to B's children)
 *       - F                      - G (maintains relative depth)
 *         - G
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
    test('children should be promoted to deleted node parent level, not merged-into node', () => {
      // Create the hierarchy:
      // A (root)
      //   └── B (depth 1)
      //         ├── C (depth 2)
      //         │     └── D (depth 3)
      //         └── E (depth 2, sibling of C)
      //               └── F (depth 3)
      //                     └── G (depth 4)
      //
      // When E is deleted (merged into D), F should become a child of B (E's parent)
      // NOT a child of D (the merged-into node)

      // Set up hierarchy in structureTree (simulates backend LIVE SELECT events)
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

      // Initialize with parentMapping for test scenario
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

      // CRITICAL: F should now be a child of B (E's parent), NOT D
      // F's depth should be 2 (same as E was - promoted to E's level)
      const fDepthAfter = nodeManager.getUIState('node-f')?.depth;
      expect(fDepthAfter).toBe(2); // Same as E was before deletion

      // G should maintain its relative depth to F
      const gDepthAfter = nodeManager.getUIState('node-g')?.depth;
      expect(gDepthAfter).toBe(3); // Was 4, now 3 (one level up)
    });

    test('children should become root nodes when parent node at root is deleted', () => {
      // Structure:
      // C (root)
      // A (root) <- will be deleted
      //   └── B (child of A)
      //
      // When A is deleted (merged into C), B should become a root node

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

      // B should now be at root level (depth 0)
      const bDepthAfter = nodeManager.getUIState('node-b')?.depth;
      expect(bDepthAfter).toBe(0);
    });

    test('depth should be adjusted correctly when children are promoted', () => {
      // Structure:
      // A (depth 0)
      //   └── B (depth 1)
      //         ├── C (depth 2)
      //         │     └── D (depth 3)
      //         └── E (depth 2)
      //               └── F (depth 3)
      //
      // When E is deleted (merged into D), F should go to depth 2 (E's level)

      const parentMapping = setupHierarchy({
        'node-a': null,
        'node-b': 'node-a',
        'node-c': 'node-b',
        'node-d': 'node-c',
        'node-e': 'node-b',
        'node-f': 'node-e'
      });

      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeC = createTestNode('node-c', 'C', 'text', 'node-b');
      const nodeD = createTestNode('node-d', 'D', 'text', 'node-c');
      const nodeE = createTestNode('node-e', 'E', 'text', 'node-b');
      const nodeF = createTestNode('node-f', 'F', 'text', 'node-e');

      nodeManager.initializeNodes([nodeA, nodeB, nodeC, nodeD, nodeE, nodeF], {
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        parentMapping
      });

      // Get initial depths
      const initialEDepth = nodeManager.getUIState('node-e')?.depth;
      const initialFDepth = nodeManager.getUIState('node-f')?.depth;

      expect(initialEDepth).toBe(2);
      expect(initialFDepth).toBe(3);

      // Delete E by combining with D
      nodeManager.combineNodes('node-e', 'node-d');

      // F should now be at depth 2 (same as E was - promoted to E's parent level)
      const finalFDepth = nodeManager.getUIState('node-f')?.depth;
      expect(finalFDepth).toBe(2);
    });

    test('multiple children should all be promoted correctly', () => {
      // Structure:
      // A (root)
      //   └── B (depth 1) <- will be deleted
      //         ├── C (depth 2, child 1 of B)
      //         ├── D (depth 2, child 2 of B)
      //         └── E (depth 2, child 3 of B)
      // P (root, merge target)
      //
      // When B is deleted (merged into A), C, D, E should become children of A

      const parentMapping = setupHierarchy({
        'node-p': null,
        'node-a': null,
        'node-b': 'node-a',
        'node-c': 'node-b',
        'node-d': 'node-b',
        'node-e': 'node-b'
      });

      const nodeP = createTestNode('node-p', 'Previous', 'text', null);
      const nodeA = createTestNode('node-a', 'A', 'text', null);
      const nodeB = createTestNode('node-b', 'B', 'text', 'node-a');
      const nodeC = createTestNode('node-c', 'C', 'text', 'node-b');
      const nodeD = createTestNode('node-d', 'D', 'text', 'node-b');
      const nodeE = createTestNode('node-e', 'E', 'text', 'node-b');

      nodeManager.initializeNodes([nodeP, nodeA, nodeB, nodeC, nodeD, nodeE], {
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

      // All children of B should now be at depth 1 (B's level under A)
      expect(nodeManager.getUIState('node-c')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-d')?.depth).toBe(1);
      expect(nodeManager.getUIState('node-e')?.depth).toBe(1);
    });
  });
});
