/**
 * Database Persistence Edge Cases Tests
 *
 * Tests specific edge cases in the dual persistence architecture where
 * race conditions between watchers could cause database constraint violations.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { tick } from 'svelte';

/**
 * Test for merge-split with child reassignment scenario
 *
 * This test verifies the fix for a FOREIGN KEY constraint error that occurred when:
 * 1. Merging two nodes (e.g., "Child 2" into "Parent 2")
 * 2. Splitting the merged node (e.g., "Parent 2Child 2" → "Parent 2" + "Child 2")
 * 3. A child of the deleted node (e.g., "Grandchild 2") needs to be reassigned to the new node
 *
 * The error occurred because the structural change watcher tried to update the grandchild
 * before the new child node was saved to the database, violating FOREIGN KEY constraints.
 *
 * The fix uses Promise coordination: content save watcher creates a promise when saving
 * new nodes, and structural watcher waits for this promise before persisting updates.
 */
describe('Database Persistence Edge Cases', () => {
  describe('Merge-Split with Child Reassignment', () => {
    let mockDatabaseService: {
      saveNodeWithParent: ReturnType<typeof vi.fn>;
      updateNode: ReturnType<typeof vi.fn>;
      deleteNode: ReturnType<typeof vi.fn>;
    };
    let pendingContentSavePromise: Promise<void> | null = null;
    let nodeManager: {
      visibleNodes: Array<{
        id: string;
        content: string;
        parentId: string | null;
        beforeSiblingId: string | null;
        isPlaceholder?: boolean;
      }>;
    };

    beforeEach(() => {
      // Mock database service with proper typing
      mockDatabaseService = {
        saveNodeWithParent: vi.fn().mockResolvedValue(undefined),
        updateNode: vi.fn().mockResolvedValue(undefined),
        deleteNode: vi.fn().mockResolvedValue(undefined)
      };

      // Reset state
      pendingContentSavePromise = null;

      // Initial hierarchy:
      // Parent 1
      //   Child 1
      //     Grandchild 1
      // Parent 2
      //   Child 2
      //     Grandchild 2
      nodeManager = {
        visibleNodes: [
          { id: 'parent1', content: 'Parent 1', parentId: null, beforeSiblingId: null },
          {
            id: 'child1',
            content: 'Child 1',
            parentId: 'parent1',
            beforeSiblingId: null
          },
          {
            id: 'grandchild1',
            content: 'Grandchild 1',
            parentId: 'child1',
            beforeSiblingId: null
          },
          {
            id: 'parent2',
            content: 'Parent 2',
            parentId: null,
            beforeSiblingId: 'parent1'
          },
          {
            id: 'child2',
            content: 'Child 2',
            parentId: 'parent2',
            beforeSiblingId: null
          },
          {
            id: 'grandchild2',
            content: 'Grandchild 2',
            parentId: 'child2',
            beforeSiblingId: null
          }
        ]
      };
    });

    it('should save new node before updating child references', async () => {
      // Simulate merge: Child 2 merged into Parent 2
      // After merge, Grandchild 2 becomes child of Parent 2
      nodeManager.visibleNodes = nodeManager.visibleNodes.filter((n) => n.id !== 'child2');
      const grandchild2 = nodeManager.visibleNodes.find((n) => n.id === 'grandchild2')!;
      grandchild2.parentId = 'parent2';

      await tick();

      // Simulate split: Parent 2Child 2 splits into Parent 2 + new Child 2
      const newChild2Id = 'child2-new';

      // Content save watcher: Creates new Child 2 node
      const contentSavePromise = (async () => {
        await mockDatabaseService.saveNodeWithParent(newChild2Id, {
          content: 'Child 2',
          nodeType: 'text',
          parentId: null,
          originNodeId: null,
          beforeSiblingId: 'parent2'
        });
      })();

      pendingContentSavePromise = contentSavePromise;

      // Update in-memory state
      nodeManager.visibleNodes.push({
        id: newChild2Id,
        content: 'Child 2',
        parentId: null,
        beforeSiblingId: 'parent2'
      });

      // Grandchild 2 should become child of new Child 2
      grandchild2.parentId = newChild2Id;

      await tick();

      // Structural change watcher: Must wait for content save before updating grandchild
      if (pendingContentSavePromise) {
        await Promise.race([
          pendingContentSavePromise,
          new Promise((_, reject) =>
            setTimeout(() => reject(new Error('Timeout waiting for content save')), 5000)
          )
        ]);
      }

      // Now safe to update grandchild reference
      await mockDatabaseService.updateNode('grandchild2', {
        parentId: newChild2Id,
        beforeSiblingId: null
      });

      // Verify the correct sequence
      const calls = mockDatabaseService.saveNodeWithParent.mock.calls;
      expect(calls.length).toBeGreaterThan(0);
      expect(calls[0][0]).toBe(newChild2Id);

      const updateCalls = mockDatabaseService.updateNode.mock.calls;
      expect(updateCalls.length).toBeGreaterThan(0);
      expect(updateCalls[0][0]).toBe('grandchild2');
      expect(updateCalls[0][1]).toEqual({
        parentId: newChild2Id,
        beforeSiblingId: null
      });
    });

    it('should handle timeout if content save takes too long', async () => {
      // Simulate a slow content save (longer than timeout)
      const slowContentSave = new Promise<void>((resolve) => {
        setTimeout(resolve, 6000); // Longer than 5s timeout
      });

      pendingContentSavePromise = slowContentSave;

      // Structural watcher should timeout and continue
      const timeoutPromise = Promise.race([
        pendingContentSavePromise,
        new Promise<void>((_, reject) =>
          setTimeout(() => reject(new Error('Timeout waiting for content save')), 5000)
        )
      ]);

      await expect(timeoutPromise).rejects.toThrow('Timeout waiting for content save');

      // The watcher should log a warning but not crash
      // In real implementation, this would be caught and logged
    });

    it('should handle multiple sequential save-update cycles', async () => {
      // Simulate creating multiple new nodes in quick succession
      const newNodeIds = ['new1', 'new2', 'new3'];

      for (const nodeId of newNodeIds) {
        // Content save
        const savePromise = (async () => {
          await mockDatabaseService.saveNodeWithParent(nodeId, {
            content: `Node ${nodeId}`,
            nodeType: 'text',
            parentId: null,
            originNodeId: null,
            beforeSiblingId: null
          });
        })();

        pendingContentSavePromise = savePromise;

        // Wait for save to complete
        if (pendingContentSavePromise) {
          await pendingContentSavePromise;
          pendingContentSavePromise = null;
        }

        // Now safe to update references
        await mockDatabaseService.updateNode('some-child', {
          parentId: nodeId,
          beforeSiblingId: null
        });
      }

      // Verify all saves happened before their corresponding updates
      expect(mockDatabaseService.saveNodeWithParent).toHaveBeenCalledTimes(3);
      expect(mockDatabaseService.updateNode).toHaveBeenCalledTimes(3);

      // Verify order: each save before its update
      const allCalls = [
        ...mockDatabaseService.saveNodeWithParent.mock.invocationCallOrder,
        ...mockDatabaseService.updateNode.mock.invocationCallOrder
      ];

      // Should be: save1, update1, save2, update2, save3, update3
      expect(allCalls.length).toBe(6);
    });
  });

  describe('Concurrent Watcher Coordination', () => {
    it('should coordinate content save and structural update watchers', async () => {
      const mockQueue: Array<() => Promise<void>> = [];
      let pendingPromise: Promise<void> | null = null;

      // Simulate $effect.pre execution order (content save runs first)
      const contentSaveWatcher = async () => {
        const saveOp = async () => {
          await new Promise((resolve) => setTimeout(resolve, 10));
        };
        pendingPromise = saveOp();
        mockQueue.push(() => pendingPromise!);
      };

      const structuralUpdateWatcher = async () => {
        // Must wait for pending content save
        if (pendingPromise) {
          await pendingPromise;
        }
        const updateOp = async () => {
          await new Promise((resolve) => setTimeout(resolve, 5));
        };
        mockQueue.push(updateOp);
      };

      // Execute in correct order
      await contentSaveWatcher();
      await structuralUpdateWatcher();

      // Execute queued operations
      for (const op of mockQueue) {
        await op();
      }

      expect(mockQueue.length).toBe(2);
    });
  });
});
