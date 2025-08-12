/**
 * Component exports for NodeSpace
 *
 * Centralizes component imports for better organization.
 */

export { default as TextNode } from './TextNode.svelte';
export { default as TextNodeDemo } from './TextNodeDemo.svelte';
export { default as NodeTree } from './NodeTree.svelte';
export { default as HierarchyDemo } from './HierarchyDemo.svelte';
export { default as SoftNewlineDemo } from './SoftNewlineDemo.svelte';
export { default as BulletConversionDemo } from './BulletConversionDemo.svelte';
export { default as WYSIWYGDemo } from './WYSIWYGDemo.svelte';
export { default as MultilineBlockDemo } from './MultilineBlockDemo.svelte';
export { default as AIIntegrationDemo } from './AIIntegrationDemo.svelte';
export type { TreeNodeData } from '$lib/types/tree';

// Re-export types from services
export type {
  TextNodeData,
  TextSaveResult,
  HierarchicalTextNode
} from '$lib/services/mockTextService';

export type { MarkdownOptions } from '$lib/services/markdownUtils';
