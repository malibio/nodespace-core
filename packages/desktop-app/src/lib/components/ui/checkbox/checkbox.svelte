<script lang="ts">
  import { Checkbox as CheckboxPrimitive } from 'bits-ui';
  import CheckIcon from '@lucide/svelte/icons/check';
  import { cn } from '$lib/utils.js';
  import type { CheckboxProps } from './index.js';

  let {
    ref = $bindable(null),
    class: className,
    checked = $bindable(false),
    indeterminate = $bindable(false),
    ...restProps
  }: CheckboxProps = $props();
</script>

<CheckboxPrimitive.Root
  bind:ref
  bind:checked
  bind:indeterminate
  data-slot="checkbox"
  class={cn(
    'peer size-4 shrink-0 rounded-sm border border-primary shadow-xs outline-none',
    'focus-visible:ring-ring/50 focus-visible:ring-[3px]',
    'disabled:cursor-not-allowed disabled:opacity-50',
    'data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground data-[state=checked]:border-primary',
    'aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40',
    className
  )}
  {...restProps}
>
  {#snippet children({ checked: isChecked })}
    <span class="flex items-center justify-center text-current">
      <CheckIcon class={cn('size-3.5', !isChecked && 'invisible')} />
    </span>
  {/snippet}
</CheckboxPrimitive.Root>
