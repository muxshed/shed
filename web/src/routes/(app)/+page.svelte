<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api';
	import {
		pipelineState,
		sources,
		destinations,
		scenes,
		isLive,
		isRecording,
		recordingState,
	} from '$lib/stores/pipeline';
	import type { StingerConfig, BroadcastConfig, OutputConfig, OutputStats, AudioRouting, Asset } from '$lib/types';
	import StatusIndicator from '../../components/StatusIndicator.svelte';
	import VideoPreview from '../../components/VideoPreview.svelte';
	import { popout } from '$lib/popout';
	import PopoutButton from '../../components/PopoutButton.svelte';
	import { Checkbox } from '$lib/components/ui/checkbox';

	let error = $state('');
	let stingers = $state<StingerConfig[]>([]);
	let assets = $state<Asset[]>([]);
	let programSourceId = $state<string | null>(null);
	let previewSourceId = $state<string | null>(null);
	let activeTab = $state<'sources' | 'library'>('sources');
	let audioRouting = $state<AudioRouting>({
		active_audio_source: null,
		channels: [],
		audio_follows_video: true,
	});

	// Output
	let outputConfig = $state<OutputConfig>({
		video_bitrate_kbps: 4500,
		audio_bitrate_kbps: 160,
		width: 1920,
		height: 1080,
		fps: 30,
	});
	let outputStats = $state<OutputStats>({
		bytes_sent: 0,
		duration_secs: 0,
		source_bitrate_kbps: 0,
		output_bitrate_kbps: 0,
		dropped_frames: 0,
	});

	// Broadcast config
	let config = $state<BroadcastConfig>({
		source_id: null,
		scene_id: null,
		start_stinger_id: null,
		destination_ids: [],
		enable_delay: false,
		delay_ms: 7000,
		auto_record: false,
	});
	let configDirty = $state(false);
	let statsInterval: ReturnType<typeof setInterval> | null = null;

	onMount(async () => {
		const [srcList, destList, sceneList, stingerList, assetList, status, recStatus, savedConfig, outConfig] =
			await Promise.all([
				api.listSources(),
				api.listDestinations(),
				api.listScenes(),
				api.listLibrary().catch(() => []),
				api.listAssets().catch(() => []),
				api.getStatus(),
				api.recordingStatus().catch(() => ({ recording: false })),
				api.getBroadcastConfig().catch(() => null),
				api.getOutputConfig().catch(() => null),
			]);
		sources.set(srcList);
		destinations.set(destList);
		scenes.set(sceneList);
		stingers = stingerList;
		assets = assetList;
		pipelineState.set(status.pipeline);
		recordingState.set(recStatus);
		if (savedConfig) config = savedConfig;
		if (outConfig) outputConfig = outConfig;

		audioRouting = await api.getAudioRouting().catch(() => audioRouting);
		if (status.pipeline.state === 'live') {
			const prog = await api.getProgram().catch(() => null);
			if (prog) {
				programSourceId = prog.program_source_id;
				previewSourceId = prog.preview_source_id;
			}
			startStatsPolling();
		}
	});

	let studioChannel: BroadcastChannel;

	onMount(() => {
		studioChannel = new BroadcastChannel('muxshed-studio');
		studioChannel.onmessage = (e) => {
			if (e.data.type === 'cut_source') cutToSource(e.data.sourceId);
			if (e.data.type === 'set_preview') previewSourceId = e.data.sourceId;
			if (e.data.type === 'audio_routing') audioRouting = e.data.routing;
			if (e.data.type === 'request_state') broadcastState();
		};
		return () => studioChannel?.close();
	});

	function broadcastState() {
		studioChannel?.postMessage({ type: 'program_source', sourceId: programSourceId });
		studioChannel?.postMessage({ type: 'preview_source', sourceId: previewSourceId });
		studioChannel?.postMessage({ type: 'audio_routing', routing: audioRouting });
	}

	// Sync popout windows when state changes
	$effect(() => {
		studioChannel?.postMessage({ type: 'program_source', sourceId: programSourceId });
	});
	$effect(() => {
		studioChannel?.postMessage({ type: 'preview_source', sourceId: previewSourceId });
	});

	onDestroy(() => {
		if (statsInterval) clearInterval(statsInterval);
		studioChannel?.close();
	});

	function startStatsPolling() {
		if (statsInterval) clearInterval(statsInterval);
		statsInterval = setInterval(async () => {
			outputStats = await api.getOutputStats().catch(() => outputStats);
		}, 2000);
	}

	function stopStatsPolling() {
		if (statsInterval) {
			clearInterval(statsInterval);
			statsInterval = null;
		}
	}

	function liveSources() {
		return $sources.filter((s) => s.state === 'live');
	}

	function liveStreamSources() {
		return $sources.filter((s) => s.state === 'live' && s.kind.type !== 'media_file');
	}

	async function cutToSource(id: string) {
		error = '';
		try {
			await api.cutToSource(id);
			programSourceId = id;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Switch failed';
		}
	}

	async function pushPreviewToLive() {
		if (!previewSourceId) {
			error = 'No source queued in Next Up';
			return;
		}
		error = '';
		try {
			await api.cutToSource(previewSourceId);
			programSourceId = previewSourceId;
			previewSourceId = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Push to live failed';
		}
	}

	async function setAudioSource(sourceId: string | null) {
		error = '';
		try {
			await api.setAudioSource(sourceId);
			audioRouting.active_audio_source = sourceId;
			audioRouting.audio_follows_video = sourceId === null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Audio routing failed';
		}
	}

	async function toggleRecording() {
		error = '';
		try {
			if ($isRecording) {
				await api.stopRecording();
			} else {
				await api.startRecording();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Recording error';
		}
	}

	async function bleep() {
		try { await api.triggerBleep(); } catch (e) {
			error = e instanceof Error ? e.message : 'Bleep failed';
		}
	}

	async function saveConfig() {
		try {
			config = await api.setBroadcastConfig(config);
			configDirty = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save';
		}
	}

	async function saveOutputConfig() {
		try {
			outputConfig = await api.setOutputConfig(outputConfig);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save output config';
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
		return `${(bytes / 1073741824).toFixed(2)} GB`;
	}

	function formatDuration(secs: number): string {
		const h = Math.floor(secs / 3600);
		const m = Math.floor((secs % 3600) / 60);
		const s = Math.floor(secs % 60);
		return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
	}

	function toggleDestination(id: string) {
		if (config.destination_ids.includes(id)) {
			config.destination_ids = config.destination_ids.filter((d) => d !== id);
		} else {
			config.destination_ids = [...config.destination_ids, id];
		}
		configDirty = true;
	}

	// After go live succeeds, start polling stats
	$effect(() => {
		if ($isLive) {
			startStatsPolling();
			api.getProgram().then((p) => {
				if (p) {
					programSourceId = p.program_source_id;
					previewSourceId = p.preview_source_id;
				}
			}).catch(() => {});
		} else {
			stopStatsPolling();
		}
	});
</script>

<div class="mx-auto max-w-7xl">
	<!-- Header -->
	<div class="mb-4 flex items-center justify-between">
		<h1 class="text-xl font-bold">Studio</h1>
		<div class="flex items-center gap-3">
			<StatusIndicator state={$pipelineState} />
			{#if $isLive && $pipelineState.state === 'live'}
				<span class="text-xs text-neutral-500">
					{formatDuration(outputStats.duration_secs)}
				</span>
			{/if}
		</div>
	</div>

		<!-- ========== STUDIO ========== -->

		<!-- Studio top bar -->
		<div class="mb-4 flex items-center gap-3">
			{#if $isLive}
				<button
					onclick={async () => { error = ''; try { await api.stopStream(); } catch(e) { error = e instanceof Error ? e.message : 'Failed'; }}}
					class="rounded-lg bg-red-700 px-5 py-2 text-sm font-bold text-white hover:bg-red-600"
				>
					End Stream
				</button>
				<button
					onclick={toggleRecording}
					class="rounded px-3 py-2 text-xs font-bold {$isRecording
						? 'bg-red-800 text-red-200'
						: 'bg-neutral-700 text-neutral-300 hover:bg-neutral-600'}"
				>
					{$isRecording ? 'Stop Rec' : 'Record'}
				</button>
				<button
					onclick={bleep}
					class="rounded bg-yellow-700 px-3 py-2 text-xs font-bold text-white hover:bg-yellow-600"
				>
					BLEEP
				</button>
			{:else}
				<button
					onclick={async () => {
						error = '';
						try {
							await api.startStream(programSourceId || undefined);
							startStatsPolling();
						} catch(e) {
							error = e instanceof Error ? e.message : 'Failed to go live';
						}
					}}
					class="rounded-lg bg-green-700 px-5 py-2 text-sm font-bold text-white hover:bg-green-600"
				>
					Go Live
				</button>
			{/if}
		</div>

		{#if $isLive && $pipelineState.state === 'live'}
			<div class="mb-4 rounded-lg border border-red-900 bg-red-950 px-4 py-2 text-center">
				<span class="text-sm text-red-300">
					Live since {new Date($pipelineState.started_at).toLocaleTimeString()}
				</span>
			</div>
		{/if}

		<!-- Next Up + Live side by side -->
		<div class="mb-4 grid grid-cols-2 gap-4">
			<!-- Next Up (left) -->
			<div>
				{#if previewSourceId}
					<div class="mb-1 flex items-center justify-between">
						<div class="flex items-center gap-1">
							<h2 class="text-xs font-semibold uppercase tracking-wide text-green-400">Next Up</h2>
							<PopoutButton section="preview" width={640} height={420} />
						</div>
						<button
							onclick={pushPreviewToLive}
							class="rounded bg-red-700 px-4 py-1 text-xs font-bold text-white hover:bg-red-600"
						>
							Push to Live
						</button>
					</div>
					{#key previewSourceId}
						<VideoPreview sourceId={previewSourceId} />
					{/key}
				{:else}
					<div class="mb-1">
						<h2 class="text-xs font-semibold uppercase tracking-wide text-neutral-500">Next Up</h2>
					</div>
					<div class="flex aspect-video items-center justify-center rounded-lg border-2 border-dashed border-neutral-700 bg-neutral-950">
						<span class="text-sm text-neutral-600">Select a source below to queue</span>
					</div>
				{/if}
			</div>

			<!-- Live (right) -->
			<div>
				<div class="mb-1 flex items-center justify-between">
					<div class="flex items-center gap-1">
						<h2 class="text-xs font-semibold uppercase tracking-wide {$isLive ? 'text-red-400' : 'text-neutral-400'}">
							Live
						</h2>
						<PopoutButton section="program" width={960} height={600} />
					</div>
					{#if programSourceId}
						{@const progSrc = $sources.find((s) => s.id === programSourceId)}
						<span class="text-xs text-neutral-500">{progSrc?.name || ''}</span>
					{/if}
				</div>
				{#if programSourceId}
					{#key programSourceId}
						<VideoPreview sourceId={programSourceId} active={true} />
					{/key}
				{:else}
					<div class="flex aspect-video items-center justify-center rounded-lg border-2 border-red-900 bg-black">
						<span class="text-sm text-neutral-600">No source on live</span>
					</div>
				{/if}
			</div>
		</div>

		<!-- Config + Stats side by side under previews -->
		<div class="mb-4 grid grid-cols-2 gap-4">
			<!-- Config (under Program) -->
			<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-3">
				<h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">Output Config</h3>
				<div class="space-y-1 text-xs">
					<div class="flex justify-between">
						<span class="text-neutral-500">Resolution</span>
						<span class="font-mono text-white">{outputConfig.width}x{outputConfig.height}@{outputConfig.fps}fps</span>
					</div>
					<div class="flex justify-between">
						<span class="text-neutral-500">Video bitrate</span>
						<span class="font-mono text-white">{outputConfig.video_bitrate_kbps} kbps</span>
					</div>
					{#if $destinations.length > 0}
						<div class="border-t border-neutral-700 pt-1 mt-1">
							<span class="text-neutral-500">Destinations</span>
							<div class="mt-1 space-y-1">
								{#each $destinations as dest (dest.id)}
									<button
										onclick={() => toggleDestination(dest.id)}
										class="flex items-center gap-2 cursor-pointer"
									>
										<Checkbox
											checked={config.destination_ids.length === 0
												? dest.enabled
												: config.destination_ids.includes(dest.id)}
										/>
										<span class="text-white text-xs">{dest.name}</span>
									</button>
								{/each}
							</div>
						</div>
					{:else}
						<div class="flex justify-between">
							<span class="text-neutral-500">Destinations</span>
							<a href="/destinations" class="text-blue-400 hover:underline">Add one</a>
						</div>
					{/if}
					<div class="flex justify-between">
						<span class="text-neutral-500">Audio</span>
						<span class="text-white">
							{#if audioRouting.audio_follows_video}
								Follows video
							{:else if audioRouting.active_audio_source}
								{@const audioSrc = $sources.find((s) => s.id === audioRouting.active_audio_source)}
								{audioSrc?.name || 'Unknown'}
							{:else}
								None
							{/if}
						</span>
					</div>
					{#if outputStats.source_encoder}
						<div class="flex justify-between">
							<span class="text-neutral-500">Encoder</span>
							<span class="truncate text-neutral-400 ml-2">{outputStats.source_encoder}</span>
						</div>
					{/if}
					{#if configDirty}
						<button
							onclick={saveConfig}
							class="mt-2 w-full rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700"
						>
							Save Config
						</button>
					{/if}
				</div>
			</div>

			<!-- Stats (under Next Up) -->
			<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-3">
				<h3 class="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">Stream Stats</h3>
				<div class="space-y-1 text-xs">
					<div class="flex justify-between">
						<span class="text-neutral-500">Duration</span>
						<span class="font-mono text-white">{formatDuration(outputStats.duration_secs)}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-neutral-500">Data sent</span>
						<span class="font-mono text-white">{formatBytes(outputStats.bytes_sent)}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-neutral-500">Source</span>
						<span class="font-mono text-white">
							{outputStats.source_width || '?'}x{outputStats.source_height || '?'}
							{#if outputStats.source_fps}@{outputStats.source_fps.toFixed(0)}fps{/if}
						</span>
					</div>
					<div class="flex justify-between">
						<span class="text-neutral-500">Source bitrate</span>
						<span class="font-mono text-white">{outputStats.source_bitrate_kbps.toFixed(0)} kbps</span>
					</div>
					<div class="flex justify-between">
						<span class="text-neutral-500">Output bitrate</span>
						<span class="font-mono text-white">{outputStats.output_bitrate_kbps} kbps</span>
					</div>
				</div>
			</div>
		</div>

		<!-- Audio Mixer -->
		{#if liveSources().length > 0}
			<div class="mb-4 rounded-lg border border-neutral-700 bg-neutral-900 p-3">
				<div class="mb-2 flex items-center justify-between">
					<div class="flex items-center gap-1">
						<h3 class="text-xs font-semibold uppercase tracking-wide text-neutral-400">Audio</h3>
						<PopoutButton section="audio" width={400} height={500} />
					</div>
					<button
						onclick={async () => {
							error = '';
							try {
								await api.toggleAudioFollowsVideo();
								audioRouting.audio_follows_video = !audioRouting.audio_follows_video;
								if (audioRouting.audio_follows_video) audioRouting.active_audio_source = null;
							} catch (e) { error = e instanceof Error ? e.message : 'Toggle failed'; }
						}}
						class="rounded px-2 py-0.5 text-xs {audioRouting.audio_follows_video
							? 'bg-green-900 text-green-400'
							: 'bg-neutral-700 text-neutral-400'}"
					>
						{audioRouting.audio_follows_video ? 'Follows video' : 'Independent'}
					</button>
				</div>
				<div class="flex gap-3">
					{#each liveSources() as source (source.id)}
						{@const isAudioSource = audioRouting.audio_follows_video
							? source.id === programSourceId
							: source.id === audioRouting.active_audio_source}
						<button
							onclick={() => setAudioSource(source.id)}
							disabled={audioRouting.audio_follows_video}
							class="flex flex-1 items-center gap-2 rounded px-3 py-2 text-left transition-all {isAudioSource
								? 'bg-green-950 ring-1 ring-green-600'
								: 'bg-neutral-800'} disabled:cursor-default"
						>
							<div class="flex h-5 w-12 items-end gap-px">
								{#each Array(8) as _, i}
									<div
										class="w-1 rounded-sm {isAudioSource ? (i < 6 ? 'bg-green-500' : i < 7 ? 'bg-yellow-500' : 'bg-red-500') : 'bg-neutral-700'}"
										style="height: {isAudioSource ? Math.max(20, Math.random() * 100) : 20}%"
									></div>
								{/each}
							</div>
							<div class="min-w-0 flex-1">
								<div class="truncate text-xs font-medium {isAudioSource ? 'text-white' : 'text-neutral-400'}">
									{source.name}
								</div>
							</div>
							{#if isAudioSource}
								<span class="shrink-0 rounded bg-green-900 px-1.5 py-0.5 text-xs text-green-400">ACTIVE</span>
							{/if}
						</button>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Tabbed section: Sources / Scenes / Library -->
		<div class="rounded-lg border border-neutral-700 bg-neutral-900">
			<div class="flex items-center border-b border-neutral-700">
				{#each [
					{ id: 'sources', label: 'Sources' },
					{ id: 'library', label: 'Library' },
				] as tab}
					<button
						onclick={() => (activeTab = tab.id as typeof activeTab)}
						class="px-5 py-2.5 text-sm font-medium transition-colors {activeTab === tab.id
							? 'border-b-2 border-blue-500 text-white'
							: 'text-neutral-400 hover:text-neutral-200'}"
					>
						{tab.label}
					</button>
				{/each}
				{#if activeTab === 'sources'}
					<div class="ml-auto pr-2"><PopoutButton section="sources" width={900} height={600} /></div>
				{/if}
			</div>

			<div class="p-4">
				{#if activeTab === 'sources'}
					{#if liveStreamSources().length === 0}
						<p class="text-sm text-neutral-500">No live sources. Connect OBS to start.</p>
					{:else}
						<div class="grid gap-3 {liveStreamSources().length <= 3 ? `grid-cols-${liveStreamSources().length}` : 'grid-cols-3'}">
							{#each liveStreamSources() as source (source.id)}
								<div
									class="cursor-pointer rounded-lg border-2 p-2 transition-all {source.id === programSourceId
										? 'border-red-500 bg-red-950/30'
										: source.id === previewSourceId
											? 'border-green-500 bg-green-950/30'
											: 'border-neutral-700 hover:border-neutral-500'}"
								>
									<VideoPreview
										sourceId={source.id}
										label={source.name}
										active={source.id === programSourceId}
									/>
									<div class="mt-2 flex gap-2">
										<button
											onclick={() => { previewSourceId = source.id; }}
											disabled={source.id === programSourceId}
											class="flex-1 rounded px-2 py-1 text-xs font-medium {source.id === previewSourceId
												? 'bg-green-700 text-white'
												: 'bg-neutral-700 text-neutral-300 hover:bg-neutral-600'} disabled:opacity-30"
										>
											Next Up
										</button>
										<button
											onclick={() => cutToSource(source.id)}
											disabled={source.id === programSourceId}
											class="flex-1 rounded px-2 py-1 text-xs font-bold bg-neutral-700 text-neutral-300 hover:bg-red-700 hover:text-white disabled:opacity-30"
										>
											Switch
										</button>
									</div>
								</div>
							{/each}
						</div>
					{/if}

				{:else if activeTab === 'library'}
					{#if assets.length === 0 && stingers.length === 0}
						<p class="text-sm text-neutral-500">No items in library. <a href="/library" class="text-blue-400 hover:underline">Add one</a></p>
					{:else}
						{#if assets.length > 0}
							<div class="mb-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
								{#each assets as asset (asset.id)}
									<div class="flex items-center justify-between rounded-lg bg-neutral-800 p-3">
										<div class="min-w-0 flex-1">
											<div class="truncate text-sm font-medium text-white">{asset.name}</div>
											<div class="text-xs text-neutral-500">{asset.asset_type}</div>
										</div>
										<div class="ml-2 flex gap-1">
											<button
												onclick={async () => {
													const source = await api.createSourceFromAsset(asset.id);
													sources.set(await api.listSources());
													previewSourceId = source.id;
													await api.setPreview(source.id).catch(() => {});
												}}
												class="rounded bg-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-neutral-600"
											>
												Preview
											</button>
											<button
												onclick={async () => {
													const source = await api.createSourceFromAsset(asset.id);
													sources.set(await api.listSources());
													await cutToSource(source.id);
												}}
												class="rounded bg-neutral-700 px-2 py-1 text-xs text-neutral-300 hover:bg-red-700 hover:text-white"
											>
												Switch
											</button>
										</div>
									</div>
								{/each}
							</div>
						{/if}
					{/if}

				{/if}
				</div>
			</div>


	{#if error}
		<p class="mt-4 text-center text-sm text-red-400">{error}</p>
	{/if}
</div>
