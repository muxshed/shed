<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children }: { children?: Snippet } = $props();
</script>
<div class="mb-4 space-y-1.5">
	{#if children}{@render children()}{/if}
</div>
