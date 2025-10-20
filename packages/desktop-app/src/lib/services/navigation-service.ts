/**
 * NavigationService - Handles nodespace:// link navigation and dynamic tab creation
 *
 * Phase 4a Implementation (Issue #297):
 * - Intercepts clicks on nodespace:// links
 * - Resolves node UUID to determine appropriate viewer type
 * - Creates new tabs or switches to existing tabs
 * - Supports Cmd+Click for opening in new tab vs regular click for current tab
 *
 * Architecture:
 * - Singleton pattern for centralized navigation logic
 * - Integrates with SharedNodeStore for node data access
 * - Uses tab store for tab management
 * - Supports all viewer types (date, task, text, ai-chat)
 */

import { v4 as uuidv4 } from 'uuid';
import { addTab, setActiveTab, tabState } from '$lib/stores/navigation';
import { sharedNodeStore } from './shared-node-store';
import { get } from 'svelte/store';

export interface NavigationTarget {
  nodeId: string;
  nodeType: string;
  title: string;
}

export class NavigationService {
  private static instance: NavigationService;

  private constructor() {}

  static getInstance(): NavigationService {
    if (!NavigationService.instance) {
      NavigationService.instance = new NavigationService();
    }
    return NavigationService.instance;
  }

  /**
   * Resolve a node UUID to navigation target information
   */
  async resolveNodeTarget(nodeId: string): Promise<NavigationTarget | null> {
    // Get node from SharedNodeStore
    const node = sharedNodeStore.getNode(nodeId);

    if (!node) {
      console.warn(`NavigationService: Node ${nodeId} not found in store`);
      return null;
    }

    return {
      nodeId: node.id,
      nodeType: node.nodeType,
      title: this.generateTabTitle(node)
    };
  }

  /**
   * Generate a human-readable tab title from node content
   */
  private generateTabTitle(node: {
    nodeType: string;
    properties?: { date?: string };
    content?: string;
  }): string {
    // For date nodes, use formatted date
    if (node.nodeType === 'date') {
      const date = new Date(node.properties?.date || Date.now());
      return this.formatDateTitle(date);
    }

    // For other nodes, use first line of content (max 40 chars)
    if (node.content && typeof node.content === 'string') {
      const firstLine = node.content.split('\n')[0].trim();
      return firstLine.length > 40 ? firstLine.substring(0, 37) + '...' : firstLine;
    }

    // Fallback to node type
    return `${node.nodeType} Node`;
  }

  private formatDateTitle(date: Date): string {
    const today = new Date();
    const tomorrow = new Date(today);
    tomorrow.setDate(tomorrow.getDate() + 1);
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    const normalizeDate = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const targetDate = normalizeDate(date);
    const todayNormalized = normalizeDate(today);
    const tomorrowNormalized = normalizeDate(tomorrow);
    const yesterdayNormalized = normalizeDate(yesterday);

    if (targetDate.getTime() === todayNormalized.getTime()) return 'Today';
    if (targetDate.getTime() === tomorrowNormalized.getTime()) return 'Tomorrow';
    if (targetDate.getTime() === yesterdayNormalized.getTime()) return 'Yesterday';

    const year = targetDate.getFullYear();
    const month = String(targetDate.getMonth() + 1).padStart(2, '0');
    const day = String(targetDate.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  /**
   * Navigate to a node by UUID
   * @param nodeId - The UUID of the node to navigate to
   * @param openInNewTab - If true, always create a new tab. If false, switch to existing tab if present.
   */
  async navigateToNode(nodeId: string, openInNewTab: boolean = false): Promise<void> {
    const target = await this.resolveNodeTarget(nodeId);

    if (!target) {
      console.error(`NavigationService: Cannot navigate to node ${nodeId}`);
      return;
    }

    // Check if tab already exists for this node
    const currentState = get(tabState);
    const existingTab = currentState.tabs.find(
      (tab) =>
        tab.content &&
        typeof tab.content === 'object' &&
        'nodeId' in tab.content &&
        (tab.content as { nodeId: string }).nodeId === nodeId
    );

    if (existingTab && !openInNewTab) {
      // Switch to existing tab
      setActiveTab(existingTab.id);
      return;
    }

    // Create new tab
    const newTab = {
      id: uuidv4(),
      title: target.title,
      type: this.mapNodeTypeToTabType(target.nodeType),
      content: {
        nodeId: target.nodeId,
        nodeType: target.nodeType
      },
      closeable: true
    };

    addTab(newTab);
  }

  /**
   * Map node type to tab type
   * Extend this as new node types are added
   */
  private mapNodeTypeToTabType(nodeType: string): 'date' | 'node' | 'placeholder' {
    switch (nodeType) {
      case 'date':
        return 'date';
      case 'task':
      case 'text':
      case 'ai-chat':
        return 'node'; // Generic node viewer
      default:
        return 'placeholder';
    }
  }
}

export const navigationService = NavigationService.getInstance();
