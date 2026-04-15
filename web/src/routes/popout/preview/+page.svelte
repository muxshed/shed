<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
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

<div class="flex h-[calc(100vh-24px)] flex-col">
	<div class="mb-2 text-xs font-semibold uppercase tracking-wide text-green-400">Preview / Next Up</div>
	{#if sourceId}
		{#key sourceId}
			<div class="flex-1"><VideoPreview {sourceId} /></div>
		{/key}
	{:else}
		<div class="flex flex-1 items-center justify-center rounded-lg border-2 border-dashed border-neutral-700 bg-black">
			<span class="text-sm text-neutral-600">No preview source selected</span>
		</div>
	{/if}
</div>
