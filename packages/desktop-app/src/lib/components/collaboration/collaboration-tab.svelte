<!--
  Collaboration tab.

  Registry wrapper that mounts the unchanged `CollaborationView` for the
  sync-enabled variants (`sign-in` / `connected`). Exists only to move the view's
  mounting behind the UI-extension registry — `CollaborationView` keeps its own
  internal `proSync.tier` gate and all its logic.
-->
<script lang="ts">
  import CollaborationView from '$lib/components/collaboration/collaboration-view.svelte';

  let { nodeId }: { nodeId: string } = $props();
</script>

<CollaborationView collectionId={nodeId} />
