/**
 * KanbanView — real drag-and-drop, rollback, and keyboard-move coverage (#1985).
 *
 * PR #1984 unit-tested the extracted grouping/write-shape logic
 * (kanban-grouping.test.ts) but left the drag/drop → store-write →
 * reactive-regroup → rollback flow, and the keyboard move-select, uncovered.
 * Happy-DOM cannot originate real `DragEvent`s with a working `DataTransfer`,
 * so this exercises the actual component against a real Chromium DOM: a real
 * `dragstart`/`dragover`/`drop` sequence, dispatched with a real
 * `DataTransfer`, driving the same handlers a real drag would.
 *
 * `sharedNodeStore` is the real singleton (not a stub) so the reactive
 * regroup is genuine: cards are bucketed by reading the store, and a move
 * writes back through the store's real `updateNode`. Only the daemon RPC
 * boundary (`backendAdapter`) is mocked, matching the pattern in
 * shared-node-store.test.ts.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/svelte';
import type { SchemaNode } from '$lib/types/schema-node';
import type { Node } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import KanbanView from '$lib/components/query/kanban-view.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { backendAdapter } from '$lib/services/backend-adapter';
import { conflictNotifications } from '$lib/stores/conflict-notifications.svelte';

function schema(): SchemaNode {
  return {
    id: 'ticket',
    content: 'Ticket',
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    isCore: false,
    schemaVersion: 1,
    fields: [
      {
        name: 'status',
        type: 'enum',
        protection: 'user',
        indexed: false,
        coreValues: [
          { value: 'open', label: 'Open' },
          { value: 'closed', label: 'Closed' }
        ],
        userValues: []
      }
    ]
  };
}

function ticket(id: string, status: string, title: string): Node {
  return {
    id,
    nodeType: 'ticket',
    content: title,
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: { status },
    mentions: []
  };
}

/** Seed a node as already-persisted, so a subsequent edit takes the real
 *  update-and-persist path (not the create path). */
function seed(node: Node): void {
  sharedNodeStore.setNode(node, { type: 'database', reason: 'seed' });
}

function cardFor(container: HTMLElement, title: string): HTMLElement {
  const el = Array.from(container.querySelectorAll('.kanban-card')).find(
    (card) => card.querySelector('.kanban-card-title')?.textContent?.trim() === title
  );
  if (!el) throw new Error(`No card found for "${title}"`);
  return el as HTMLElement;
}

function columnFor(container: HTMLElement, label: string): HTMLElement {
  const el = Array.from(container.querySelectorAll('.kanban-column')).find(
    (col) => col.querySelector('.kanban-column-title')?.textContent?.trim() === label
  );
  if (!el) throw new Error(`No column found for "${label}"`);
  return el as HTMLElement;
}

function cardsIn(column: HTMLElement): string[] {
  return Array.from(column.querySelectorAll('.kanban-card-title')).map(
    (el) => el.textContent?.trim() ?? ''
  );
}

/** Drive a real HTML5 drag-and-drop sequence with a real DataTransfer. */
async function dragAndDrop(source: HTMLElement, target: HTMLElement): Promise<void> {
  const dataTransfer = new DataTransfer();
  source.dispatchEvent(
    new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer })
  );
  target.dispatchEvent(
    new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer })
  );
  target.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer }));
  source.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer }));
}

