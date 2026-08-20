/**
 * Schema Plugin Auto-Registration System
 *
 * Automatically registers custom entity schemas as plugins, enabling reference
 * components and viewer fallback without manual plugin registration or app restart.
 * Entity types are not slash-creatable (core or user-defined alike) — creation goes
 * through the sidenav's type view instead.
 *
 * ## Features
 *
 * - Converts schema definitions into plugin definitions
 * - Auto-registers plugins when schemas are created
 * - Auto-unregisters when schemas are deleted
 * - Registers existing schemas on app startup
 * - Hot-reload support (no restart required)
 *
 * ## Architecture
 *
 * ```
 * Schema Creation → Plugin Auto-Registration → Reference/Viewer Available
 *      ↓                    ↓                           ↓
 * SchemaService    createPluginFromSchema()      PluginRegistry
 * ```
 *
 * The plugin registry already supports runtime registration without restart.
 *
 * ## Usage
 *
 * ```typescript
 * // Initialize on app startup
 * await initializeSchemaPluginSystem();
 *
 * // Custom entities automatically become available for reference/viewer resolution
 * // User creates "invoice" schema → invoice nodes render via BaseNode fallback
 * ```
 *
 * @see packages/desktop-app/src/lib/plugins/plugin-registry.ts - Plugin registration
 * @see packages/desktop-app/src/lib/services/schema-service.ts - Schema management
 */

import type { PluginDefinition } from './types';
import { pluginRegistry } from './plugin-registry';
import { backendAdapter } from '$lib/services/backend-adapter';
import { type SchemaNode, isSchemaNode } from '$lib/types/schema-node';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('SchemaPluginLoader');

/**
 * Humanize a schema ID into a readable display name
 *
 * Converts technical IDs into user-friendly names:
 * - camelCase → Camel Case
 * - snake_case → Snake Case
 * - kebab-case → Kebab Case
 * - Capitalizes each word
 *
 * @param id - Schema ID to humanize
 * @returns Humanized display name
 *
 * @example
 * ```typescript
 * humanizeSchemaId('invoice') // 'Invoice'
 * humanizeSchemaId('salesInvoice') // 'Sales Invoice'
 * humanizeSchemaId('sales_invoice') // 'Sales Invoice'
 * humanizeSchemaId('sales-invoice') // 'Sales Invoice'
 * ```
 */
export function humanizeSchemaId(id: string): string {
  return (
    id
      // Insert space before uppercase letters (camelCase → camel Case)
      .replace(/([A-Z])/g, ' $1')
      // Replace underscores and hyphens with spaces
      .replace(/[_-]/g, ' ')
      // Trim leading/trailing spaces
      .trim()
      // Capitalize first letter of each word
      .replace(/\b\w/g, (char) => char.toUpperCase())
  );
}

/**
 * Convert a schema node into a plugin definition
 *
 * Creates a minimal plugin that registers the custom entity for reference/viewer
 * resolution via the BaseNode fallback. No slash command is generated — entity types
 * are not slash-creatable; instances are created via the sidenav's type view.
 *
 * @param schema - Schema node to convert
 * @returns Plugin definition ready for registration
 *
 * @example
 * ```typescript
 * const schemaNode = await backendAdapter.getSchema('invoice');
 * if (isSchemaNode(schemaNode)) {
 *   const plugin = createPluginFromSchema(schemaNode);
 * }
 * ```
 */
export function createPluginFromSchema(schema: SchemaNode): PluginDefinition {
  const schemaId = schema.id;
  const version = schema.schemaVersion;

  // Display name comes from schema content (the schema's name, e.g. "Customer").
  // description is for human-readable purpose/context, not the display name.
  const displayName = schema.content || humanizeSchemaId(schemaId);
  const description = schema.description ?? '';

  return {
    id: schemaId,
    name: displayName,
    description: description || `Create ${displayName}`,
    version: `${version}.0.0`, // Use schema version as plugin version
    config: {
      // No slash command — entity types (core and user-defined alike) are not
      // slash-creatable. User-defined types are created via the sidenav's type view
      // (customSchemas → handleSchemaClick → create instance).
      slashCommands: [],
      canHaveChildren: true,
      canBeChild: true
    }
    // No node component — custom entities render via BaseNode fallback.
    // No custom viewer — falls back to BaseNodeViewer.
    // No custom reference — falls back to BaseNodeReference.
  };
}

