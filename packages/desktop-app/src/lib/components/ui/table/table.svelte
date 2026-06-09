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

<div class="relative w-full overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
  <table
    bind:this={ref}
    data-slot="table"
    class={cn('w-full min-w-full caption-bottom text-sm', className)}
    {...restProps}
  >
    {@render children?.()}
  </table>
</div>
