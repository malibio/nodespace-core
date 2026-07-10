/**
 * Built-in Pro UI extension
 * =================================
 *
 * The single {@link UiExtensionDefinition} for the Pro-sync surface. It maps each
 * variant of the two-signal state machine (see `ui-extensions.svelte.ts`) to the
 * component that renders it:
 *
 *   | variant        | overlay pill              | modal              | collection tab              |
 *   |----------------|---------------------------|--------------------|-----------------------------|
 *   | teaser         | pro-teaser-pill (upsell)  | —                  | — (none)                    |
 *   | enable-prompt  | enable-sync-pill          | —                  | collaboration-locked        |
 *   | sign-in        | pro-sync-pill (existing)  | pro-relogin-slot   | collaboration-tab           |
 *   | connected      | pro-sync-pill (existing)  | pro-relogin-slot   | collaboration-tab           |
 *
 * Every component is referenced only through `() => import(...)`, so nothing Pro
 * is imported eagerly — the shared shell stays free of Pro component imports.
 * Existing Pro components (`pro-sync-pill`, `pro-relogin-modal`,
 * `collaboration-view`) are mounted unchanged; the new small wrappers
 * (`pro-relogin-slot`, `collaboration-tab`) exist only to move their mounting
 * behind the registry.
 *
 * Registration is a module-load side effect so the contributions are present
 * before the first render (the registry is static config, not reactive state).
 */

import { uiExtensionRegistry, type UiExtensionDefinition } from './ui-extensions';

const COLLABORATION_TAB = { appliesTo: 'collection', id: 'collaboration', label: 'Collaboration' };

export const proSyncUiExtension: UiExtensionDefinition = {
  id: 'pro-sync',
  name: 'NodeSpace Pro Sync',
  version: '1.0.0',
  chrome: [
    // Overlay pill — one per variant.
    {
      slot: 'app-shell-overlay',
      variant: 'teaser',
      lazyLoad: () => import('$lib/components/pro-teaser-pill.svelte')
    },
    {
      slot: 'app-shell-overlay',
      variant: 'enable-prompt',
      lazyLoad: () => import('$lib/components/enable-sync-pill.svelte')
    },
    {
      slot: 'app-shell-overlay',
      variant: 'sign-in',
      lazyLoad: () => import('$lib/components/pro-sync-pill.svelte')
    },
    {
      slot: 'app-shell-overlay',
      variant: 'connected',
      lazyLoad: () => import('$lib/components/pro-sync-pill.svelte')
    },
    // Re-login modal — only meaningful once sync is enabled for the database; the
    // wrapper itself only shows the modal on an AUTH_REQUIRED transition.
    {
      slot: 'app-shell-modal',
      variant: 'sign-in',
      lazyLoad: () => import('$lib/components/pro-relogin-slot.svelte')
    },
    {
      slot: 'app-shell-modal',
      variant: 'connected',
      lazyLoad: () => import('$lib/components/pro-relogin-slot.svelte')
    }
  ],
  viewerExtensions: [
    // Collaboration tab. Locked placeholder while sync is disabled for this
    // database (Pro daemon, sync_enabled: false); the live view once enabled.
    {
      tab: COLLABORATION_TAB,
      variant: 'enable-prompt',
      lazyLoad: () => import('$lib/components/collaboration/collaboration-locked.svelte')
    },
    {
      tab: COLLABORATION_TAB,
      variant: 'sign-in',
      lazyLoad: () => import('$lib/components/collaboration/collaboration-tab.svelte')
    },
    {
      tab: COLLABORATION_TAB,
      variant: 'connected',
      lazyLoad: () => import('$lib/components/collaboration/collaboration-tab.svelte')
    }
  ]
};

uiExtensionRegistry.register(proSyncUiExtension);
