<!--
  First-Pro consent slot.

  Registry wrapper contributed to the `app-shell-modal` slot for the
  `enable-prompt` variant (a Pro daemon whose active database has sync disabled).
  Owns the enable-sync / sign-in orchestration and mounts the presentational
  `FirstProConsentModal`. Visibility is driven off `proSync.consentPromptOpen`,
  which the enable-sync pill (and the locked Collaboration placeholder) set.

  Never rendered in the community build (that resolves to `teaser`), so invoking
  Pro commands here is safe.
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import FirstProConsentModal from '$lib/components/first-pro-consent-modal.svelte';
  import { proSync } from '$lib/stores/pro-sync.svelte';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('FirstProConsentSlot');

  let pending = $state(false);

  const open = $derived(proSync.isPro && proSync.consentPromptOpen);

  async function handleMerge() {
    if (pending) return;
    pending = true;
    try {
      // Record the merge consent first (flips sync_enabled for this database),
      // then start sign-in so the daemon can bind the tenant and push. The daemon
      // only pushes once sync is enabled, so nothing has left the device before
      // this point.
      await invoke('pro_enable_sync');
      await invoke('pro_initiate_oauth');
    } catch (e) {
      log.warn('first-pro consent: enabling sync failed', { error: e });
    } finally {
      pending = false;
      proSync.consentPromptOpen = false;
    }
  }

  function handleKeepLocal() {
    // Decline: leave sync disabled, share nothing. The pill remains, so the user
    // can revisit this choice later.
    proSync.consentPromptOpen = false;
  }
</script>

<FirstProConsentModal {open} {pending} onMerge={handleMerge} onKeepLocal={handleKeepLocal} />
