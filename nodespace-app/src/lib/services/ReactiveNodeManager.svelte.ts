/**
 * ReactiveNodeManager - Svelte 5 reactive wrapper for NodeManager
 * 
 * This file provides Svelte 5 $state() reactivity for the NodeManager service.
 * The core logic remains in NodeManager.ts for testability.
 */

import { NodeManager, type NodeManagerEvents, type Node } from './NodeManager';

export class ReactiveNodeManager extends NodeManager {
  private _reactiveNodes = $state(new Map<string, Node>());
  private _reactiveRootNodeIds = $state<string[]>([]);
  
  constructor(events: NodeManagerEvents) {
    super(events);
    
    // Sync reactive state with base class state
    this.syncReactiveState();
  }
  
  /**
   * Override to use reactive state
   */
  get nodes(): Map<string, Node> {
    return this._reactiveNodes;
  }
  
  /**
   * Override to use reactive state
   */
  get rootNodeIds(): string[] {
    return this._reactiveRootNodeIds;
  }
  
  /**
   * Override to sync reactive state after operations
   */
  initializeFromLegacyData(legacyNodes: any[]): void {
    super.initializeFromLegacyData(legacyNodes);
    this.syncReactiveState();
  }
  
  /**
   * Override to sync reactive state after operations
   */
  createNode(afterNodeId: string, content: string = '', nodeType: string = 'text'): string {
    const result = super.createNode(afterNodeId, content, nodeType);
    this.syncReactiveState();
    return result;
  }
  
  /**
   * Override to sync reactive state after operations
   */
  deleteNode(nodeId: string): void {
    super.deleteNode(nodeId);
    this.syncReactiveState();
  }
  
  /**
   * Override to sync reactive state after operations
   */
  indentNode(nodeId: string): boolean {
    const result = super.indentNode(nodeId);
    if (result) {
      this.syncReactiveState();
    }
    return result;
  }
  
  /**
   * Override to sync reactive state after operations
   */
  outdentNode(nodeId: string): boolean {
    const result = super.outdentNode(nodeId);
    if (result) {
      this.syncReactiveState();
    }
    return result;
  }
  
  /**
   * Override to sync reactive state after operations
   */
  toggleExpanded(nodeId: string): boolean {
    const result = super.toggleExpanded(nodeId);
    if (result) {
      this.syncReactiveState();
    }
    return result;
  }
  
  /**
   * Override to sync reactive state after operations
   */
  combineNodes(currentNodeId: string, previousNodeId: string): void {
    super.combineNodes(currentNodeId, previousNodeId);
    this.syncReactiveState();
  }
  
  /**
   * Sync reactive state with base class state
   */
  private syncReactiveState(): void {
    // Clear reactive state
    this._reactiveNodes.clear();
    this._reactiveRootNodeIds.length = 0;
    
    // Copy from base class
    const baseNodes = super.nodes;
    const baseRootIds = super.rootNodeIds;
    
    // Update reactive state
    for (const [id, node] of baseNodes) {
      this._reactiveNodes.set(id, node);
    }
    
    this._reactiveRootNodeIds.push(...baseRootIds);
  }
}