describe('KanbanView — drag-and-drop (browser mode)', () => {
  beforeEach(() => {
    sharedNodeStore.clearAll();
    conflictNotifications.dismissAll();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    sharedNodeStore.clearAll();
    conflictNotifications.dismissAll();
  });

  it('dragging a card to another column moves it and persists the write', async () => {
    seed(ticket('t1', 'open', 'Fix the bug'));
    seed(ticket('t2', 'closed', 'Ship the feature'));

    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockResolvedValue({
      ...ticket('t1', 'closed', 'Fix the bug'),
      version: 2
    });

    const { container } = render(KanbanView, {
      props: {
        nodeIds: ['t1', 't2'],
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    expect(cardsIn(columnFor(container, 'Open'))).toEqual(['Fix the bug']);
    expect(cardsIn(columnFor(container, 'Closed'))).toEqual(['Ship the feature']);

    await dragAndDrop(cardFor(container, 'Fix the bug'), columnFor(container, 'Closed'));

    await waitFor(() => {
      expect(cardsIn(columnFor(container, 'Closed'))).toEqual(
        expect.arrayContaining(['Ship the feature', 'Fix the bug'])
      );
    });
    expect(cardsIn(columnFor(container, 'Open'))).toEqual([]);

    expect(updateSpy).toHaveBeenCalledWith(
      't1',
      1,
      expect.objectContaining({ properties: expect.objectContaining({ status: 'closed' }) })
    );
  });

  it('dropping a card into its own column performs no write and leaves it in place', async () => {
    seed(ticket('t1', 'open', 'Fix the bug'));

    const updateSpy = vi.spyOn(backendAdapter, 'updateNode');

    const { container } = render(KanbanView, {
      props: {
        nodeIds: ['t1'],
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    await dragAndDrop(cardFor(container, 'Fix the bug'), columnFor(container, 'Open'));

    // Give any (wrongly-fired) write a tick to land before asserting its absence.
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(updateSpy).not.toHaveBeenCalled();
    expect(cardsIn(columnFor(container, 'Open'))).toEqual(['Fix the bug']);
  });

  it('rolls a card back to its original column when the persisted write is rejected', async () => {
    seed(ticket('t1', 'open', 'Fix the bug'));

    vi.spyOn(backendAdapter, 'updateNode').mockRejectedValue(new Error('daemon offline'));
    // The write never landed — the server still has the pre-drag state. This
    // is what the store's failure-path resync fetches to correct the local
    // optimistic state back to the truth.
    vi.spyOn(backendAdapter, 'getNode').mockResolvedValue(ticket('t1', 'open', 'Fix the bug'));

    const { container } = render(KanbanView, {
      props: {
        nodeIds: ['t1'],
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    await dragAndDrop(cardFor(container, 'Fix the bug'), columnFor(container, 'Closed'));

    // Optimistic move lands first — the card visibly moves before the
    // rejected write is even known to have failed.
    await waitFor(() => {
      expect(cardsIn(columnFor(container, 'Closed'))).toEqual(['Fix the bug']);
    });

    // Once the rejection is processed, the card returns to its original column.
    await waitFor(
      () => {
        expect(cardsIn(columnFor(container, 'Open'))).toEqual(['Fix the bug']);
      },
      { timeout: 3000 }
    );
    expect(cardsIn(columnFor(container, 'Closed'))).toEqual([]);

    await waitFor(() => {
      expect(
        conflictNotifications.notifications.some(
          (n) => n.nodeId === 't1' && n.conflictType === 'write-failure'
        )
      ).toBe(true);
    });
  });
});

describe('KanbanView — keyboard-accessible move (browser mode)', () => {
  beforeEach(() => {
    sharedNodeStore.clearAll();
    conflictNotifications.dismissAll();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    sharedNodeStore.clearAll();
    conflictNotifications.dismissAll();
  });

  it('moves a card to another column via the per-card "Move to" select and persists it', async () => {
    seed(ticket('t1', 'open', 'Fix the bug'));

    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockResolvedValue({
      ...ticket('t1', 'closed', 'Fix the bug'),
      version: 2
    });

    const { container, getByRole } = render(KanbanView, {
      props: {
        nodeIds: ['t1'],
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    const moveSelect = getByRole('combobox', {
      name: 'Move Fix the bug to another column'
    }) as HTMLSelectElement;
    expect(moveSelect.value).toBe('open');

    await fireEvent.change(moveSelect, { target: { value: 'closed' } });

    await waitFor(() => {
      expect(cardsIn(columnFor(container, 'Closed'))).toEqual(['Fix the bug']);
    });
    expect(cardsIn(columnFor(container, 'Open'))).toEqual([]);
    expect(updateSpy).toHaveBeenCalledWith(
      't1',
      1,
      expect.objectContaining({ properties: expect.objectContaining({ status: 'closed' }) })
    );
  });

  it('moving a card to Unassigned via the select clears the field and no-ops on re-selecting it', async () => {
    seed(ticket('t1', 'open', 'Fix the bug'));

    const updateSpy = vi.spyOn(backendAdapter, 'updateNode').mockResolvedValue({
      ...ticket('t1', '', 'Fix the bug'),
      version: 2
    });

    const { container, getByRole } = render(KanbanView, {
      props: {
        nodeIds: ['t1'],
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    const moveSelect = getByRole('combobox', {
      name: 'Move Fix the bug to another column'
    }) as HTMLSelectElement;

    await fireEvent.change(moveSelect, { target: { value: '__unassigned__' } });

    await waitFor(() => {
      expect(cardsIn(columnFor(container, 'Unassigned'))).toEqual(['Fix the bug']);
    });
    expect(updateSpy).toHaveBeenCalledTimes(1);

    // Re-selecting the column the card is already in is a no-op — no second write.
    const settledSelect = getByRole('combobox', {
      name: 'Move Fix the bug to another column'
    }) as HTMLSelectElement;
    await fireEvent.change(settledSelect, { target: { value: '__unassigned__' } });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(updateSpy).toHaveBeenCalledTimes(1);
  });
});
