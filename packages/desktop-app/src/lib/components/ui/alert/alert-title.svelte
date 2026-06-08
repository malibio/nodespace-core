<script lang="ts">
	import { cn } from '$lib/utils.js';
	import type { AlertTitleProps } from './types.js';

	let {
		class: className,
		ref = $bindable(null),
		children,
		...restProps
	}: AlertTitleProps = $props();
</script>

<h5
	bind:this={ref}
	data-slot="alert-title"
	class={cn('mb-1 font-medium leading-none tracking-tight', className)}
	{...restProps}
>
	{@render children?.()}
</h5>
