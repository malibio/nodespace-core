<!--
  Collaboration locked placeholder.

  Shown in the collection viewer's Collaboration tab for the `enable-prompt`
  variant — a Pro daemon whose active database has `sync_enabled: false`. People &
  roles live in the cloud tenant a database is bound to, so there is nothing to
  show until sync is enabled. Purely static: no membership calls, no daemon
  commands. Accepts the host collection's `nodeId` for a uniform viewer-extension
  signature (surfaced as a data attribute for debugging).
-->
<script lang="ts">
  let { nodeId }: { nodeId: string } = $props();
</script>

<div class="collab-locked" data-collection-id={nodeId}>
  <div class="lock" aria-hidden="true">🔒</div>
  <p class="headline">Collaboration is off for this database</p>
  <p class="detail">
    Enable cloud sync for this database to invite people, manage roles, and share
    collections.
  </p>
</div>

<style>
  .collab-locked {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 48px 24px;
    text-align: center;
    color: hsl(var(--muted-foreground));
  }

  .lock {
    font-size: 28px;
    opacity: 0.8;
  }

  .headline {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .detail {
    margin: 0;
    max-width: 360px;
    font-size: 13px;
    line-height: 1.5;
  }
</style>
