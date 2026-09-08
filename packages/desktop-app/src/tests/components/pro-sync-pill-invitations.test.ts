/**
 * pro-sync-pill — invitations auto-prompt.
 *
 * The invitations modal must NOT auto-open on launch from local-storage
 * first-run state. It auto-shows ONLY for the one case that needs it: a
 * genuinely signed-in user, with sync active, who is a member of no collection
 * yet. A signed-out or has-access state never triggers it — and it never queries
 * the daemon (`pro_list_joinable_collections`) while signed out.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, fireEvent } from '@testing-library/svelte';
import type { Node } from '$lib/types';
import type { CollectionInfo } from '$lib/services/collection-service';

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
import { proSync } from '$lib/stores/pro-sync.svelte';
import { membership } from '$lib/stores/membership.svelte';
import { collectionsData } from '$lib/stores/collections.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import { DATABASE_SETTINGS_NODE_ID } from '$lib/plugins/ui-extensions';

/** The stale first-run localStorage key this change removes — must be inert now. */
const LEGACY_FIRST_RUN_KEY = 'ns:invitations-firstrun-seen';

/**
 * Seed the active database's settings singleton (FLAT props, as the daemon
 * serializes them). `sync_enabled: true` + `auth_status: 'connected'` is the
 * only combination that makes sync active (isProSyncActive).
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

function collection(id: string, memberCount: number): CollectionInfo {
  return {
    id,
    content: id,
    nodeType: 'collection',
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1,
    properties: {},
    memberCount,
    parentCollectionIds: []
  };
}

/** Loaded, with a visible collection → the user has access. */
function seedHasAccess(): void {
  collectionsData._setTestData([collection('col-1', 3)], new Map());
}

/** Loaded, but no visible collection → the "no access yet" case. */
function seedNoAccessLoaded(): void {
  collectionsData._setTestData([], new Map());
}

/** Sign the user in with sync active for the active database. */
function signInWithSync(): void {
  proSync.tier = 'pro';
  proSync.state = 'connected';
  proSync.userEmail = 'mayank@nodespace.dev';
  seedSettings({ sync_enabled: true, auth_status: 'connected' });
}

/** The invitations modal is open iff its unique "Redeem an invite code" heading renders. */
function modalOpen(container: HTMLElement): boolean {
  return !!Array.from(container.querySelectorAll('h3')).find(
    (h) => h.textContent === 'Redeem an invite code'
  );
}

describe('ProSyncPill — invitations auto-prompt', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    // Discovery list resolves empty so the auto-loaded inbox has nothing to error on.
    mockInvoke.mockResolvedValue([]);
    SharedNodeStore.resetInstance();
    collectionsData.reset();
    membership.reset();
    localStorage.clear();
    proSync.tier = 'pro';
    proSync.state = 'unspecified';
    proSync.userEmail = '';
  });

  afterEach(() => {
    cleanup();
    proSync.tier = 'unknown';
    proSync.state = 'unspecified';
    proSync.userEmail = '';
    SharedNodeStore.resetInstance();
    collectionsData.reset();
    membership.reset();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('does NOT auto-open on launch, even with the legacy first-run localStorage flag absent or unset', () => {
    // Signed-in user WITH access — the common launch case. The old behavior
    // popped the modal on first run regardless; it must stay closed now.
    signInWithSync();
    seedHasAccess();
    const { container } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(false);
    // Nothing reads/writes the legacy first-run key anymore.
    expect(localStorage.getItem(LEGACY_FIRST_RUN_KEY)).toBeNull();
  });

  it('a pre-existing legacy first-run flag does not force the modal open (key is inert)', () => {
    localStorage.setItem(LEGACY_FIRST_RUN_KEY, '1');
    signInWithSync();
    seedHasAccess();
    const { container } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(false);
  });

  it('auto-shows for a signed-in user with sync active and NO collection access', () => {
    signInWithSync();
    seedNoAccessLoaded();
    const { container } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(true);
  });

  it('does NOT auto-show while collections are still loading (no pre-load flash)', () => {
    signInWithSync();
    // collectionsData.reset() in beforeEach leaves hasLoaded=false → not yet known.
    const { container } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(false);
  });

  it('does NOT auto-show when the user has collection access', () => {
    signInWithSync();
    seedHasAccess();
    const { container } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(false);
  });

  it('does NOT auto-show — and never queries list_joinable — while signed out', () => {
    // Signed out: no identity, sync not active, even if collections read empty.
    proSync.tier = 'pro';
    proSync.state = 'auth-required';
    proSync.userEmail = '';
    seedSettings({ sync_enabled: false, auth_status: 'local' });
    seedNoAccessLoaded();

    const { container } = render(ProSyncPill);

    expect(modalOpen(container)).toBe(false);
    expect(mockInvoke).not.toHaveBeenCalledWith('pro_list_joinable_collections');
  });

  it('dismissing the auto-prompt keeps it closed for the session (no immediate reopen)', async () => {
    signInWithSync();
    seedNoAccessLoaded();
    const { container, getByText } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(true);

    await fireEvent.click(getByText('Close'));
    expect(modalOpen(container)).toBe(false);
  });

  it('the account-menu Invitations entry still opens the modal (Settings path intact)', async () => {
    // Signed in WITH access → no auto-prompt, but the manual entry must work.
    signInWithSync();
    seedHasAccess();
    const { container, getByText } = render(ProSyncPill);
    expect(modalOpen(container)).toBe(false);

    // Open the account menu, then the Invitations entry.
    const pill = container.querySelector('.pro-sync-pill');
    expect(pill).not.toBeNull();
    await fireEvent.click(pill!);
    await fireEvent.click(getByText('Invitations'));

    expect(modalOpen(container)).toBe(true);
  });
});
