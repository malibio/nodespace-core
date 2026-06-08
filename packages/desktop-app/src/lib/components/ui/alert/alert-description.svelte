<script lang="ts">
	import { cn } from '$lib/utils.js';
	import type { AlertDescriptionProps } from './types.js';

	let {
		class: className,
		ref = $bindable(null),
		children,
		...restProps
	}: AlertDescriptionProps = $props();
</script>

<p
	bind:this={ref}
	data-slot="alert-description"
	class={cn('text-sm [&_p]:leading-relaxed', className)}
	{...restProps}
>
	{@render children?.()}
</p>
