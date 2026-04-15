// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { tv, type VariantProps } from 'tailwind-variants';
export { default as Button } from './Button.svelte';

export const buttonVariants = tv({
	base: 'inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-neutral-400 disabled:pointer-events-none disabled:opacity-50',
	variants: {
		variant: {
			default: 'bg-white text-neutral-900 shadow hover:bg-neutral-200',
			destructive: 'bg-red-600 text-white shadow-sm hover:bg-red-700',
			outline: 'border border-neutral-700 bg-transparent text-neutral-300 hover:bg-neutral-800 hover:text-white',
			secondary: 'bg-neutral-800 text-neutral-300 shadow-sm hover:bg-neutral-700',
			ghost: 'text-neutral-400 hover:bg-neutral-800 hover:text-white',
			link: 'text-blue-400 underline-offset-4 hover:underline',
			live: 'bg-red-600 text-white shadow-sm hover:bg-red-700',
			success: 'bg-green-600 text-white shadow-sm hover:bg-green-700',
		},
		size: {
			default: 'h-9 px-4 py-2',
			sm: 'h-8 rounded-md px-3 text-xs',
			lg: 'h-10 rounded-md px-8',
			icon: 'h-9 w-9',
		},
	},
	defaultVariants: {
		variant: 'default',
		size: 'default',
	},
});

export type Variant = VariantProps<typeof buttonVariants>['variant'];
export type Size = VariantProps<typeof buttonVariants>['size'];
