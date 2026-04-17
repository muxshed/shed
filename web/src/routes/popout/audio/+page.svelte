<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api';
	import { connectWs, disconnectWs } from '$lib/ws';
	import { sources } from '$lib/stores/pipeline';
	import type { AudioRouting, Source } from '$lib/types';

	let programId = $state<string | null>(null);
	let audioRouting = $state<AudioRouting>({ active_audio_source: null, channels: [], audio_follows_video: true });
	let channel: BroadcastChannel;

	function liveSources(): Source[] {
		return $sources.filter((s) => s.state === 'live');
	}

	onMount(async () => {
		connectWs();
		sources.set(await api.listSources());
		audioRouting = await api.getAudioRouting();

		channel = new BroadcastChannel('muxshed-studio');
		channel.onmessage = (e) => {
			if (e.data.type === 'program_source') programId = e.data.sourceId;
			if (e.data.type === 'audio_routing') audioRouting = e.data.routing;
		};
		channel.postMessage({ type: 'request_state' });
	});

	onDestroy(() => {
		disconnectWs();
		channel?.close();
	});

	async function setAudioSource(id: string | null) {
		await api.setAudioSource(id);
		audioRouting.active_audio_source = id;
		audioRouting.audio_follows_video = id === null;
		channel?.postMessage({ type: 'audio_routing', routing: audioRouting });
	}

	async function toggleFollows() {
		await api.toggleAudioFollowsVideo();
		audioRouting.audio_follows_video = !audioRouting.audio_follows_video;
		if (audioRouting.audio_follows_video) audioRouting.active_audio_source = null;
		channel?.postMessage({ type: 'audio_routing', routing: audioRouting });
	}
</script>

<svelte:head><title>Audio - Muxshed</title></svelte:head>

<div>
	<div class="mb-3 flex items-center justify-between">
		<span class="text-xs font-semibold uppercase tracking-wide text-neutral-400">Audio Mixer</span>
		<button
			onclick={toggleFollows}
			class="rounded px-2 py-0.5 text-xs {audioRouting.audio_follows_video ? 'bg-green-900 text-green-400' : 'bg-neutral-700 text-neutral-400'}"
		>
			{audioRouting.audio_follows_video ? 'Follows video' : 'Independent'}
		</button>
	</div>
	<div class="space-y-2">
		{#each liveSources() as source (source.id)}
			{@const isActive = audioRouting.audio_follows_video ? source.id === programId : source.id === audioRouting.active_audio_source}
			<button
				onclick={() => setAudioSource(source.id)}
				disabled={audioRouting.audio_follows_video}
				class="flex w-full items-center gap-3 rounded px-4 py-3 text-left {isActive ? 'bg-green-950 ring-1 ring-green-600' : 'bg-neutral-800'} disabled:cursor-default"
			>
				<div class="flex h-6 w-16 items-end gap-px">
					{#each Array(10) as _, i}
						<div
							class="w-1 rounded-sm {isActive ? (i < 7 ? 'bg-green-500' : i < 9 ? 'bg-yellow-500' : 'bg-red-500') : 'bg-neutral-700'}"
							style="height: {isActive ? Math.max(20, Math.random() * 100) : 20}%"
						></div>
					{/each}
				</div>
				<span class="flex-1 text-sm {isActive ? 'text-white' : 'text-neutral-400'}">{source.name}</span>
				{#if isActive}
					<span class="rounded bg-green-900 px-1.5 py-0.5 text-xs text-green-400">ACTIVE</span>
				{/if}
			</button>
		{/each}
	</div>
</div>
