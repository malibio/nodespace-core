/**
 * PersonSchemaForm — adopt-existing suggestion (core#1734 / ADR-065).
 *
 * `person.email` carries a store-aware `unique` schema rule. This form is the
 * creation/edit surface where a collision must surface as a dismissible
 * suggestion — never a blocking error, and never a skipped save. These tests
 * drive a real blur through the component and assert:
 *   - a colliding email shows "use existing / keep as new"
 *   - the field save is never gated on the lookup (suggest-don't-block)
 *   - "Use existing" navigates to the match and never deletes/merges anything
 *   - "Keep as new" just dismisses the suggestion
 *   - no collision (or a "collision" with itself) shows nothing
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Node } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const navigateToNodeInOtherPane = vi.fn();
vi.mock('$lib/services/navigation-service', () => ({
  getNavigationService: () => ({ navigateToNodeInOtherPane })
}));

import PersonSchemaForm from '$lib/components/property-forms/person-schema-form.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { backendAdapter } from '$lib/services/backend-adapter';

function personNode(overrides: Partial<Node> = {}): Node {
  return {
    id: 'person-1',
    nodeType: 'person',
    content: 'Alice',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: { person: { name: 'Alice', email: '' } },
    ...overrides
  } as Node;
}

function existingMatch(overrides: Partial<Node> = {}): Node {
  return {
    id: 'person-existing',
    nodeType: 'person',
    content: 'Bob Existing',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: { person: { name: 'Bob Existing', email: 'bob@example.com' } },
    ...overrides
  } as Node;
}

let updateNodeSpy: ReturnType<typeof vi.fn>;
let findDuplicateForSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  updateNodeSpy = vi.fn().mockResolvedValue(personNode());
  vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
  vi.spyOn(backendAdapter, 'updateNode').mockImplementation(
    updateNodeSpy as unknown as typeof backendAdapter.updateNode
  );
  findDuplicateForSpy = vi.fn().mockResolvedValue(null);
  vi.spyOn(backendAdapter, 'findDuplicateFor').mockImplementation(
    findDuplicateForSpy as unknown as typeof backendAdapter.findDuplicateFor
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function blurEmail(value: string) {
  const input = screen.getByLabelText('Email');
  await fireEvent.blur(input, { target: { value } });
}

describe('PersonSchemaForm — adopt-existing suggestion', () => {
  it('surfaces the suggestion when the blurred email collides with another person', async () => {
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('bob@example.com');

    await waitFor(() =>
      expect(screen.getByText(/already exists: Bob Existing/i)).toBeTruthy()
    );
    expect(screen.getByRole('button', { name: 'Use existing' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Keep as new' })).toBeTruthy();
  });

  it('never blocks or skips the save, even when a collision is found', async () => {
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('bob@example.com');
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    // The write happens regardless of the suggestion — suggest, never block.
    expect(updateNodeSpy).toHaveBeenCalledTimes(1);
    const [, , update] = updateNodeSpy.mock.calls[0];
    expect((update.properties.person as Record<string, unknown>).email).toBe('bob@example.com');
  });

  it('"Use existing" navigates to the match and dismisses the suggestion', async () => {
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('bob@example.com');
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    await fireEvent.click(screen.getByRole('button', { name: 'Use existing' }));

    expect(navigateToNodeInOtherPane).toHaveBeenCalledWith('person-existing');
    expect(screen.queryByText(/already exists/i)).toBeNull();

    // Non-destructive: adopting never touches the current node.
    expect(updateNodeSpy.mock.calls.every(([id]) => id === 'person-1')).toBe(true);
  });

  it('"Keep as new" dismisses the suggestion without navigating', async () => {
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('bob@example.com');
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    await fireEvent.click(screen.getByRole('button', { name: 'Keep as new' }));

    expect(navigateToNodeInOtherPane).not.toHaveBeenCalled();
    expect(screen.queryByText(/already exists/i)).toBeNull();
  });

  it('shows no suggestion when the email has no conflict', async () => {
    findDuplicateForSpy.mockResolvedValue(null);
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('nobody@example.com');
    await waitFor(() => expect(findDuplicateForSpy).toHaveBeenCalled());

    expect(screen.queryByText(/already exists/i)).toBeNull();
  });

  it('never suggests the node adopt itself', async () => {
    // A pathological/self-referential lookup result must never render.
    findDuplicateForSpy.mockResolvedValue(personNode({ id: 'person-1' }));
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('alice@example.com');
    await waitFor(() => expect(findDuplicateForSpy).toHaveBeenCalled());

    expect(screen.queryByText(/already exists/i)).toBeNull();
  });

  it('does not look up a duplicate for an empty email', async () => {
    render(PersonSchemaForm, { props: { nodeId: 'person-1' } });

    await blurEmail('');

    expect(findDuplicateForSpy).not.toHaveBeenCalled();
    expect(screen.queryByText(/already exists/i)).toBeNull();
  });
});
