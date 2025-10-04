/**
 * ReactiveNodeService - Pure Svelte 5 Runes Reactive Architecture (Refactored)
 *
 * Now uses unified Node type from $lib/types with separated UI state.
 * This is a TEMPORARY BRIDGE implementation that maintains backward compatibility
 * with existing components while transitioning to the real backend.
 *
 * Key changes:
 * - Uses Node from $lib/types (with node_type, properties, etc.)
 * - Separates data (Node) from UI state (NodeUIState)
 * - Computes children on-demand instead of storing arrays
 * - Maintains compatibility with existing component interfaces
 */

import { v4 as uuidv4 } from 'uuid';
import { ContentProcessor } from './contentProcessor';
import { eventBus } from './eventBus';
import type { Node, NodeUIState } from '$lib/types';
import { createDefaultUIState } from '$lib/types';

// IMPORTANT: This is a LEGACY interface for backward compatibility
// Components still expect this shape, but internally we use the unified Node
export interface LegacyNodeShape {
  id: string;
  content: string;
  nodeType: string; // Maps to node_type
  depth: number; // From UI state
  parentId?: string; // Maps to parent_id
  children: string[]; // Computed on-demand
  expanded: boolean; // From UI state
  autoFocus: boolean; // From UI state
  inheritHeaderLevel: number; // From UI state
  metadata: Record<string, unknown>; // Maps to properties
  mentions?: string[];
  before_sibling_id?: string;
  isPlaceholder?: boolean; // From UI state
}

export interface NodeManagerEvents {
  focusRequested: (nodeId: string, position?: number) => void;
  hierarchyChanged: () => void;
  nodeCreated: (nodeId: string) => void;
  nodeDeleted: (nodeId: string) => void;
}

