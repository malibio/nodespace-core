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
import type { SchemaNode } from '$lib/types/schema-node';

const log = createLogger('SchemasStore');

// Core schema IDs that are user-queryable and should appear in the sidenav.
// Structural/inline types (text, date, header, code-block, etc.) are excluded —
// they are node content primitives, not entity types users browse or filter.
const SIDENAV_CORE_TYPES = new Set(['task', 'project', 'skill']);

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
    try {
      const schemas = await backendAdapter.getAllSchemas();
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
