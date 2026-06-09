<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn, type WithElementRef } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>> = $props();
</script>

<thead
  bind:this={ref}
  data-slot="table-header"
  class={cn('bg-background sticky top-0 z-10', className)}
  {...restProps}
>
  {@render children?.()}
</thead>
