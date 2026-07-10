<!--
  ExtensionOutlet — mounts one lazily-loaded UI-extension component.

  Generic host used by the app shell (chrome slots) and node viewers (viewer
  extensions) to render a registry contribution without importing it directly.
  The dynamic import runs once per `load` value; callers key their {#each} by the
  contribution's variant so a variant change mounts a fresh outlet with a new load.
-->
<script lang="ts" generics="Props extends Record<string, unknown> = Record<string, never>">
  import type { Component } from 'svelte';

  let {
    load,
    props = {} as Props
  }: {
    load: () => Promise<{ default: Component<Props> }>;
    props?: Props;
  } = $props();
</script>

{#await load() then mod}
  {@const Loaded = mod.default}
  <Loaded {...props} />
{/await}
