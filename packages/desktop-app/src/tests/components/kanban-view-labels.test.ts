/**
 * KanbanView — group-by picker labels (issue #2100).
 *
 * The "Group by" <select> previously labelled each option with the field's
 * `description` when present, leaking help-text prose into the picker
 * (kanban-view.svelte's now-removed `fieldLabel()`). These tests cover the
 * fix: options are labelled via the shared `labelForField()` helper (name-
 * derived), and `description`, when present, survives only as the option's
 * tooltip.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import type { SchemaNode } from '$lib/types/schema-node';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import KanbanView from '$lib/components/query/kanban-view.svelte';
import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';

function schema(): SchemaNode {
  return {
    id: 'person',
    content: 'Person',
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
        description: 'Current lifecycle status of this person record, set by the onboarding workflow',
        coreValues: [{ value: 'active', label: 'Active' }],
        userValues: [],
      },
      {
        name: 'employment_type',
        type: 'enum',
        protection: 'user',
        indexed: false,
        coreValues: [{ value: 'ft', label: 'Full time' }],
        userValues: [],
      },
    ],
  };
}

describe('KanbanView — group-by picker labels', () => {
  beforeEach(() => sharedNodeStore.clearAll());
  afterEach(() => {
    cleanup();
    sharedNodeStore.clearAll();
  });

  it('labels group-by options from field name, not the description prose', () => {
    const { getByRole } = render(KanbanView, {
      props: {
        nodeIds: [],
        schema: schema(),
        groupBy: undefined,
        onGroupByChange: () => {},
        onRowClick: () => {},
      },
    });

    const select = getByRole('combobox', { name: 'Group by' }) as HTMLSelectElement;
    const options = Array.from(select.options).map((o) => o.textContent);
    expect(options).toContain('Status');
    expect(options).toContain('Employment type');
    expect(options.some((o) => o?.includes('Current lifecycle status'))).toBe(false);
  });

  it('carries the field description through as the option tooltip', () => {
    const { getByRole } = render(KanbanView, {
      props: {
        nodeIds: [],
        schema: schema(),
        groupBy: undefined,
        onGroupByChange: () => {},
        onRowClick: () => {},
      },
    });

    const select = getByRole('combobox', { name: 'Group by' }) as HTMLSelectElement;
    const statusOption = Array.from(select.options).find((o) => o.value === 'status');
    expect(statusOption?.title).toBe(
      'Current lifecycle status of this person record, set by the onboarding workflow',
    );

    const employmentOption = Array.from(select.options).find((o) => o.value === 'employment_type');
    expect(employmentOption?.title).toBeFalsy();
  });
});
