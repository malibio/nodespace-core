<!--
  Pro upgrade teaser.

  Rendered in the app-shell overlay slot for the `teaser` variant — i.e. whenever
  the daemon is not Pro (community build, or before the capability probe resolves).
  ADR-039 calls for an upgrade CTA in place of sync controls rather than hiding the
  slot entirely.

  Deliberately static: no daemon commands, no Pro data, no network. It is purely an
  informational upsell, so the community build stays behaviorally inert with
  respect to sync (the free-user guardrail asserts no Pro command is ever invoked).
  Copy is hard-coded here, not data-driven.
-->
<script lang="ts">
  const LEARN_MORE_URL = 'https://nodespace.ai/pro';

  function openLearnMore() {
    // Best-effort: open in the user's browser when the web API is available.
    // No Tauri/daemon command — this must stay inert in the community build.
    if (typeof window !== 'undefined' && typeof window.open === 'function') {
      window.open(LEARN_MORE_URL, '_blank', 'noopener,noreferrer');
    }
  }
</script>

<button
  class="pro-teaser-pill"
  type="button"
  title="Sync your notes across devices with NodeSpace Pro"
  aria-label="Upgrade to NodeSpace Pro"
  onclick={openLearnMore}
>
  <span class="spark" aria-hidden="true">✦</span>
  <span class="label">Upgrade to Pro</span>
</button>

<style>
  .pro-teaser-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    color: hsl(var(--muted-foreground));
    font-size: 12px;
    font-weight: 500;
    line-height: 1;
    cursor: pointer;
  }

  .pro-teaser-pill:hover {
    background: hsl(var(--muted));
    color: hsl(var(--foreground));
  }

  .spark {
    color: #a855f7;
    font-size: 11px;
  }
</style>
