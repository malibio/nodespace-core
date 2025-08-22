<script lang="ts" module>
  // Re-export types and variants from separate file for better TypeScript support
  export { badgeVariants, type BadgeVariant } from './types.js';
</script>

<script lang="ts">
  import type { HTMLAnchorAttributes } from 'svelte/elements';
  import { cn, type WithElementRef } from '$lib/utils';
  import { badgeVariants, type BadgeVariant } from './types.js';

  let {
    ref = $bindable(null),
    href,
    class: className,
    variant = 'default',
    children,
    ...restProps
  }: WithElementRef<HTMLAnchorAttributes> & {
    variant?: BadgeVariant;
  } = $props();
</script>

<svelte:element
  this={href ? 'a' : 'span'}
  bind:this={ref}
  data-slot="badge"
  {href}
  class={cn(badgeVariants({ variant }), className)}
  {...restProps}
>
  {@render children?.()}
</svelte:element>
