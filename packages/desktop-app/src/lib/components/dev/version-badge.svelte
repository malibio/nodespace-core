<script lang="ts">
  import { onMount } from 'svelte';
  import { createLogger } from '$lib/utils/logger';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { isBadgeEnabled, versionMismatch } from './version-badge-model';

  const log = createLogger('VersionBadge');

  // Frontend's own version, injected at build time from package.json.
  const frontendVersion = __APP_VERSION__;

  // Hidden by default; only shown when the opt-in flag is set. Read once on
  // mount so toggling the flag has no effect until the next load.
  let enabled = $state(false);
  // null = unknown (not yet fetched, or the fetch failed) — rendered as "?".
  let daemonVersion = $state<string | null>(null);

  const mismatch = $derived(versionMismatch(frontendVersion, daemonVersion));

  onMount(() => {
    try {
      enabled = isBadgeEnabled(typeof localStorage !== 'undefined' ? localStorage : null);
    } catch (error) {
      // Some environments make even touching localStorage throw; stay hidden.
      enabled = false;
      log.debug('localStorage unavailable; build badge stays hidden', { error });
    }

    if (!enabled) return;

    // Best-effort: leave daemonVersion as null (shown as "?") if this fails.
    void backendAdapter
      .getDaemonVersion()
      .then((version) => {
        daemonVersion = version;
      })
      .catch((error: unknown) => {
        log.debug('Failed to fetch daemon version for build badge', { error });
        daemonVersion = null;
      });
  });
</script>

{#if enabled}
  <div class="build-badge" class:mismatch aria-hidden="true">
    <span class="part">app {frontendVersion}</span>
    <span class="sep">·</span>
    <span class="part">daemon {daemonVersion ?? '?'}</span>
    {#if mismatch}<span class="warn">⚠ mismatch</span>{/if}
  </div>
{/if}

<style>
  .build-badge {
    position: fixed;
    bottom: 4px;
    left: 6px;
    /* High enough to sit above app chrome, but the badge is inert (see
       pointer-events) so it never intercepts interaction. */
    z-index: 2147483000;
    pointer-events: none;
    user-select: none;
    white-space: nowrap;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 10px;
    line-height: 1.2;
    color: rgba(127, 127, 127, 0.55);
  }

  .sep {
    margin: 0 3px;
    opacity: 0.5;
  }

  .warn {
    margin-left: 5px;
  }

  .build-badge.mismatch,
  .build-badge.mismatch .warn {
    color: #e5a50a;
  }
</style>
