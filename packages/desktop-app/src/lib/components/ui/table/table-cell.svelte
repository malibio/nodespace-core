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

<td
  bind:this={ref}
  data-slot="table-cell"
  class={cn(
    'border-border max-w-[20rem] overflow-hidden text-ellipsis whitespace-nowrap border-b px-4 py-3 align-middle',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</td>
