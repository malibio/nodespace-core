<script lang="ts">
  import { Tabs as TabsPrimitive } from 'bits-ui';

  let { ref = $bindable(null), ...restProps }: TabsPrimitive.RootProps & { ref?: HTMLElement | null } = $props();
</script>

<TabsPrimitive.Root bind:ref data-slot="tabs" {...restProps} />
