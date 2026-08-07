/**
 * SchemaFormLoader tests — generic schema fallback gating.
 *
 * The loader must fetch the generic, schema-driven form for any type without a hardcoded
 * one. It previously gated that fetch on core/custom classification, which starved core
 * types that ship no frontend form (`project`) of any properties UI at all.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SchemaFormLoader } from '$lib/design/components/schema-form-loader.svelte';
import { backendAdapter } from '$lib/services/backend-adapter';
import type { SchemaNode } from '$lib/types/schema-node';

vi.mock('$lib/services/backend-adapter', () => ({
  backendAdapter: {
    getSchema: vi.fn()
  }
}));

const getSchema = vi.mocked(backendAdapter.getSchema);

function schemaFor(id: string): SchemaNode {
  return {
    id,
    nodeType: 'schema',
    content: id,
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1,
    isCore: true,
    schemaVersion: 1,
    description: '',
    fields: []
  };
}

describe('SchemaFormLoader', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads the generic schema for a core type with no hardcoded form', async () => {
    getSchema.mockResolvedValue(schemaFor('project'));
    const loader = new SchemaFormLoader();

    // `project` registers no schema form, so the generic fallback must be fetched.
    expect(await loader.loadForm('project')).toBe(false);
    await vi.waitFor(() => expect(loader.genericSchema?.id).toBe('project'));
    expect(getSchema).toHaveBeenCalledWith('project');
  });

  it('does not fetch a generic schema when a hardcoded form is registered', async () => {
    const loader = new SchemaFormLoader();

    // `task` registers TaskSchemaForm — the hardcoded form wins, no generic fetch.
    expect(await loader.loadForm('task')).toBe(true);
    expect(getSchema).not.toHaveBeenCalled();
    expect(loader.genericSchema).toBeNull();
  });

  it('re-fetches the generic schema when revisiting a type after a reset', async () => {
    getSchema.mockResolvedValue(schemaFor('project'));
    const loader = new SchemaFormLoader();

    await loader.loadForm('project');
    await vi.waitFor(() => expect(loader.genericSchema).not.toBeNull());

    // Navigating away clears the schema; the form-component lookup stays cached. Revisiting
    // must still repopulate the schema, or the properties panel renders nothing.
    loader.resetGenericSchema();
    expect(loader.genericSchema).toBeNull();

    await loader.loadForm('project');
    await vi.waitFor(() => expect(loader.genericSchema?.id).toBe('project'));
  });

  it('leaves genericSchema null when the backend has no schema for the type', async () => {
    getSchema.mockRejectedValue(new Error('schema not found'));
    const loader = new SchemaFormLoader();

    // A type with no schema at all must not render an empty form shell — the lookup is
    // swallowed and genericSchema stays null, so the viewer renders no properties panel.
    expect(await loader.loadForm('horizontal-line')).toBe(false);
    await vi.waitFor(() => expect(getSchema).toHaveBeenCalledWith('horizontal-line'));
    expect(loader.genericSchema).toBeNull();
  });

  it('ignores a response that is not a schema node', async () => {
    getSchema.mockResolvedValue({ id: 'text', nodeType: 'text' } as unknown as SchemaNode);
    const loader = new SchemaFormLoader();

    await loader.loadForm('text');
    await vi.waitFor(() => expect(getSchema).toHaveBeenCalledWith('text'));
    expect(loader.genericSchema).toBeNull();
  });
});
