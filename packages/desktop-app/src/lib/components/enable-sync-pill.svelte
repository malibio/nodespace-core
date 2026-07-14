<!--
  Enable-sync prompt.

  Rendered in the app-shell overlay slot for the `enable-prompt` variant — a Pro
  daemon whose active database has `sync_enabled: false`. Clicking opens the
  first-Pro data-sharing consent modal (rendered by `first-pro-consent-slot` in
  the app-shell modal slot for the same variant), which is where the user makes
  the informed, irreversible choice to merge into the public workspace. Nothing
  leaves the device until they opt in there.

  Never rendered in the community build (that resolves to `teaser`), so touching
  Pro state here is safe.
-->
<script lang="ts">
  import { proSync } from '$lib/stores/pro-sync.svelte';

  function openConsent() {
    proSync.consentPromptOpen = true;
  }
</script>

<button
  class="enable-sync-pill"
  type="button"
  title="Turn on cloud sync for this database"
  aria-label="Enable cloud sync for this database"
  onclick={openConsent}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="label">Enable sync</span>
</button>

<style>
  .enable-sync-pill {
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

  .enable-sync-pill:hover {
    background: hsl(var(--muted));
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.18);
  }
</style>
