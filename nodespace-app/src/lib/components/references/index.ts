/**
 * Node Reference Components Registry
 * 
 * Exports all node reference components and provides utilities
 * for dynamic component resolution.
 */

import BaseNodeReference from './BaseNodeReference.svelte';
import TaskNodeReference from './TaskNodeReference.svelte';
import DateNodeReference from './DateNodeReference.svelte';
import UserNodeReference from './UserNodeReference.svelte';

import type { ComponentDecoration } from '$lib/types/ComponentDecoration';
import type { SvelteComponent } from 'svelte';

// Export components
export {
  BaseNodeReference,
  TaskNodeReference,
  DateNodeReference,
  UserNodeReference
};

// Core node reference components registry
// Plugin node types can extend this registry at build time
export const NODE_REFERENCE_COMPONENTS: Record<string, any> = {
  // Core node types (built-in)
  'BaseNodeReference': BaseNodeReference,
  'TaskNodeReference': TaskNodeReference, 
  'DateNodeReference': DateNodeReference,
  'UserNodeReference': UserNodeReference,
  
  // Node type mappings (for backward compatibility)
  'base': BaseNodeReference,
  'text': BaseNodeReference,
  'task': TaskNodeReference,
  'date': DateNodeReference,
  'user': UserNodeReference,
  'document': BaseNodeReference,  // Will use BaseNodeReference until DocumentNodeReference is implemented
  'ai_chat': BaseNodeReference,   // Will use BaseNodeReference until AINodeReference is implemented
  'entity': BaseNodeReference,    // Will use BaseNodeReference until EntityNodeReference is implemented
  'query': BaseNodeReference,     // Will use BaseNodeReference until QueryNodeReference is implemented
  
  // Plugin node types would be added here at build time, e.g.:
  // 'pdf': PdfNodeReference,     // From @nodespace/pdf-plugin
  // 'image': ImageNodeReference, // From @nodespace/image-plugin
  // 'video': VideoNodeReference, // From @nodespace/media-plugin
};

/**
 * Get the appropriate component for a node type
 */
export function getNodeReferenceComponent(nodeType: string): any {
  return NODE_REFERENCE_COMPONENTS[nodeType] || BaseNodeReference;
}

/**
 * Get component name for a node type (used for serialization)
 */
export function getComponentName(nodeType: string): string {
  const component = getNodeReferenceComponent(nodeType);
  
  // Map component back to name
  if (component === TaskNodeReference) return 'TaskNodeReference';
  if (component === DateNodeReference) return 'DateNodeReference';
  if (component === UserNodeReference) return 'UserNodeReference';
  
  return 'BaseNodeReference';
}

/**
 * Get component by name (used for deserialization and hydration)
 */
export function getComponentByName(componentName: string): any {
  // Try registry first (supports plugin components)
  const component = NODE_REFERENCE_COMPONENTS[componentName];
  if (component) {
    return component;
  }

  // Fallback to core components
  switch (componentName) {
    case 'TaskNodeReference': return TaskNodeReference;
    case 'DateNodeReference': return DateNodeReference;
    case 'UserNodeReference': return UserNodeReference;
    case 'BaseNodeReference':
    default:
      return BaseNodeReference;
  }
}

/**
 * Register a plugin node reference component (for build-time extension)
 * This allows plugin packages to register their components during the build process
 */
export function registerNodeReferenceComponent(
  componentName: string, 
  nodeType: string, 
  component: any
): void {
  NODE_REFERENCE_COMPONENTS[componentName] = component;
  NODE_REFERENCE_COMPONENTS[nodeType] = component;
  
  console.debug(`NodeSpace: Registered node reference component`, { 
    componentName, 
    nodeType, 
    component: component.name 
  });
}

/**
 * Create a component decoration for a node type
 */
export function createComponentDecoration(
  nodeType: string, 
  props: Record<string, any>
): ComponentDecoration {
  return {
    component: getNodeReferenceComponent(nodeType),
    props,
    metadata: {
      nodeType,
      componentName: getComponentName(nodeType)
    }
  };
}