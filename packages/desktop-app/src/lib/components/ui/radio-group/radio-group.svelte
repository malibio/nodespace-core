<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: RadioGroupPrimitive.RootProps & { ref?: HTMLElement | null } = $props();
</script>

<RadioGroupPrimitive.Root
  bind:ref
  data-slot="radio-group"
  class={cn('grid gap-2', className)}
  {...restProps}
/>
