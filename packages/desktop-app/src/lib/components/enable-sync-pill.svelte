<!--
  Enable-sync prompt.

  Rendered in the app-shell overlay slot for the `enable-prompt` variant — a Pro
  daemon whose active database has `sync_enabled: false`. Offers to turn cloud sync
  on for this database.

  The concrete "turn sync on" action available today is starting the interactive
  sign-in (`pro_initiate_oauth`): a Pro user opts a database into sync by signing
  in to the cloud, after which the daemon binds the tenant and flips
  `sync_enabled` / `auth_status` on the DatabaseSettingsNode, advancing the variant.
  (A dedicated per-database "enable sync" intent command is backend follow-up.)

  Never rendered in the community build (that resolves to `teaser`), so invoking a
  Pro command here is safe.
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('EnableSyncPill');

  let pending = $state(false);

  async function enableSync() {
    if (pending) return;
    pending = true;
    try {
      // Starting sign-in is how a Pro user enables sync for a database today.
      await invoke('pro_initiate_oauth');
      log.info('enable-sync: sign-in started');
    } catch (e) {
      log.warn('enable-sync: pro_initiate_oauth failed', { error: e });
    } finally {
      pending = false;
    }
  }
</script>

<button
  class="enable-sync-pill"
  type="button"
  title="Turn on cloud sync for this database"
  aria-label="Enable cloud sync for this database"
  disabled={pending}
  onclick={enableSync}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="label">{pending ? 'Enabling…' : 'Enable sync'}</span>
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

  .enable-sync-pill:hover:not(:disabled) {
    background: hsl(var(--muted));
  }

  .enable-sync-pill:disabled {
    cursor: default;
    opacity: 0.85;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #2563eb;
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.18);
  }
</style>
