<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import type { Asset, AssetFolder } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '$lib/components/ui/dialog';

	let assets = $state<Asset[]>([]);
	let folders = $state<AssetFolder[]>([]);
	let currentFolder = $state<string | null>(null);
	let error = $state('');
	let dragging = $state(false);
	let viewMode = $state<'grid' | 'list'>('grid');

	// Upload
	let showUpload = $state(false);
	let uploadName = $state('');
	let uploadFile = $state<File | null>(null);
	let uploading = $state(false);

	// New folder
	let showNewFolder = $state(false);
	let newFolderName = $state('');
	let newFolderColor = $state('#6366f1');

	// Asset detail/edit
	let showDetail = $state(false);
	let detailAsset = $state<Asset | null>(null);
	let editLoopMode = $state<'one_shot' | 'loop'>('one_shot');
	let editStart = $state(0);
	let editOpaque = $state(0);
	let editClear = $state(0);
	let editEnd = $state(0);

	// Delete
	let showDelete = $state(false);
	let maxMs = $derived(Math.max(editEnd, editClear, editOpaque, editStart, 1000));
	let deleteTarget = $state<{ type: 'asset' | 'folder'; id: string; name: string } | null>(null);

	onMount(refresh);

	async function refresh() {
		[assets, folders] = await Promise.all([
			api.listAssets(currentFolder || undefined),
			api.listFolders(),
		]);
	}

	async function navigateFolder(id: string | null) {
		currentFolder = id;
		await refresh();
	}

	async function upload() {
		if (!uploadName.trim() || !uploadFile) return;
		uploading = true;
		error = '';
		try {
			await api.uploadAsset(uploadName.trim(), uploadFile, currentFolder || undefined);
			showUpload = false;
			uploadName = '';
			uploadFile = null;
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Upload failed';
		} finally {
			uploading = false;
		}
	}

	async function createFolder() {
		if (!newFolderName.trim()) return;
		try {
			await api.createFolder(newFolderName.trim(), currentFolder || undefined, newFolderColor);
			showNewFolder = false;
			newFolderName = '';
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed';
		}
	}

	function openDetail(asset: Asset) {
		detailAsset = asset;
		editLoopMode = asset.loop_mode;
		editStart = asset.start_ms;
		editOpaque = asset.opaque_ms;
		editClear = asset.clear_ms;
		editEnd = asset.end_ms;
		showDetail = true;
	}

	async function saveDetail() {
		if (!detailAsset) return;
		try {
			await api.updateAsset(detailAsset.id, {
				loop_mode: editLoopMode,
				start_ms: editStart,
				opaque_ms: editOpaque,
				clear_ms: editClear,
				end_ms: editEnd,
			});
			showDetail = false;
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Save failed';
		}
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		try {
			if (deleteTarget.type === 'asset') await api.deleteAsset(deleteTarget.id);
			else await api.deleteFolder(deleteTarget.id);
			showDelete = false;
			deleteTarget = null;
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Delete failed';
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragging = false;
		const file = e.dataTransfer?.files[0];
		if (file) {
			uploadFile = file;
			uploadName = file.name.replace(/\.[^.]+$/, '');
			showUpload = true;
		}
	}

	function handleFileSelect(e: Event) {
		const input = e.target as HTMLInputElement;
		if (input.files?.[0]) {
			uploadFile = input.files[0];
			if (!uploadName) uploadName = uploadFile.name.replace(/\.[^.]+$/, '');
		}
	}

	function typeIcon(type: string): string {
		switch (type) {
			case 'image': return 'IMG';
			case 'video': return 'VID';
			case 'stinger': return 'STG';
			default: return 'FILE';
		}
	}

	function typeColor(type: string): string {
		switch (type) {
			case 'image': return 'bg-blue-900/50 text-blue-400';
			case 'video': return 'bg-purple-900/50 text-purple-400';
			case 'stinger': return 'bg-orange-900/50 text-orange-400';
			default: return 'bg-neutral-800 text-neutral-400';
		}
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
		return `${(bytes / 1048576).toFixed(1)} MB`;
	}

	function breadcrumbs(): { id: string | null; name: string }[] {
		const trail: { id: string | null; name: string }[] = [{ id: null, name: 'Library' }];
		if (currentFolder) {
			const folder = folders.find((f) => f.id === currentFolder);
			if (folder) trail.push({ id: folder.id, name: folder.name });
		}
		return trail;
	}

	// Folders that belong to current level
	function currentFolders(): AssetFolder[] {
		return folders.filter((f) => f.parent_id === currentFolder);
	}
</script>

<div
	class="mx-auto max-w-6xl"
	ondragover={(e) => { e.preventDefault(); dragging = true; }}
	ondragleave={() => (dragging = false)}
	ondrop={handleDrop}
	role="region"
>
	<!-- Header -->
	<div class="mb-4 flex items-center justify-between">
		<div class="flex items-center gap-2">
			{#each breadcrumbs() as crumb, i}
				{#if i > 0}<span class="text-neutral-600">/</span>{/if}
				<button
					onclick={() => navigateFolder(crumb.id)}
					class="text-sm {crumb.id === currentFolder ? 'font-bold text-white' : 'text-neutral-400 hover:text-white'}"
				>
					{crumb.name}
				</button>
			{/each}
		</div>
		<div class="flex gap-2">
			<Button variant="ghost" size="sm" onclick={() => (viewMode = viewMode === 'grid' ? 'list' : 'grid')}>
				{viewMode === 'grid' ? 'List' : 'Grid'}
			</Button>
			<Button variant="outline" size="sm" onclick={() => (showNewFolder = true)}>New Folder</Button>
			<Button size="sm" onclick={() => (showUpload = true)}>Upload</Button>
		</div>
	</div>

	<!-- Drop overlay -->
	{#if dragging}
		<div class="mb-4 flex h-32 items-center justify-center rounded-lg border-2 border-dashed border-blue-500 bg-blue-950/20">
			<span class="text-sm text-blue-400">Drop file to upload</span>
		</div>
	{/if}

	<!-- Folders -->
	{#if currentFolders().length > 0}
		<div class="mb-4 flex flex-wrap gap-2">
			{#each currentFolders() as folder (folder.id)}
				<div
					class="group flex items-center gap-2 rounded-lg border border-neutral-700 bg-neutral-900 px-4 py-2.5 transition-colors hover:border-neutral-600 cursor-pointer"
					onclick={() => navigateFolder(folder.id)}
					onkeydown={(e) => { if (e.key === 'Enter') navigateFolder(folder.id); }}
					role="button"
					tabindex="0"
				>
					<div class="h-3 w-3 rounded" style="background-color: {folder.color}"></div>
					<span class="text-sm text-neutral-300">{folder.name}</span>
					<span
						onclick={(e) => {
							e.stopPropagation();
							deleteTarget = { type: 'folder', id: folder.id, name: folder.name };
							showDelete = true;
						}}
						onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); deleteTarget = { type: 'folder', id: folder.id, name: folder.name }; showDelete = true; } }}
						role="button"
						tabindex="0"
						class="ml-1 hidden text-xs text-neutral-600 hover:text-red-400 group-hover:inline"
					>x</span>
				</div>
			{/each}
		</div>
	{/if}

	<!-- Assets -->
	{#if assets.length === 0 && currentFolders().length === 0}
		<div class="flex min-h-[200px] flex-col items-center justify-center rounded-lg border-2 border-dashed border-neutral-700 bg-neutral-900/50">
			<div class="mb-2 text-3xl text-neutral-700">+</div>
			<p class="mb-1 text-sm text-neutral-500">No files here</p>
			<p class="text-xs text-neutral-600">Drag and drop or click Upload</p>
		</div>
	{:else if viewMode === 'grid'}
		<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
			{#each assets as asset (asset.id)}
				<button
					onclick={() => openDetail(asset)}
					class="group rounded-lg border border-neutral-700 bg-neutral-900 p-3 text-left transition-colors hover:border-neutral-500"
				>
					<!-- Preview -->
					<div class="relative mb-2 flex aspect-video items-center justify-center overflow-hidden rounded bg-neutral-800">
						{#if asset.asset_type === 'image'}
							<img src={api.assetFileUrl(asset.id)} alt={asset.name} class="h-full w-full object-cover" />
						{:else if asset.has_thumbnail}
							<img src={api.assetThumbnailUrl(asset.id)} alt={asset.name} class="h-full w-full object-cover" />
						{:else}
							<span class="{typeColor(asset.asset_type)} rounded px-2 py-1 text-xs font-bold">{typeIcon(asset.asset_type)}</span>
						{/if}
						{#if asset.duration_ms > 0}
							<span class="absolute bottom-1 right-1 rounded bg-black/80 px-1.5 py-0.5 text-[10px] font-mono text-white">
								{Math.floor(asset.duration_ms / 60000)}:{String(Math.floor((asset.duration_ms % 60000) / 1000)).padStart(2, '0')}
							</span>
						{/if}
						{#if asset.loop_mode === 'loop'}
							<span class="absolute top-1 left-1 rounded bg-blue-600/80 px-1 py-0.5 text-[9px] font-bold text-white">LOOP</span>
						{/if}
					</div>
					<div class="truncate text-sm font-medium text-neutral-200">{asset.name}</div>
					<div class="flex items-center justify-between text-xs text-neutral-500">
						<span>
							{#if asset.metadata?.width}
								{asset.metadata.width}x{asset.metadata.height}
							{:else}
								{formatSize(asset.file_size)}
							{/if}
						</span>
						<span class="rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] uppercase">{asset.asset_type}</span>
					</div>
				</button>
			{/each}
		</div>
	{:else}
		<!-- List view -->
		<div class="rounded-lg border border-neutral-700 bg-neutral-900">
			{#each assets as asset, i (asset.id)}
				<button
					onclick={() => openDetail(asset)}
					class="flex w-full items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-neutral-800 {i > 0 ? 'border-t border-neutral-800' : ''}"
				>
					<div class="flex h-8 w-8 items-center justify-center rounded {typeColor(asset.asset_type)}">
						<span class="text-[10px] font-bold">{typeIcon(asset.asset_type)}</span>
					</div>
					<div class="min-w-0 flex-1">
						<div class="truncate text-sm text-white">{asset.name}</div>
					</div>
					<span class="text-xs text-neutral-500">{formatSize(asset.file_size)}</span>
					<span class="text-xs text-neutral-600">{new Date(asset.created_at).toLocaleDateString()}</span>
				</button>
			{/each}
		</div>
	{/if}

	{#if error}
		<p class="mt-4 text-sm text-red-400">{error}</p>
	{/if}
</div>

<!-- Upload Dialog -->
<Dialog bind:open={showUpload}>
	<DialogContent>
		<DialogHeader><DialogTitle>Upload Asset</DialogTitle></DialogHeader>
		<div class="space-y-3">
			<div>
				<label for="up-name" class="mb-1 block text-xs text-neutral-400">Name</label>
				<input id="up-name" bind:value={uploadName} placeholder="Asset name" class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none" />
			</div>
			<div>
				<label for="up-file" class="mb-1 block text-xs text-neutral-400">File</label>
				<input id="up-file" type="file" accept="image/*,video/*,.webm,.mov,.mp4,.mkv" onchange={handleFileSelect} class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-neutral-400 file:mr-3 file:rounded file:border-0 file:bg-neutral-700 file:px-3 file:py-1 file:text-sm file:text-white" />
				{#if uploadFile}
					<p class="mt-1 text-xs text-neutral-500">{uploadFile.name} ({formatSize(uploadFile.size)})</p>
				{/if}
			</div>
		</div>
		<DialogFooter>
			<Button variant="ghost" onclick={() => (showUpload = false)}>Cancel</Button>
			<Button disabled={uploading || !uploadName.trim() || !uploadFile} onclick={upload}>
				{uploading ? 'Uploading...' : 'Upload'}
			</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>

<!-- New Folder Dialog -->
<Dialog bind:open={showNewFolder}>
	<DialogContent>
		<DialogHeader><DialogTitle>New Folder</DialogTitle></DialogHeader>
		<div class="space-y-3">
			<div>
				<label for="folder-name" class="mb-1 block text-xs text-neutral-400">Folder Name</label>
				<input id="folder-name" bind:value={newFolderName} placeholder="e.g. Transitions" class="w-full rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm text-white focus:border-blue-500 focus:outline-none" />
			</div>
			<div>
				<label for="folder-color" class="mb-1 block text-xs text-neutral-400">Color</label>
				<div class="flex gap-2">
					{#each ['#6366f1', '#ef4444', '#22c55e', '#f59e0b', '#3b82f6', '#ec4899', '#8b5cf6', '#14b8a6'] as color}
						<button
							onclick={() => (newFolderColor = color)}
							class="h-7 w-7 rounded-full border-2 {newFolderColor === color ? 'border-white' : 'border-transparent'}"
							style="background-color: {color}"
						></button>
					{/each}
				</div>
			</div>
		</div>
		<DialogFooter>
			<Button variant="ghost" onclick={() => (showNewFolder = false)}>Cancel</Button>
			<Button disabled={!newFolderName.trim()} onclick={createFolder}>Create</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>

<!-- Asset Detail Dialog -->
<Dialog bind:open={showDetail}>
	<DialogContent class="max-w-2xl">
		{#if detailAsset}
			<DialogHeader><DialogTitle>{detailAsset.name}</DialogTitle></DialogHeader>

			<!-- Preview -->
			{#if detailAsset.asset_type === 'image'}
				<img
					src={api.assetFileUrl(detailAsset.id)}
					alt={detailAsset.name}
					class="mb-4 max-h-64 w-full rounded object-contain bg-neutral-800"
				/>
			{:else}
				<!-- Video player with timeline -->
				<div class="mb-4">
					<video
						src={api.assetFileUrl(detailAsset.id)}
						controls
						class="w-full rounded bg-black"
						preload="metadata"
					></video>
				</div>
			{/if}

			<!-- Info -->
			<div class="mb-4 grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
				<div class="rounded bg-neutral-800 p-2">
					<span class="text-neutral-500">Type</span>
					<div class="font-medium text-white capitalize">{detailAsset.asset_type}</div>
				</div>
				<div class="rounded bg-neutral-800 p-2">
					<span class="text-neutral-500">Size</span>
					<div class="font-medium text-white">{formatSize(detailAsset.file_size)}</div>
				</div>
				{#if detailAsset.metadata?.width}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">Resolution</span>
						<div class="font-medium text-white">{detailAsset.metadata.width}x{detailAsset.metadata.height}</div>
					</div>
				{/if}
				{#if detailAsset.metadata?.codec}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">Codec</span>
						<div class="font-medium text-white">{detailAsset.metadata.codec}</div>
					</div>
				{/if}
				{#if detailAsset.metadata?.fps}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">FPS</span>
						<div class="font-medium text-white">{detailAsset.metadata.fps.toFixed(1)}</div>
					</div>
				{/if}
				{#if detailAsset.metadata?.bitrate_kbps}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">Bitrate</span>
						<div class="font-medium text-white">{detailAsset.metadata.bitrate_kbps} kbps</div>
					</div>
				{/if}
				{#if detailAsset.duration_ms > 0}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">Duration</span>
						<div class="font-medium text-white">{(detailAsset.duration_ms / 1000).toFixed(1)}s</div>
					</div>
				{/if}
				{#if detailAsset.metadata?.audio_codec}
					<div class="rounded bg-neutral-800 p-2">
						<span class="text-neutral-500">Audio</span>
						<div class="font-medium text-white">{detailAsset.metadata.audio_codec}</div>
					</div>
				{/if}
			</div>

			<!-- Playback mode -->
			<div class="mb-4">
				<label class="mb-1 block text-xs text-neutral-400">Playback Mode</label>
				<div class="flex gap-2">
					<Button
						variant={editLoopMode === 'one_shot' ? 'default' : 'outline'}
						size="sm"
						onclick={() => (editLoopMode = 'one_shot')}
					>One Shot</Button>
					<Button
						variant={editLoopMode === 'loop' ? 'default' : 'outline'}
						size="sm"
						onclick={() => (editLoopMode = 'loop')}
					>Loop</Button>
				</div>
			</div>

			<!-- Markers (for video/stinger) -->
			{#if detailAsset.asset_type !== 'image'}
				<div class="mb-4">
					<label class="mb-2 block text-xs text-neutral-400">Transition Markers (ms)</label>

					<!-- Visual timeline -->
					<div class="relative mb-3 h-8 rounded bg-neutral-800">
						<div class="absolute top-0 bottom-0 rounded-l bg-green-900/40" style="left: 0; width: {(editStart / maxMs) * 100}%"></div>
						<div class="absolute top-0 bottom-0 bg-yellow-900/40" style="left: {(editStart / maxMs) * 100}%; width: {((editOpaque - editStart) / maxMs) * 100}%"></div>
						<div class="absolute top-0 bottom-0 bg-red-900/40" style="left: {(editOpaque / maxMs) * 100}%; width: {((editClear - editOpaque) / maxMs) * 100}%"></div>
						<div class="absolute top-0 bottom-0 rounded-r bg-blue-900/40" style="left: {(editClear / maxMs) * 100}%; width: {((editEnd - editClear) / maxMs) * 100}%"></div>
						<div class="absolute top-1 text-[9px] font-bold text-green-400" style="left: {(editStart / maxMs) * 100}%">S</div>
						<div class="absolute top-1 text-[9px] font-bold text-yellow-400" style="left: {(editOpaque / maxMs) * 100}%">O</div>
						<div class="absolute top-1 text-[9px] font-bold text-red-400" style="left: {(editClear / maxMs) * 100}%">C</div>
						<div class="absolute top-1 text-[9px] font-bold text-blue-400" style="left: {Math.min((editEnd / maxMs) * 100, 97)}%">E</div>
					</div>

					<div class="grid grid-cols-4 gap-2">
						<div>
							<label for="d-start" class="block text-[10px] text-green-400">Start</label>
							<input id="d-start" bind:value={editStart} type="number" min="0" class="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-white" />
						</div>
						<div>
							<label for="d-opaque" class="block text-[10px] text-yellow-400">Opaque (cut)</label>
							<input id="d-opaque" bind:value={editOpaque} type="number" min="0" class="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-white" />
						</div>
						<div>
							<label for="d-clear" class="block text-[10px] text-red-400">Clear</label>
							<input id="d-clear" bind:value={editClear} type="number" min="0" class="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-white" />
						</div>
						<div>
							<label for="d-end" class="block text-[10px] text-blue-400">End</label>
							<input id="d-end" bind:value={editEnd} type="number" min="0" class="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-white" />
						</div>
					</div>
				</div>
			{/if}

			<DialogFooter>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => {
						deleteTarget = { type: 'asset', id: detailAsset!.id, name: detailAsset!.name };
						showDetail = false;
						showDelete = true;
					}}
				>
					<span class="text-red-400">Delete</span>
				</Button>
				<div class="flex-1"></div>
				<Button variant="ghost" onclick={() => (showDetail = false)}>Cancel</Button>
				<Button onclick={saveDetail}>Save</Button>
			</DialogFooter>
		{/if}
	</DialogContent>
</Dialog>

<!-- Delete Confirm -->
<Dialog bind:open={showDelete}>
	<DialogContent>
		<DialogHeader><DialogTitle>Delete {deleteTarget?.type === 'folder' ? 'Folder' : 'Asset'}</DialogTitle></DialogHeader>
		<p class="text-sm text-neutral-400">
			Are you sure you want to delete <strong class="text-white">{deleteTarget?.name}</strong>?
			{#if deleteTarget?.type === 'folder'}
				Assets in this folder will be moved to the root.
			{:else}
				The file will be permanently deleted.
			{/if}
		</p>
		<DialogFooter>
			<Button variant="ghost" onclick={() => (showDelete = false)}>Cancel</Button>
			<Button variant="destructive" onclick={confirmDelete}>Delete</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
