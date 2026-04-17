<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api';
	import { connectWs, disconnectWs } from '$lib/ws';
	import { sources } from '$lib/stores/pipeline';
	import VideoPreview from '../../../components/VideoPreview.svelte';
	import type { Source } from '$lib/types';

	let programId = $state<string | null>(null);
	let channel: BroadcastChannel;

	function liveSources(): Source[] {
		return $sources.filter((s) => s.state === 'live');
	}

	onMount(async () => {
		connectWs();
		sources.set(await api.listSources());

		channel = new BroadcastChannel('muxshed-studio');
		channel.onmessage = (e) => {
			if (e.data.type === 'program_source') programId = e.data.sourceId;
		};
		channel.postMessage({ type: 'request_state' });
	});

	onDestroy(() => {
		disconnectWs();
		channel?.close();
	});

	function switchSource(id: string) {
		channel?.postMessage({ type: 'cut_source', sourceId: id });
	}

	function previewSource(id: string) {
		channel?.postMessage({ type: 'set_preview', sourceId: id });
	}
</script>

<svelte:head><title>Sources - Muxshed</title></svelte:head>

<div>
	<div class="mb-3 text-xs font-semibold uppercase tracking-wide text-neutral-400">Sources</div>
	{#if liveSources().length === 0}
		<p class="text-sm text-neutral-500">No live sources</p>
	{:else}
		<div class="grid gap-3 {liveSources().length <= 2 ? 'grid-cols-2' : 'grid-cols-3'}">
			{#each liveSources() as source (source.id)}
				<div class="rounded-lg border-2 p-2 {source.id === programId ? 'border-red-500 bg-red-950/30' : 'border-neutral-700'}">
					<VideoPreview sourceId={source.id} label={source.name} active={source.id === programId} />
					<div class="mt-2 flex gap-2">
						<button
							onclick={() => previewSource(source.id)}
							disabled={source.id === programId}
							class="flex-1 rounded bg-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-600 disabled:opacity-30"
						>Next Up</button>
						<button
							onclick={() => switchSource(source.id)}
							disabled={source.id === programId}
							class="flex-1 rounded bg-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-red-700 hover:text-white disabled:opacity-30"
						>Switch</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
