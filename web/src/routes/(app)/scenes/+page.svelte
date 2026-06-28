<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { scenes, sources, pipelineState } from '$lib/stores/pipeline';
	import { notify } from '$lib/notify';
	import type { Scene, Layer, Asset, LayerFit } from '$lib/types';

	const OUT_W = 1920;
	const OUT_H = 1080;

	let name = $state('');
	let creating = $state(false);
	let editing = $state<Scene | null>(null);
	let selectedLayer = $state<string | null>(null);
	let addSourceId = $state('');
	let canvasEl = $state<HTMLDivElement | null>(null);
	let assets = $state<Asset[]>([]);

	onMount(async () => {
		await refresh();
		sources.set(await api.listSources());
		assets = await api.listAssets();
	});

	async function refresh() {
		const list = await api.listScenes();
		scenes.set(list);
		editing = editing ? (list.find((s) => s.id === editing!.id) ?? null) : (list[0] ?? null);
	}

	const activeId = $derived($pipelineState.state === 'live' ? $pipelineState.active_scene : null);

	const orderedLayers = $derived(
		editing ? [...editing.layers].sort((a, b) => a.z_index - b.z_index) : []
	);
	const selected = $derived(editing?.layers.find((l) => l.id === selectedLayer) ?? null);

	function srcName(id: string): string {
		return $sources.find((s) => s.id === id)?.name ?? id.slice(0, 8);
	}

	async function create() {
		if (!name.trim()) return;
		creating = true;
		try {
			const s = await api.createScene(name.trim());
			name = '';
			await refresh();
			editing = $scenes.find((x) => x.id === s.id) ?? null;
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
			notify.success('Scene is live');
		} catch (e) {
			notify.error(e);
		}
	}

	async function del(id: string) {
		await api.deleteScene(id);
		if (editing?.id === id) editing = null;
		await refresh();
	}

	async function addLayer() {
		if (!editing || !addSourceId) return;
		try {
			// A media-library asset (image/video) becomes a source first, then a layer.
			let sourceId = addSourceId;
			if (sourceId.startsWith('asset:')) {
				const src = await api.createSourceFromAsset(sourceId.slice(6));
				sourceId = src.id;
				sources.set(await api.listSources());
			}
			const n = editing.layers.length;
			const layer =
				n === 0
					? { source_id: sourceId, x: 0, y: 0, width: OUT_W, height: OUT_H, z_index: 0, opacity: 1 }
					: { source_id: sourceId, x: 1180 - n * 30, y: 620 - n * 30, width: 640, height: 360, z_index: n, opacity: 1 };
			await api.addLayer(editing.id, layer);
			addSourceId = '';
			await refresh();
		} catch (e) {
			notify.error(e);
		}
	}

	async function removeLayer(id: string) {
		if (!editing) return;
		await api.deleteLayer(editing.id, id);
		if (selectedLayer === id) selectedLayer = null;
		await refresh();
	}

	async function saveLayer(l: Layer) {
		if (!editing) return;
		try {
			await api.updateLayer(editing.id, l.id, {
				x: Math.round(l.position.x),
				y: Math.round(l.position.y),
				width: Math.round(l.size.width),
				height: Math.round(l.size.height),
				z_index: l.z_index,
				opacity: l.opacity,
				fit: l.fit
			});
		} catch (e) {
			notify.error(e);
		}
	}

	// --- drag + resize on the canvas ---
	let drag: {
		id: string;
		mode: 'move' | 'resize';
		sx: number;
		sy: number;
		ox: number;
		oy: number;
		ow: number;
		oh: number;
	} | null = null;

	function onDown(l: Layer, mode: 'move' | 'resize', e: PointerEvent) {
		e.preventDefault();
		e.stopPropagation();
		selectedLayer = l.id;
		drag = { id: l.id, mode, sx: e.clientX, sy: e.clientY, ox: l.position.x, oy: l.position.y, ow: l.size.width, oh: l.size.height };
		window.addEventListener('pointermove', onMove);
		window.addEventListener('pointerup', onUp);
	}

	function onMove(e: PointerEvent) {
		if (!drag || !editing || !canvasEl) return;
		const rect = canvasEl.getBoundingClientRect();
		const dx = ((e.clientX - drag.sx) / rect.width) * OUT_W;
		const dy = ((e.clientY - drag.sy) / rect.height) * OUT_H;
		const l = editing.layers.find((x) => x.id === drag!.id);
		if (!l) return;
		if (drag.mode === 'move') {
			l.position.x = Math.max(0, Math.min(OUT_W - l.size.width, drag.ox + dx));
			l.position.y = Math.max(0, Math.min(OUT_H - l.size.height, drag.oy + dy));
		} else {
			l.size.width = Math.max(80, Math.min(OUT_W - l.position.x, drag.ow + dx));
			l.size.height = Math.max(45, Math.min(OUT_H - l.position.y, drag.oh + dy));
		}
	}

	async function onUp() {
		window.removeEventListener('pointermove', onMove);
		window.removeEventListener('pointerup', onUp);
		const d = drag;
		drag = null;
		if (d && editing) {
			const l = editing.layers.find((x) => x.id === d.id);
			if (l) await saveLayer(l);
		}
	}

	async function nudgeZ(l: Layer, dir: 1 | -1) {
		l.z_index = Math.max(0, l.z_index + dir);
		await saveLayer(l);
		await refresh();
	}
	async function setOpacity(l: Layer, v: number) {
		l.opacity = v;
		await saveLayer(l);
	}
	async function setFit(l: Layer, v: LayerFit) {
		l.fit = v;
		await saveLayer(l);
	}
