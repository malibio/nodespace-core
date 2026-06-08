import type { Snippet } from 'svelte';
import { type VariantProps, tv } from 'tailwind-variants';
import type { WithElementRef } from '$lib/utils';
import type { HTMLAttributes } from 'svelte/elements';

export const itemVariants = tv({
	base: 'flex items-center gap-2 px-2 py-1.5 rounded-md text-sm cursor-default select-none outline-none transition-colors',
	variants: {
		variant: {
			default: 'text-foreground hover:bg-accent hover:text-accent-foreground',
			active: 'bg-accent text-accent-foreground',
			muted: 'text-muted-foreground hover:bg-accent hover:text-accent-foreground'
		},
		size: {
			sm: 'px-1.5 py-1 text-xs',
			md: 'px-2 py-1.5 text-sm'
		}
	},
	defaultVariants: {
		variant: 'default',
		size: 'md'
	}
});

export type ItemVariant = VariantProps<typeof itemVariants>['variant'];
export type ItemSize = VariantProps<typeof itemVariants>['size'];

export type ItemProps = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
	variant?: ItemVariant;
	size?: ItemSize;
	leading?: Snippet;
	trailing?: Snippet;
};
