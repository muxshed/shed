// Licensed under the Business Source License 1.1 — see LICENSE.

import { tv, type VariantProps } from 'tailwind-variants';
export { default as Button } from './Button.svelte';

export const buttonVariants = tv({
	base: 'btn',
	variants: {
		variant: {
			default: '',
			destructive: 'btn--danger',
			outline: '',
			secondary: 'btn--ghost',
			ghost: 'btn--ghost',
			link: 'btn--ghost',
			live: 'btn--danger',
			success: 'btn--go',
		},
		size: {
			default: '',
			sm: 'text-[11px] px-3',
			lg: 'px-8 min-h-10',
			icon: 'btn--icon',
		},
	},
	defaultVariants: {
		variant: 'default',
		size: 'default',
	},
});

export type Variant = VariantProps<typeof buttonVariants>['variant'];
export type Size = VariantProps<typeof buttonVariants>['size'];