</script>

<div class="mx-auto flex max-w-6xl gap-4">
	<!-- Scene list -->
	<aside class="w-56 shrink-0 space-y-3">
		<section class="panel">
			<header class="panel__head">▮ Scenes</header>
			<div class="panel__body space-y-2">
				<form onsubmit={(e) => { e.preventDefault(); create(); }} class="space-y-2">
					<div>
						<label class="field-label" for="scene-name">Scene name</label>
						<input id="scene-name" bind:value={name} placeholder="e.g. Intro" class="input" />
					</div>
					<button type="submit" disabled={creating || !name.trim()} class="btn w-full">
						{creating ? 'Creating…' : '+ Create Scene'}
					</button>
				</form>
				<div class="border-t border-border-dim pt-2"></div>
				{#each $scenes as scene (scene.id)}
					<button
						onclick={() => { editing = scene; selectedLayer = null; }}
						class="rack-item w-full justify-between {editing?.id === scene.id ? 'rack-item--active' : ''}"
					>
						<span class="truncate">{scene.name}</span>
						<span class="pill {activeId === scene.id ? 'pill--live' : 'pill--idle'} text-[9px]">
							{activeId === scene.id ? '● LIVE' : scene.layers.length}
						</span>
					</button>
				{/each}
			</div>
		</section>
	</aside>

	<!-- Editor -->
	<main class="min-w-0 flex-1 space-y-3">
		{#if !editing}
			<div class="panel"><div class="panel__body text-center text-sm text-amber-muted">Create or select a scene to start.</div></div>
		{:else}
			<section class="panel">
				<header class="panel__head">
					<span>▮ {editing.name}</span>
					<div class="flex gap-2">
						<button onclick={() => activate(editing!.id)} class="btn btn--go">● Take to Program</button>
						<button onclick={() => del(editing!.id)} class="btn btn--danger">Delete</button>
					</div>
				</header>
				<div class="panel__body space-y-3">
					<!-- Canvas -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						bind:this={canvasEl}
						class="scanlines-well relative w-full overflow-hidden rounded-md border border-border bg-black"
						style="aspect-ratio: 16 / 9"
						onpointerdown={() => (selectedLayer = null)}
					>
						{#each orderedLayers as l (l.id)}
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="layer {selectedLayer === l.id ? 'layer--sel' : ''}"
								style="left:{(l.position.x / OUT_W) * 100}%;top:{(l.position.y / OUT_H) * 100}%;width:{(l.size.width / OUT_W) * 100}%;height:{(l.size.height / OUT_H) * 100}%;opacity:{l.opacity}"
								onpointerdown={(e) => onDown(l, 'move', e)}
							>
								<span class="layer__label">{srcName(l.source_id)}</span>
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div class="layer__resize" onpointerdown={(e) => onDown(l, 'resize', e)}></div>
							</div>
						{/each}
						{#if editing.layers.length === 0}
							<div class="absolute inset-0 flex items-center justify-center text-sm text-amber-muted">
								Add a source layer below
							</div>
						{/if}
					</div>

					<!-- Add layer -->
					<div class="flex items-end gap-2">
						<div class="flex-1">
							<label class="field-label" for="add-src">Add layer (source or image)</label>
							<select id="add-src" bind:value={addSourceId} class="select">
								<option value="">Select source or image…</option>
								{#if $sources.length > 0}
									<optgroup label="Sources">
										{#each $sources as s}
											<option value={s.id}>{s.name}</option>
										{/each}
									</optgroup>
								{/if}
								{#if assets.length > 0}
									<optgroup label="Media Library">
										{#each assets as a}
											<option value="asset:{a.id}">{a.name}</option>
										{/each}
									</optgroup>
								{/if}
							</select>
						</div>
						<button onclick={addLayer} disabled={!addSourceId} class="btn">+ Add Layer</button>
					</div>
				</div>
			</section>

			<!-- Layer list / controls -->
			<section class="panel">
				<header class="panel__head">▮ Layers (top = front)</header>
				<div class="panel__body space-y-1">
					{#if editing.layers.length === 0}
						<p class="text-xs text-amber-muted">No layers yet.</p>
					{:else}
						{#each [...orderedLayers].reverse() as l (l.id)}
							<div class="row items-center {selectedLayer === l.id ? 'border-[--color-amber]' : ''}">
								<button class="flex-1 truncate text-left text-amber-dim" onclick={() => (selectedLayer = l.id)}>
									<span class="text-amber-muted">▸</span>
									{srcName(l.source_id)}
									<span class="text-[11px] text-amber-muted">
										· {Math.round(l.size.width)}×{Math.round(l.size.height)} @ {Math.round(l.position.x)},{Math.round(l.position.y)} · z{l.z_index}
									</span>
								</button>
								<div class="flex items-center gap-1">
									<select
										value={l.fit ?? 'fill'}
										onchange={(e) => setFit(l, e.currentTarget.value as LayerFit)}
										class="select h-7 py-0 text-[11px]" title="Fit mode"
										aria-label="Fit mode"
									>
										<option value="fill">Fill</option>
										<option value="contain">Contain</option>
										<option value="cover">Cover</option>
									</select>
									<input
										type="range" min="0" max="1" step="0.05" value={l.opacity}
										oninput={(e) => setOpacity(l, +e.currentTarget.value)}
										class="w-20" title="Opacity"
									/>
									<button class="btn btn--ghost" onclick={() => nudgeZ(l, 1)} title="Bring forward" aria-label="Bring forward">▲</button>
									<button class="btn btn--ghost" onclick={() => nudgeZ(l, -1)} title="Send back" aria-label="Send back">▼</button>
									<button class="btn btn--danger" onclick={() => removeLayer(l.id)}>✕</button>
								</div>
							</div>
						{/each}
					{/if}
				</div>
			</section>
		{/if}
	</main>
</div>

<style>
	.layer {
		position: absolute;
		border: 1px solid var(--color-amber-dim);
		background: rgba(255, 176, 0, 0.06);
		cursor: move;
		touch-action: none;
		box-sizing: border-box;
	}
	.layer--sel {
		border-color: var(--color-amber-bright);
		box-shadow: 0 0 0 1px var(--color-amber-bright);
		background: rgba(255, 176, 0, 0.12);
	}
	.layer__label {
		position: absolute;
		top: 2px;
		left: 4px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--color-amber-bright);
		pointer-events: none;
		text-shadow: 0 1px 2px #000;
	}
	.layer__resize {
		position: absolute;
		right: -5px;
		bottom: -5px;
		width: 12px;
		height: 12px;
		background: var(--color-amber-bright);
		border: 1px solid var(--color-bg);
		border-radius: 2px;
		cursor: nwse-resize;
		touch-action: none;
	}
</style>
