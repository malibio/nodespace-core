/**
 * pro-sync-pill component — fail-safe label mapping (#1674).
 *
 * The realtime axis reads 'connected' whenever the daemon's session is live,
 * which only proves sign-in — not that data syncs. The pill must never claim
 * "Synced"/"Syncing…" unless the active database's DatabaseSettingsNode
 * confirms `sync_enabled: true` (via isProSyncActive, which fails safe to
 * false while the node is unhydrated). Otherwise it shows a neutral
 * "Signed in — sync off".
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import type { Node } from '$lib/types';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));
const mockInvoke = vi.fn();
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import ProSyncPill from '$lib/components/pro-sync-pill.svelte';
import { proSync, type SyncState } from '$lib/stores/pro-sync.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { DATABASE_SETTINGS_NODE_ID } from '$lib/plugins/ui-extensions';

/**
 * Seed the active database's settings singleton with the given props. The daemon
 * serializes DatabaseSettingsNode with FLAT properties (`sync_enabled`/`auth_status`
 * directly on `properties`), so mirror that shape here.
 */
function seedSettings(props: { sync_enabled?: boolean; auth_status?: string }): void {
  const node: Node = {
    id: DATABASE_SETTINGS_NODE_ID,
    nodeType: 'database-settings',
    content: '',
    properties: props,
    mentions: [],
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1
  };
  SharedNodeStore.getInstance().setNode(node, { type: 'database', reason: 'seed' }, true);
}

function pillButton(container: HTMLElement): Element {
  const button = container.querySelector('.pro-sync-pill');
  expect(button).not.toBeNull();
  return button!;
}

function renderPill(state: SyncState) {
  proSync.state = state;
  return render(ProSyncPill);
}

describe('ProSyncPill — fail-safe sync_enabled cross-check (#1674)', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    SharedNodeStore.resetInstance();
    proSync.tier = 'pro';
    proSync.userEmail = 'mayank@nodespace.dev';
  });

  afterEach(() => {
    cleanup();
    proSync.tier = 'unknown';
    proSync.state = 'unspecified';
    proSync.userEmail = '';
    SharedNodeStore.resetInstance();
    vi.restoreAllMocks();
  });

  it("state 'connected' + sync_enabled false → 'Signed in — sync off', never 'Synced'", () => {
    seedSettings({ sync_enabled: false, auth_status: 'connected' });
    const { container, queryByText, getByText } = renderPill('connected');

    expect(getByText('Signed in — sync off')).toBeTruthy();
    expect(queryByText('Synced')).toBeNull();
    // Neutral tone, not the green "all synced" dot.
    expect(pillButton(container).getAttribute('data-tone')).toBe('grey');
  });

  it("state 'connected' + sync_enabled true → 'Synced'", () => {
    seedSettings({ sync_enabled: true, auth_status: 'connected' });
    const { container, getByText, queryByText } = renderPill('connected');

    expect(getByText('Synced')).toBeTruthy();
    expect(queryByText('Signed in — sync off')).toBeNull();
    expect(pillButton(container).getAttribute('data-tone')).toBe('green');
  });

  it("state 'syncing' + sync_enabled false → 'Signed in — sync off', never 'Syncing…'", () => {
    seedSettings({ sync_enabled: false, auth_status: 'connected' });
    const { container, queryByText, getByText } = renderPill('syncing');

    expect(getByText('Signed in — sync off')).toBeTruthy();
    expect(queryByText('Syncing…')).toBeNull();
    expect(pillButton(container).getAttribute('data-tone')).toBe('grey');
    // The tooltip must not claim syncing either.
    expect(pillButton(container).getAttribute('title')).toBe('Signed in as mayank@nodespace.dev');
  });

  it("state 'syncing' + sync_enabled true → 'Syncing…'", () => {
    seedSettings({ sync_enabled: true, auth_status: 'connected' });
    const { container, getByText } = renderPill('syncing');

    expect(getByText('Syncing…')).toBeTruthy();
    expect(pillButton(container).getAttribute('data-tone')).toBe('amber');
  });

  it('fails safe when the settings node is not hydrated: connected reads as sync off', () => {
    // No seedSettings call — the axis-2 node is absent (e.g. hydration lost).
    const { queryByText, getByText } = renderPill('connected');

    expect(getByText('Signed in — sync off')).toBeTruthy();
    expect(queryByText('Synced')).toBeNull();
  });

  it('signed-out states keep their realtime labels (no override outside connected/syncing)', () => {
    seedSettings({ sync_enabled: false, auth_status: 'local' });
    proSync.userEmail = '';
    const { getByText } = renderPill('auth-required');

    expect(getByText('Sign in required')).toBeTruthy();
  });
});
