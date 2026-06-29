<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
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
		failover,
	} from '$lib/stores/pipeline';
	import type { StingerConfig, BroadcastConfig, OutputConfig, OutputStats, AudioRouting, Asset } from '$lib/types';
	import StatusIndicator from '../../components/StatusIndicator.svelte';
	import VideoPreview from '../../components/VideoPreview.svelte';
	import { popout } from '$lib/popout';
	import PopoutButton from '../../components/PopoutButton.svelte';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { notify } from '$lib/notify';

	let stingers = $state<StingerConfig[]>([]);
	let assets = $state<Asset[]>([]);
	let programSourceId = $state<string | null>(null);
	let previewSourceId = $state<string | null>(null);
	let activeTab = $state<'sources' | 'scenes' | 'library'>('sources');
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
		try {
			await api.cutToSource(id);
			programSourceId = id;
		} catch (e) {
			notify.error(e);
		}
	}

	async function takeScene(id: string) {
		try {
			await api.activateScene(id);
			programSourceId = id;
			previewSourceId = null;
			notify.success('Scene is live');
		} catch (e) {
			notify.error(e);
		}
	}

	async function pushPreviewToLive() {
		if (!previewSourceId) {
			notify.error('No source queued in Next Up');
			return;
		}
		try {
			await api.cutToSource(previewSourceId);
			programSourceId = previewSourceId;
			previewSourceId = null;
		} catch (e) {
			notify.error(e);
		}
	}

	async function setAudioSource(sourceId: string | null) {
		try {
			await api.setAudioSource(sourceId);
			audioRouting.active_audio_source = sourceId;
			audioRouting.audio_follows_video = sourceId === null;
		} catch (e) {
			notify.error(e);
		}
	}

	async function toggleRecording() {
		try {
			if ($isRecording) {
				await api.stopRecording();
			} else {
				await api.startRecording();
			}
		} catch (e) {
			notify.error(e);
		}
	}

	async function saveConfig() {
		try {
			config = await api.setBroadcastConfig(config);
			configDirty = false;
			notify.success('Config saved');
		} catch (e) {
			notify.error(e);
		}
	}

	async function saveOutputConfig() {
		try {
			outputConfig = await api.setOutputConfig(outputConfig);
		} catch (e) {
			notify.error(e);
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

<div class="mx-auto max-w-[1400px]">
	{#if $failover.active}
		{@const fbName = $sources.find((s) => s.id === $failover.fallback_source_id)?.name ?? 'fallback'}
		{@const mainName = $sources.find((s) => s.id === $failover.intent_source_id)?.name ?? 'your source'}
		<div class="mb-3 flex items-center gap-3 rounded border border-warning bg-warning/10 px-4 py-2 text-warning" role="status">
			<span class="led text-[16px] animate-pulse">⚠ FAILOVER ACTIVE</span>
			<span class="text-xs">{mainName} is offline — broadcasting <strong>{fbName}</strong>. Will switch back automatically when it returns.</span>
		</div>
	{/if}

	<!-- Header -->
	<div class="mb-4 flex items-center justify-between border-b border-border pb-3">
		<h1 class="text-[13px] tracking-widest text-amber-bright">STUDIO</h1>
		<div class="flex items-center gap-3">
			<StatusIndicator state={$pipelineState} />
			{#if $isLive && $pipelineState.state === 'live'}
				<span class="led text-[18px]">{formatDuration(outputStats.duration_secs)}</span>
			{/if}
		</div>
	</div>

		<!-- ========== STUDIO ========== -->

		<!-- Master controls -->
		<section class="panel mb-4">
			<header class="panel__head">▮ CONTROLS</header>
			<div class="panel__body flex flex-wrap items-center gap-3">
				{#if $isLive}
					<button
						onclick={async () => { try { await api.stopStream(); } catch(e) { notify.error(e); }}}
						class="btn btn--danger"
					>
						■ End Stream
					</button>
					<button
						onclick={toggleRecording}
						class="btn {$isRecording ? 'btn--danger' : ''}"
					>
						{$isRecording ? '● Stop Rec' : '● Record'}
					</button>
					{#if $pipelineState.state === 'live'}
						<span class="pill pill--live ml-auto">● ON AIR — since {new Date($pipelineState.started_at).toLocaleTimeString()}</span>
					{/if}
				{:else}
					<button
						onclick={async () => {
							try {
								await api.startStream(programSourceId || undefined);
								startStatsPolling();
							} catch(e) {
								notify.error(e);
							}
						}}
						class="btn btn--go"
					>
						● Go Live
					</button>
					<span class="pill pill--idle ml-auto">○ OFF AIR</span>
				{/if}
			</div>
		</section>

		<!-- Preview + Program monitors -->
		<div class="mb-4 grid grid-cols-2 gap-4">
			<!-- Preview / Next Up (left) -->
			<section class="panel">
				<header class="panel__head">
					<span class="flex items-center gap-2">
						▮ PREVIEW
						<PopoutButton section="preview" width={640} height={420} />
					</span>
					{#if previewSourceId}
						<button onclick={pushPreviewToLive} class="btn btn--danger" style="min-height:24px;padding:2px 10px">
							▸ Push to Live
						</button>
					{/if}
				</header>
				{#if previewSourceId}
					{#key previewSourceId}
						<VideoPreview sourceId={previewSourceId} />
					{/key}
				{:else}
					<div class="scanlines-well flex aspect-video items-center justify-center border-t border-border">
						<span class="text-amber-muted">Select a source below to queue</span>
					</div>
				{/if}
			</section>

			<!-- Program / Live (right) -->
			<section class="panel">
				<header class="panel__head">
					<span class="flex items-center gap-2 {$isLive ? 'text-danger-glow' : ''}">
						▮ PROGRAM
						<PopoutButton section="program" width={960} height={600} />
					</span>
					{#if programSourceId}
						{@const progSrc = $sources.find((s) => s.id === programSourceId)}
						<span class="text-amber-dim normal-case tracking-normal">{progSrc?.name || ''}</span>
					{/if}
				</header>
				{#if programSourceId}
					{#key programSourceId}
						<VideoPreview sourceId={programSourceId} active={true} />
					{/key}
				{:else}
					<div class="scanlines-well flex aspect-video items-center justify-center border-t border-border">
						<span class="text-amber-muted">No source on live</span>
					</div>
				{/if}
			</section>
		</div>

		<!-- Config + Stats side by side under previews -->
		<div class="mb-4 grid grid-cols-2 gap-4">
			<!-- Config (under Program) -->
			<section class="panel">
				<header class="panel__head">▮ OUTPUT CONFIG</header>
				<div class="panel__body space-y-1">
					<div class="flex justify-between">
						<span class="text-amber-dim">Resolution</span>
						<span class="text-amber-bright">{outputConfig.width}x{outputConfig.height}@{outputConfig.fps}fps</span>
					</div>
					<div class="flex justify-between">
						<span class="text-amber-dim">Video bitrate</span>
						<span class="text-amber-bright">{outputConfig.video_bitrate_kbps} kbps</span>
					</div>
					{#if $destinations.length > 0}
						<div class="mt-2 border-t border-border-dim pt-2">
							<span class="label">Destinations</span>
							<div class="mt-1 space-y-1">
								{#each $destinations as dest (dest.id)}
									<button
										onclick={() => toggleDestination(dest.id)}
										class="flex cursor-pointer items-center gap-2"
									>
										<Checkbox
											checked={config.destination_ids.length === 0
												? dest.enabled
												: config.destination_ids.includes(dest.id)}
										/>
										<span class="text-amber">{dest.name}</span>
									</button>
								{/each}
							</div>
						</div>
					{:else}
						<div class="flex justify-between">
							<span class="text-amber-dim">Destinations</span>
							<a href="/destinations" class="text-amber hover:text-amber-bright">Add one ▸</a>
						</div>
					{/if}
					<div class="flex justify-between">
						<span class="text-amber-dim">Audio</span>
						<span class="text-amber">
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
							<span class="text-amber-dim">Encoder</span>
							<span class="ml-2 truncate text-amber-dim">{outputStats.source_encoder}</span>
						</div>
					{/if}
					{#if configDirty}
						<button onclick={saveConfig} class="btn mt-2 w-full">Save Config</button>
					{/if}
				</div>
			</section>

			<!-- Stats (under Next Up) -->
			<section class="panel">
				<header class="panel__head">▮ STREAM STATS</header>
				<div class="panel__body space-y-1">
					<div class="flex items-baseline justify-between">
						<span class="text-amber-dim">Duration</span>
						<span class="led text-[18px]">{formatDuration(outputStats.duration_secs)}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-amber-dim">Data sent</span>
						<span class="text-amber-bright">{formatBytes(outputStats.bytes_sent)}</span>
					</div>
					<div class="flex justify-between">
						<span class="text-amber-dim">Source</span>
						<span class="text-amber-bright">
							{outputStats.source_width || '?'}x{outputStats.source_height || '?'}
							{#if outputStats.source_fps}@{outputStats.source_fps.toFixed(0)}fps{/if}
						</span>
					</div>
					<div class="flex items-baseline justify-between">
						<span class="text-amber-dim">Source bitrate</span>
						<span class="led text-[18px]">{outputStats.source_bitrate_kbps.toFixed(0)} kbps</span>
					</div>
					<div class="flex items-baseline justify-between">
						<span class="text-amber-dim">Output bitrate</span>
						<span class="led text-[18px]">{outputStats.output_bitrate_kbps} kbps</span>
					</div>
				</div>
			</section>
		</div>

		<!-- Audio Mixer -->
		{#if liveSources().length > 0}
			<section class="panel mb-4">
				<header class="panel__head">
					<span class="flex items-center gap-2">
						▮ AUDIO
						<PopoutButton section="audio" width={400} height={500} />
					</span>
					<button
						onclick={async () => {
							try {
								await api.toggleAudioFollowsVideo();
								audioRouting.audio_follows_video = !audioRouting.audio_follows_video;
								if (audioRouting.audio_follows_video) audioRouting.active_audio_source = null;
							} catch (e) { notify.error(e); }
						}}
						class="btn {audioRouting.audio_follows_video ? 'btn--go' : ''}"
						style="min-height:24px;padding:2px 8px"
					>
						{audioRouting.audio_follows_video ? 'Follows Video' : 'Independent'}
					</button>
				</header>
				<div class="panel__body flex gap-3">
					{#each liveSources() as source (source.id)}
						{@const isAudioSource = audioRouting.audio_follows_video
							? source.id === programSourceId
							: source.id === audioRouting.active_audio_source}
						<button
							onclick={() => setAudioSource(source.id)}
							disabled={audioRouting.audio_follows_video}
							class="row flex-1 text-left {isAudioSource ? 'border-live' : ''} disabled:cursor-default"
						>
							<div class="scanlines-well flex h-5 w-12 items-end gap-px border border-border-dim p-px">
								{#each Array(8) as _, i}
									<div
										class="w-1 {isAudioSource ? (i < 6 ? 'bg-live' : i < 7 ? 'bg-warning' : 'bg-danger') : 'bg-border-dim'}"
										style="height: {isAudioSource ? Math.max(20, Math.random() * 100) : 12}%"
									></div>
								{/each}
							</div>
							<div class="min-w-0 flex-1">
								<div class="truncate {isAudioSource ? 'text-amber-bright' : 'text-amber-dim'}">
									{source.name}
								</div>
							</div>
							{#if isAudioSource}
								<span class="pill pill--live shrink-0">● ACTIVE</span>
							{/if}
						</button>
					{/each}
				</div>
			</section>
		{/if}

		<!-- Tabbed section: Sources / Library -->
		<section class="panel">
			<div class="flex items-center border-b border-border-dim bg-panel-raised">
				{#each [
					{ id: 'sources', label: 'Sources' },
					{ id: 'scenes', label: 'Scenes' },
					{ id: 'library', label: 'Library' },
				] as tab}
					<button
						onclick={() => (activeTab = tab.id as typeof activeTab)}
						class="px-5 py-2.5 text-[11px] uppercase tracking-[1px] transition-colors {activeTab === tab.id
							? 'border-b-2 border-amber text-amber-bright'
							: 'border-b-2 border-transparent text-amber-dim hover:text-amber'}"
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
						<p class="text-amber-muted">No live sources. Connect OBS to start.</p>
					{:else}
						<div class="grid gap-3" style="grid-template-columns: repeat({Math.min(liveStreamSources().length, 3)}, minmax(0, 1fr))">
							{#each liveStreamSources() as source (source.id)}
								<div
									class="rounded-sm border p-2 transition-colors {source.id === programSourceId
										? 'border-danger bg-panel-raised'
										: source.id === previewSourceId
											? 'border-live bg-panel-raised'
											: 'border-border-dim hover:border-border'}"
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
											class="btn flex-1 {source.id === previewSourceId ? 'btn--go' : ''}"
										>
											Next Up
										</button>
										<button
											onclick={() => cutToSource(source.id)}
											disabled={source.id === programSourceId}
											class="btn btn--danger flex-1"
										>
											Switch
										</button>
									</div>
								</div>
							{/each}
						</div>
					{/if}

				{:else if activeTab === 'scenes'}
					{#if $scenes.length === 0}
						<p class="text-amber-muted">No scenes yet. <a href="/scenes" class="text-amber hover:text-amber-bright">Build one ▸</a></p>
					{:else}
						<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
							{#each $scenes as scene (scene.id)}
								<div class="rounded-sm border p-3 {scene.id === programSourceId ? 'border-danger bg-panel-raised' : 'border-border-dim hover:border-border'}">
									<div class="mb-2 flex items-center justify-between gap-2">
										<span class="truncate text-amber">{scene.name}</span>
										<span class="pill {scene.id === programSourceId ? 'pill--live' : 'pill--idle'} text-[9px] shrink-0">
											{scene.id === programSourceId ? '● LIVE' : `${scene.layers.length} layer${scene.layers.length !== 1 ? 's' : ''}`}
										</span>
									</div>
									<button
										onclick={() => takeScene(scene.id)}
										disabled={scene.id === programSourceId}
										class="btn btn--danger w-full"
									>
										Take to Program
									</button>
								</div>
							{/each}
						</div>
						<p class="mt-3 text-[11px] text-amber-muted"><a href="/scenes" class="text-amber hover:text-amber-bright">Edit scenes ▸</a></p>
					{/if}

				{:else if activeTab === 'library'}
					{#if assets.length === 0 && stingers.length === 0}
						<p class="text-amber-muted">No items in library. <a href="/library" class="text-amber hover:text-amber-bright">Add one ▸</a></p>
					{:else}
						{#if assets.length > 0}
							<div class="mb-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
								{#each assets as asset (asset.id)}
									<div class="row">
										<div class="min-w-0 flex-1">
											<div class="truncate text-amber">{asset.name}</div>
											<div class="label">{asset.asset_type}</div>
										</div>
										<div class="ml-2 flex gap-1">
											<button
												onclick={async () => {
													const source = await api.createSourceFromAsset(asset.id);
													sources.set(await api.listSources());
													previewSourceId = source.id;
													await api.setPreview(source.id).catch(() => {});
												}}
												class="btn"
											>
												Preview
											</button>
											<button
												onclick={async () => {
													const source = await api.createSourceFromAsset(asset.id);
													sources.set(await api.listSources());
													await cutToSource(source.id);
												}}
												class="btn btn--danger"
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
			</section>
</div>
