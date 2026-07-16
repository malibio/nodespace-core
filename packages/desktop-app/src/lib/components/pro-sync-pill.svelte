<!--
  Pro-tier sync-status pill. Renders only when the daemon answers
  the `nodespace.pro.v1.CloudSyncService` probe; community mode
  hides it entirely.

  The visual contract: pill color follows the orchestrator's state
  (`SyncStatusEvent.state`):
    grey       — DISCONNECTED / UNSPECIFIED
    amber      — CONNECTING / AUTHENTICATING / SYNCING
    blue       — AUTH_REQUIRED (action needed)
    green      — CONNECTED
    red        — ERROR

  Exception (#1674): CONNECTED/SYNCING only prove the daemon's realtime session
  is live — i.e. signed in. When the active database has not opted into sync
  (`sync_enabled: false` on its DatabaseSettingsNode, via isProSyncActive), the
  pill shows a neutral "Signed in — sync off" instead, never "Synced".

  Click: when the daemon is signed out (DISCONNECTED, AUTH_REQUIRED,
  UNSPECIFIED, ERROR), clicking triggers the PKCE flow via
  `pro_initiate_oauth`. The daemon opens the browser; this UI just
  watches `sync:status` for the resulting transitions.
-->

<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { proSync, type SyncState } from '$lib/stores/pro-sync.svelte';
  import { membership } from '$lib/stores/membership.svelte';
  import { isProSyncActive } from '$lib/plugins/ui-extensions.svelte';
  import InvitationsInbox from '$lib/components/collaboration/invitations-inbox.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('ProSyncPill');

  const labels: Record<SyncState, string> = {
    unspecified: 'Sign in',
    disconnected: 'Sign in',
    connecting: 'Connecting…',
    authenticating: 'Signing in…',
    'auth-required': 'Sign in required',
    syncing: 'Syncing…',
    connected: 'Synced',
    error: 'Retry sign-in'
  };

  const tones: Record<SyncState, string> = {
    unspecified: 'grey',
    disconnected: 'grey',
    connecting: 'amber',
    authenticating: 'amber',
    'auth-required': 'blue',
    syncing: 'amber',
    connected: 'green',
    error: 'red'
  };

  // States where clicking should kick off a fresh sign-in attempt.
  const SIGN_IN_STATES: SyncState[] = ['unspecified', 'disconnected', 'auth-required', 'error'];

  // While an InitiateOAuth call is in flight, disable the pill so a
  // double-click doesn't spawn two browser windows.
  let pending = $state(false);
  // Account menu (shown when signed in) + its in-flight sign-out.
  let menuOpen = $state(false);
  let signingOut = $state(false);

  // Invitations inbox (onboarding). Opened from the account menu and,
  // once per device, automatically on the first sign-in.
  //
  // `inboxOpenedManually` tracks explicit opens (account menu). The first-run
  // auto-open is derived from proSync.signedInEpisode instead of pushed by an effect:
  // when a fresh sign-in episode arrives on a device that hasn't seen the onboarding,
  // the inbox shows until dismissed — no $effect reading/writing its own seen flag.
  let inboxOpenedManually = $state(false);
  let dismissedSignInEpisode = $state(0);

  const FIRST_RUN_KEY = 'ns:invitations-firstrun-seen';
  function firstRunSeen(): boolean {
    try {
      return typeof localStorage !== 'undefined' && localStorage.getItem(FIRST_RUN_KEY) === '1';
    } catch {
      return true; // storage unavailable — don't pester
    }
  }
  function markFirstRunSeen() {
    try {
      if (typeof localStorage !== 'undefined') localStorage.setItem(FIRST_RUN_KEY, '1');
    } catch {
      /* ignore */
    }
  }

  // Signed in = the daemon surfaced an identity. When set, clicking the
  // pill opens the account menu ("signed in as <email>" + Sign out) instead of
  // starting a new sign-in.
  let signedIn = $derived(proSync.userEmail !== '');
  let clickable = $derived(SIGN_IN_STATES.includes(proSync.state) || signedIn);

  // Axis-2 cross-check (#1674): the realtime state reads 'connected' whenever
  // the daemon's session is live — that only proves sign-in, not sync. Whether
  // data actually leaves the device is the settings node's `sync_enabled`
  // (surfaced via isProSyncActive, which fails safe to false while the node is
  // unhydrated). Without this, the sign-in variant's pill claimed "Synced"
  // while sync was off. Never claim synced/syncing unless the settings node
  // confirms it.
  const syncEnabled = $derived(isProSyncActive());
  const syncOffOverride = $derived(
    !syncEnabled && (proSync.state === 'connected' || proSync.state === 'syncing')
  );
  const label = $derived(syncOffOverride ? 'Signed in — sync off' : labels[proSync.state]);
  const tone = $derived(syncOffOverride ? 'grey' : tones[proSync.state]);

  // While the daemon catches up in the background (state 'syncing'), the app is
  // fully usable on the local cache — so the tooltip surfaces that progress even
  // when signed in, rather than only "Signed in as <email>". Never a blocking gate;
  // the pill is purely a status indicator.
  let pillTitle = $derived(
    signedIn
      ? proSync.state === 'syncing' && syncEnabled
        ? `${labels.syncing} — signed in as ${proSync.userEmail}`
        : `Signed in as ${proSync.userEmail}`
      : proSync.detail || label
  );

  // The inbox is open when explicitly opened, or auto-opened for a fresh, undismissed
  // first sign-in on a device that hasn't seen onboarding yet.
  const firstRunAutoOpen = $derived(
    signedIn &&
      !firstRunSeen() &&
      proSync.signedInEpisode > 0 &&
      proSync.signedInEpisode !== dismissedSignInEpisode
  );
  const inboxOpen = $derived(inboxOpenedManually || firstRunAutoOpen);

  function closeInbox() {
    inboxOpenedManually = false;
    // Dismiss the current first-run episode and remember it device-wide so it
    // doesn't reopen for this sign-in or on future launches.
    dismissedSignInEpisode = proSync.signedInEpisode;
    markFirstRunSeen();
  }

  // A pill click opens a menu — the account menu when signed in, the
  // sign-in options menu when signed out — so a single click never
  // commits to one sign-in method (email vs Google).
  function onClick() {
    if (pending) return;
    if (signedIn) {
      menuOpen = !menuOpen;
      return;
    }
    if (!SIGN_IN_STATES.includes(proSync.state)) return;
    menuOpen = !menuOpen;
  }

  // Kick off a PKCE sign-in. `provider` empty = the Worker email/password
  // form; `'google'` = direct Supabase GoTrue OAuth. The daemon opens the
  // browser; this UI just tracks progress via the `sync:status` stream.
  async function startSignIn(provider = '') {
    if (pending) return;
    if (!SIGN_IN_STATES.includes(proSync.state)) return;
    menuOpen = false;
    pending = true;
    try {
      const attemptId = await invoke<string>('pro_initiate_oauth', provider ? { provider } : {});
      log.info('PKCE attempt started', { attemptId, provider: provider || 'email' });
    } catch (e) {
      log.warn('pro_initiate_oauth failed', { error: e, provider });
    } finally {
      pending = false;
    }
  }

  function closeMenu() {
    menuOpen = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && menuOpen) closeMenu();
  }

  async function onSignOut() {
    if (signingOut) return;
    signingOut = true;
    try {
      await proSync.signOut();
      // Drop cached membership state so the next user doesn't inherit this
      // session's roster/identity (identity is per-session).
      membership.reset();
      log.info('signed out');
    } finally {
      signingOut = false;
      menuOpen = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if proSync.isPro}
  <div class="pro-sync-pill-wrap">
    <button
      class="pro-sync-pill"
      class:clickable
      data-tone={tone}
      title={pillTitle}
      type="button"
      aria-label="NodeSpace Pro sync status: {label}"
      aria-haspopup={clickable ? 'menu' : undefined}
      aria-expanded={clickable ? menuOpen : undefined}
      disabled={pending || !clickable}
      onclick={onClick}
    >
      <span class="dot" aria-hidden="true"></span>
      <span class="label">{label}</span>
    </button>

    {#if menuOpen && clickable}
      <!-- Transparent backdrop so a click anywhere else closes the menu. -->
      <button class="menu-backdrop" type="button" aria-label="Close menu" onclick={closeMenu}
      ></button>
      <div class="menu" role="menu">
        {#if signedIn}
          <div class="menu-identity">
            <span class="menu-identity-label">Signed in as</span>
            <span class="menu-email">{proSync.userEmail}</span>
          </div>
          <button
            class="menu-item"
            type="button"
            role="menuitem"
            onclick={() => {
              inboxOpenedManually = true;
              menuOpen = false;
            }}
          >
            Invitations
          </button>
          <button
            class="menu-signout"
            type="button"
            role="menuitem"
            disabled={signingOut}
            onclick={onSignOut}
          >
            {signingOut ? 'Signing out…' : 'Sign out'}
          </button>
        {:else}
          <button
            class="menu-item"
            type="button"
            role="menuitem"
            onclick={() => startSignIn('google')}
          >
            Continue with Google
          </button>
          <button class="menu-item" type="button" role="menuitem" onclick={() => startSignIn('')}>
            Sign in with email
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <InvitationsInbox
    open={inboxOpen}
    onClose={closeInbox}
    onLogout={async () => {
      await onSignOut();
      closeInbox();
    }}
  />
{/if}

<style>
  .pro-sync-pill-wrap {
    position: relative;
    display: inline-flex;
  }

  .pro-sync-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    color: hsl(var(--foreground));
    font-size: 12px;
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
  }

  .pro-sync-pill:hover:not(:disabled) {
    background: hsl(var(--muted));
  }

  .pro-sync-pill:disabled {
    cursor: default;
    opacity: 0.85;
  }

  .pro-sync-pill:not(.clickable) {
    cursor: default;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #9ca3af;
  }

  .pro-sync-pill[data-tone='amber'] .dot {
    background: #f59e0b;
    box-shadow: 0 0 0 2px rgba(245, 158, 11, 0.18);
  }
  .pro-sync-pill[data-tone='blue'] .dot {
    background: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.18);
  }
  .pro-sync-pill[data-tone='green'] .dot {
    background: #16a34a;
    box-shadow: 0 0 0 2px rgba(22, 163, 74, 0.18);
  }
  .pro-sync-pill[data-tone='red'] .dot {
    background: #dc2626;
    box-shadow: 0 0 0 2px rgba(220, 38, 38, 0.18);
  }

  /* Account menu — shown when signed in. */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
  }

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 21;
    min-width: 180px;
    max-width: 260px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--popover));
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.12);
  }

  .menu-identity {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 4px 6px;
    border-bottom: 1px solid hsl(var(--border));
  }

  .menu-identity-label {
    font-size: 11px;
    color: hsl(var(--muted-foreground));
  }

  .menu-email {
    font-size: 12px;
    font-weight: 600;
    color: hsl(var(--popover-foreground));
    overflow-wrap: anywhere;
  }

  .menu-item,
  .menu-signout {
    appearance: none;
    text-align: left;
    padding: 6px 8px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: hsl(var(--popover-foreground));
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .menu-item:hover:not(:disabled),
  .menu-signout:hover:not(:disabled) {
    background: hsl(var(--muted));
  }

  .menu-signout:disabled {
    cursor: default;
    opacity: 0.7;
  }
</style>
