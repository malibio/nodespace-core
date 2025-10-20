import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { NavigationService } from '$lib/services/navigation-service';
import { sharedNodeStore } from '$lib/services/shared-node-store';
import { tabState } from '$lib/stores/navigation';
import { get } from 'svelte/store';
import type { Node } from '$lib/types';

describe('NavigationService', () => {
  let service: NavigationService;

  beforeEach(() => {
    service = NavigationService.getInstance();
    // Reset stores
    sharedNodeStore.__resetForTesting();
    // Reset tab state to initial state
    tabState.set({
      tabs: [{ id: 'today', title: 'Today', type: 'date', closeable: false }],
      activeTabId: 'today'
    });
  });

  afterEach(() => {
    sharedNodeStore.__resetForTesting();
  });

  describe('resolveNodeTarget', () => {
    it('should resolve node target for existing text node', async () => {
      // Setup: Add a text node to the store
      const mockNode: Node = {
        id: 'test-123',
        nodeType: 'text',
        content: 'Test Content for Navigation',
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('test-123');

      expect(target).toBeDefined();
      expect(target?.nodeId).toBe('test-123');
      expect(target?.nodeType).toBe('text');
      expect(target?.title).toBe('Test Content for Navigation');
    });

    it('should truncate long content for title (max 40 chars)', async () => {
      const longContent =
        'This is a very long content string that should be truncated to 40 characters';
      const mockNode: Node = {
        id: 'long-text',
        nodeType: 'text',
        content: longContent,
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('long-text');

      expect(target?.title).toBe('This is a very long content string th...');
      expect(target?.title.length).toBeLessThanOrEqual(40);
    });

    it('should return null for non-existent node', async () => {
      const target = await service.resolveNodeTarget('does-not-exist');
      expect(target).toBeNull();
    });

    it('should generate appropriate tab title for date nodes - Today', async () => {
      const today = new Date();
      const mockDateNode: Node = {
        id: 'date-today',
        nodeType: 'date',
        content: '',
        properties: { date: today.toISOString() },
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockDateNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('date-today');
      expect(target?.title).toBe('Today');
    });

    it('should generate appropriate tab title for date nodes - Tomorrow', async () => {
      const tomorrow = new Date();
      tomorrow.setDate(tomorrow.getDate() + 1);
      const mockDateNode: Node = {
        id: 'date-tomorrow',
        nodeType: 'date',
        content: '',
        properties: { date: tomorrow.toISOString() },
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockDateNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('date-tomorrow');
      expect(target?.title).toBe('Tomorrow');
    });

    it('should generate appropriate tab title for date nodes - Yesterday', async () => {
      const yesterday = new Date();
      yesterday.setDate(yesterday.getDate() - 1);
      const mockDateNode: Node = {
        id: 'date-yesterday',
        nodeType: 'date',
        content: '',
        properties: { date: yesterday.toISOString() },
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockDateNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('date-yesterday');
      expect(target?.title).toBe('Yesterday');
    });

    it('should use first line of multi-line content', async () => {
      const multiLineContent = 'First Line\nSecond Line\nThird Line';
      const mockNode: Node = {
        id: 'multi-line',
        nodeType: 'text',
        content: multiLineContent,
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('multi-line');
      expect(target?.title).toBe('First Line');
    });

    it('should fallback to node type when content is empty', async () => {
      const mockNode: Node = {
        id: 'empty-node',
        nodeType: 'task',
        content: '',
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      const target = await service.resolveNodeTarget('empty-node');
      expect(target?.title).toBe('task Node');
    });
  });

  describe('navigateToNode', () => {
    it('should create new tab when node does not have existing tab', async () => {
      const mockNode: Node = {
        id: 'new-node',
        nodeType: 'text',
        content: 'New Node Content',
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      await service.navigateToNode('new-node', false);

      const state = get(tabState);
      expect(state.tabs.length).toBe(2); // Initial tab + new tab
      expect(state.tabs[1].content?.nodeId).toBe('new-node');
      expect(state.tabs[1].title).toBe('New Node Content');
      expect(state.activeTabId).toBe(state.tabs[1].id);
    });

    it('should switch to existing tab when openInNewTab is false', async () => {
      const mockNode: Node = {
        id: 'existing-node',
        nodeType: 'text',
        content: 'Existing Node',
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      // Create initial tab
      await service.navigateToNode('existing-node', false);
      const firstState = get(tabState);
      const firstTabId = firstState.tabs[1].id;

      // Navigate again - should switch to existing tab
      await service.navigateToNode('existing-node', false);
      const secondState = get(tabState);

      expect(secondState.tabs.length).toBe(2); // No new tab created
      expect(secondState.activeTabId).toBe(firstTabId);
    });

    it('should create new tab when openInNewTab is true, even if tab exists', async () => {
      const mockNode: Node = {
        id: 'duplicate-node',
        nodeType: 'text',
        content: 'Duplicate Node',
        properties: {},
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

      // Create first tab
      await service.navigateToNode('duplicate-node', false);
      expect(get(tabState).tabs.length).toBe(2);

      // Create duplicate tab (Cmd+Click)
      await service.navigateToNode('duplicate-node', true);
      const state = get(tabState);

      expect(state.tabs.length).toBe(3); // New duplicate tab created
      expect(state.tabs[1].content?.nodeId).toBe('duplicate-node');
      expect(state.tabs[2].content?.nodeId).toBe('duplicate-node');
    });

    it('should map date nodes to date tab type', async () => {
      const mockDateNode: Node = {
        id: 'date-node',
        nodeType: 'date',
        content: '',
        properties: { date: new Date().toISOString() },
        parentId: null,
        beforeSiblingId: null,
        containerNodeId: null,
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      };
      sharedNodeStore.setNode(mockDateNode, { type: 'database', reason: 'test-setup' }, true);

      await service.navigateToNode('date-node', false);

      const state = get(tabState);
      expect(state.tabs[1].type).toBe('date');
    });

    it('should map text/task/ai-chat nodes to node tab type', async () => {
      const nodeTypes = ['text', 'task', 'ai-chat'];

      for (const nodeType of nodeTypes) {
        // Reset state for each test
        sharedNodeStore.__resetForTesting();
        tabState.set({
          tabs: [{ id: 'today', title: 'Today', type: 'date', closeable: false }],
          activeTabId: 'today'
        });

        const mockNode: Node = {
          id: `${nodeType}-node`,
          nodeType,
          content: `${nodeType} content`,
          properties: {},
          parentId: null,
          beforeSiblingId: null,
          containerNodeId: null,
          createdAt: new Date().toISOString(),
          modifiedAt: new Date().toISOString()
        };
        sharedNodeStore.setNode(mockNode, { type: 'database', reason: 'test-setup' }, true);

        await service.navigateToNode(`${nodeType}-node`, false);

        const state = get(tabState);
        expect(state.tabs[1].type).toBe('node');
        expect(state.tabs[1].content?.nodeType).toBe(nodeType);
      }
    });

    it('should handle navigation to non-existent node gracefully', async () => {
      await service.navigateToNode('non-existent', false);

      // Should not create a tab
      const state = get(tabState);
      expect(state.tabs.length).toBe(1); // Only initial tab
    });
  });
});
