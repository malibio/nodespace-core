<script lang="ts">
  import { cn } from '$lib/utils';
  import type { WithElementRef } from '$lib/utils';
  import type { HTMLAnchorAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAnchorAttributes> = $props();
</script>

<a
  bind:this={ref}
  data-slot="breadcrumb-link"
  class={cn('transition-colors hover:text-foreground', className)}
  {...restProps}
>
  {@render children?.()}
</a>
