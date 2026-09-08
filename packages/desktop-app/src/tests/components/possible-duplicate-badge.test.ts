/**
 * PossibleDuplicateBadge — inline node-view indicator for the convergence
 * "possible duplicate" marker (ADR-065 §4, core#2116).
 *
 * Mirrors the RecoveredItemsBadge inline-pill-with-popover pattern: renders
 * nothing unless the node it's attached to is flagged, and reuses the same
 * adopt-existing lookup/navigate action as the creation-time suggestion in
 * person-schema-form.svelte. These tests drive a real click through the
 * component and assert:
 *   - nothing renders for an unflagged person, or a flagged non-person node
 *   - the badge shows for a flagged person
 *   - clicking it re-runs findDuplicateFor and offers "Use existing" / "Dismiss"
 *   - "Use existing" navigates (non-destructively) and closes the popover
 *   - "Dismiss" just closes the popover
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Node } from '$lib/types';

import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () => mockTauriCore());

const navigateToNodeInOtherPane = vi.fn();
vi.mock('$lib/services/navigation-service', () => ({
  getNavigationService: () => ({ navigateToNodeInOtherPane })
}));

import PossibleDuplicateBadge from '$lib/components/possible-duplicate-badge.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { backendAdapter } from '$lib/services/backend-adapter';

function personNode(overrides: Partial<Node> = {}): Node {
  return {
    id: 'person-1',
    nodeType: 'person',
    content: '',
    title: 'Alice',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: {
      person: { first_name: 'Alice', email: 'alice@example.com', _possible_duplicate: true }
    },
    ...overrides
  } as Node;
}

function existingMatch(overrides: Partial<Node> = {}): Node {
  return {
    id: 'person-existing',
    nodeType: 'person',
    content: '',
    title: 'Bob Existing',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: { person: { first_name: 'Bob', last_name: 'Existing', email: 'alice@example.com' } },
    ...overrides
  } as Node;
}

let findDuplicateForSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  findDuplicateForSpy = vi.fn().mockResolvedValue(null);
  vi.spyOn(backendAdapter, 'findDuplicateFor').mockImplementation(
    findDuplicateForSpy as unknown as typeof backendAdapter.findDuplicateFor
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('PossibleDuplicateBadge', () => {
  it('renders nothing for a person node without the marker', () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(
      personNode({ properties: { person: { first_name: 'Alice', email: 'alice@example.com' } } })
    );
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    expect(screen.queryByText('Possible duplicate')).toBeNull();
  });

  it('renders nothing for a flagged non-person node', () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue({
      id: 'task-1',
      nodeType: 'task',
      content: 'Do the thing',
      createdAt: '2026-01-01T00:00:00Z',
      modifiedAt: '2026-01-01T00:00:00Z',
      version: 1,
      properties: { task: { _possible_duplicate: true } }
    } as Node);
    render(PossibleDuplicateBadge, { props: { nodeId: 'task-1' } });

    expect(screen.queryByText('Possible duplicate')).toBeNull();
  });

  it('renders the badge for a flagged person node', () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    expect(screen.getByText('Possible duplicate')).toBeTruthy();
  });

  it('clicking the badge re-runs the duplicate lookup and shows the adopt-existing choice', async () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    await fireEvent.click(screen.getByRole('button', { name: /Possible duplicate/i }));

    expect(findDuplicateForSpy).toHaveBeenCalledWith(
      'person',
      'email',
      'alice@example.com',
      'person-1'
    );
    await waitFor(() =>
      expect(screen.getByText(/already exists: Bob Existing/i)).toBeTruthy()
    );
    expect(screen.getByRole('button', { name: 'Use existing' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeTruthy();
  });

  it('"Use existing" navigates to the match, non-destructively, and closes the popover', async () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    await fireEvent.click(screen.getByRole('button', { name: /Possible duplicate/i }));
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    await fireEvent.click(screen.getByRole('button', { name: 'Use existing' }));

    expect(navigateToNodeInOtherPane).toHaveBeenCalledWith('person-existing');
    expect(screen.queryByText(/already exists/i)).toBeNull();
  });

  it('"Dismiss" closes the popover without navigating', async () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    findDuplicateForSpy.mockResolvedValue(existingMatch());
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    await fireEvent.click(screen.getByRole('button', { name: /Possible duplicate/i }));
    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    await fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));

    expect(navigateToNodeInOtherPane).not.toHaveBeenCalled();
    expect(screen.queryByText(/already exists/i)).toBeNull();
  });

  it('shows a "no conflicting person found" message when the marker is stale (no live collision)', async () => {
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    findDuplicateForSpy.mockResolvedValue(null);
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    await fireEvent.click(screen.getByRole('button', { name: /Possible duplicate/i }));

    await waitFor(() => expect(findDuplicateForSpy).toHaveBeenCalled());
    expect(screen.getByText(/No conflicting person found/i)).toBeTruthy();
  });

  it('never offers to adopt itself, even if the backend misbehaves', async () => {
    // Defensive backstop, not the primary exclusion mechanism (that's
    // excludeId, asserted above) — mirrors the equivalent test for the
    // creation-time suggestion in person-schema-form.test.ts.
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    findDuplicateForSpy.mockResolvedValue(personNode({ id: 'person-1' }));
    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });

    await fireEvent.click(screen.getByRole('button', { name: /Possible duplicate/i }));
    await waitFor(() => expect(findDuplicateForSpy).toHaveBeenCalled());

    expect(screen.queryByText(/already exists/i)).toBeNull();
    expect(screen.getByText(/No conflicting person found/i)).toBeTruthy();
  });

  it('a stale response from a superseded check does not clobber a newer result', async () => {
    // Regression guard for the staleness fix: open the popover (check #1,
    // slow), close it, reopen it (check #2, resolves first with a real
    // match). Check #1 finally resolving afterward must not wipe check #2's
    // valid result or flip `checking` back on.
    vi.spyOn(sharedNodeStore, 'getNode').mockReturnValue(personNode());
    let resolveFirst!: (node: Node | null) => void;
    findDuplicateForSpy
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveFirst = resolve;
        })
      )
      .mockResolvedValueOnce(existingMatch());

    render(PossibleDuplicateBadge, { props: { nodeId: 'person-1' } });
    const trigger = screen.getByRole('button', { name: /Possible duplicate/i });

    await fireEvent.click(trigger); // opens, starts check #1 (slow)
    await fireEvent.click(trigger); // closes
    await fireEvent.click(trigger); // reopens, starts check #2

    await waitFor(() => expect(screen.getByText(/already exists/i)).toBeTruthy());

    resolveFirst(null); // the stale check finally resolves
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(screen.getByText(/already exists: Bob Existing/i)).toBeTruthy();
  });
});
