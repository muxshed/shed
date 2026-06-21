<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { scenes, sources, pipelineState } from '$lib/stores/pipeline';
	import { notify } from '$lib/notify';
	import type { Scene, Asset } from '$lib/types';

	let name = $state('');
	let creating = $state(false);
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
		try {
			await api.createScene(name.trim());
			name = '';
			await refresh();
			notify.success('Scene created');
		} catch (e) {
			notify.error(e);
		} finally {
			creating = false;
		}
	}

	async function activate(id: string) {
		try {
			await api.activateScene(id);
		} catch (e) {
			notify.error(e);
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

<div class="mx-auto max-w-4xl space-y-6">
	<section class="panel">
		<header class="panel__head">▮ Create Scene</header>
		<div class="panel__body">
			<form onsubmit={(e) => { e.preventDefault(); create(); }} class="flex items-end gap-3">
				<div class="flex-1">
					<label class="field-label" for="scene-name">Scene Name</label>
					<input id="scene-name" bind:value={name} placeholder="Scene name" class="input" />
				</div>
				<button type="submit" disabled={creating || !name.trim()} class="btn">
					{creating ? 'Creating…' : '+ Create'}
				</button>
			</form>
		</div>
	</section>

	<section class="panel">
		<header class="panel__head">▮ Scenes</header>
		<div class="panel__body">
			{#if $scenes.length === 0}
				<p class="text-xs text-amber-muted">No scenes yet.</p>
			{:else}
				<div class="grid gap-3 sm:grid-cols-2">
					{#each $scenes as scene (scene.id)}
						<div class="rounded-md border bg-panel-raised p-3 {activeSceneId() === scene.id ? 'border-live' : 'border-border-dim'}">
							<div class="mb-3 flex items-center justify-between">
								<span class="text-amber">{scene.name}</span>
								<span class="pill {activeSceneId() === scene.id ? 'pill--live' : 'pill--idle'}">
									{activeSceneId() === scene.id ? '● LIVE' : `○ ${scene.layers.length} LAYER${scene.layers.length !== 1 ? 'S' : ''}`}
								</span>
							</div>
							<div class="flex gap-2">
								<button onclick={() => activate(scene.id)} class="btn btn--go">
									Activate
								</button>
								<button
									onclick={() => (editingScene = editingScene?.id === scene.id ? null : scene)}
									class="btn btn--ghost"
								>
									{editingScene?.id === scene.id ? 'Close' : 'Edit Layers'}
								</button>
								<button onclick={() => remove(scene.id)} class="btn btn--danger">
									Delete
								</button>
							</div>

							{#if editingScene?.id === scene.id}
								<div class="mt-3 border-t border-border-dim pt-3">
									<h3 class="label mb-2">Layers</h3>
									{#if scene.layers.length === 0}
										<p class="text-xs text-amber-muted">No layers</p>
									{:else}
										{#each scene.layers as layer (layer.id)}
											<div class="row mb-1 text-xs">
												<span class="text-amber-dim">
													<span class="text-amber-muted">▸</span>
													{$sources.find((s) => s.id === layer.source_id)?.name || layer.source_id.slice(0, 8)}
													| {layer.size.width}x{layer.size.height} @ ({layer.position.x},{layer.position.y})
													| z:{layer.z_index}
												</span>
												<button
													onclick={() => removeLayer(scene.id, layer.id)}
													class="btn btn--ghost text-danger"
													aria-label="Remove layer"
													title="Remove layer"
												>
													Remove
												</button>
											</div>
										{/each}
									{/if}
									<div class="mt-2 flex items-end gap-2">
										<div class="flex-1">
											<label class="field-label" for="scene-{scene.id}-add-source">Add Layer Source</label>
											<select id="scene-{scene.id}-add-source" bind:value={addLayerSourceId} class="select">
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
										</div>
										<button onclick={() => addLayer(scene.id)} disabled={!addLayerSourceId} class="btn">
											+ Add Layer
										</button>
									</div>
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</section>
</div>
