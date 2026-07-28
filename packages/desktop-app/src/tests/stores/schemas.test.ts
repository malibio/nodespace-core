/**
 * Schemas store — sidenav built-in filter.
 *
 * `builtInSchemas` surfaces core schemas whose id is in the private
 * SIDENAV_CORE_TYPES set. `project` is a built-in core node type (backend
 * core#134) and must appear alongside `task`, while non-core schemas and core
 * schemas outside the set stay excluded.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { SchemaNode } from '$lib/types/schema-node';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    getAllSchemas: vi.fn(async () => [])
  }
}));

// Neutralize the module-load reconnect registration so importing the store
// does not touch the real daemon-status service.
vi.mock('$lib/services/daemon-status', () => ({
  onDaemonReconnect: vi.fn(() => () => {})
}));

import { schemasStore } from '$lib/stores/schemas.svelte';

function makeSchema(id: string, isCore: boolean): SchemaNode {
  return {
    id,
    content: id,
    createdAt: '2026-07-28T00:00:00Z',
    modifiedAt: '2026-07-28T00:00:00Z',
    version: 1,
    isCore,
    schemaVersion: 1,
    fields: []
  };
}

describe('schemasStore.builtInSchemas — sidenav core types', () => {
  beforeEach(() => {
    schemasStore.schemas = [];
  });

  it('includes the core project schema alongside task and skill', () => {
    schemasStore.schemas = [
      makeSchema('task', true),
      makeSchema('project', true),
      makeSchema('skill', true),
      makeSchema('text', true), // core, but not a sidenav core type
      makeSchema('project', false) // custom schema that happens to share the id
    ];

    const ids = schemasStore.builtInSchemas.map((s) => s.id);

    expect(ids).toContain('project');
    expect(ids).toContain('task');
    expect(ids).toContain('skill');
    // Structural core types and non-core schemas are excluded.
    expect(ids).not.toContain('text');
    // The custom (non-core) project schema is routed to customSchemas instead.
    expect(schemasStore.builtInSchemas.filter((s) => s.id === 'project')).toHaveLength(1);
    expect(schemasStore.customSchemas.map((s) => s.id)).toContain('project');
  });
});
