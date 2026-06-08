<script lang="ts">
  import { Accordion as AccordionPrimitive } from 'bits-ui';
  import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
  import { cn } from '$lib/utils';

  let { ref = $bindable(null), class: className, children, ...restProps }: AccordionPrimitive.TriggerProps = $props();
</script>

<AccordionPrimitive.Header class="flex">
  <AccordionPrimitive.Trigger
    bind:ref
    data-slot="accordion-trigger"
    class={cn(
      'flex flex-1 items-center justify-between py-4 text-sm font-medium transition-all hover:underline text-left [&[data-state=open]>svg]:rotate-180',
      className
    )}
    {...restProps}
  >
    {@render children?.()}
    <ChevronDownIcon class="text-muted-foreground h-4 w-4 shrink-0 transition-transform duration-200" />
  </AccordionPrimitive.Trigger>
</AccordionPrimitive.Header>
