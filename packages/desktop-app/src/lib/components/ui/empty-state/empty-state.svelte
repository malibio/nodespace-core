<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  let {
    title,
    description,
    icon,
    action,
    class: className,
    ...restProps
  }: {
    title: string;
    description?: string;
    icon?: Snippet;
    action?: Snippet;
    class?: string;
  } = $props();
</script>

<div
  data-slot="empty-state"
  class={cn('flex flex-col items-center justify-center gap-4 p-8 text-center', className)}
  {...restProps}
>
  {#if icon}
    <div class="bg-muted rounded-full p-4">
      {@render icon()}
    </div>
  {/if}
  <div class="flex flex-col gap-1">
    <p class="text-foreground text-sm font-medium">{title}</p>
    {#if description}
      <p class="text-muted-foreground text-xs">{description}</p>
    {/if}
  </div>
  {#if action}
    {@render action()}
  {/if}
</div>
