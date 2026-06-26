<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
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

<div class={cn('panel w-full max-w-lg', className)}>
	<div class="panel__body">
		{#if children}{@render children()}{/if}
	</div>
</div>
