/**
 * SchemaFormLoader — lazy-loads type-specific and generic schema forms for the viewed node.
 *
 * Type-specific forms (e.g. TaskSchemaForm) come from the plugin registry. Custom schema
 * node types (UUID nodeType) with no registered form fall back to a generic schema form
 * driven by the node's SchemaNode definition, fetched from the backend.
 *
 * State lives here as `$state` so the viewer re-renders when a form or schema resolves.
 * One instance per viewer component; loading is event-driven (the viewer calls the load
 * methods when the viewed node changes), not `$effect`-driven.
 */

import { backendAdapter } from '$lib/services/backend-adapter';
import { pluginRegistry } from '$lib/plugins/plugin-registry';
import { isSchemaNode, type SchemaNode } from '$lib/types/schema-node';
import { isCustomSchemaType } from './node-type-predicates';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('SchemaFormLoader');

export class SchemaFormLoader {
  /**
   * Cache of loaded schema-form components keyed by node type.
   * `null` = checked, no type-specific form registered (core types); a component = typed form.
   */
  loadedForms = $state<Record<string, unknown>>({});

  /** Generic schema definition for a custom schema node type (UUID nodeType). */
  genericSchema = $state<SchemaNode | null>(null);

  /** True when the current generic schema has a title_template — header should be read-only. */
  get hasTitleTemplate(): boolean {
    return this.genericSchema?.titleTemplate != null;
  }

  /** The loaded type-specific form for a node type (may be null if none registered). */
  getForm(nodeType: string): unknown {
    return this.loadedForms[nodeType];
  }

  /** Reset the generic schema (call when navigating to a different node). */
  resetGenericSchema(): void {
    this.genericSchema = null;
  }

  /**
   * Load a schema form component from the plugin registry if not already loaded.
   * Returns true if a type-specific form exists, false if the generic fallback should be used.
   * Cached in `loadedForms` for subsequent renders.
   */
  async loadForm(nodeType: string): Promise<boolean> {
    // Skip if already loaded (check for both component and explicit null)
    if (nodeType in this.loadedForms) {
      return this.loadedForms[nodeType] !== null;
    }

    // Check if plugin has a schema form registered
    if (!pluginRegistry.hasSchemaForm(nodeType)) {
      // Mark as null to indicate we checked and there's no type-specific form
      this.loadedForms = { ...this.loadedForms, [nodeType]: null };
      // For custom schema types (UUID nodeType), load generic schema
      if (isCustomSchemaType(nodeType)) {
        this.loadGenericSchema(nodeType);
      }
      return false;
    }

    try {
      const component = await pluginRegistry.getSchemaForm(nodeType);
      if (component) {
        this.loadedForms = { ...this.loadedForms, [nodeType]: component };
        return true;
      }
      // Mark as null if loading failed
      this.loadedForms = { ...this.loadedForms, [nodeType]: null };
      return false;
    } catch (error) {
      log.warn(`Failed to load schema form for ${nodeType}:`, error);
      this.loadedForms = { ...this.loadedForms, [nodeType]: null };
      return false;
    }
  }

  /** Load the generic schema definition for a custom schema node type from the backend. */
  async loadGenericSchema(nodeType: string): Promise<void> {
    try {
      const schemaNode = await backendAdapter.getSchema(nodeType);
      if (isSchemaNode(schemaNode)) {
        this.genericSchema = schemaNode;
      }
    } catch (error) {
      log.warn(`Failed to load generic schema for ${nodeType}:`, error);
    }
  }
}
