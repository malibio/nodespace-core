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

<th
  bind:this={ref}
  data-slot="table-head"
  class={cn(
    'text-muted-foreground border-border h-10 border-b-2 px-4 text-left align-middle text-xs font-semibold tracking-wide whitespace-nowrap',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</th>
