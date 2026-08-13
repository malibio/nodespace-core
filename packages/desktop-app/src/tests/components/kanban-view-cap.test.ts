/**
 * KanbanView — per-column render cap (#1985).
 *
 * kanban-view.svelte buckets and rendered ALL matching nodes per column (a
 * card plus a full-options <select> each) with no bound, while List/Table
 * paginate at PAGE_SIZE = 25. These tests cover the fix: each column renders
 * at most a batch of cards, with a "+N more" control that grows the visible
 * count (never shrinks it, so an on-screen card can't disappear mid-drag).
 *
 * DnD, rollback, and keyboard-move coverage live under the browser tier
 * (src/tests/browser/kanban-dnd.test.ts) — this file only covers the cap
 * itself, which is plain click-driven rendering Happy-DOM handles fine.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, fireEvent } from '@testing-library/svelte';
import type { SchemaNode } from '$lib/types/schema-node';
import type { Node } from '$lib/types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import KanbanView from '$lib/components/query/kanban-view.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';

function schema(): SchemaNode {
  return {
    id: 'widget',
    content: 'Widget',
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
        coreValues: [{ value: 'open', label: 'Open' }],
        userValues: []
      }
    ]
  };
}

function makeNodes(count: number): Node[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `n${i}`,
    nodeType: 'widget',
    content: `Card ${i}`,
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    version: 1,
    properties: { status: 'open' },
    mentions: []
  }));
}

function seed(nodes: Node[]): void {
  for (const n of nodes) sharedNodeStore.setNode(n, { type: 'database', reason: 'seed' });
}

describe('KanbanView — per-column cap', () => {
  beforeEach(() => {
    sharedNodeStore.clearAll();
  });

  afterEach(() => {
    cleanup();
    sharedNodeStore.clearAll();
  });

  it('caps a large column at the first batch and shows a +N more control', () => {
    const nodes = makeNodes(40);
    seed(nodes);

    const { container, getByText } = render(KanbanView, {
      props: {
        nodeIds: nodes.map((n) => n.id),
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    expect(container.querySelectorAll('.kanban-card').length).toBe(25);
    expect(getByText('+15 more')).toBeTruthy();
  });

  it('grows the visible count on "+N more" click without removing already-shown cards', async () => {
    const nodes = makeNodes(40);
    seed(nodes);

    const { container, getByText } = render(KanbanView, {
      props: {
        nodeIds: nodes.map((n) => n.id),
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    const initiallyShown = new Set(
      Array.from(container.querySelectorAll('.kanban-card-title')).map((el) => el.textContent?.trim())
    );

    await fireEvent.click(getByText('+15 more'));

    const nowShown = Array.from(container.querySelectorAll('.kanban-card-title')).map((el) =>
      el.textContent?.trim()
    );
    expect(nowShown.length).toBe(40);
    // Every card visible before the click is still visible after it.
    for (const title of initiallyShown) {
      expect(nowShown).toContain(title);
    }
    expect(container.querySelector('.kanban-show-more')).toBeNull();
  });

  it('does not show a cap control for a column at or under the batch size', () => {
    const nodes = makeNodes(10);
    seed(nodes);

    const { container } = render(KanbanView, {
      props: {
        nodeIds: nodes.map((n) => n.id),
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    expect(container.querySelectorAll('.kanban-card').length).toBe(10);
    expect(container.querySelector('.kanban-show-more')).toBeNull();
  });

  it('caps independently per column', () => {
    const openNodes = makeNodes(30);
    const closedNodes = Array.from({ length: 5 }, (_, i) => ({
      id: `closed-${i}`,
      nodeType: 'widget',
      content: `Closed ${i}`,
      createdAt: '2026-01-01T00:00:00Z',
      modifiedAt: '2026-01-01T00:00:00Z',
      version: 1,
      properties: { status: 'unassigned-column-probe' },
      mentions: []
    }));
    seed([...openNodes, ...closedNodes]);

    const { container, getByText } = render(KanbanView, {
      props: {
        nodeIds: [...openNodes, ...closedNodes].map((n) => n.id),
        schema: schema(),
        groupBy: 'status',
        onGroupByChange: () => {},
        onRowClick: () => {}
      }
    });

    // Open column (30 nodes) is capped; Unassigned (the 5 nodes with an
    // unrecognized status) is not.
    expect(getByText('+5 more')).toBeTruthy();
    expect(container.querySelectorAll('.kanban-card').length).toBe(25 + 5);
  });
});
