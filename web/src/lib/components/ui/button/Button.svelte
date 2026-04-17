<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
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
