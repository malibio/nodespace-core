/**
 * Type definitions for the node viewer plugin system
 */

import type { Component, Snippet } from 'svelte';

/**
 * Unified interface for page-level node viewers
 * Used by: DateNodeViewer, BaseNodeViewer, and future custom viewers
 *
 * Viewers are decoupled from the tab system - they report navigation via callbacks
 * and the parent component decides what to do with it. There is deliberately no
 * onTitleChange/title-push callback: tab titles are always derived from tab content
 * (see computeTabTitle in tab-title.ts), never pushed by a viewer as a side effect of
 * rendering — pushing a computed title during mount/render is what caused a
 * state_unsafe_mutation bug.
 */
export interface NodeViewerProps {
  nodeId: string; // The node to display (date string "2025-10-20", UUID, etc.)
  onNodeIdChange?: (nodeId: string) => void; // Callback when viewer navigates to different node
  header?: Snippet; // Optional custom header snippet
}

/**
 * Props interface for individual node components (TaskNode, TextNode, etc.)
 * These wrap BaseNode and are used WITHIN BaseNodeViewer
 */
export interface NodeComponentProps {
  nodeId: string;
  autoFocus?: boolean;
  content?: string;
  nodeType?: string;
  inheritHeaderLevel?: number;
  children?: string[];
}

export interface NodeViewerEventDetails {
  createNewNode: {
    afterNodeId: string;
    nodeType: string;
    currentContent?: string;
    newContent?: string;
    originalContent?: string;
    inheritHeaderLevel?: number;
    cursorAtBeginning?: boolean;
    insertAtBeginning?: boolean;
    focusOriginalNode?: boolean; // When true, focus original node instead of new node
  };
  contentChanged: { content: string };
  indentNode: { nodeId: string };
  outdentNode: { nodeId: string };
  navigateArrow: {
    nodeId: string;
    direction: 'up' | 'down';
    columnHint: number;
  };
  combineWithPrevious: {
    nodeId: string;
    currentContent: string;
  };
  deleteNode: { nodeId: string };
}

export type NodeViewerComponent = Component<NodeViewerProps>;

export interface ViewerRegistration {
  component?: NodeViewerComponent;
  lazyLoad?: () => Promise<{ default: NodeViewerComponent }>;
  priority: number; // Higher priority overrides default viewers
}

export interface BaseNodeViewer {
  // Required props
  nodeId: string;
  content: string;
  nodeType: string;

  // Optional props
  autoFocus?: boolean;
  inheritHeaderLevel?: number;
  children?: string[];
}
