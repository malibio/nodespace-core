<script lang="ts">
  import { cn } from '$lib/utils';
  import type { WithElementRef } from '$lib/utils';
  import type { HTMLLiAttributes } from 'svelte/elements';
  import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: WithElementRef<HTMLLiAttributes> = $props();
</script>

<li
  bind:this={ref}
  data-slot="breadcrumb-ellipsis"
  role="presentation"
  aria-hidden="true"
  class={cn('flex size-9 items-center justify-center', className)}
  {...restProps}
>
  <MoreHorizontalIcon class="size-4" />
  <span class="sr-only">More</span>
</li>
