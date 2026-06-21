<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import VideoPreview from '../../../components/VideoPreview.svelte';

	let sourceId = $state<string | null>(null);
	let channel: BroadcastChannel;

	onMount(() => {
		channel = new BroadcastChannel('muxshed-studio');
		channel.onmessage = (e) => {
			if (e.data.type === 'preview_source') {
				sourceId = e.data.sourceId;
			}
		};
		channel.postMessage({ type: 'request_state' });
	});

	onDestroy(() => channel?.close());
</script>

<svelte:head><title>Preview - Muxshed</title></svelte:head>

<section class="panel flex h-[calc(100vh-24px)] flex-col">
	<header class="panel__head"><span class="text-amber-dim">▮ PREVIEW / NEXT UP</span></header>
	{#if sourceId}
		{#key sourceId}
			<div class="flex-1"><VideoPreview {sourceId} /></div>
		{/key}
	{:else}
		<div class="scanlines-well flex flex-1 items-center justify-center border-t border-border">
			<span class="text-amber-muted">No preview source selected</span>
		</div>
	{/if}
</section>