/**
 * Register a schema as a plugin immediately
 *
 * Fetches the schema node and registers it as a plugin. Core types
 * are skipped since they're already registered in core-plugins.ts.
 *
 * @param schemaId - ID of the schema to register
 * @throws {Error} If schema cannot be fetched or registration fails
 *
 * @example
 * ```typescript
 * // Register an invoice schema
 * await registerSchemaPlugin('invoice');
 * // Invoice nodes now resolve to a reference/viewer via the plugin registry
 * ```
 */
export async function registerSchemaPlugin(schemaId: string): Promise<void> {
  try {
    const node = await backendAdapter.getSchema(schemaId);

    // Verify it's a schema node
    if (!isSchemaNode(node)) {
      log.warn(`Node ${schemaId} is not a schema node`);
      return;
    }

    // Don't register core types (already registered in core-plugins.ts)
    // Access typed field directly (no helper needed)
    if (node.isCore) {
      log.debug(`Skipping core type registration: ${schemaId}`);
      return;
    }

    // Check if already registered (idempotent)
    if (pluginRegistry.hasPlugin(schemaId)) {
      log.debug(`Plugin already registered: ${schemaId}`);
      return;
    }

    const plugin = createPluginFromSchema(node);
    pluginRegistry.register(plugin);

    log.info(`Registered plugin for custom entity: ${schemaId}`);
  } catch (error) {
    log.error(`Failed to register schema plugin: ${schemaId}`, error);
    throw error;
  }
}

/**
 * Unregister a schema plugin
 *
 * Removes the plugin from the registry. Reference/viewer resolution for the
 * type will no longer be available.
 *
 * @param schemaId - ID of the schema to unregister
 *
 * @example
 * ```typescript
 * // Remove invoice plugin
 * unregisterSchemaPlugin('invoice');
 * ```
 */
export function unregisterSchemaPlugin(schemaId: string): void {
  if (!pluginRegistry.hasPlugin(schemaId)) {
    log.debug(`Skipping unregister, plugin not found: ${schemaId}`);
    return;
  }

  pluginRegistry.unregister(schemaId);
  log.info(`Unregistered plugin: ${schemaId}`);
}

/**
 * Result of schema plugin system initialization
 */
export interface InitializationResult {
  success: boolean;
  registeredCount: number;
  error?: string;
}

/**
 * Initialize schema plugin auto-registration system
 *
 * Registers all existing custom entity schemas on startup so their
 * reference/viewer resolution is available immediately on launch.
 *
 * Dynamic registration for schemas created/deleted at runtime is handled
 * by BrowserSyncService and TauriSyncListener, which call
 * registerSchemaPlugin / unregisterSchemaPlugin on domain events.
 *
 * Call this once during app startup in the root layout.
 *
 * @returns Initialization result with success status and details
 *
 * @example
 * ```typescript
 * // In +layout.svelte
 * onMount(async () => {
 *   const result = await initializeSchemaPluginSystem();
 *   if (!result.success) {
 *     // Handle initialization failure
 *     console.warn(`Custom entities unavailable: ${result.error}`);
 *   }
 * });
 * ```
 */
export async function initializeSchemaPluginSystem(): Promise<InitializationResult> {
  try {
    log.debug('Initializing schema plugin system...');

    // Register existing custom entity schemas on startup
    const nodes = await backendAdapter.getAllSchemas();
    // Access typed field directly (no helper needed)
    const customSchemas = nodes.filter(
      (node) => isSchemaNode(node) && !node.isCore
    );

    await Promise.all(
      customSchemas.map((node) => registerSchemaPlugin(node.id))
    );

    log.info(
      `Schema plugin system initialized (${customSchemas.length} custom entities registered)`
    );

    return {
      success: true,
      registeredCount: customSchemas.length
    };
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    log.error('Failed to initialize:', error);

    return {
      success: false,
      registeredCount: 0,
      error: errorMessage
    };
  }
}
