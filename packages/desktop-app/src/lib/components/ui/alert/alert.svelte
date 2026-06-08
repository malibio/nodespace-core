<script lang="ts">
	import { cn } from '$lib/utils.js';
	import { alertVariants, type AlertProps } from './types.js';

	let {
		class: className,
		variant = 'default',
		ref = $bindable(null),
		children,
		...restProps
	}: AlertProps = $props();
</script>

<div
	bind:this={ref}
	role="alert"
	data-slot="alert"
	class={cn(alertVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</div>
