/**
 * Property-form field dispatch — one shared editor, three namespaces.
 *
 * All three property forms render the same leaf controls and open the SAME
 * nested (object/array) editor modal, but they store values in three different
 * places:
 *   - GenericSchemaForm   → flat        properties[<field>]
 *   - TaskSchemaForm      → task ns     properties.task[<field>]
 *   - SchemaPropertyForm  → type ns     properties[<nodeType>][<field>]
 *
 * The modal therefore owns no persistence: each form supplies the current value
 * and the write. These tests drive a real edit through each form's modal and
 * assert the rebuilt value lands at the right path — the crux of the shared
 * modal being safe to reuse across namespaces — plus the boolean leaf that
 * SchemaPropertyForm previously left unimplemented.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { SchemaField, SchemaNode } from '$lib/types/schema-node';
import type { Node } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// GenericSchemaForm gates its Relationships trigger on this service; stub it so
// the gate never reaches a daemon. It is a plain function export, not a singleton.
vi.mock('$lib/services/relationship-viewer-service', () => ({
  loadNodeRelationshipsView: vi.fn().mockResolvedValue({ nodeType: 'gadget', groups: [] })
}));

import GenericSchemaForm from '$lib/components/schema/generic-schema-form.svelte';
import TaskSchemaForm from '$lib/components/property-forms/task-schema-form.svelte';
import SchemaPropertyForm from '$lib/components/property-forms/schema-property-form.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { backendAdapter } from '$lib/services/backend-adapter';

function field(partial: Partial<SchemaField> & { name: string; type: string }): SchemaField {
  return { protection: 'user', indexed: false, ...partial };
}

// One object field with a single string sub-field: enough to make an edit and
// observe the whole rebuilt object being persisted.
const ADDRESS_FIELD = field({
  name: 'address',
  type: 'object',
  fields: [field({ name: 'street', type: 'string' })]
});

function schemaWith(fields: SchemaField[], nodeType: string): SchemaNode {
  return {
    id: `schema-${nodeType}`,
    content: nodeType,
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    isCore: false,
    schemaVersion: 1,
    fields
  };
}

function nodeWith(nodeType: string, properties: Record<string, unknown>): Node {
  return {
    id: 'node-1',
    nodeType,
    content: 'A node',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties
  } as Node;
}

/** The `properties` bag of the single updateNode call under test. */
function persistedProperties(updateNode: ReturnType<typeof vi.fn>): Record<string, unknown> {
  expect(updateNode).toHaveBeenCalledTimes(1);
  const changes = updateNode.mock.calls[0][1] as Partial<Node>;
  return changes.properties as Record<string, unknown>;
}

/**
 * Open the nested editor for the only nested field on screen and type into its
 * one string sub-field, returning once the edit has been emitted. The modal is
 * portalled to <body>, so this queries via `screen` rather than the container.
 */
async function editStreetThroughModal(): Promise<void> {
  // The nested field's trigger summarises an empty object as "0 fields".
  await fireEvent.click(screen.getByText('0 fields'));
  await waitFor(() => expect(screen.getByLabelText('Street')).toBeTruthy());
  await fireEvent.input(screen.getByLabelText('Street'), { target: { value: '1 Main' } });
}

let updateNodeSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {

  // Spy (never module-mock) the shared singletons so nothing leaks across the fork.
  updateNodeSpy = vi.fn();
  vi.spyOn(sharedNodeStore, 'updateNode').mockImplementation(
    updateNodeSpy as unknown as typeof sharedNodeStore.updateNode
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('GenericSchemaForm — nested values persist FLAT', () => {
  beforeEach(() => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      nodeWith('gadget', { unrelated: 'keep me' })
    );
  });

  it('writes properties[<field>] and leaves other properties intact', async () => {
    render(GenericSchemaForm, {
      props: { nodeId: 'node-1', schema: schemaWith([ADDRESS_FIELD], 'gadget'), autoOpen: true }
    });

    await editStreetThroughModal();

    expect(persistedProperties(updateNodeSpy)).toEqual({
      unrelated: 'keep me',
      address: { street: '1 Main' }
    });
  });
});

