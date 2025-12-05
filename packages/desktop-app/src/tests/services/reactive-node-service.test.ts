/**
 * Unit tests for ReactiveNodeService
 *
 * Tests reactive node management including:
 * - Service creation and lifecycle
 * - Node CRUD operations
 * - Hierarchy management (indent/outdent)
 * - Node combination and deletion
 * - Expansion state management
 * - Visible nodes computation
 * - UI state management
 * - Content processing and debouncing
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  createReactiveNodeService,
  type ReactiveNodeService,
  type NodeManagerEvents
} from '$lib/services/reactive-node-service.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { getFocusManager as _getFocusManager } from '$lib/services/focus-manager.svelte';
import type { Node } from '$lib/types';
import { DEFAULT_PANE_ID as _DEFAULT_PANE_ID } from '$lib/stores/navigation';

// Mock tauri-commands to avoid backend calls in tests
vi.mock('$lib/services/tauri-commands', () => ({
  moveNode: vi.fn().mockResolvedValue(undefined),
  getNode: vi.fn().mockResolvedValue(null)
}));

// Mock reactive-structure-tree to avoid complex dependency setup
vi.mock('$lib/stores/reactive-structure-tree.svelte', () => ({
  structureTree: {
    addInMemoryRelationship: vi.fn(),
    moveInMemoryRelationship: vi.fn(),
    getChildren: vi.fn(() => []),
    getChildrenWithOrder: vi.fn(() => []),
    getParent: vi.fn(() => null)
  }
}));

describe('ReactiveNodeService - Service Lifecycle', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    // Reset SharedNodeStore singleton for each test
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);
  });

  afterEach(() => {
    // Clean up service subscriptions to prevent memory leaks
    service.destroy();
  });

  it('creates service with unique viewer ID', () => {
    const service1 = createReactiveNodeService(events);
    const service2 = createReactiveNodeService(events);

    // Services should be different instances
    expect(service1).not.toBe(service2);

    service1.destroy();
    service2.destroy();
  });

  it('initializes with empty state', () => {
    expect(service.rootNodeIds).toEqual([]);
    expect(service.nodes.size).toBe(0);
    expect(service.activeNodeId).toBeUndefined();
  });

  it('destroy() cleans up subscriptions', () => {
    const initialUpdateTrigger = service._updateTrigger;

    // Add a node to trigger subscription
    const node: Node = {
      id: 'test-1',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    _sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });

    // Update trigger should have incremented
    expect(service._updateTrigger).toBeGreaterThan(initialUpdateTrigger);

    // Destroy the service
    service.destroy();

    // Further changes should not update this service instance
    const triggerAfterDestroy = service._updateTrigger;
    _sharedNodeStore.updateNode('test-1', { content: 'Updated' }, { type: 'database', reason: 'test' });

    // Trigger should not change after destroy
    expect(service._updateTrigger).toBe(triggerAfterDestroy);
  });

  it('destroy() is idempotent (safe to call multiple times)', () => {
    expect(() => {
      service.destroy();
      service.destroy();
      service.destroy();
    }).not.toThrow();
  });
});

describe('ReactiveNodeService - Node Finding', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);
  });

  afterEach(() => {
    service.destroy();
  });

  it('findNode returns node from SharedNodeStore', () => {
    const node: Node = {
      id: 'find-test-1',
      nodeType: 'text',
      content: 'Find me',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    _sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });

    const found = service.findNode('find-test-1');
    expect(found).not.toBeNull();
    expect(found?.id).toBe('find-test-1');
    expect(found?.content).toBe('Find me');
  });

  it('findNode returns null for non-existent node', () => {
    const found = service.findNode('non-existent-id');
    expect(found).toBeNull();
  });
});

describe('ReactiveNodeService - Initialize Nodes', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);
  });

  afterEach(() => {
    service.destroy();
  });

  it('initializes with root nodes', () => {
    const nodes: Node[] = [
      {
        id: 'root-1',
        nodeType: 'text',
        content: 'Root 1',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'root-2',
        nodeType: 'text',
        content: 'Root 2',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes);

    expect(service.rootNodeIds).toEqual(['root-1', 'root-2']);
    expect(service.nodes.size).toBe(2);
  });

  it('initializes with multiple nodes and sets UI state', () => {
    const nodes: Node[] = [
      {
        id: 'node-1',
        nodeType: 'text',
        content: 'Node 1',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'node-2',
        nodeType: 'text',
        content: 'Node 2',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'node-3',
        nodeType: 'text',
        content: 'Node 3',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes);

    expect(service.rootNodeIds).toContain('node-1');
    expect(service.rootNodeIds).toContain('node-2');
    expect(service.rootNodeIds).toContain('node-3');
    expect(service.nodes.size).toBe(3);

    // Check UI state exists for all nodes
    const node1UIState = service.getUIState('node-1');
    expect(node1UIState).toBeDefined();
    expect(node1UIState?.depth).toBe(0);

    const node2UIState = service.getUIState('node-2');
    expect(node2UIState).toBeDefined();
    expect(node2UIState?.depth).toBe(0);

    const node3UIState = service.getUIState('node-3');
    expect(node3UIState).toBeDefined();
    expect(node3UIState?.depth).toBe(0);
  });

  it('initializes with custom options', () => {
    const nodes: Node[] = [
      {
        id: 'test-1',
        nodeType: 'text',
        content: '',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes, {
      expanded: false,
      autoFocus: true,
      inheritHeaderLevel: 2,
      isInitialPlaceholder: true
    });

    const uiState = service.getUIState('test-1');
    expect(uiState?.expanded).toBe(false);
    expect(uiState?.inheritHeaderLevel).toBe(2);
    expect(uiState?.isPlaceholder).toBe(true);
  });

  it('initializes nodes and allows custom depth via options', () => {
    const nodes: Node[] = [
      {
        id: 'test-node',
        nodeType: 'text',
        content: 'Test',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes, {
      expanded: false,
      inheritHeaderLevel: 2
    });

    const uiState = service.getUIState('test-node');
    expect(uiState?.expanded).toBe(false);
    expect(uiState?.inheritHeaderLevel).toBe(2);
    expect(uiState?.depth).toBe(0); // Computed as root node
  });
});

describe('ReactiveNodeService - Update Node Content', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    // Initialize with test node
    const node: Node = {
      id: 'update-test',
      nodeType: 'text',
      content: 'Original',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    service.initializeNodes([node]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('updates node content', () => {
    service.updateNodeContent('update-test', 'Updated content');

    const node = service.findNode('update-test');
    expect(node?.content).toBe('Updated content');
  });

  it('preserves nodeType when updating content', () => {
    // Change node type first
    service.updateNodeType('update-test', 'task');

    // Update content
    service.updateNodeContent('update-test', 'New task content');

    const node = service.findNode('update-test');
    expect(node?.nodeType).toBe('task');
    expect(node?.content).toBe('New task content');
  });

  it('updates isPlaceholder when content becomes empty', () => {
    service.updateNodeContent('update-test', '');

    const uiState = service.getUIState('update-test');
    expect(uiState?.isPlaceholder).toBe(true);
  });

  it('updates isPlaceholder when content becomes non-empty', () => {
    // Start with empty content
    service.updateNodeContent('update-test', '');
    expect(service.getUIState('update-test')?.isPlaceholder).toBe(true);

    // Add content
    service.updateNodeContent('update-test', 'Now has content');
    expect(service.getUIState('update-test')?.isPlaceholder).toBe(false);
  });

  it('handles header level detection', () => {
    service.updateNodeContent('update-test', '# Header 1');

    const uiState = service.getUIState('update-test');
    expect(uiState?.inheritHeaderLevel).toBe(1);
  });

  it('does nothing for non-existent node', () => {
    expect(() => {
      service.updateNodeContent('non-existent', 'Test');
    }).not.toThrow();

    const node = service.findNode('non-existent');
    expect(node).toBeNull();
  });
});

describe('ReactiveNodeService - Update Node Type', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const node: Node = {
      id: 'type-test',
      nodeType: 'text',
      content: 'Test content',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    service.initializeNodes([node]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('updates node type', () => {
    service.updateNodeType('type-test', 'task');

    const node = service.findNode('type-test');
    expect(node?.nodeType).toBe('task');
  });

  it('preserves content when updating type', () => {
    service.updateNodeType('type-test', 'task');

    const node = service.findNode('type-test');
    expect(node?.content).toBe('Test content');
  });

  it('does nothing for non-existent node', () => {
    expect(() => {
      service.updateNodeType('non-existent', 'task');
    }).not.toThrow();
  });
});

describe('ReactiveNodeService - Update Node Mentions', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const node: Node = {
      id: 'mentions-test',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString(),
      mentions: []
    };

    service.initializeNodes([node]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('updates node mentions', () => {
    service.updateNodeMentions('mentions-test', ['user-1', 'user-2']);

    const node = service.findNode('mentions-test');
    expect(node?.mentions).toEqual(['user-1', 'user-2']);
  });

  it('replaces existing mentions', () => {
    service.updateNodeMentions('mentions-test', ['user-1']);
    service.updateNodeMentions('mentions-test', ['user-2', 'user-3']);

    const node = service.findNode('mentions-test');
    expect(node?.mentions).toEqual(['user-2', 'user-3']);
  });

  it('does nothing for non-existent node', () => {
    expect(() => {
      service.updateNodeMentions('non-existent', ['user-1']);
    }).not.toThrow();
  });
});

describe('ReactiveNodeService - Update Node Properties', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const node: Node = {
      id: 'props-test',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: { existingKey: 'existingValue' },
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    service.initializeNodes([node]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('merges properties by default', () => {
    service.updateNodeProperties('props-test', { newKey: 'newValue' });

    const node = service.findNode('props-test');
    expect(node?.properties).toEqual({
      existingKey: 'existingValue',
      newKey: 'newValue'
    });
  });

  it('replaces properties when merge is false', () => {
    service.updateNodeProperties('props-test', { newKey: 'newValue' }, false);

    const node = service.findNode('props-test');
    expect(node?.properties).toEqual({ newKey: 'newValue' });
  });

  it('does nothing for non-existent node', () => {
    expect(() => {
      service.updateNodeProperties('non-existent', { key: 'value' });
    }).not.toThrow();
  });
});

describe('ReactiveNodeService - Expansion State', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const nodes: Node[] = [
      {
        id: 'expand-test',
        nodeType: 'text',
        content: 'Parent',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      }
    ];

    service.initializeNodes(nodes);
  });

  afterEach(() => {
    service.destroy();
  });

  it('toggleExpanded toggles expansion state', () => {
    const initialState = service.getUIState('expand-test')?.expanded;

    service.toggleExpanded('expand-test');

    const newState = service.getUIState('expand-test')?.expanded;
    expect(newState).toBe(!initialState);
  });

  it('toggleExpanded fires hierarchyChanged event', () => {
    service.toggleExpanded('expand-test');

    expect(events.hierarchyChanged).toHaveBeenCalled();
  });

  it('toggleExpanded returns false for non-existent node', () => {
    const result = service.toggleExpanded('non-existent');
    expect(result).toBe(false);
  });

  it('setExpanded sets specific state', () => {
    service.setExpanded('expand-test', true);
    expect(service.getUIState('expand-test')?.expanded).toBe(true);

    service.setExpanded('expand-test', false);
    expect(service.getUIState('expand-test')?.expanded).toBe(false);
  });

  it('setExpanded returns false when no change needed', () => {
    service.setExpanded('expand-test', true);

    // Try to set to same state
    const result = service.setExpanded('expand-test', true);
    expect(result).toBe(false);
  });

  it('setExpanded returns true when state changes', () => {
    service.setExpanded('expand-test', true);

    const result = service.setExpanded('expand-test', false);
    expect(result).toBe(true);
  });

  it('setExpanded creates UI state if missing for node in SharedNodeStore', () => {
    // Create a new service instance
    const newService = createReactiveNodeService(events);

    const newNode: Node = {
      id: 'new-node',
      nodeType: 'text',
      content: 'New',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    // Add directly to SharedNodeStore
    _sharedNodeStore.setNode(newNode, { type: 'database', reason: 'test' }, true);

    // Reactive subscription auto-initializes UI state for new nodes
    // So UI state should exist after node is added
    const uiState = newService.getUIState('new-node');
    expect(uiState).toBeDefined();

    // Test that setExpanded works on this node
    newService.setExpanded('new-node', false);
    expect(newService.getUIState('new-node')?.expanded).toBe(false);

    newService.destroy();
  });

  it('batchSetExpanded updates multiple nodes efficiently', () => {
    const nodes: Node[] = [
      {
        id: 'batch-1',
        nodeType: 'text',
        content: 'Node 1',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      },
      {
        id: 'batch-2',
        nodeType: 'text',
        content: 'Node 2',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      },
      {
        id: 'batch-3',
        nodeType: 'text',
        content: 'Node 3',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      }
    ];

    service.initializeNodes(nodes, { expanded: true });

    // Reset event mock
    (events.hierarchyChanged as ReturnType<typeof vi.fn>).mockClear();

    const changedCount = service.batchSetExpanded([
      { nodeId: 'batch-1', expanded: false },
      { nodeId: 'batch-2', expanded: false },
      { nodeId: 'batch-3', expanded: true } // No change since already expanded
    ]);

    // Should return count of changed nodes (only batch-1 and batch-2 change)
    expect(changedCount).toBe(2);

    // Should only fire hierarchyChanged once (batched)
    expect(events.hierarchyChanged).toHaveBeenCalledTimes(1);

    // Verify states
    expect(service.getUIState('batch-1')?.expanded).toBe(false);
    expect(service.getUIState('batch-2')?.expanded).toBe(false);
    expect(service.getUIState('batch-3')?.expanded).toBe(true);
  });

  it('batchSetExpanded skips nodes with no change', () => {
    const nodes: Node[] = [
      {
        id: 'batch-1',
        nodeType: 'text',
        content: 'Node 1',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      }
    ];

    service.initializeNodes(nodes);
    service.setExpanded('batch-1', true);

    const changedCount = service.batchSetExpanded([
      { nodeId: 'batch-1', expanded: true } // No change
    ]);

    expect(changedCount).toBe(0);
  });
});

describe('ReactiveNodeService - Delete Node', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const nodes: Node[] = [
      {
        id: 'delete-test',
        nodeType: 'text',
        content: 'To delete',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString()
      }
    ];

    service.initializeNodes(nodes);
  });

  afterEach(() => {
    service.destroy();
  });

  it('deletes node from store', () => {
    service.deleteNode('delete-test');

    const node = service.findNode('delete-test');
    expect(node).toBeNull();
  });

  it('removes node from rootNodeIds', () => {
    expect(service.rootNodeIds).toContain('delete-test');

    service.deleteNode('delete-test');

    expect(service.rootNodeIds).not.toContain('delete-test');
  });

  it('fires nodeDeleted event', () => {
    service.deleteNode('delete-test');

    expect(events.nodeDeleted).toHaveBeenCalledWith('delete-test');
  });

  it('fires hierarchyChanged event', () => {
    service.deleteNode('delete-test');

    expect(events.hierarchyChanged).toHaveBeenCalled();
  });

  it('cleans up UI state', () => {
    service.deleteNode('delete-test');

    const uiState = service.getUIState('delete-test');
    expect(uiState).toBeUndefined();
  });

  it('does nothing for non-existent node', () => {
    expect(() => {
      service.deleteNode('non-existent');
    }).not.toThrow();
  });
});

describe('ReactiveNodeService - Visible Nodes', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);
  });

  afterEach(() => {
    service.destroy();
  });

  it('returns empty array when no nodes', () => {
    const visible = service.visibleNodes(null);
    expect(visible).toEqual([]);
  });

  it('returns root nodes when parentId is null', () => {
    const nodes: Node[] = [
      {
        id: 'root-1',
        nodeType: 'text',
        content: 'Root 1',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'root-2',
        nodeType: 'text',
        content: 'Root 2',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes);

    const visible = service.visibleNodes(null);
    expect(visible.length).toBe(2);
    expect(visible.map((n) => n.id)).toEqual(['root-1', 'root-2']);
  });

  it('includes expanded node children', () => {
    const nodes: Node[] = [
      {
        id: 'parent',
        nodeType: 'text',
        content: 'Parent',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'child',
        nodeType: 'text',
        content: 'Child',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: 'parent'
      }
    ];

    service.initializeNodes(nodes, { expanded: true });

    const visible = service.visibleNodes(null);
    expect(visible.length).toBe(2);
    expect(visible.map((n) => n.id)).toEqual(['parent', 'child']);
  });

  it('excludes collapsed node children', () => {
    const nodes: Node[] = [
      {
        id: 'parent',
        nodeType: 'text',
        content: 'Parent',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes, { expanded: true });
    service.setExpanded('parent', false);

    // With no children, visible nodes should just be the parent
    const visible = service.visibleNodes(null);
    expect(visible.length).toBe(1);
    expect(visible[0].id).toBe('parent');
    expect(visible[0].expanded).toBe(false);
  });

  it('includes depth in visible node data', () => {
    const nodes: Node[] = [
      {
        id: 'root-node',
        nodeType: 'text',
        content: 'Root',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes, { expanded: true });

    const visible = service.visibleNodes(null);
    expect(visible[0].depth).toBe(0);
    expect(visible[0].id).toBe('root-node');
  });
});

describe('ReactiveNodeService - Content Processing Methods', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const node: Node = {
      id: 'content-test',
      nodeType: 'text',
      content: '# Header\n\nSome **bold** text',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    service.initializeNodes([node]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('parseNodeContent returns parsed markdown', () => {
    const parsed = service.parseNodeContent('content-test');
    expect(parsed).toBeDefined();
    // ContentProcessor.parseMarkdown returns a structure
    expect(parsed).not.toBeNull();
  });

  it('parseNodeContent returns null for non-existent node', () => {
    const parsed = service.parseNodeContent('non-existent');
    expect(parsed).toBeNull();
  });

  it('getNodeHeaderLevel detects header level', () => {
    const level = service.getNodeHeaderLevel('content-test');
    expect(level).toBe(1);
  });

  it('getNodeHeaderLevel returns 0 for non-header content', () => {
    service.updateNodeContent('content-test', 'Plain text');
    const level = service.getNodeHeaderLevel('content-test');
    expect(level).toBe(0);
  });

  it('getNodeHeaderLevel returns 0 for non-existent node', () => {
    const level = service.getNodeHeaderLevel('non-existent');
    expect(level).toBe(0);
  });

  it('getNodeDisplayText returns text without markdown formatting', () => {
    const displayText = service.getNodeDisplayText('content-test');
    expect(displayText).toBeTruthy();
    expect(displayText.length).toBeGreaterThan(0);
  });

  it('getNodeDisplayText returns empty string for non-existent node', () => {
    const displayText = service.getNodeDisplayText('non-existent');
    expect(displayText).toBe('');
  });

  it('updateNodeContentWithProcessing updates content', () => {
    const result = service.updateNodeContentWithProcessing('content-test', 'New content');
    expect(result).toBe(true);

    const node = service.findNode('content-test');
    expect(node?.content).toBe('New content');
  });

  it('updateNodeContentWithProcessing returns false for non-existent node', () => {
    const result = service.updateNodeContentWithProcessing('non-existent', 'Test');
    expect(result).toBe(false);
  });
});

describe('ReactiveNodeService - Reactive Updates', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);
  });

  afterEach(() => {
    service.destroy();
  });

  it('_updateTrigger increments when SharedNodeStore changes', () => {
    const initialTrigger = service._updateTrigger;

    const node: Node = {
      id: 'reactive-test',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString()
    };

    _sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });

    expect(service._updateTrigger).toBeGreaterThan(initialTrigger);
  });

  it('initializes UI state for new nodes added to SharedNodeStore', () => {
    const node: Node = {
      id: 'new-reactive-node',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString(),
      parentId: null
    };

    _sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });

    // UI state should be automatically initialized
    const uiState = service.getUIState('new-reactive-node');
    expect(uiState).toBeDefined();
    expect(uiState?.depth).toBe(0);
  });

  it('initializes UI state for new nodes added to SharedNodeStore', () => {
    const node: Node = {
      id: 'new-reactive-node',
      nodeType: 'text',
      content: 'Test',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString(),
      parentId: null
    };

    _sharedNodeStore.setNode(node, { type: 'database', reason: 'test' });

    // UI state should be automatically initialized by subscription
    const uiState = service.getUIState('new-reactive-node');
    expect(uiState).toBeDefined();
    expect(uiState?.depth).toBe(0);
  });
});

describe('ReactiveNodeService - Create Node', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    // Initialize with a reference node
    const referenceNode: Node = {
      id: 'reference-node',
      nodeType: 'text',
      content: 'Reference',
      version: 1,
      properties: {},
      createdAt: new Date().toISOString(),
      modifiedAt: new Date().toISOString(),
      parentId: null
    };

    service.initializeNodes([referenceNode]);
  });

  afterEach(() => {
    service.destroy();
  });

  it('creates node after reference node', () => {
    const newNodeId = service.createNode('reference-node', 'New content');

    expect(newNodeId).toBeTruthy();
    expect(newNodeId.length).toBeGreaterThan(0);

    const newNode = service.findNode(newNodeId);
    expect(newNode).not.toBeNull();
    expect(newNode?.content).toBe('New content');
  });

  it('creates placeholder node with empty content', () => {
    const placeholderId = service.createPlaceholderNode('reference-node');

    expect(placeholderId).toBeTruthy();

    const placeholder = service.findNode(placeholderId);
    expect(placeholder?.content).toBe('');

    const uiState = service.getUIState(placeholderId);
    expect(uiState?.isPlaceholder).toBe(true);
  });

  it('fires nodeCreated event', () => {
    service.createNode('reference-node', 'Test');

    expect(events.nodeCreated).toHaveBeenCalled();
  });

  it('fires hierarchyChanged event', () => {
    service.createNode('reference-node', 'Test');

    expect(events.hierarchyChanged).toHaveBeenCalled();
  });

  it('returns empty string when reference node does not exist', () => {
    const nodeId = service.createNode('non-existent', 'Test');

    expect(nodeId).toBe('');
  });

  it('creates node with specific node type', () => {
    const taskNodeId = service.createNode('reference-node', '[ ] Task content', 'task');

    const taskNode = service.findNode(taskNodeId);
    expect(taskNode?.nodeType).toBe('task');
  });

  it('inherits header formatting from reference node', () => {
    // Update reference node to be a header
    service.updateNodeContent('reference-node', '## Header');

    const newNodeId = service.createNode('reference-node', '');

    const newNode = service.findNode(newNodeId);
    expect(newNode?.content).toMatch(/^##\s+/);
  });
});

describe('ReactiveNodeService - Combine Nodes', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const nodes: Node[] = [
      {
        id: 'node-1',
        nodeType: 'text',
        content: 'First node',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'node-2',
        nodeType: 'text',
        content: 'Second node',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes);
  });

  afterEach(() => {
    service.destroy();
  });

  it('combines current node into previous node', () => {
    service.combineNodes('node-2', 'node-1');

    const node1 = service.findNode('node-1');
    expect(node1?.content).toBe('First nodeSecond node');
  });

  it('deletes current node after combining', () => {
    service.combineNodes('node-2', 'node-1');

    const node2 = service.findNode('node-2');
    expect(node2).toBeNull();
  });

  it('fires nodeDeleted event', () => {
    service.combineNodes('node-2', 'node-1');

    expect(events.nodeDeleted).toHaveBeenCalledWith('node-2');
  });

  it('fires hierarchyChanged event', () => {
    service.combineNodes('node-2', 'node-1');

    expect(events.hierarchyChanged).toHaveBeenCalled();
  });

  it('fires focusRequested event with merge position', () => {
    service.combineNodes('node-2', 'node-1');

    expect(events.focusRequested).toHaveBeenCalledWith('node-1', 'First node'.length);
  });

  it('does nothing when current node does not exist', () => {
    expect(() => {
      service.combineNodes('non-existent', 'node-1');
    }).not.toThrow();

    const node1 = service.findNode('node-1');
    expect(node1?.content).toBe('First node');
  });

  it('does nothing when previous node does not exist', () => {
    expect(() => {
      service.combineNodes('node-2', 'non-existent');
    }).not.toThrow();

    const node2 = service.findNode('node-2');
    expect(node2?.content).toBe('Second node');
  });

  it('strips header formatting when combining', () => {
    service.updateNodeContent('node-2', '## Header content');

    service.combineNodes('node-2', 'node-1');

    const node1 = service.findNode('node-1');
    expect(node1?.content).toBe('First nodeHeader content');
  });

  it('strips task checkbox when combining', () => {
    service.updateNodeContent('node-2', '[ ] Task content');

    service.combineNodes('node-2', 'node-1');

    const node1 = service.findNode('node-1');
    expect(node1?.content).toBe('First nodeTask content');
  });
});

describe('ReactiveNodeService - Indent/Outdent Node', () => {
  let service: ReactiveNodeService;
  let events: NodeManagerEvents;
  let _sharedNodeStore: SharedNodeStore;

  beforeEach(() => {
    SharedNodeStore.resetInstance();
    _sharedNodeStore = SharedNodeStore.getInstance();

    events = {
      focusRequested: vi.fn(),
      hierarchyChanged: vi.fn(),
      nodeCreated: vi.fn(),
      nodeDeleted: vi.fn()
    };

    service = createReactiveNodeService(events);

    const nodes: Node[] = [
      {
        id: 'parent',
        nodeType: 'text',
        content: 'Parent',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      },
      {
        id: 'sibling',
        nodeType: 'text',
        content: 'Sibling',
        version: 1,
        properties: {},
        createdAt: new Date().toISOString(),
        modifiedAt: new Date().toISOString(),
        parentId: null
      }
    ];

    service.initializeNodes(nodes);
  });

  afterEach(() => {
    service.destroy();
  });

  it('indentNode returns false when node does not exist', async () => {
    const result = await service.indentNode('non-existent');
    expect(result).toBe(false);
  });

  it('indentNode returns false when node is first sibling (no previous sibling)', async () => {
    const result = await service.indentNode('parent');
    expect(result).toBe(false);
  });

  it('indentNode fires hierarchyChanged event', async () => {
    await service.indentNode('sibling');

    expect(events.hierarchyChanged).toHaveBeenCalled();
  });

  it('outdentNode returns false when node does not exist', async () => {
    const result = await service.outdentNode('non-existent');
    expect(result).toBe(false);
  });

  it('outdentNode returns false when node is root (no parent)', async () => {
    const result = await service.outdentNode('parent');
    expect(result).toBe(false);
  });
});
