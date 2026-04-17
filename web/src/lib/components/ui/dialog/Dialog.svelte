<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		open = $bindable(false),
		children,
	}: {
		open?: boolean;
		children?: Snippet;
	} = $props();
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center">
		<button class="fixed inset-0 bg-black/60" onclick={() => (open = false)}></button>
		<div class="relative z-50">
			{#if children}{@render children()}{/if}
		</div>
	</div>
{/if}
