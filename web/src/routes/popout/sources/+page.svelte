<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
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

<section class="panel">
	<header class="panel__head">▮ SOURCES</header>
	<div class="panel__body">
		{#if liveSources().length === 0}
			<p class="text-amber-muted">No live sources</p>
		{:else}
			<div class="grid gap-3 {liveSources().length <= 2 ? 'grid-cols-2' : 'grid-cols-3'}">
				{#each liveSources() as source (source.id)}
					<div class="rounded-sm border p-2 {source.id === programId ? 'border-danger bg-panel-raised' : 'border-border-dim bg-panel-raised'}">
						<VideoPreview sourceId={source.id} label={source.name} active={source.id === programId} />
						<div class="mt-2 flex gap-2">
							<button
								onclick={() => previewSource(source.id)}
								disabled={source.id === programId}
								class="btn flex-1"
							>Next Up</button>
							<button
								onclick={() => switchSource(source.id)}
								disabled={source.id === programId}
								class="btn btn--danger flex-1"
							>Switch</button>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</section>
