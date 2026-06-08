<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import CircleIcon from '@lucide/svelte/icons/circle';
  import { cn } from '$lib/utils';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: RadioGroupPrimitive.ItemProps & { ref?: HTMLElement | null } = $props();
</script>

<RadioGroupPrimitive.Item
  bind:ref
  data-slot="radio-group-item"
  class={cn(
    'border-input text-primary focus-visible:ring-ring aspect-square size-4 rounded-full border shadow-xs focus:outline-none focus-visible:ring-1 disabled:cursor-not-allowed disabled:opacity-50',
    className
  )}
  {...restProps}
>
  {#snippet children({ checked })}
    <span class="flex items-center justify-center">
      {#if checked}
        <CircleIcon class="size-2.5 fill-current text-current" />
      {/if}
    </span>
  {/snippet}
</RadioGroupPrimitive.Item>
