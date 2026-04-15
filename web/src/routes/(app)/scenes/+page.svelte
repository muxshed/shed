<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { scenes, sources, pipelineState } from '$lib/stores/pipeline';
	import type { Scene, Asset } from '$lib/types';

	let name = $state('');
	let creating = $state(false);
	let error = $state('');
	let editingScene = $state<Scene | null>(null);
	let addLayerSourceId = $state('');
	let assets = $state<Asset[]>([]);

	onMount(async () => {
		await refresh();
		sources.set(await api.listSources());
		assets = await api.listAssets();
	});

	async function refresh() {
		scenes.set(await api.listScenes());
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		error = '';
		try {
			await api.createScene(name.trim());
			name = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create scene';
		} finally {
			creating = false;
		}
	}

	async function activate(id: string) {
		try {
			await api.activateScene(id);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to activate scene';
		}
	}

	async function remove(id: string) {
		await api.deleteScene(id);
		if (editingScene?.id === id) editingScene = null;
		await refresh();
	}

	async function addLayer(sceneId: string) {
		if (!addLayerSourceId) return;
		let sourceId = addLayerSourceId;
		if (sourceId.startsWith('asset:')) {
			const assetId = sourceId.slice(6);
			const source = await api.createSourceFromAsset(assetId);
			sourceId = source.id;
			sources.set(await api.listSources());
		}
		await api.addLayer(sceneId, { source_id: sourceId });
		addLayerSourceId = '';
		await refresh();
		if (editingScene) {
			editingScene = $scenes.find((s) => s.id === editingScene!.id) || null;
		}
	}

	async function removeLayer(sceneId: string, layerId: string) {
		await api.deleteLayer(sceneId, layerId);
		await refresh();
		if (editingScene) {
			editingScene = $scenes.find((s) => s.id === editingScene!.id) || null;
		}
	}

	function activeSceneId(): string | null {
		if ($pipelineState.state === 'live') return $pipelineState.active_scene;
		return null;
	}
</script>

<div class="mx-auto max-w-4xl">
	<h1 class="mb-6 text-2xl font-bold">Scenes</h1>

	<div class="mb-6 rounded-lg border border-neutral-700 bg-neutral-900 p-4">
		<h2 class="mb-3 text-sm font-semibold text-neutral-400">Create Scene</h2>
		<form onsubmit={(e) => { e.preventDefault(); create(); }} class="flex gap-3">
			<input
				bind:value={name}
				placeholder="Scene name"
				class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none"
			/>
			<button
				type="submit"
				disabled={creating || !name.trim()}
				class="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
			>
				Create
			</button>
		</form>
		{#if error}
			<p class="mt-2 text-sm text-red-400">{error}</p>
		{/if}
	</div>

	{#if $scenes.length === 0}
		<p class="text-sm text-neutral-500">No scenes yet.</p>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2">
			{#each $scenes as scene (scene.id)}
				<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-4 {activeSceneId() === scene.id ? 'border-green-600' : ''}">
					<div class="mb-2 flex items-center justify-between">
						<span class="font-medium">{scene.name}</span>
						<span class="text-xs text-neutral-500">{scene.layers.length} layer{scene.layers.length !== 1 ? 's' : ''}</span>
					</div>
					<div class="flex gap-2">
						<button
							onclick={() => activate(scene.id)}
							class="rounded bg-green-800 px-3 py-1 text-xs text-green-300 hover:bg-green-700"
						>
							Activate
						</button>
						<button
							onclick={() => (editingScene = editingScene?.id === scene.id ? null : scene)}
							class="rounded bg-neutral-700 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-600"
						>
							{editingScene?.id === scene.id ? 'Close' : 'Edit Layers'}
						</button>
						<button
							onclick={() => remove(scene.id)}
							class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
						>
							Delete
						</button>
					</div>

					{#if editingScene?.id === scene.id}
						<div class="mt-3 border-t border-neutral-700 pt-3">
							<h3 class="mb-2 text-xs font-semibold text-neutral-400">Layers</h3>
							{#if scene.layers.length === 0}
								<p class="text-xs text-neutral-500">No layers</p>
							{:else}
								{#each scene.layers as layer (layer.id)}
									<div class="mb-1 flex items-center justify-between rounded bg-neutral-800 p-2 text-xs">
										<span>
											{$sources.find((s) => s.id === layer.source_id)?.name || layer.source_id.slice(0, 8)}
											| {layer.size.width}x{layer.size.height} @ ({layer.position.x},{layer.position.y})
											| z:{layer.z_index}
										</span>
										<button
											onclick={() => removeLayer(scene.id, layer.id)}
											class="text-red-400 hover:text-red-300"
										>
											Remove
										</button>
									</div>
								{/each}
							{/if}
							<div class="mt-2 flex gap-2">
								<select
									bind:value={addLayerSourceId}
									class="flex-1 rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-white"
								>
									<option value="">Select source...</option>
									{#if $sources.length > 0}
										<optgroup label="Sources">
											{#each $sources as src}
												<option value={src.id}>{src.name}</option>
											{/each}
										</optgroup>
									{/if}
									{#if assets.length > 0}
										<optgroup label="Media Library">
											{#each assets as asset}
												<option value="asset:{asset.id}">{asset.name}</option>
											{/each}
										</optgroup>
									{/if}
								</select>
								<button
									onclick={() => addLayer(scene.id)}
									disabled={!addLayerSourceId}
									class="rounded bg-blue-600 px-3 py-1 text-xs text-white hover:bg-blue-700 disabled:opacity-50"
								>
									Add Layer
								</button>
							</div>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>
