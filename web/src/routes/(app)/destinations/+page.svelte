<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { destinations } from '$lib/stores/pipeline';
	import type { DestinationKind } from '$lib/types';
	import DestinationCard from '../../../components/DestinationCard.svelte';

	let name = $state('');
	let platform = $state('custom');
	let url = $state('');
	let streamKey = $state('');
	let creating = $state(false);
	let error = $state('');

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
		error = '';
		try {
			const kind: DestinationKind = { type: 'rtmp', url: url.trim(), stream_key: streamKey.trim() };
			await api.createDestination(name.trim(), kind);
			name = '';
			streamKey = '';
			platform = 'custom';
			url = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create destination';
		} finally {
			creating = false;
		}
	}
</script>

<div class="mx-auto max-w-4xl">
	<h1 class="mb-6 text-2xl font-bold">Destinations</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Add Destination</h2>
		<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-3">
			<div class="flex gap-3">
				<input
					bind:value={name}
					placeholder="Name (e.g. My YouTube)"
					class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<select
					bind:value={platform}
					class="rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				>
					{#each Object.entries(platforms) as [key, p]}
						<option value={key}>{p.label}</option>
					{/each}
				</select>
			</div>
			<input
				bind:value={url}
				placeholder="RTMP URL"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<input
				bind:value={streamKey}
				placeholder="Stream key"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={creating || !name.trim() || !url.trim() || !streamKey.trim()}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				Add Destination
			</button>
		</form>
		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	{#if $destinations.length === 0}
		<p class="text-sm text-neutral-500">No destinations yet. Add one above.</p>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2">
			{#each $destinations as dest (dest.id)}
				<DestinationCard destination={dest} onupdate={refresh} />
			{/each}
		</div>
	{/if}
</div>
