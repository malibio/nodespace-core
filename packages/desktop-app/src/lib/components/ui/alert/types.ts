import type { HTMLAttributes } from 'svelte/elements';
import { type VariantProps, tv } from 'tailwind-variants';
import type { WithElementRef } from '$lib/utils';

export const alertVariants = tv({
	base: 'relative w-full rounded-lg border-l-4 p-4 [&>svg~*]:pl-7 [&>svg+div]:translate-y-[-3px] [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-4',
	variants: {
		variant: {
			default: 'border-l-blue-500 bg-blue-50 text-blue-900 dark:bg-blue-950/30 dark:text-blue-100 [&>svg]:text-blue-500',
			destructive: 'border-l-red-500 bg-red-50 text-red-900 dark:bg-red-950/30 dark:text-red-100 [&>svg]:text-red-500',
			warning: 'border-l-yellow-500 bg-yellow-50 text-yellow-900 dark:bg-yellow-950/30 dark:text-yellow-100 [&>svg]:text-yellow-500',
			success: 'border-l-green-500 bg-green-50 text-green-900 dark:bg-green-950/30 dark:text-green-100 [&>svg]:text-green-500',
		},
	},
	defaultVariants: {
		variant: 'default',
	},
});

export type AlertVariant = VariantProps<typeof alertVariants>['variant'];

export type AlertProps = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
	variant?: AlertVariant;
};

export type AlertTitleProps = WithElementRef<HTMLAttributes<HTMLParagraphElement>>;

export type AlertDescriptionProps = WithElementRef<HTMLAttributes<HTMLParagraphElement>>;
