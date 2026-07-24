/**
 * Schema-aware property updates for viewer-rendered nodes.
 *
 * Routes task node fields through the type-safe task update path and everything
 * else through the generic nested-namespace properties path
 * (`properties[nodeType][fieldName]`), migrating any legacy flat-format properties
 * into the namespace on first write.
 */

import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { pluginRegistry } from '$lib/plugins/plugin-registry';

/**
 * Extract and transform node properties into component-compatible metadata.
 * Delegates to the plugin registry for type-specific transformations.
 */
export function extractNodeMetadata(node: {
  nodeType: string;
  properties?: Record<string, unknown>;
}): Record<string, unknown> {
  return pluginRegistry.extractNodeMetadata(node);
}

/**
 * Update a schema field value for a node.
 *
 * For task nodes, task-specific fields (status, priority, dueDate, assignee) route
 * through the type-safe task update path. All other fields — and all non-task
 * nodes — use the generic properties path.
 *
 * @param viewerId - Origin viewer id, recorded on the store update for echo suppression
 * @param targetNodeId - Node to update
 * @param fieldName - Schema field name (e.g. 'status', 'due_date')
 * @param value - New value for the field
 */
export function updateSchemaField(
  viewerId: string,
  targetNodeId: string,
  fieldName: string,
  value: unknown
): void {
  const targetNode = sharedNodeStore.getNode(targetNodeId);
  if (!targetNode) return;

  // Route task node property updates through type-safe path
  if (targetNode.nodeType === 'task') {
    // Map field names to TaskNodeUpdate structure
    // The task-specific fields are: status, priority, dueDate, assignee
    const taskFields = ['status', 'priority', 'due_date', 'dueDate', 'assignee'];

    if (taskFields.includes(fieldName)) {
      // Use type-safe task node update
      sharedNodeStore.updateTaskNode(
        targetNodeId,
        { [fieldName === 'due_date' ? 'dueDate' : fieldName]: value },
        { type: 'viewer', viewerId }
      );
      return;
    }
  }

  // Fallback: Generic update path via properties JSON
  // Build nested namespace (properties[nodeType][fieldName])
  const typeNamespace = targetNode.properties?.[targetNode.nodeType];
  const isOldFormat = !typeNamespace || typeof typeNamespace !== 'object';

  let updatedNamespace: Record<string, unknown> = {};

  if (isOldFormat) {
    // Migrate from old flat format - copy ALL existing flat properties into namespace
    updatedNamespace = { ...targetNode.properties };
    // Remove internal fields that shouldn't be in namespace
    delete updatedNamespace._schema_version;
  } else {
    // Already in new format - copy namespace
    updatedNamespace = { ...(typeNamespace as Record<string, unknown>) };
  }

  // Apply the update
  updatedNamespace[fieldName] = value;

  // Build final properties with ONLY the nested namespace
  // CRITICAL: When migrating from old format, ALL flat properties are now in the namespace
  // So we start fresh with ONLY the nested structure, dropping all flat properties
  const updatedProperties = isOldFormat
    ? {
        // Old format: Start fresh with ONLY nested structure (drops ALL flat properties)
        [targetNode.nodeType]: updatedNamespace
      }
    : {
        // New format: Preserve existing properties structure
        ...targetNode.properties,
        [targetNode.nodeType]: updatedNamespace
      };

  // Persist via sharedNodeStore. This builds the full intended properties bag
  // itself (and the old-format branch deliberately drops flat keys), so replace
  // rather than deep-merge — otherwise the dropped flat keys would reappear on
  // the local optimistic node.
  sharedNodeStore.updateNode(
    targetNodeId,
    { properties: updatedProperties },
    { type: 'viewer', viewerId },
    { replaceProperties: true }
  );
}
