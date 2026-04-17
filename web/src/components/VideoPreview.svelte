<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import mpegts from 'mpegts.js';

	let {
		sourceId,
		label = '',
		active = false,
		onclick,
	}: {
		sourceId: string;
		label?: string;
		active?: boolean;
		onclick?: () => void;
	} = $props();

	let videoEl: HTMLVideoElement;
	let player: mpegts.Player | null = null;
	let destroyed = false;

	function createPlayer() {
		if (!mpegts.isSupported() || destroyed) return;

		const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		const url = `${proto}//${window.location.host}/api/v1/sources/${sourceId}/preview`;

		player = mpegts.createPlayer(
			{
				type: 'flv',
				isLive: true,
				url,
			},
			{
				enableWorker: false,
				liveBufferLatencyChasing: true,
				liveBufferLatencyMaxLatency: 1.5,
				liveBufferLatencyMinRemain: 0.3,
			},
		);

		player.on(mpegts.Events.ERROR, () => {
			destroyPlayer();
			// Retry after a short delay
			if (!destroyed) {
				setTimeout(createPlayer, 2000);
			}
		});

		player.attachMediaElement(videoEl);
		player.load();

		// Wait for data before playing to avoid play/pause race
		videoEl.onloadeddata = () => {
			if (!destroyed && videoEl) {
				videoEl.play().catch(() => {});
			}
		};
	}

	function destroyPlayer() {
		if (player) {
			try {
				player.unload();
				player.detachMediaElement();
				player.destroy();
			} catch {
				// ignore cleanup errors
			}
			player = null;
		}
	}

	onMount(() => {
		createPlayer();
	});

	onDestroy(() => {
		destroyed = true;
		destroyPlayer();
	});
</script>

<button
	class="group relative aspect-video w-full overflow-hidden rounded-lg border-2 bg-black {active
		? 'border-red-500'
		: 'border-neutral-700 hover:border-neutral-500'}"
	onclick={onclick}
>
	<video
		bind:this={videoEl}
		class="h-full w-full object-contain"
		muted
		playsinline
	></video>
	{#if label}
		<div
			class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent px-3 py-2"
		>
			<span class="text-xs font-medium text-white">{label}</span>
		</div>
	{/if}
	{#if active}
		<div
			class="absolute top-2 right-2 rounded bg-red-600 px-2 py-0.5 text-xs font-bold text-white"
		>
			LIVE
		</div>
	{/if}
</button>
