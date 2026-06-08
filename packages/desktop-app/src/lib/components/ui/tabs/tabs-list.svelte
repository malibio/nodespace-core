<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  let { ref = $bindable(null), class: className, ...restProps }: TabsPrimitive.ListProps & { ref?: HTMLElement | null } = $props();
</script>

<TabsPrimitive.List
  bind:ref
  data-slot="tabs-list"
  class={cn(
    'bg-muted text-muted-foreground inline-flex h-9 items-center justify-center rounded-lg p-1',
    className
  )}
  {...restProps}
/>
