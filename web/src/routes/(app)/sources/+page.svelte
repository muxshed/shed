<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { sources } from '$lib/stores/pipeline';
	import SourceCard from '../../../components/SourceCard.svelte';
	import type { SourceKind } from '$lib/types';

	let name = $state('');
	let protocol = $state<'rtmp' | 'srt'>('rtmp');
	let srtPassphrase = $state('');
	let creating = $state(false);
	let error = $state('');

	onMount(refresh);

	async function refresh() {
		sources.set(await api.listSources());
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		error = '';
		try {
			let kind: SourceKind;
			if (protocol === 'srt') {
				kind = {
					type: 'srt',
					port: 0,
					passphrase: srtPassphrase.trim() || undefined,
				};
			} else {
				kind = { type: 'rtmp', stream_key: '' };
			}
			await api.createSource(name.trim(), kind);
			name = '';
			srtPassphrase = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create source';
		} finally {
			creating = false;
		}
	}
</script>

<div class="mx-auto max-w-4xl">
	<h1 class="mb-6 text-2xl font-bold">Sources</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Add Source</h2>
		<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-3">
			<div class="flex gap-3">
				<select
					bind:value={protocol}
					class="rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white"
				>
					<option value="rtmp">RTMP</option>
					<option value="srt">SRT</option>
				</select>
				<input
					bind:value={name}
					placeholder="Source name (e.g. OBS Main)"
					class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
				<button
					type="submit"
					disabled={creating || !name.trim()}
					class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
				>
					Add Source
				</button>
			</div>
			{#if protocol === 'srt'}
				<input
					bind:value={srtPassphrase}
					placeholder="Passphrase (optional, min 10 chars)"
					class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
				/>
			{/if}
		</form>
		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	{#if $sources.length === 0}
		<p class="text-sm text-neutral-500">No sources yet. Create one above.</p>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2">
			{#each $sources as source (source.id)}
				<SourceCard {source} ondelete={refresh} />
			{/each}
		</div>
	{/if}
</div>
