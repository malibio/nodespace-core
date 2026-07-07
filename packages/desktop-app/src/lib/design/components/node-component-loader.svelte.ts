/**
 * NodeComponentLoader — lazy-loads and caches node components from the plugin registry.
 *
 * Node components are loaded on demand (as node data arrives) and cached in a reactive
 * `$state` record so the viewer re-renders once a component becomes available. One
 * instance is created per viewer component. Loading is event-driven — the viewer calls
 * `load()` when it sees a new node type; there is no `$effect` watching for changes.
 */

import { pluginRegistry } from '$lib/plugins/plugin-registry';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('NodeComponentLoader');

export class NodeComponentLoader {
  /**
   * Cache of loaded node components keyed by node type.
   * Uses a plain object (not a Map) for Svelte 5 reactive tracking.
   */
  loaded = $state<Record<string, unknown>>({});

  /** Whether a component for the given node type has been loaded. */
  has(nodeType: string): boolean {
    return nodeType in this.loaded;
  }

  /** The loaded component for the given node type, or undefined if not yet loaded. */
  get(nodeType: string): unknown {
    return this.loaded[nodeType];
  }

  /**
   * Seed the cache from the registry's persistent cache of already-loaded components.
   * Uses in-place assignment to avoid triggering reactive updates during mount.
   */
  seedFromRegistry(): void {
    Object.assign(this.loaded, pluginRegistry.getAllLoadedNodeComponents());
  }

  /**
   * Load a node component from the plugin registry if not already loaded.
   * Cached in `loaded` for subsequent renders.
   */
  async load(nodeType: string): Promise<void> {
    // Skip if already loaded
    if (nodeType in this.loaded) return;

    try {
      const component = await pluginRegistry.getNodeComponent(nodeType);
      if (component) {
        this.loaded = { ...this.loaded, [nodeType]: component };
      }
    } catch (error) {
      log.warn(`Failed to load component for ${nodeType}:`, error);
    }
  }
}
