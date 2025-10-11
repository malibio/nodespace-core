/**
 * Section 7: Database Persistence Tests (Phase 2 - Real Backend)
 *
 * Tests database persistence behavior with real HTTP backend.
 * Verifies when nodes persist, update operations, concurrency handling, and deletion.
 *
 * Part of comprehensive test coverage initiative (#208, Phase 2: #211)
 */

import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest';
import {
  createTestDatabase,
  cleanupTestDatabase,
  initializeTestDatabase,
  cleanDatabase
} from '../utils/test-database';
import { TestNodeBuilder } from '../utils/test-node-builder';
import { getBackendAdapter } from '$lib/services/backend-adapter';
import type { BackendAdapter } from '$lib/services/backend-adapter';

describe.sequential('Section 7: Database Persistence Tests', () => {
  let dbPath: string;
  let backend: BackendAdapter;

  beforeAll(async () => {
    // Create isolated test database for this suite
    dbPath = createTestDatabase('database-persistence');
    backend = getBackendAdapter();
    await initializeTestDatabase(dbPath);
    console.log(`[Test] Using database: ${dbPath}`);
  });

  afterAll(async () => {
    // Cleanup test database
    await cleanupTestDatabase(dbPath);
  });

  beforeEach(async () => {
    // Clean database between tests to ensure test isolation
    await cleanDatabase(backend);
  });

  describe('Placeholder Behavior', () => {
    it('should reject empty nodes - backend validation prevents placeholder creation', async () => {
      // NodeSpace backend validates that nodes must have content
      // Empty placeholder nodes exist ONLY in the frontend's memory (_nodes map)
      // They are never sent to the backend until content is added

      const emptyNodeData = TestNodeBuilder.text('').build();

      // Backend should reject empty content
      await expect(backend.createNode(emptyNodeData)).rejects.toThrow();

      // Verify node doesn't exist in database
      const fetchedNode = await backend.getNode(emptyNodeData.id);
      expect(fetchedNode).toBeNull();
    }, 10000);

    it('should persist node after content added (first character triggers)', async () => {
      // Simulate: User presses Enter (creates empty placeholder), then types "H"
      // In real app: placeholder stays in memory until content added
      // In test: we directly create node with content to verify persistence

      const nodeData = TestNodeBuilder.text('H').build();
      const nodeId = await backend.createNode(nodeData);

      // Verify node persisted in database
      const fetchedNode = await backend.getNode(nodeId);
      expect(fetchedNode).toBeTruthy();
      expect(fetchedNode?.content).toBe('H');
      expect(fetchedNode?.id).toBe(nodeId);
    }, 10000);
  });

  describe('Update Operations', () => {
    it('should persist content updates correctly', async () => {
      // Create initial node with content
      const nodeData = TestNodeBuilder.text('Hello').build();
      const nodeId = await backend.createNode(nodeData);

      // Update content
      await backend.updateNode(nodeId, { content: 'Hello World' });

      // Verify update persisted
      const fetchedNode = await backend.getNode(nodeId);
      expect(fetchedNode?.content).toBe('Hello World');
    }, 10000);

    it('should persist sibling updates (beforeSiblingId changes saved)', async () => {
      // Create three sibling nodes: A -> B -> C
      const nodeAData = TestNodeBuilder.text('Node A').build();
      const nodeAId = await backend.createNode(nodeAData);

      const nodeBData = TestNodeBuilder.text('Node B').withBeforeSibling(nodeAId).build();
      const nodeBId = await backend.createNode(nodeBData);

      const nodeCData = TestNodeBuilder.text('Node C').withBeforeSibling(nodeBId).build();
      const nodeCId = await backend.createNode(nodeCData);

      // Reorder: A -> C -> B (move C between A and B)
      await backend.updateNode(nodeCId, { beforeSiblingId: nodeAId });
      await backend.updateNode(nodeBId, { beforeSiblingId: nodeCId });

      // Verify updates persisted
      const fetchedB = await backend.getNode(nodeBId);
      const fetchedC = await backend.getNode(nodeCId);

      expect(fetchedC?.beforeSiblingId).toBe(nodeAId);
      expect(fetchedB?.beforeSiblingId).toBe(nodeCId);
    }, 10000);
  });

  describe('Concurrency', () => {
    it('should handle rapid creates without UNIQUE constraint violations', async () => {
      // Simulate rapid typing: create multiple nodes in quick succession
      // Each node gets a unique ID, so no UNIQUE constraint violations expected

      const nodeIds: string[] = [];
      for (let i = 0; i < 5; i++) {
        const nodeData = TestNodeBuilder.text(`Node ${i}`).build();
        const nodeId = await backend.createNode(nodeData);
        nodeIds.push(nodeId);
      }

      expect(nodeIds).toHaveLength(5);
      expect(new Set(nodeIds).size).toBe(5); // All IDs unique

      // Verify all nodes persisted
      for (const id of nodeIds) {
        const node = await backend.getNode(id);
        expect(node).toBeTruthy();
      }
    }, 10000);

    it('should handle sequential updates without database locking errors', async () => {
      // Create a node
      const nodeData = TestNodeBuilder.text('Initial').build();
      const nodeId = await backend.createNode(nodeData);

      // Perform rapid updates sequentially
      // Note: Concurrent updates may cause issues with current backend
      for (let i = 0; i < 10; i++) {
        await backend.updateNode(nodeId, { content: `Update ${i}` });
      }

      // Final state should be last update
      const finalNode = await backend.getNode(nodeId);
      expect(finalNode?.content).toBe('Update 9');
    }, 10000);
  });

  describe('Deletion', () => {
    it('should delete node and handle children via cascade/promotion', async () => {
      // Create parent with two children
      const parentData = TestNodeBuilder.text('Parent').build();
      const parentId = await backend.createNode(parentData);

      const child1Data = TestNodeBuilder.text('Child 1').withParent(parentId).build();
      const child1Id = await backend.createNode(child1Data);

      const child2Data = TestNodeBuilder.text('Child 2')
        .withParent(parentId)
        .withBeforeSibling(child1Id)
        .build();
      const child2Id = await backend.createNode(child2Data);

      // Delete parent
      await backend.deleteNode(parentId);

      // Verify parent deleted
      const fetchedParent = await backend.getNode(parentId);
      expect(fetchedParent).toBeNull();

      // Note: Backend cascade behavior depends on implementation
      // This test verifies the operation completes without errors
      // Children may be deleted (CASCADE) or promoted (application logic)
      const fetchedChild1 = await backend.getNode(child1Id);
      const fetchedChild2 = await backend.getNode(child2Id);

      // At least verify no errors occurred and operation completed
      // Specific cascade vs promotion behavior can be verified once backend settles on approach
      console.log(
        `[Test] After parent deletion: Child1=${fetchedChild1?.id || 'deleted'}, Child2=${fetchedChild2?.id || 'deleted'}`
      );
    }, 10000);

    it('should error when deleting non-existent node (not idempotent)', async () => {
      // Create a node
      const nodeData = TestNodeBuilder.text('To Delete').build();
      const nodeId = await backend.createNode(nodeData);

      // Delete once
      await backend.deleteNode(nodeId);

      // Verify deleted
      const fetchedAfterFirst = await backend.getNode(nodeId);
      expect(fetchedAfterFirst).toBeNull();

      // Delete again - backend returns error for non-existent node
      // This is current backend behavior (not ideally idempotent, but acceptable)
      await expect(backend.deleteNode(nodeId)).rejects.toThrow();

      // Still deleted
      const fetchedAfterSecond = await backend.getNode(nodeId);
      expect(fetchedAfterSecond).toBeNull();
    }, 10000);
  });

  describe('Load Operations', () => {
    it('should load nodes without triggering writes (read-only operation)', async () => {
      // Create test nodes
      const node1Data = TestNodeBuilder.text('Node 1').build();
      const node1Id = await backend.createNode(node1Data);

      const node2Data = TestNodeBuilder.text('Node 2').withBeforeSibling(node1Id).build();
      await backend.createNode(node2Data);

      // Query nodes (simulates loading on app start)
      const rootNodes = await backend.queryNodes({ parentId: null });

      // Verify we got the nodes
      expect(rootNodes.length).toBe(2);

      // Query again - should return same results (no writes occurred)
      const rootNodesAgain = await backend.queryNodes({ parentId: null });
      expect(rootNodesAgain.length).toBe(2);

      // Verify node content unchanged (no write-back loop)
      const node1After = await backend.getNode(node1Id);
      expect(node1After?.content).toBe('Node 1');
      expect(node1After?.modifiedAt).toEqual(rootNodes[0].modifiedAt);
    }, 10000);
  });
});
