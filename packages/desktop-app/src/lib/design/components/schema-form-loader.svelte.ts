/**
 * SchemaFormLoader — lazy-loads type-specific and generic schema forms for the viewed node.
 *
 * Type-specific forms (e.g. TaskSchemaForm) come from the plugin registry. Any node type
 * with no registered form — user-defined schema types and core types alike — falls back to
 * a generic schema form driven by the node's SchemaNode definition, fetched from the backend.
 *
 * State lives here as `$state` so the viewer re-renders when a form or schema resolves.
 * One instance per viewer component; loading is event-driven (the viewer calls the load
 * methods when the viewed node changes), not `$effect`-driven.
 */

import { backendAdapter } from '$lib/services/backend-adapter';
import { pluginRegistry } from '$lib/plugins/plugin-registry';
import { isSchemaNode, type SchemaNode } from '$lib/types/schema-node';
import { needsGenericSchemaForm } from './node-type-predicates';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('SchemaFormLoader');

export class SchemaFormLoader {
  /**
   * Cache of loaded schema-form components keyed by node type.
   * `null` = checked, no type-specific form registered; a component = typed form.
   */
  loadedForms = $state<Record<string, unknown>>({});

  /** Generic schema definition for a node type with no hardcoded schema form. */
  genericSchema = $state<SchemaNode | null>(null);

  /**
   * True when the current generic schema has a title_template — header should be read-only,
   * and its displayed value comes from the node's computed `title` rather than its content.
   *
   * `genericSchema` now loads for any type with no hardcoded form — core types included —
   * so this is no longer scoped to custom schema types. It stays false for core types only
   * because none of them ships a title_template today. The backend's `compute_title()` uses
   * a title_template for any type whose schema has one, core or not; the two agree solely on
   * that "no core template exists yet" assumption. Shipping a title_template on a core type
   * would need both sides updated together, or the header will display content where a
   * computed title is expected.
   */
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
    // Skip if already loaded (check for both component and explicit null).
    // `null` means "checked, no type-specific form" — the component lookup is cached, but
    // the generic schema is not: `resetGenericSchema()` clears it on every navigation, so
    // it must be re-fetched here or revisiting a type would render no properties form.
    if (nodeType in this.loadedForms) {
      const cached = this.loadedForms[nodeType];
      if (cached === null) {
        this.loadGenericSchema(nodeType);
        return false;
      }
      return true;
    }

    // Check if plugin has a schema form registered
    if (needsGenericSchemaForm(nodeType)) {
      // Mark as null to indicate we checked and there's no type-specific form
      this.loadedForms = { ...this.loadedForms, [nodeType]: null };
      // No hardcoded form for this type — fall back to the generic schema-driven form.
      // Unconditional by design: reaching this branch already means "no type-specific
      // form", which is the only thing the generic fallback depends on. Core types with
      // no registered form (e.g. project) need it exactly as much as user-defined ones.
      // Types with no schema at all simply resolve to no schema and render nothing.
      this.loadGenericSchema(nodeType);
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

  /** Load the generic schema definition for a node type from the backend. */
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
