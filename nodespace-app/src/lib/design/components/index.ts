/**
 * Design Component Exports
 * 
 * Centralizes exports for all design system components.
 */

export { default as BaseNode } from './BaseNode.svelte';
export { default as SmartTextNode } from './SmartTextNode.svelte';
export { default as ThemeProvider } from './ThemeProvider.svelte';
export { default as MockTextElement } from './MockTextElement.svelte';
export { default as MockPositioningTest } from './MockPositioningTest.svelte';
export { default as BaseNodeTest } from './BaseNodeTest.svelte';

// Re-export node types for convenience
export type { NodeType } from './BaseNode.svelte';

// Re-export services and utilities
export type {
  BulletConversionConfig,
  BulletConversionResult
} from '$lib/services/bulletToNodeConverter';

export { 
  BulletToNodeConverter, 
  BulletProcessingUtils,
  bulletToNodeConverter,
  taskBulletConverter 
} from '$lib/services/bulletToNodeConverter';