describe('TaskSchemaForm — nested values persist under properties.task', () => {
  beforeEach(() => {
    vi.spyOn(backendAdapter, 'getSchema').mockResolvedValue(
      schemaWith([ADDRESS_FIELD], 'task') as never
    );
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      nodeWith('task', { task: { estimate: 3 } })
    );
  });

  it('writes properties.task[<field>] and preserves sibling task fields', async () => {
    const { container } = render(TaskSchemaForm, { props: { nodeId: 'node-1' } });

    // The form's Collapsible starts collapsed; its trigger is the first button.
    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);
    await editStreetThroughModal();

    expect(persistedProperties(updateNodeSpy)).toEqual({
      task: { estimate: 3, address: { street: '1 Main' } }
    });
  });
});

describe('SchemaPropertyForm — nested values persist under properties[nodeType]', () => {
  beforeEach(() => {
    vi.spyOn(backendAdapter, 'getSchema').mockResolvedValue(
      schemaWith([ADDRESS_FIELD], 'invoice') as never
    );
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      nodeWith('invoice', { invoice: { total: 10 } })
    );
  });

  it('writes properties[nodeType][<field>] and preserves sibling type fields', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);
    await editStreetThroughModal();

    expect(persistedProperties(updateNodeSpy)).toEqual({
      invoice: { total: 10, address: { street: '1 Main' } }
    });
  });
});

describe('SchemaPropertyForm — un-migrated flat properties', () => {
  beforeEach(() => {
    vi.spyOn(backendAdapter, 'getSchema').mockResolvedValue(
      schemaWith([ADDRESS_FIELD], 'invoice') as never
    );
    // Old FLAT shape: `address` sits at the top level, not yet migrated into the
    // `invoice` namespace. updateProperty migrates on first write, so the read
    // path must see it too.
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      nodeWith('invoice', { address: { street: 'A', city: 'B' }, total: 10 })
    );
  });

  it('shows an un-migrated nested value instead of an empty editor', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);

    // Reading the flat location means the summary counts the real keys. Reading only
    // the namespace would render "0 fields" and open an empty editor.
    await waitFor(() => expect(screen.getByText('2 fields')).toBeTruthy());
  });

  it('does not destroy sibling keys the user was never shown', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);
    await fireEvent.click(screen.getByText('2 fields'));
    await waitFor(() => expect(screen.getByLabelText('Street')).toBeTruthy());
    await fireEvent.input(screen.getByLabelText('Street'), { target: { value: 'X' } });

    // `city` was never rendered (it is not a declared sub-field) but must survive the
    // migration write, which persists the namespace wholesale.
    const persisted = persistedProperties(updateNodeSpy) as {
      invoice: { address: Record<string, unknown> };
    };
    expect(persisted.invoice.address).toEqual({ street: 'X', city: 'B' });
  });

  it('persists with replaceProperties so the migration is not merged back', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);
    await fireEvent.click(screen.getByText('2 fields'));
    await waitFor(() => expect(screen.getByLabelText('Street')).toBeTruthy());
    await fireEvent.input(screen.getByLabelText('Street'), { target: { value: 'X' } });

    expect(updateNodeSpy.mock.calls[0][3]).toEqual(
      expect.objectContaining({ replaceProperties: true })
    );
  });
});

describe('SchemaPropertyForm — boolean fields', () => {
  beforeEach(() => {
    vi.spyOn(backendAdapter, 'getSchema').mockResolvedValue(
      schemaWith([field({ name: 'paid', type: 'boolean' })], 'invoice') as never
    );
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      nodeWith('invoice', { invoice: { total: 10 } })
    );
  });

  it('renders a checkbox (no longer a "not yet implemented" placeholder)', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);

    const checkbox = await waitFor(() => screen.getByLabelText('Paid') as HTMLInputElement);
    expect(checkbox.type).toBe('checkbox');
    expect(checkbox.checked).toBe(false);
    expect(screen.queryByText(/not yet implemented/i)).toBeNull();
  });

  it('persists the toggled value under properties[nodeType]', async () => {
    const { container } = render(SchemaPropertyForm, {
      props: { nodeId: 'node-1', nodeType: 'invoice' }
    });

    await waitFor(() => expect(container.querySelector('button')).toBeTruthy());
    await fireEvent.click(container.querySelector('button')!);
    await waitFor(() => expect(screen.getByLabelText('Paid')).toBeTruthy());
    await fireEvent.click(screen.getByLabelText('Paid'));

    expect(persistedProperties(updateNodeSpy)).toEqual({ invoice: { total: 10, paid: true } });
  });
});
