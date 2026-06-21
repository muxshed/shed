<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { destinations } from '$lib/stores/pipeline';
	import type { DestinationKind } from '$lib/types';
	import DestinationCard from '../../../components/DestinationCard.svelte';
	import { notify } from '$lib/notify';

	let name = $state('');
	let platform = $state('custom');
	let url = $state('');
	let streamKey = $state('');
	let creating = $state(false);

	const platforms: Record<string, { label: string; url: string }> = {
		youtube: { label: 'YouTube', url: 'rtmp://a.rtmp.youtube.com/live2' },
		twitch: { label: 'Twitch', url: 'rtmp://live.twitch.tv/app' },
		kick: { label: 'Kick', url: 'rtmps://fa723fc1b171.global-contribute.live-video.net/app' },
		custom: { label: 'Custom RTMP', url: '' },
	};

	$effect(() => {
		const p = platforms[platform];
		if (p && p.url) url = p.url;
	});

	onMount(refresh);

	async function refresh() {
		destinations.set(await api.listDestinations());
	}

	async function create() {
		if (!name.trim() || !url.trim() || !streamKey.trim()) return;
		creating = true;
		try {
			const kind: DestinationKind = { type: 'rtmp', url: url.trim(), stream_key: streamKey.trim() };
			await api.createDestination(name.trim(), kind);
			name = '';
			streamKey = '';
			platform = 'custom';
			url = '';
			await refresh();
			notify.success('Destination added');
		} catch (e) {
			notify.error(e);
		} finally {
			creating = false;
		}
	}
</script>

<div class="mx-auto max-w-4xl space-y-6">
	<section class="panel">
		<header class="panel__head">▮ Add Destination</header>
		<div class="panel__body">
			<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-3">
				<div class="flex gap-3">
					<div class="flex-1">
						<label class="field-label" for="dest-name">Name</label>
						<input
							id="dest-name"
							bind:value={name}
							placeholder="e.g. My YouTube"
							class="input"
						/>
					</div>
					<div>
						<label class="field-label" for="dest-platform">Platform</label>
						<select id="dest-platform" bind:value={platform} class="select">
							{#each Object.entries(platforms) as [key, p]}
								<option value={key}>{p.label}</option>
							{/each}
						</select>
					</div>
				</div>
				<div>
					<label class="field-label" for="dest-url">RTMP URL</label>
					<input id="dest-url" bind:value={url} placeholder="rtmp://…" class="input" />
				</div>
				<div>
					<label class="field-label" for="dest-key">Stream Key</label>
					<input id="dest-key" bind:value={streamKey} placeholder="Stream key" class="input" />
				</div>
				<button
					type="submit"
					disabled={creating || !name.trim() || !url.trim() || !streamKey.trim()}
					class="btn"
				>
					{creating ? 'Adding…' : '+ Add Destination'}
				</button>
			</form>
		</div>
	</section>

	<section class="panel">
		<header class="panel__head">▮ Destinations</header>
		<div class="panel__body">
			{#if $destinations.length === 0}
				<p class="text-xs text-amber-muted">No destinations yet. Add one above.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2">
					{#each $destinations as dest (dest.id)}
						<DestinationCard destination={dest} onupdate={refresh} />
					{/each}
				</div>
			{/if}
		</div>
	</section>
</div>
