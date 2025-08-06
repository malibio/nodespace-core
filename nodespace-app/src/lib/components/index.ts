/**
 * Component exports for NodeSpace
 *
 * Centralizes component imports for better organization.
 */

export { default as TextNode } from './TextNode.svelte';
export { default as TextNodeDemo } from './TextNodeDemo.svelte';
export { default as NodeTree } from './NodeTree.svelte';
export { default as NodeTreeDemo } from './NodeTreeDemo.svelte';

// Re-export types from services
export type { TextNodeData, TextSaveResult } from '$lib/services/mockTextService';

// Re-export types from NodeTree
export type { HierarchicalNode, TreeState } from './NodeTree.svelte';

export type { MarkdownOptions } from '$lib/services/markdownUtils';
