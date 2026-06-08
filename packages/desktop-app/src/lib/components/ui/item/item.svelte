<script lang="ts">
	import { cn } from '$lib/utils.js';
	import { itemVariants, type ItemProps } from './types.js';

	let {
		class: className,
		variant = 'default',
		size = 'md',
		leading,
		trailing,
		ref = $bindable(null),
		children,
		...restProps
	}: ItemProps = $props();
</script>

<div
	bind:this={ref}
	data-slot="item"
	class={cn(itemVariants({ variant, size }), className)}
	{...restProps}
>
	{#if leading}
		<span class="shrink-0 text-muted-foreground [&_svg]:size-4">
			{@render leading()}
		</span>
	{/if}
	<span class="flex-1 truncate">
		{@render children?.()}
	</span>
	{#if trailing}
		<span class="shrink-0 ml-auto text-muted-foreground [&_svg]:size-4">
			{@render trailing()}
		</span>
	{/if}
</div>
