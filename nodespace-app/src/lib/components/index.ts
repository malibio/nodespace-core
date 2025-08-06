/**
 * Component exports for NodeSpace
 * 
 * Centralizes component imports for better organization.
 */

export { default as TextNode } from './TextNode.svelte';
export { default as TextNodeDemo } from './TextNodeDemo.svelte';

// Re-export types from services
export type { 
  TextNodeData, 
  TextSaveResult 
} from '$lib/services/mockTextService';

export type {
  MarkdownOptions
} from '$lib/services/markdownUtils';