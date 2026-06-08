<script lang="ts">
  import { cn } from '$lib/utils';
  import type { WithElementRef } from '$lib/utils';
  import type { HTMLLiAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLLiAttributes> = $props();
</script>

<li
  bind:this={ref}
  data-slot="breadcrumb-item"
  class={cn('inline-flex items-center gap-1.5', className)}
  {...restProps}
>
  {@render children?.()}
</li>
