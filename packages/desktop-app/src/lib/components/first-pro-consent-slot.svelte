<!--
  First-Pro consent slot.

  Registry wrapper contributed to the `app-shell-modal` slot for the `consent`
  variant — a Pro daemon whose active database is signed in but has not yet opted
  into sync. Owns the enable-sync action and mounts the presentational
  `FirstProConsentModal`.

  Sign-in has already happened by the time this slot mounts (the sign-in-first
  flow), so the modal only decides the public-workspace publish. It auto-opens
  once per fresh sign-in episode; a "Keep local" decline records that episode so
  it doesn't immediately reopen, while the enable-sync pill can reopen it. A brief
  status line confirms the decline registered.

  Never rendered in the community build (that resolves to `teaser`), so invoking
  Pro commands here is safe.
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import FirstProConsentModal from '$lib/components/first-pro-consent-modal.svelte';
  import { proSync } from '$lib/stores/pro-sync.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('FirstProConsentSlot');

  const KEPT_LOCAL_NOTICE_MS = 4000;

  let pending = $state(false);
  let showKeptLocalNotice = $state(false);
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  // Auto-open once per fresh sign-in episode (unless declined this episode), plus
  // any manual reopen via the enable-sync pill. Derived from the episode counter
  // rather than pushed by an effect (ADR-049): a decline sets
  // `consentDeclinedEpisode` to the current episode, and a later sign-out/in bumps
  // `signedInEpisode` so the prompt returns for the new session.
  const autoOpen = $derived(
    proSync.signedInEpisode > 0 && proSync.signedInEpisode !== proSync.consentDeclinedEpisode
  );
  const open = $derived(proSync.isPro && (proSync.consentPromptOpen || autoOpen));

  async function handleMerge() {
    if (pending) return;
    pending = true;
    try {
      // The user is already signed in (sign-in precedes this consent), so this only
      // records the publish consent — it flips `sync_enabled` for this database, and
      // the daemon's push sweep, gated on that flag, uploads the graph. Nothing has
      // left the device before this point.
      await invoke('pro_enable_sync');
    } catch (e) {
      log.warn('first-pro consent: enabling sync failed', { error: e });
    } finally {
      pending = false;
      proSync.consentPromptOpen = false;
      // Enabling flips the variant to `connected`, unmounting this slot.
    }
  }

  function handleKeepLocal() {
    // Decline: leave sync disabled and share nothing, but stay signed in. Record the
    // decline for this sign-in episode so the auto-open doesn't immediately reopen,
    // and surface a brief confirmation so the choice visibly registers. The pill
    // remains, so the user can revisit this later.
    proSync.consentPromptOpen = false;
    proSync.consentDeclinedEpisode = proSync.signedInEpisode;
    showKeptLocalNotice = true;
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => {
      showKeptLocalNotice = false;
      noticeTimer = null;
    }, KEPT_LOCAL_NOTICE_MS);
  }

  // Clear a pending notice timer if the slot unmounts (e.g. the user enables sync).
  $effect(() => () => {
    if (noticeTimer) clearTimeout(noticeTimer);
  });
</script>

<FirstProConsentModal {open} {pending} onMerge={handleMerge} onKeepLocal={handleKeepLocal} />

{#if showKeptLocalNotice}
  <div class="kept-local-notice" role="status" aria-live="polite">Kept local — sync stays off</div>
{/if}

<style>
  .kept-local-notice {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1000;
    padding: 8px 16px;
    border-radius: 8px;
    background: hsl(var(--popover));
    color: hsl(var(--popover-foreground));
    border: 1px solid hsl(var(--border));
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    font-size: 13px;
    font-weight: 500;
  }
</style>
