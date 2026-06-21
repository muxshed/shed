<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import VideoPreview from '../../../components/VideoPreview.svelte';

	let sourceId = $state<string | null>(null);
	let channel: BroadcastChannel;

	onMount(() => {
		channel = new BroadcastChannel('muxshed-studio');
		channel.onmessage = (e) => {
			if (e.data.type === 'program_source') {
				sourceId = e.data.sourceId;
			}
		};
		channel.postMessage({ type: 'request_state' });
	});

	onDestroy(() => channel?.close());
</script>

<svelte:head><title>Program - Muxshed</title></svelte:head>

<section class="panel flex h-[calc(100vh-24px)] flex-col">
	<header class="panel__head"><span class="text-danger-glow">▮ PROGRAM</span></header>
	{#if sourceId}
		{#key sourceId}
			<div class="flex-1"><VideoPreview {sourceId} active={true} /></div>
		{/key}
	{:else}
		<div class="scanlines-well flex flex-1 items-center justify-center border-t border-border">
			<span class="text-amber-muted">Waiting for program source…</span>
		</div>
	{/if}
</section>
