/**
 * UI-extension registry + Pro-sync variant machine.
 *
 * Exercises the two-signal state machine (`proSync.tier` × the active database's
 * DatabaseSettingsNode) and the registry filtering that resolves which chrome /
 * viewer contributions are active for each variant.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { Node } from '$lib/types';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

// SharedNodeStore.setNode does not persist here (skipPersistence), but stub the
// Tauri bridge so nothing reaches a real daemon.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import { proSync } from '$lib/stores/pro-sync.svelte';
import { SharedNodeStore } from '$lib/services/shared-node-store.svelte';
import {
  resolveProSyncVariant,
  isProSyncActive,
  getActiveChromeContributions,
  getActiveViewerExtensions
} from '$lib/plugins/ui-extensions.svelte';
import { uiExtensionRegistry, DATABASE_SETTINGS_NODE_ID } from '$lib/plugins/ui-extensions';

/** Seed the active database's settings singleton with the given namespaced props. */
function seedSettings(props: { sync_enabled?: boolean; auth_status?: string }): void {
  const node: Node = {
    id: DATABASE_SETTINGS_NODE_ID,
    nodeType: 'database-settings',
    content: '',
    properties: { 'database-settings': props },
    mentions: [],
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
    version: 1
  };
  SharedNodeStore.getInstance().setNode(node, { type: 'database', reason: 'seed' }, true);
}

describe('UI-extension registry', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    SharedNodeStore.resetInstance();
    proSync.tier = 'unknown';
  });

  afterEach(() => {
    proSync.tier = 'unknown';
    SharedNodeStore.resetInstance();
    vi.restoreAllMocks();
  });

  describe('registry registration', () => {
    it('registers the built-in pro-sync extension with all contributions', () => {
      expect(uiExtensionRegistry.has('pro-sync')).toBe(true);
      // 4 overlay pill variants + 3 modal (enable-prompt consent / sign-in / connected).
      expect(uiExtensionRegistry.chromeFor('app-shell-overlay')).toHaveLength(4);
      expect(uiExtensionRegistry.chromeFor('app-shell-modal')).toHaveLength(3);
      // 3 collaboration viewer extensions (enable-prompt/sign-in/connected).
      expect(uiExtensionRegistry.viewersFor('collection')).toHaveLength(3);
      expect(uiExtensionRegistry.viewersFor('text')).toEqual([]);
    });
  });

  describe('variant resolution', () => {
    it("tier !== 'pro' → teaser, regardless of the settings node", () => {
      proSync.tier = 'community';
      seedSettings({ sync_enabled: true, auth_status: 'connected' });
      expect(resolveProSyncVariant()).toBe('teaser');
      expect(isProSyncActive()).toBe(false);
    });

    it('pro + no hydrated settings node → enable-prompt (sync not enabled)', () => {
      proSync.tier = 'pro';
      expect(resolveProSyncVariant()).toBe('enable-prompt');
      expect(isProSyncActive()).toBe(false);
    });

    it('pro + sync_enabled: false → enable-prompt', () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: false, auth_status: 'local' });
      expect(resolveProSyncVariant()).toBe('enable-prompt');
      expect(isProSyncActive()).toBe(false);
    });

    it("pro + sync_enabled + auth_status 'local' → sign-in (sync active)", () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: true, auth_status: 'local' });
      expect(resolveProSyncVariant()).toBe('sign-in');
      expect(isProSyncActive()).toBe(true);
    });

    it("pro + sync_enabled + auth_status 'connected' → connected (sync active)", () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: true, auth_status: 'connected' });
      expect(resolveProSyncVariant()).toBe('connected');
      expect(isProSyncActive()).toBe(true);
    });

    it('re-resolves when the settings node changes (derived reads, no cache)', () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: false, auth_status: 'local' });
      expect(resolveProSyncVariant()).toBe('enable-prompt');
      seedSettings({ sync_enabled: true, auth_status: 'local' });
      expect(resolveProSyncVariant()).toBe('sign-in');
      seedSettings({ sync_enabled: true, auth_status: 'connected' });
      expect(resolveProSyncVariant()).toBe('connected');
    });
  });

  describe('active contribution filtering', () => {
    it('teaser: only the teaser overlay pill, no modal, no collab tab', () => {
      proSync.tier = 'community';
      const overlay = getActiveChromeContributions('app-shell-overlay');
      expect(overlay).toHaveLength(1);
      expect(overlay[0].variant).toBe('teaser');
      expect(getActiveChromeContributions('app-shell-modal')).toEqual([]);
      expect(getActiveViewerExtensions('collection')).toEqual([]);
    });

    it('enable-prompt: enable-sync pill + the consent modal + a locked collab tab', () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: false, auth_status: 'local' });
      const overlay = getActiveChromeContributions('app-shell-overlay');
      expect(overlay).toHaveLength(1);
      expect(overlay[0].variant).toBe('enable-prompt');
      const modal = getActiveChromeContributions('app-shell-modal');
      expect(modal).toHaveLength(1);
      expect(modal[0].variant).toBe('enable-prompt');
      const viewers = getActiveViewerExtensions('collection');
      expect(viewers).toHaveLength(1);
      expect(viewers[0].variant).toBe('enable-prompt');
      expect(viewers[0].tab.id).toBe('collaboration');
    });

    it('connected: the live pill + the relogin modal + the live collab tab', () => {
      proSync.tier = 'pro';
      seedSettings({ sync_enabled: true, auth_status: 'connected' });
      const overlay = getActiveChromeContributions('app-shell-overlay');
      expect(overlay).toHaveLength(1);
      expect(overlay[0].variant).toBe('connected');
      const modal = getActiveChromeContributions('app-shell-modal');
      expect(modal).toHaveLength(1);
      expect(modal[0].variant).toBe('connected');
      const viewers = getActiveViewerExtensions('collection');
      expect(viewers).toHaveLength(1);
      expect(viewers[0].variant).toBe('connected');
    });
  });
});