export function createReactiveNodeService(events: NodeManagerEvents) {
  // Separated data and UI state - following unified type system
  const _nodes = $state<Record<string, Node>>({});
  const _uiState = $state<Record<string, NodeUIState>>({});
  let _rootNodeIds = $state<string[]>([]);
  const _activeNodeId = $state<string | undefined>(undefined);

  // Manual reactivity trigger for debugging
  let _updateTrigger = $state(0);

  // Helper function to convert Node to legacy shape for backward compatibility
  function toLegacyShape(node: Node): LegacyNodeShape {
    const uiState = _uiState[node.id] || createDefaultUIState(node.id);
    const children = Object.values(_nodes)
      .filter((n) => n.parent_id === node.id)
      .map((n) => n.id);

    return {
      id: node.id,
      content: node.content,
      nodeType: node.node_type,
      depth: uiState.depth,
      parentId: node.parent_id || undefined,
      children,
      expanded: uiState.expanded,
      autoFocus: uiState.autoFocus,
      inheritHeaderLevel: uiState.inheritHeaderLevel,
      metadata: node.properties,
      mentions: node.mentions,
      before_sibling_id: node.before_sibling_id || undefined,
      isPlaceholder: uiState.isPlaceholder
    };
  }

  // REACTIVITY FIX: Properly reactive visibleNodes computation using $derived.by
  const _visibleNodes = $derived.by(() => {
    const allNodes = Object.values(_nodes);
    for (const node of allNodes) {
      void node.node_type;
    }
    void _updateTrigger;
    void _rootNodeIds;
    return getVisibleNodesRecursive(_rootNodeIds);
  });

  const serviceName = 'ReactiveNodeService';
  const contentProcessor = ContentProcessor.getInstance();

  function getVisibleNodesRecursive(nodeIds: string[]): LegacyNodeShape[] {
    const result: LegacyNodeShape[] = [];

    for (const nodeId of nodeIds) {
      const node = _nodes[nodeId];
      const uiState = _uiState[nodeId];
      if (node) {
        result.push(toLegacyShape(node));

        const children = Object.values(_nodes)
          .filter((n) => n.parent_id === nodeId)
          .map((n) => n.id);

        if (uiState?.expanded && children.length > 0) {
          result.push(...getVisibleNodesRecursive(children));
        }
      }
    }

    return result;
  }

  function findNode(nodeId: string): LegacyNodeShape | null {
    const node = _nodes[nodeId];
    return node ? toLegacyShape(node) : null;
  }

  function clearAllAutoFocus(): void {
    for (const [nodeId, state] of Object.entries(_uiState)) {
      if (state.autoFocus) {
        _uiState[nodeId] = { ...state, autoFocus: false };
      }
    }
  }

  function createNode(
    afterNodeId: string,
    content: string = '',
    nodeType: string = 'text',
    headerLevel?: number,
    insertAtBeginning?: boolean,
    originalNodeContent?: string,
    focusNewNode?: boolean
  ): string {
    const afterNode = findNode(afterNodeId);
    if (!afterNode) {
      return '';
    }

    clearAllAutoFocus();

    const nodeId = uuidv4();
    let newDepth: number;
    let newParentId: string | null;

    if (insertAtBeginning) {
      newDepth = afterNode.depth;
      newParentId = afterNode.parentId || null;
    } else {
      newDepth = afterNode.depth;
      newParentId = afterNode.parentId || null;
    }

    let initialContent = content;
    const sourceContent = originalNodeContent || afterNode.content;

    if (content.trim() === '') {
      const headerMatch = sourceContent.match(/^(#{1,6}\s+)/);
      if (headerMatch) {
        initialContent = headerMatch[1];
      }
    } else {
      const sourceHeaderMatch = sourceContent.match(/^(#{1,6}\s+)/);
      const contentHeaderMatch = content.match(/^(#{1,6}\s+)/);

      if (sourceHeaderMatch && !contentHeaderMatch) {
        initialContent = sourceHeaderMatch[1] + content;
      }
    }

    const shouldFocusNewNode = focusNewNode !== undefined ? focusNewNode : !insertAtBeginning;
    const isPlaceholder = initialContent.trim() === '' || /^#{1,6}\s*$/.test(initialContent.trim());

    // Create Node with unified type system
    const newNode: Node = {
      id: nodeId,
      node_type: nodeType,
      content: initialContent,
      parent_id: newParentId,
      root_id: newParentId || nodeId, // Root ID logic
      before_sibling_id: null,
      created_at: new Date().toISOString(),
      modified_at: new Date().toISOString(),
      properties: {},
      mentions: []
    };

    // Create UI state
    const newUIState = createDefaultUIState(nodeId, {
      depth: newDepth,
      expanded: true,
      autoFocus: shouldFocusNewNode,
      inheritHeaderLevel: headerLevel !== undefined ? headerLevel : afterNode.inheritHeaderLevel,
      isPlaceholder
    });

    _nodes[nodeId] = newNode;
    _uiState[nodeId] = newUIState;

    // Handle hierarchy positioning
    if (insertAtBeginning) {
      if (afterNode.parentId) {
        // const siblings = Object.values(_nodes)
        //   .filter(n => n.parent_id === afterNode.parentId)
        //   .map(n => n.id);
        // const afterNodeIndex = siblings.indexOf(afterNodeId);
        // Update sibling pointers (simplified for now)
      } else {
        const afterNodeIndex = _rootNodeIds.indexOf(afterNodeId);
        _rootNodeIds = [
          ..._rootNodeIds.slice(0, afterNodeIndex),
          nodeId,
          ..._rootNodeIds.slice(afterNodeIndex)
        ];
      }
    } else {
      if (afterNode.parentId) {
        // Insert after sibling
      } else {
        const afterNodeIndex = _rootNodeIds.indexOf(afterNodeId);
        _rootNodeIds = [
          ..._rootNodeIds.slice(0, afterNodeIndex + 1),
          nodeId,
          ..._rootNodeIds.slice(afterNodeIndex + 1)
        ];
        _updateTrigger++;
      }
    }

    events.nodeCreated(nodeId);
    events.hierarchyChanged();

    eventBus.emit<import('./eventTypes').NodeCreatedEvent>({
      type: 'node:created',
      namespace: 'lifecycle',
      source: serviceName,
      nodeId,
      nodeType,
      metadata: {}
    });

    if (!shouldFocusNewNode && afterNode) {
      _uiState[afterNodeId] = { ..._uiState[afterNodeId], autoFocus: true };
    }

    return nodeId;
  }

  function createPlaceholderNode(
    afterNodeId: string,
    nodeType: string = 'text',
    headerLevel?: number,
    insertAtBeginning: boolean = false,
    originalNodeContent?: string,
    focusNewNode?: boolean
  ): string {
    return createNode(
      afterNodeId,
      '',
      nodeType,
      headerLevel,
      insertAtBeginning,
      originalNodeContent,
      focusNewNode
    );
  }

  const debouncedOperations = new Map<
    string,
    {
      fastTimer?: ReturnType<typeof setTimeout>;
      expensiveTimer?: ReturnType<typeof setTimeout>;
      pendingContent?: string;
    }
  >();

  function updateNodeContent(nodeId: string, content: string): void {
    const node = _nodes[nodeId];
    if (!node) return;

    const headerLevel = contentProcessor.parseHeaderLevel(content);
    const isPlaceholder = content.trim() === '';

    _nodes[nodeId] = {
      ...node,
      content,
      modified_at: new Date().toISOString()
    };

    _uiState[nodeId] = {
      ..._uiState[nodeId],
      isPlaceholder,
      inheritHeaderLevel: headerLevel
    };

    emitNodeUpdated(nodeId, 'content', content);
    emitReferenceUpdateNeeded(nodeId);
    scheduleContentProcessing(nodeId, content);
  }

  function updateNodeType(nodeId: string, nodeType: string): void {
    const node = _nodes[nodeId];
    if (!node) return;

    _nodes[nodeId] = {
      ...node,
      node_type: nodeType,
      modified_at: new Date().toISOString()
    };
    _uiState[nodeId] = { ..._uiState[nodeId], autoFocus: true };

    _updateTrigger++;

    emitNodeUpdated(nodeId, 'nodeType', nodeType);

    setTimeout(() => {
      const state = _uiState[nodeId];
      if (state?.autoFocus) {
        _uiState[nodeId] = { ...state, autoFocus: false };
      }
    }, 250);

    scheduleContentProcessing(nodeId, node.content);
  }

  function scheduleContentProcessing(nodeId: string, content: string): void {
    const existing = debouncedOperations.get(nodeId);
    if (existing?.fastTimer) clearTimeout(existing.fastTimer);
    if (existing?.expensiveTimer) clearTimeout(existing.expensiveTimer);

    debouncedOperations.set(nodeId, { pendingContent: content });

    const fastTimer = setTimeout(() => {
      processFastContentOperations(nodeId, content);
    }, 300);

    const expensiveTimer = setTimeout(() => {
      processExpensiveContentOperations(nodeId, content);
    }, 2000);

    debouncedOperations.set(nodeId, {
      pendingContent: content,
      fastTimer,
      expensiveTimer
    });
  }

  function processFastContentOperations(nodeId: string, _content: string): void {
    const node = _nodes[nodeId];
    if (!node) return;

    emitReferenceUpdateNeeded(nodeId);

    const operations = debouncedOperations.get(nodeId);
    if (operations) {
      operations.fastTimer = undefined;
      debouncedOperations.set(nodeId, operations);
    }
  }

  function processExpensiveContentOperations(nodeId: string, content: string): void {
    const node = _nodes[nodeId];
    if (!node) return;

    emitExpensivePersistenceNeeded(nodeId, content);
    emitVectorEmbeddingNeeded(nodeId, content);
    emitReferencePropagatationNeeded(nodeId, content);

    debouncedOperations.delete(nodeId);
  }

  function emitReferenceUpdateNeeded(nodeId: string): void {
    eventBus.emit<import('./eventTypes').ReferencesUpdateNeededEvent>({
      type: 'references:update-needed',
      namespace: 'coordination',
      source: serviceName,
      nodeId,
      updateType: 'content',
      metadata: {}
    });
  }

  function emitExpensivePersistenceNeeded(nodeId: string, content: string): void {
    eventBus.emit<import('./eventTypes').NodePersistenceEvent>({
      type: 'node:persistence-needed',
      namespace: 'backend',
      source: serviceName,
      nodeId,
      content,
      metadata: {}
    });
  }

  function emitVectorEmbeddingNeeded(nodeId: string, content: string): void {
    eventBus.emit<import('./eventTypes').NodeEmbeddingEvent>({
      type: 'node:embedding-needed',
      namespace: 'ai',
      source: serviceName,
      nodeId,
      content,
      metadata: {}
    });
  }

  function emitReferencePropagatationNeeded(nodeId: string, content: string): void {
    eventBus.emit<import('./eventTypes').NodeReferencePropagationEvent>({
      type: 'node:reference-propagation-needed',
      namespace: 'references',
      source: serviceName,
      nodeId,
      content,
      metadata: {}
    });
  }

  function combineNodes(currentNodeId: string, previousNodeId: string): void {
    const currentNode = _nodes[currentNodeId];
    const previousNode = _nodes[previousNodeId];

    if (!currentNode || !previousNode) return;

    // const isChildToParent = currentNode.parent_id === previousNodeId;
    const cleanedContent = stripFormattingSyntax(currentNode.content);
    const combinedContent = previousNode.content + cleanedContent;
    const mergePosition = previousNode.content.length;

    _nodes[previousNodeId] = {
      ...previousNode,
      content: combinedContent,
      modified_at: new Date().toISOString()
    };
    _uiState[previousNodeId] = { ..._uiState[previousNodeId], autoFocus: false };

    // Handle child promotion if needed
    const currentChildren = Object.values(_nodes)
      .filter((n) => n.parent_id === currentNodeId)
      .map((n) => n.id);

    for (const childId of currentChildren) {
      const child = _nodes[childId];
      if (child) {
        _nodes[childId] = {
          ...child,
          parent_id: previousNodeId,
          modified_at: new Date().toISOString()
        };
      }
    }

    delete _nodes[currentNodeId];
    delete _uiState[currentNodeId];

    const rootIndex = _rootNodeIds.indexOf(currentNodeId);
    if (rootIndex >= 0) {
      _rootNodeIds.splice(rootIndex, 1);
    }

    clearAllAutoFocus();
    events.focusRequested(previousNodeId, mergePosition);
    events.hierarchyChanged();
  }

  function stripFormattingSyntax(content: string): string {
    let cleaned = content;
    cleaned = cleaned.replace(/^#{1,6}\s+/, '');
    cleaned = cleaned.replace(/^\[\s*[x\s]*\]\s*/, '');
    return cleaned.trim();
  }

  function indentNode(nodeId: string): boolean {
    const node = _nodes[nodeId];
    if (!node) return false;

    let siblings: string[];
    let nodeIndex: number;

    if (node.parent_id) {
      siblings = Object.values(_nodes)
        .filter((n) => n.parent_id === node.parent_id)
        .map((n) => n.id);
      nodeIndex = siblings.indexOf(nodeId);
    } else {
      siblings = _rootNodeIds;
      nodeIndex = siblings.indexOf(nodeId);
    }

    if (nodeIndex === 0) return false;

    const prevSiblingId = siblings[nodeIndex - 1];
    const prevSibling = _nodes[prevSiblingId];
    if (!prevSibling) return false;

    const uiState = _uiState[nodeId];
    const prevSiblingUIState = _uiState[prevSiblingId];

    _nodes[nodeId] = {
      ...node,
      parent_id: prevSiblingId,
      modified_at: new Date().toISOString()
    };
    _uiState[nodeId] = { ...uiState, depth: (prevSiblingUIState?.depth || 0) + 1 };

    if (node.parent_id) {
      // Remove from old parent's children list
    } else {
      _rootNodeIds.splice(nodeIndex, 1);
    }

    events.hierarchyChanged();
    _updateTrigger++;

    eventBus.emit<import('./eventTypes').HierarchyChangedEvent>({
      type: 'hierarchy:changed',
      namespace: 'lifecycle',
      source: serviceName,
      changeType: 'indent',
      affectedNodes: [nodeId]
    });

    return true;
  }

  function outdentNode(nodeId: string): boolean {
    const node = _nodes[nodeId];
    if (!node || !node.parent_id) return false;

    const parent = _nodes[node.parent_id];
    if (!parent) return false;

    const newParentId = parent.parent_id || null;
    const uiState = _uiState[nodeId];
    const newDepth = newParentId ? (_uiState[newParentId]?.depth || 0) + 1 : 0;

    _nodes[nodeId] = {
      ...node,
      parent_id: newParentId,
      modified_at: new Date().toISOString()
    };
    _uiState[nodeId] = { ...uiState, depth: newDepth };

    if (!newParentId) {
      const parentIndex = _rootNodeIds.indexOf(parent.id);
      _rootNodeIds.splice(parentIndex + 1, 0, nodeId);
    }

    events.hierarchyChanged();
    _updateTrigger++;

    eventBus.emit<import('./eventTypes').HierarchyChangedEvent>({
      type: 'hierarchy:changed',
      namespace: 'lifecycle',
      source: serviceName,
      changeType: 'outdent',
      affectedNodes: [nodeId]
    });

    return true;
  }

  function deleteNode(nodeId: string): void {
    const node = _nodes[nodeId];
    if (!node) return;

    cleanupDebouncedOperations(nodeId);

    // Promote children
    const children = Object.values(_nodes)
      .filter((n) => n.parent_id === nodeId)
      .map((n) => n.id);

    for (const childId of children) {
      const child = _nodes[childId];
      if (child) {
        _nodes[childId] = {
          ...child,
          parent_id: node.parent_id,
          modified_at: new Date().toISOString()
        };
      }
    }

    delete _nodes[nodeId];
    delete _uiState[nodeId];

    const rootIndex = _rootNodeIds.indexOf(nodeId);
    if (rootIndex >= 0) {
      _rootNodeIds.splice(rootIndex, 1);
    }

    events.nodeDeleted(nodeId);
    _updateTrigger++;

    eventBus.emit<import('./eventTypes').NodeDeletedEvent>({
      type: 'node:deleted',
      namespace: 'lifecycle',
      source: serviceName,
      nodeId,
      parentId: node.parent_id || undefined
    });
  }

  function toggleExpanded(nodeId: string): boolean {
    try {
      const uiState = _uiState[nodeId];
      if (!uiState) return false;

      const newExpandedState = !uiState.expanded;
      _uiState[nodeId] = { ...uiState, expanded: newExpandedState };

      events.hierarchyChanged();
      _updateTrigger++;

      const status: import('./eventTypes').NodeStatus = newExpandedState ? 'expanded' : 'collapsed';
      const changeType: import('./eventTypes').HierarchyChangedEvent['changeType'] =
        newExpandedState ? 'expand' : 'collapse';

      eventBus.emit<import('./eventTypes').NodeStatusChangedEvent>({
        type: 'node:status-changed',
        namespace: 'coordination',
        source: serviceName,
        nodeId,
        status
      });

      eventBus.emit<import('./eventTypes').HierarchyChangedEvent>({
        type: 'hierarchy:changed',
        namespace: 'lifecycle',
        source: serviceName,
        changeType,
        affectedNodes: [nodeId]
      });

      return true;
    } catch (error) {
      console.error('Error toggling node expansion:', error);
      return false;
    }
  }

  function cleanupDebouncedOperations(nodeId: string): void {
    const operations = debouncedOperations.get(nodeId);
    if (operations) {
      if (operations.fastTimer) clearTimeout(operations.fastTimer);
      if (operations.expensiveTimer) clearTimeout(operations.expensiveTimer);
      debouncedOperations.delete(nodeId);
    }
  }

  function emitNodeUpdated(nodeId: string, updateType: string, newValue: unknown): void {
    eventBus.emit<import('./eventTypes').NodeUpdatedEvent>({
      type: 'node:updated',
      namespace: 'lifecycle',
      source: serviceName,
      nodeId,
      updateType: updateType as import('./eventTypes').NodeUpdatedEvent['updateType'],
      newValue
    });

    if (updateType === 'content') {
      eventBus.emit<import('./eventTypes').DecorationUpdateNeededEvent>({
        type: 'decoration:update-needed',
        namespace: 'interaction',
        source: serviceName,
        nodeId,
        decorationType: 'content',
        reason: 'content-changed',
        metadata: {}
      });

      eventBus.emit<import('./eventTypes').CacheInvalidateEvent>({
        type: 'cache:invalidate',
        namespace: 'coordination',
        source: serviceName,
        cacheKey: `node:${nodeId}`,
        scope: 'node',
        nodeId,
        reason: 'content-updated'
      });
    }
  }

  function initializeWithRichDemoData(): void {
    // Clear existing data
    Object.keys(_nodes).forEach((key) => delete _nodes[key]);
    Object.keys(_uiState).forEach((key) => delete _uiState[key]);
    _rootNodeIds = [];

    // Demo data would go here, but we're phasing this out
    // For now, just create an empty placeholder
  }

  // Helper methods for backward compatibility
  // function updateDescendantDepths(nodeId: string): void {
  //   const node = _nodes[nodeId];
  //   const uiState = _uiState[nodeId];
  //   if (!node || !uiState) return;

  //   const children = Object.values(_nodes)
  //     .filter(n => n.parent_id === nodeId)
  //     .map(n => n.id);

  //   for (const childId of children) {
  //     _uiState[childId] = {
  //       ..._uiState[childId],
  //       depth: uiState.depth + 1
  //     };
  //     updateDescendantDepths(childId);
  //   }
  // }

  return {
    // Reactive getters
    get nodes() {
      return new Map(Object.entries(_nodes).map(([id, node]) => [id, toLegacyShape(node)]));
    },
    get rootNodeIds() {
      return _rootNodeIds;
    },
    get activeNodeId() {
      return _activeNodeId;
    },
    get visibleNodes() {
      return _visibleNodes;
    },
    get _updateTrigger() {
      return _updateTrigger;
    },

    // Node operations
    findNode,
    createNode,
    createPlaceholderNode,
    updateNodeContent,
    updateNodeType,
    combineNodes,
    indentNode,
    outdentNode,
    deleteNode,
    toggleExpanded,

    // Initialization
    initializeWithSampleData: initializeWithRichDemoData,
    initializeWithRichDemoData,

    // Content processing methods for integration tests
    parseNodeContent(nodeId: string) {
      const node = _nodes[nodeId];
      if (!node) return null;
      return contentProcessor.parseMarkdown(node.content);
    },

    async renderNodeAsHTML(nodeId: string): Promise<string> {
      const node = _nodes[nodeId];
      if (!node) return '';
      const result = await contentProcessor.markdownToDisplay(node.content);
      return result || '';
    },

    getNodeHeaderLevel(nodeId: string): number {
      const node = _nodes[nodeId];
      if (!node) return 0;
      const headerMatch = node.content.match(/^(#{1,6})\s+/);
      return headerMatch ? headerMatch[1].length : 0;
    },

    getNodeDisplayText(nodeId: string): string {
      const node = _nodes[nodeId];
      if (!node) return '';
      return contentProcessor
        .displayToMarkdown(node.content)
        .replace(/[#*`[\]()]/g, '')
        .trim();
    },

    updateNodeContentWithProcessing(nodeId: string, content: string): boolean {
      const node = _nodes[nodeId];
      if (!node) return false;
      updateNodeContent(nodeId, content);
      return true;
    },

    // Legacy data initialization for tests
    initializeFromLegacyData(
      legacyData: Array<{
        id: string;
        type?: string;
        nodeType?: string;
        content: string;
        inheritHeaderLevel: number;
        children: string[];
        expanded: boolean;
        autoFocus: boolean;
        metadata?: Record<string, unknown>;
        parentId?: string;
      }>
    ): void {
      Object.keys(_nodes).forEach((id) => delete _nodes[id]);
      Object.keys(_uiState).forEach((id) => delete _uiState[id]);
      _rootNodeIds = [];

      const validLegacyData = legacyData.filter(
        (legacyNode): legacyNode is NonNullable<typeof legacyNode> =>
          legacyNode !== null &&
          legacyNode !== undefined &&
          typeof legacyNode === 'object' &&
          'id' in legacyNode &&
          Boolean(legacyNode.id)
      );

      for (const legacyNode of validLegacyData) {
        const node: Node = {
          id: legacyNode.id,
          node_type: legacyNode.type || legacyNode.nodeType || 'text',
          content: legacyNode.content || '',
          parent_id: legacyNode.parentId || null,
          root_id: legacyNode.parentId || legacyNode.id,
          before_sibling_id: null,
          created_at: new Date().toISOString(),
          modified_at: new Date().toISOString(),
          properties: legacyNode.metadata || {},
          mentions: []
        };

        const uiState = createDefaultUIState(legacyNode.id, {
          depth: 0,
          expanded: legacyNode.expanded ?? true,
          autoFocus: legacyNode.autoFocus ?? false,
          inheritHeaderLevel: legacyNode.inheritHeaderLevel ?? 0,
          isPlaceholder: false
        });

        _nodes[legacyNode.id] = node;
        _uiState[legacyNode.id] = uiState;
      }

      // Set root nodes
      const allChildIds = new Set(
        validLegacyData.flatMap((n) => (Array.isArray(n.children) ? n.children : []))
      );
      _rootNodeIds = validLegacyData.filter((n) => !allChildIds.has(n.id)).map((n) => n.id);

      // Update parent references and depths
      for (const nodeId of Object.keys(_nodes)) {
        const children = Object.values(_nodes)
          .filter((n) => n.parent_id === nodeId)
          .map((n) => n.id);

        for (const childId of children) {
          const child = _nodes[childId];
          const childUIState = _uiState[childId];
          if (child && childUIState) {
            _uiState[childId] = {
              ...childUIState,
              depth: (_uiState[nodeId]?.depth || 0) + 1
            };
          }
        }
      }

      _updateTrigger++;
      events.hierarchyChanged();
    }
  };
}

export type ReactiveNodeService = ReturnType<typeof createReactiveNodeService>;

// For backward compatibility with existing imports
export { createReactiveNodeService as ReactiveNodeService };

// Re-export LegacyNodeShape as Node for backward compatibility
export type { LegacyNodeShape as Node };
