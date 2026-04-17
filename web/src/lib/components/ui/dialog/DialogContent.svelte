<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { cn } from '$lib/utils';
	import type { Snippet } from 'svelte';

	let {
		class: className,
		children,
	}: {
		class?: string;
		children?: Snippet;
	} = $props();
</script>

<div class={cn('w-full max-w-lg rounded-lg border border-neutral-700 bg-neutral-900 p-6 shadow-xl', className)}>
	{#if children}{@render children()}{/if}
</div>
