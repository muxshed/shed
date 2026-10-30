<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api, type BroadcastAutomationStatus } from '$lib/api';
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
	import Dialog from '$lib/components/ui/dialog/Dialog.svelte';
	import { notify } from '$lib/notify';

	// Output config popup
	let showOutput = $state(false);
	let draft = $state<OutputConfig>({
		video_bitrate_kbps: 4500,
		audio_bitrate_kbps: 160,
		width: 1920,
		height: 1080,
		fps: 30,
	});
	let resSelect = $state('1080p');
	const RES_PRESETS = [
		{ label: '2160p', w: 3840, h: 2160 },
		{ label: '1440p', w: 2560, h: 1440 },
		{ label: '1080p', w: 1920, h: 1080 },
		{ label: '720p', w: 1280, h: 720 },
		{ label: '480p', w: 854, h: 480 },
	];

	let stingers = $state<StingerConfig[]>([]);
	let assets = $state<Asset[]>([]);
	let selectedAdId = $state('');
	let adBusy = $state(false);
	let adState = $state<{ playing: boolean; asset_id: string | null; source_id: string | null; return_to: string | null }>({ playing: false, asset_id: null, source_id: null, return_to: null });
	let adPoll: ReturnType<typeof setInterval> | null = null;
	let commercialAssets = $derived(assets.filter((asset) => asset.asset_type === 'video'));
	// Persistent server-side automatic advertising. Initial defaults are not yet saved.
	let autoAdsEnabled = $state(true);
	let autoAdsInterval = $state(30);
	let autoAdsPlaylistIds = $state<string[]>([]);
	function setPlaylistChoice(assetId: string, checked: boolean) {
		if (checked) {
			if (!autoAdsPlaylistIds.includes(assetId) && autoAdsPlaylistIds.length < 20) {
				autoAdsPlaylistIds = [...autoAdsPlaylistIds, assetId];
			}
		} else {
			autoAdsPlaylistIds = autoAdsPlaylistIds.filter((id) => id !== assetId);
		}
	}
	function movePlaylistChoice(id: string, offset: number) {
		const index = autoAdsPlaylistIds.indexOf(id);
		const next = index + offset;
		if (index < 0 || next < 0 || next >= autoAdsPlaylistIds.length) return;
		const reordered = [...autoAdsPlaylistIds];
		[reordered[index], reordered[next]] = [reordered[next], reordered[index]];
		autoAdsPlaylistIds = reordered;
	}
	let autoAdsSaving = $state(false);
	let autoAdsStatus = $state<{ enabled: boolean; interval_minutes: number; asset_id: string | null; playlist_ids: string[]; next_asset_id: string | null; broadcasting: boolean; next_ad_in_seconds: number | null; skip_next: boolean } | null>(null);
	let autoAdsLoaded = $state(false);
	let autoAdsTimer: ReturnType<typeof setInterval> | null = null;
	function formatNextAd(seconds: number | null | undefined): string {
		if (seconds == null) return 'Waiting for broadcast';
		return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
	}
	let nextAdCountdown = $derived(formatNextAd(autoAdsStatus?.next_ad_in_seconds));


	// Browser Program video is the unfiltered source feed. The graphics below
	// are an on-screen layout simulation, driven by the server's live timing.
	// YouTube remains the authoritative view of encoded outgoing video.
	let broadcastGraphics = $state<BroadcastAutomationStatus | null>(null);
	let simulateWeatherCard = $state(false);
	let showProgramGraphics = $state(true);
	let graphicsPoll: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		let active = true;
		const refreshGraphics = async () => {
			const result = await api.getAutomation().catch(() => null);
			if (active && result) broadcastGraphics = result;
		};
		void refreshGraphics();
		graphicsPoll = setInterval(refreshGraphics, 3000);
		return () => {
			active = false;
			if (graphicsPoll) clearInterval(graphicsPoll);
		};
	});

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
		if (!selectedAdId) selectedAdId = assetList.find((asset) => asset.asset_type === 'video')?.id || '';
		adState = await api.getAdStatus().catch(() => adState);
		const schedule = await api.getAutomaticAds().catch(() => null);
		if (schedule) {
			autoAdsStatus = schedule;
			autoAdsEnabled = schedule.enabled;
			autoAdsInterval = schedule.interval_minutes;
			autoAdsPlaylistIds = schedule.playlist_ids?.length ? schedule.playlist_ids : (schedule.asset_id ? [schedule.asset_id] : []);
			autoAdsLoaded = true;
		}

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
		if (adPoll) clearInterval(adPoll);
		if (autoAdsTimer) clearInterval(autoAdsTimer);
		studioChannel?.close();
	});

	onMount(() => {
		autoAdsTimer = setInterval(async () => {
			const schedule = await api.getAutomaticAds().catch(() => null);
			if (schedule) {
				autoAdsStatus = schedule;
				if (!autoAdsLoaded) {
					autoAdsEnabled = schedule.enabled;
					autoAdsInterval = schedule.interval_minutes;
							autoAdsPlaylistIds = schedule.playlist_ids?.length ? schedule.playlist_ids : (schedule.asset_id ? [schedule.asset_id] : []);
					autoAdsLoaded = true;
				}
			}
		}, 3000);
	});

	async function saveAutomaticAdSettings() {
		if (autoAdsSaving) return;
		if (autoAdsEnabled && autoAdsPlaylistIds.length === 0) {
			notify.error('Choose an MP4 advertisement before enabling automatic ads');
			return;
		}
		autoAdsSaving = true;
		try {
			const status = await api.saveAutomaticAds({
				enabled: autoAdsEnabled,
				interval_minutes: Number(autoAdsInterval),
				asset_id: autoAdsPlaylistIds[0] || null,
				playlist_ids: autoAdsPlaylistIds,
			});
			autoAdsStatus = status;
			notify.success('Automatic commercial schedule saved');
		} catch (e) { notify.error(e); }
		finally { autoAdsSaving = false; }
	}

	async function skipNextAutomaticAd() {
		try {
			autoAdsStatus = await api.skipNextAutomaticAd();
			notify.success('Next scheduled advertisement will be skipped');
		} catch (e) { notify.error(e); }
	}

	onMount(() => {
		adPoll = setInterval(async () => {
			const next = await api.getAdStatus().catch(() => null);
			if (next) {
				const changed = adState.playing !== next.playing;
				adState = next;
				if (next.playing || changed) {
					const p = await api.getProgram().catch(() => null);
					if (p) programSourceId = p.program_source_id;
				}
			}
		}, 1500);
	});

	async function playCommercial() {
		if (!selectedAdId || adBusy || adState.playing) return;
		adBusy = true;
		try {
			adState = await api.playAd(selectedAdId);
			const p = await api.getProgram().catch(() => null);
			if (p) programSourceId = p.program_source_id;
			notify.success('Commercial playing — automatic return enabled');
		} catch (e) { notify.error(e); }
		finally { adBusy = false; }
	}

	async function stopCommercial() {
		if (adBusy) return;
		adBusy = true;
		try {
			await api.stopAd();
			adState = await api.getAdStatus();
			const p = await api.getProgram().catch(() => null);
			if (p) programSourceId = p.program_source_id;
			notify.success('Commercial stopped');
		} catch (e) { notify.error(e); }
		finally { adBusy = false; }
	}

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

	function openOutput() {
		draft = { ...outputConfig };
		const m = RES_PRESETS.find((p) => p.w === draft.width && p.h === draft.height);
		resSelect = m ? m.label : 'custom';
		showOutput = true;
	}

	function onResChange() {
		const p = RES_PRESETS.find((x) => x.label === resSelect);
		if (p) {
			draft.width = p.w;
			draft.height = p.h;
		}
	}

	async function saveOutput() {
		try {
			outputConfig = await api.setOutputConfig(draft);
			showOutput = false;
			notify.success($isLive ? 'Output config applied' : 'Output config saved');
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
		<h1 class="text-sm font-semibold text-amber-bright">Studio <span class="ml-2 text-xs font-normal text-amber-muted">OBS workspace</span></h1>
		<div class="flex items-center gap-3">
			<StatusIndicator state={$pipelineState} />
			{#if $isLive && $pipelineState.state === 'live'}
				<span class="led text-[18px]">{formatDuration(outputStats.duration_secs)}</span>
			{/if}
		</div>
	</div>

		<!-- ========== STUDIO ========== -->

		<!-- Preview + Program monitors -->
		<div class="obs-monitor-grid">
			<!-- Preview / Next Up (left) -->
			<section class="panel">
				<header class="panel__head">
					<span class="flex items-center gap-2">
						PREVIEW / NEXT
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
						PROGRAM / LIVE
						<PopoutButton section="program" width={960} height={600} />
					</span>
					{#if programSourceId}
						{@const progSrc = $sources.find((s) => s.id === programSourceId)}
						<span class="text-amber-dim normal-case tracking-normal">{progSrc?.name || ''}</span>
					{/if}
				</header>
					{#if programSourceId}
						<div class="relative" aria-label="Program camera and simulated graphics layout">
							{#key programSourceId}
								<VideoPreview sourceId={programSourceId} active={true} />
							{/key}
							{#if showProgramGraphics && $isLive && broadcastGraphics?.config.broadcast_overlay_enabled}
								<div class="pointer-events-none absolute inset-0 z-10 overflow-hidden rounded-md" aria-hidden="true">
									{#if broadcastGraphics.config.weather_enabled && broadcastGraphics.weather && (broadcastGraphics.weather_popup_active || simulateWeatherCard) && !adState.playing}
										<div class="absolute bottom-[13%] left-[2.5%] max-w-[58%] rounded-md bg-[#0d2a42]/95 px-[3%] py-[2%] text-white shadow-xl ring-1 ring-white/15">
											<div class="font-bold leading-tight" style="font-size:clamp(10px,1.5vw,22px)">{broadcastGraphics.config.weather_city} · {Math.round(broadcastGraphics.weather.temperature_c)}°C</div>
											<div class="mt-1 leading-tight opacity-90" style="font-size:clamp(8px,0.9vw,14px)">Humidity {Math.round(broadcastGraphics.weather.humidity_percent)}% · Wind {Math.round(broadcastGraphics.weather.wind_kmh)} km/h</div>
											<div class="leading-tight opacity-90" style="font-size:clamp(8px,0.9vw,14px)">Rain {broadcastGraphics.weather.rainfall_mm} mm</div>
										</div>
									{/if}
									{#if broadcastGraphics.config.sponsor_enabled && broadcastGraphics.config.sponsor_text.trim()}
										<div class="absolute bottom-[3%] right-[2.5%] max-w-[43%] rounded-md bg-[#142236]/95 px-[2%] py-[1.4%] text-right text-white shadow-xl ring-1 ring-white/15">
											<div class="font-medium uppercase tracking-wide text-[#8ed7ff]" style="font-size:clamp(7px,0.75vw,11px)">Sponsored by</div>
											<div class="break-words font-bold leading-tight" style="font-size:clamp(9px,1.15vw,18px)">{broadcastGraphics.config.sponsor_text}</div>
										</div>
								{/if}
								</div>
							{/if}
						</div>
					{:else}
					<div class="scanlines-well flex aspect-video items-center justify-center border-t border-border">
						<span class="text-amber-muted">No source on live</span>
					</div>
				{/if}
				<div class="flex flex-wrap items-center justify-between gap-2 border-t border-border-dim px-2 py-1.5 text-[10px] text-amber-muted">
					<span>Source video + graphics layout preview. Verify final encoded video on YouTube.</span>
					<label class="flex cursor-pointer items-center gap-1"><input type="checkbox" bind:checked={showProgramGraphics}/> Show graphics</label>
					{#if broadcastGraphics?.config.weather_enabled}
						<label class="flex cursor-pointer items-center gap-1"><input type="checkbox" bind:checked={simulateWeatherCard}/> Preview weather</label>
					{/if}
				</div>
			</section>
		</div>


		<!-- Master controls -->
		<section class="panel obs-dock">
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

		<!-- Dedicated server-controlled commercial playout -->
		<section class="panel obs-dock">
			<header class="panel__head">▮ COMMERCIAL BREAK</header>
			<div class="panel__body flex flex-wrap items-center gap-3">
				<label for="commercial-choice" class="label">Full-screen video</label>
				<select id="commercial-choice" bind:value={selectedAdId} disabled={adState.playing || adBusy} class="min-w-0 flex-1 rounded border border-border bg-panel-raised px-3 py-2 text-amber" aria-label="Choose commercial">
					{#if commercialAssets.length === 0}<option value="">Upload an MP4 in Library first</option>{/if}
					{#each commercialAssets as asset (asset.id)}
						<option value={asset.id}>{asset.name}</option>
					{/each}
				</select>
				{#if adState.playing}
					<span class="pill pill--live">● AD ON AIR</span>
					<button class="btn btn--danger" onclick={stopCommercial} disabled={adBusy}>■ Stop Ad / Return Live</button>
				{:else}
					<button class="btn btn--go" onclick={playCommercial} disabled={!$isLive || !selectedAdId || adBusy}>▶ PLAY AD</button>
				{/if}
				<a href="/library" class="text-amber text-xs">Manage Ads</a>
				<a class="obs-automation-link" href="/automation">Weather / Sponsors / Switching / Alerts</a>
				<p class="w-full text-[11px] text-amber-muted">Plays once with its audio, then returns automatically to the previous live camera or scene. The camera stays connected.</p>
			</div>
		</section>

		<!-- Automatic advertisements: countdown and controls are stored in Muxshed, not Chrome -->
		<section class="panel obs-dock">
			<header class="panel__head">◷ AUTOMATIC COMMERCIALS</header>
			<div class="panel__body flex flex-wrap items-center gap-3">
				<label class="flex items-center gap-2 text-sm">
					<input type="checkbox" bind:checked={autoAdsEnabled} aria-label="Enable automatic commercials" />
					Enabled
				</label>
				<details class="w-full rounded-lg border border-border-dim bg-panel-raised px-3 py-2">
					<summary class="cursor-pointer text-sm text-amber-bright">Commercial playlist · {autoAdsPlaylistIds.length} selected · round-robin order</summary>
					<p class="my-2 text-xs text-amber-dim">Select up to 20 Library videos. Use the arrows to choose the order each sponsor plays.</p>
					{#if commercialAssets.length === 0}
						<p class="text-xs text-amber-muted">Upload an MP4 to Library first.</p>
					{:else}
						<div class="max-h-52 space-y-1 overflow-y-auto">
							{#each autoAdsPlaylistIds as id, position (id)}
								{@const asset = commercialAssets.find((a) => a.id === id)}
								{#if asset}
									<div class="flex items-center gap-2 rounded border border-border-dim bg-panel px-2 py-1">
										<span class="w-5 shrink-0 text-xs text-amber-muted">{position + 1}</span>
										<span class="min-w-0 flex-1 truncate text-sm text-amber-bright">{asset.name}</span>
										<button class="btn btn--icon" type="button" onclick={() => movePlaylistChoice(id, -1)} disabled={position === 0} aria-label="Move {asset.name} earlier">↑</button>
										<button class="btn btn--icon" type="button" onclick={() => movePlaylistChoice(id, 1)} disabled={position === autoAdsPlaylistIds.length - 1} aria-label="Move {asset.name} later">↓</button>
										<button class="btn btn--ghost" type="button" onclick={() => setPlaylistChoice(id, false)} aria-label="Remove {asset.name} from playlist">Remove</button>
									</div>
								{/if}
							{/each}
							{#each commercialAssets.filter((a) => !autoAdsPlaylistIds.includes(a.id)) as asset (asset.id)}
								<button class="btn btn--ghost w-full justify-start" type="button" disabled={autoAdsPlaylistIds.length >= 20} onclick={() => setPlaylistChoice(asset.id, true)}>+ Add {asset.name}</button>
							{/each}
						</div>
					{/if}
				</details>
				<label for="automatic-ad-interval" class="label">Every (minutes)</label>
				<input id="automatic-ad-interval" type="number" min="1" max="1440" step="1" bind:value={autoAdsInterval} class="w-20 rounded border border-border bg-panel-raised px-3 py-2 text-amber" />
				<button class="btn btn--go" onclick={saveAutomaticAdSettings} disabled={autoAdsSaving || !autoAdsLoaded || autoAdsInterval < 1 || autoAdsInterval > 1440 || (autoAdsEnabled && autoAdsPlaylistIds.length === 0)}>
					{autoAdsSaving ? 'Saving…' : 'Save Schedule'}
				</button>
				{#if autoAdsStatus?.enabled && autoAdsStatus?.asset_id}
					<span class="pill pill--live">NEXT AD: {nextAdCountdown}</span>
					{#if autoAdsStatus.next_asset_id}
						<span class="text-xs text-amber-dim">Up next: {commercialAssets.find((asset) => asset.id === autoAdsStatus?.next_asset_id)?.name || 'Selected commercial'}</span>
					{/if}
					<button class="btn btn--danger" onclick={skipNextAutomaticAd} disabled={autoAdsStatus?.skip_next}>
						{autoAdsStatus?.skip_next ? 'Next Ad Skipped' : 'Skip Next Ad'}
					</button>
				{/if}
				<p class="w-full text-[11px] text-amber-muted">Ads rotate in the order above on the server at your chosen interval, only while broadcasting. Each video plays once and returns to your previous live camera and audio. Save to start the countdown. The playlist survives server restarts.</p>
			</div>
		</section>

		<!-- Config + Stats side by side under previews -->
		<div class="mb-4 grid grid-cols-2 gap-4">
			<!-- Config (under Program) -->
			<section class="panel">
				<header class="panel__head flex items-center justify-between">
					<span>▮ OUTPUT CONFIG</span>
					<button
						onclick={openOutput}
						class="btn btn--ghost"
						style="min-height:20px;padding:1px 8px;font-size:11px"
						aria-label="Edit output config">Edit</button
					>
				</header>
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
						<div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(200px, 1fr))">
							{#each liveStreamSources() as source (source.id)}
								<div
									class="rounded-sm border p-1.5 transition-colors {source.id === programSourceId
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
									<div class="mt-1.5 flex gap-1.5">
										<button
											onclick={() => { previewSourceId = source.id; }}
											disabled={source.id === programSourceId}
											class="btn flex-1 px-2 py-1 text-[10px] {source.id === previewSourceId ? 'btn--go' : ''}"
										>
											Next Up
										</button>
										<button
											onclick={() => cutToSource(source.id)}
											disabled={source.id === programSourceId}
											class="btn btn--danger flex-1 px-2 py-1 text-[10px]"
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

<Dialog bind:open={showOutput}>
	<div class="panel w-[380px] max-w-[90vw]">
		<header class="panel__head flex items-center justify-between">
			<span>▮ OUTPUT CONFIG</span>
			<button onclick={() => (showOutput = false)} class="text-amber-muted hover:text-amber-bright" aria-label="Close">✕</button>
		</header>
		<div class="panel__body space-y-3">
			{#if $isLive}
				<p class="text-[11px] text-amber-bright">You are live. Saving will briefly reconnect your destinations.</p>
			{/if}
			<div>
				<label class="field-label" for="oc-res">Resolution</label>
				<select id="oc-res" bind:value={resSelect} onchange={onResChange} class="select w-full">
					{#each RES_PRESETS as p}<option value={p.label}>{p.label} ({p.w}x{p.h})</option>{/each}
					<option value="custom">Custom</option>
				</select>
			</div>
			{#if resSelect === 'custom'}
				<div class="grid grid-cols-2 gap-2">
					<div>
						<label class="field-label" for="oc-w">Width</label>
						<input id="oc-w" type="number" min="160" max="3840" bind:value={draft.width} class="input" />
					</div>
					<div>
						<label class="field-label" for="oc-h">Height</label>
						<input id="oc-h" type="number" min="120" max="2160" bind:value={draft.height} class="input" />
					</div>
				</div>
			{/if}
			<div>
				<label class="field-label" for="oc-fps">Frame rate</label>
				<select id="oc-fps" bind:value={draft.fps} class="select w-full">
					{#each [24, 25, 30, 50, 60] as f}<option value={f}>{f} fps</option>{/each}
				</select>
			</div>
			<div>
				<label class="field-label" for="oc-vb">Video bitrate (kbps)</label>
				<input id="oc-vb" type="number" min="500" max="20000" step="100" bind:value={draft.video_bitrate_kbps} class="input" />
			</div>
			<div>
				<label class="field-label" for="oc-ab">Audio bitrate (kbps)</label>
				<select id="oc-ab" bind:value={draft.audio_bitrate_kbps} class="select w-full">
					{#each [96, 128, 160, 192, 256] as a}<option value={a}>{a} kbps</option>{/each}
				</select>
			</div>
			<div class="flex gap-2 pt-1">
				<button onclick={saveOutput} class="btn btn--go">Save</button>
				<button onclick={() => (showOutput = false)} class="btn btn--ghost">Cancel</button>
			</div>
		</div>
	</div>
</Dialog>
