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

<div class="flex h-[calc(100vh-24px)] flex-col">
	<div class="mb-2 text-xs font-semibold uppercase tracking-wide text-red-400">Program Output</div>
	{#if sourceId}
		{#key sourceId}
			<div class="flex-1"><VideoPreview {sourceId} active={true} /></div>
		{/key}
	{:else}
		<div class="flex flex-1 items-center justify-center rounded-lg border-2 border-red-900 bg-black">
			<span class="text-sm text-neutral-600">Waiting for program source...</span>
		</div>
	{/if}
</div>
