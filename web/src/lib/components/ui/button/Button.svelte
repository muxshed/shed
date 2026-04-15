<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { cn } from '$lib/utils';
	import { buttonVariants, type Variant, type Size } from '.';
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	let {
		class: className,
		variant = 'default',
		size = 'default',
		children,
		...restProps
	}: HTMLButtonAttributes & {
		variant?: Variant;
		size?: Size;
		children?: Snippet;
	} = $props();
</script>

<button class={cn(buttonVariants({ variant, size }), className)} {...restProps}>
	{#if children}{@render children()}{/if}
</button>
