<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children }: { children?: Snippet } = $props();
</script>
<div class="mt-4 flex justify-end gap-2">
	{#if children}{@render children()}{/if}
</div>
