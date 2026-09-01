/**
 * Schemas Store
 *
 * Global reactive store for schema definitions.
 * Used by navigation-sidebar and any other UI that needs the schema list.
 *
 * Mirrors the collectionsData pattern so schema changes from MCP/external
 * sources are reflected in the sidebar without requiring a page refresh.
 *
 * Svelte 5 rune store (ADR-049): the raw list lives on the class as `$state`;
 * `builtInSchemas` / `customSchemas` are computed getters, not `derived` stores.
 */

import { backendAdapter } from '$lib/services/backend-adapter';
import { createLogger } from '$lib/utils/logger';
import { onDaemonReconnect } from '$lib/services/daemon-status';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import type { SchemaNode } from '$lib/types/schema-node';

const log = createLogger('SchemasStore');

// Core schema IDs that are user-queryable and should appear in the sidenav.
// Structural/inline types (text, date, header, code-block, etc.) are excluded —
// they are node content primitives, not entity types users browse or filter.
const SIDENAV_CORE_TYPES = new Set(['task', 'project', 'skill', 'person', 'agent-guidance']);

class SchemasStore {
  /** Raw schema list */
  schemas = $state<SchemaNode[]>([]);

  /** Core schemas shown in sidenav (only user-queryable ones like "task") */
  get builtInSchemas(): SchemaNode[] {
    return this.schemas.filter((s) => s.isCore && SIDENAV_CORE_TYPES.has(s.id));
  }

  /** User-created custom schemas */
  get customSchemas(): SchemaNode[] {
    return this.schemas.filter((s) => !s.isCore);
  }

  /** Load all schemas from the backend and update the store. */
  async loadSchemas(): Promise<void> {
    // ADR-053: capture the database generation before the daemon read so a
    // switch mid-flight is detectable below and the response is dropped
    // rather than written into the now-active database's store (this store
    // has no private generation counter of its own, so it shares the
    // cross-store epoch the same way `refreshDatabaseSettings` and
    // `loadChildrenForParent` do).
    const epoch = sharedNodeStore.currentEpoch();
    try {
      const schemas = await backendAdapter.getAllSchemas();
      if (sharedNodeStore.currentEpoch() !== epoch) {
        log.debug('Discarding schemas load that resolved after the database switched');
        return;
      }
      this.schemas = schemas;
      log.debug('Schemas loaded', { count: schemas.length });
    } catch (err) {
      log.error('Failed to load schemas', err);
    }
  }
}

export const schemasStore = new SchemasStore();

/** Load all schemas from the backend and update the store. */
export const loadSchemas = (): Promise<void> => schemasStore.loadSchemas();

export const schemasData = { loadSchemas };

// Registered once at module load (this file is a singleton — ES modules only
// evaluate once), not per component mount. Retries loadSchemas whenever the
// daemon becomes reachable, so a load that failed while the daemon was still
// starting up recovers automatically without a manual reload.
onDaemonReconnect(() => schemasStore.loadSchemas());
