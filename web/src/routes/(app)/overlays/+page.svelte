<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { overlays } from '$lib/stores/pipeline';
	import type { Overlay } from '$lib/types';

	let name = $state('');
	let filePath = $state('');
	let creating = $state(false);
	let error = $state('');

	onMount(refresh);

	async function refresh() {
		overlays.set(await api.listOverlays());
	}

	async function create() {
		if (!name.trim() || !filePath.trim()) return;
		creating = true;
		error = '';
		try {
			await api.createOverlay(name.trim(), { type: 'image', file_path: filePath.trim() });
			name = '';
			filePath = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create overlay';
		} finally {
			creating = false;
		}
	}

	async function toggleVisibility(overlay: Overlay) {
		if (overlay.visible) {
			await api.hideOverlay(overlay.id);
		} else {
			await api.showOverlay(overlay.id);
		}
		await refresh();
	}

	async function remove(id: string) {
		await api.deleteOverlay(id);
		await refresh();
	}
</script>

<div class="mx-auto max-w-4xl">
	<h1 class="mb-6 text-2xl font-bold">Overlays</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Add Image Overlay</h2>
		<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-3">
			<input
				bind:value={name}
				placeholder="Overlay name"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<input
				bind:value={filePath}
				placeholder="Image file path (e.g. /config/overlays/logo.png)"
				class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={creating || !name.trim() || !filePath.trim()}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				Add Overlay
			</button>
		</form>
		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	{#if $overlays.length === 0}
		<p class="text-sm text-neutral-500">No overlays yet.</p>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2">
			{#each $overlays as overlay (overlay.id)}
				<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-4 {overlay.visible ? '' : 'opacity-60'}">
					<div class="mb-2 flex items-center justify-between">
						<span class="font-medium">{overlay.name}</span>
						<span class="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-400 uppercase">
							{overlay.kind.type}
						</span>
					</div>
					<div class="mb-3 text-xs text-neutral-500">
						{overlay.position.x},{overlay.position.y} | z:{overlay.z_index}
					</div>
					<div class="flex gap-2">
						<button
							onclick={() => toggleVisibility(overlay)}
							class="rounded px-3 py-1 text-xs {overlay.visible
								? 'bg-green-900 text-green-400'
								: 'bg-neutral-700 text-neutral-400'} hover:opacity-80"
						>
							{overlay.visible ? 'Visible' : 'Hidden'}
						</button>
						<button
							onclick={() => remove(overlay.id)}
							class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
						>
							Delete
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
