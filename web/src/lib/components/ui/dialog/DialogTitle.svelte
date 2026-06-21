<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children }: { children?: Snippet } = $props();
</script>
<h2 class="flex items-center gap-2 text-[11px] font-medium tracking-[2px] uppercase text-amber-dim">
	<span class="text-amber">▮</span>{#if children}{@render children()}{/if}
</h2>
