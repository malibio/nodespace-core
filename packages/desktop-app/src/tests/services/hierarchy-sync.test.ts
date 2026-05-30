import { describe, it, expect, beforeEach } from 'vitest';
import { applyHasChildCreated, applyHasChildUpdated, applyHasChildDeleted } from '$lib/services/hierarchy-sync';
import { structureTree } from '$lib/stores/reactive-structure-tree.svelte';

describe('hierarchy-sync', () => {
  beforeEach(() => {
    structureTree.children.clear();
  });

  describe('applyHasChildCreated', () => {
    it('uses incoming order when it is a real number', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 5 });
      const kids = structureTree.getChildrenWithOrder('p');
      expect(kids[0].order).toBe(5);
    });

    it('honors order: 0 (falsy but valid)', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 0 });
      expect(structureTree.getChildrenWithOrder('p')[0].order).toBe(0);
    });

    it('preserves existing order when incoming order is missing and child already exists', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 3 });
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: undefined });
      expect(structureTree.getChildrenWithOrder('p')[0].order).toBe(3);
    });

    it('appends at tail (not Date.now()) when child is new and order missing', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c1', order: 10 });
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c2', order: undefined });
      const [first, second] = structureTree.getChildrenWithOrder('p');
      expect(first.nodeId).toBe('c1');
      expect(second.nodeId).toBe('c2');
      // Must NOT be Date.now()-range (>1e12)
      expect(second.order).toBeLessThan(1e12);
      // Must be > first sibling's order
      expect(second.order).toBeGreaterThan(10);
    });

    it('produces identical structureTree state for same event through both paths (anti-drift)', () => {
      // Simulate same event through "tauri path" (has stripNodePrefix applied upstream)
      // and "browser path" — both call applyHasChildCreated, results must match
      const tree1 = structureTree;
      applyHasChildCreated(tree1, { parentId: 'parent', childId: 'child', order: 2.5 });
      const tauri = structureTree.getChildrenWithOrder('parent');

      structureTree.children.clear();
      applyHasChildCreated(structureTree, { parentId: 'parent', childId: 'child', order: 2.5 });
      const browser = structureTree.getChildrenWithOrder('parent');

      expect(tauri).toEqual(browser);
    });
  });

  describe('applyHasChildUpdated', () => {
    it('updates child order in structureTree', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 1 });
      applyHasChildUpdated(structureTree, { parentId: 'p', childId: 'c', order: 5 });
      expect(structureTree.getChildrenWithOrder('p')[0].order).toBe(5);
    });

    it('no-ops when order is missing (logs warning)', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 1 });
      applyHasChildUpdated(structureTree, { parentId: 'p', childId: 'c', order: undefined });
      expect(structureTree.getChildrenWithOrder('p')[0].order).toBe(1);
    });
  });

  describe('applyHasChildDeleted', () => {
    it('removes child from structureTree', () => {
      applyHasChildCreated(structureTree, { parentId: 'p', childId: 'c', order: 1 });
      applyHasChildDeleted(structureTree, { parentId: 'p', childId: 'c' });
      expect(structureTree.getChildren('p')).toEqual([]);
    });
  });
});
