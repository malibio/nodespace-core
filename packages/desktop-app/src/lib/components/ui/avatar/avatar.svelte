<script lang="ts">
  import { Avatar as AvatarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils.js';
  import { avatarVariants, type AvatarProps } from './types.js';

  let {
    ref = $bindable(null),
    class: className,
    size = 'md',
    shape = 'circle',
    children,
    ...restProps
  }: AvatarProps = $props();
</script>

<AvatarPrimitive.Root
  bind:ref
  data-slot="avatar"
  class={cn(avatarVariants({ size, shape }), className)}
  {...restProps}
>
  {@render children?.()}
</AvatarPrimitive.Root>
