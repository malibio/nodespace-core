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

<tr
  bind:this={ref}
  data-slot="table-row"
  class={cn(
    'border-border hover:bg-muted/50 border-b transition-colors',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</tr>
