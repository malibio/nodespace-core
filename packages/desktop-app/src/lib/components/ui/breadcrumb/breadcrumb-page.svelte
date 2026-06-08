<script lang="ts">
  import { cn } from '$lib/utils';
  import type { WithElementRef } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLSpanElement>> = $props();
</script>

<span
  bind:this={ref}
  data-slot="breadcrumb-page"
  aria-current="page"
  aria-disabled="true"
  class={cn('font-normal text-foreground', className)}
  {...restProps}
>
  {@render children?.()}
</span>
