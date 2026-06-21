<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import Hls from 'hls.js';
	import type { ChannelInfo } from '$lib/types';

	const token = $derived($page.params.token);

	let info = $state<ChannelInfo | null>(null);
	let loadError = $state<string | null>(null);
	let loading = $state(true);

	// Playback state
	let unlocked = $state(false);
	let videoEl = $state<HTMLVideoElement | null>(null);
	let hls: Hls | null = null;
	let streamOffline = $state(false);
	let attaching = $state(false);
	let playing = $state(false);

	// Custom player controls
	let playerWell = $state<HTMLDivElement | null>(null);
	let paused = $state(true);
	let muted = $state(false);
	let curTime = $state(0);
	let duration = $state(0);
	let isLive = $state(true);
	let isFullscreen = $state(false);

	// Password form
	let password = $state('');
	let unlocking = $state(false);
	let passwordError = $state<string | null>(null);

	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const accent = $derived(info?.accent || 'var(--color-amber)');
	const playlistUrl = $derived(`/api/v1/public/channel/${token}/index.m3u8`);

	onMount(() => {
		fetchInfo(true);
		const onFs = () => (isFullscreen = !!document.fullscreenElement);
		document.addEventListener('fullscreenchange', onFs);
		return () => {
			stopPolling();
			document.removeEventListener('fullscreenchange', onFs);
		};
	});

	onDestroy(() => {
		destroyHls();
		stopPolling();
	});

	async function fetchInfo(initial = false) {
		try {
			const res = await fetch(`/api/v1/public/channel/${token}`);
			if (!res.ok) {
				if (res.status === 404) {
					loadError = 'This channel does not exist or is not available.';
				} else {
					loadError = 'Unable to load channel.';
				}
				info = null;
				return;
			}
			const data: ChannelInfo = await res.json();
			info = data;
			loadError = null;

			if (initial && !data.requires_password) {
				unlocked = true;
			}

			if (data.live) {
				// Live: (re)attach whenever we're allowed to watch but aren't playing yet.
				// Retried on every poll so a failed first attempt (segments not ready) recovers.
				if (unlocked && !playing) attachPlayer();
			} else {
				// Stream went/is offline — tear down and wait for it to return.
				playing = false;
				destroyHls();
				streamOffline = true;
				startPolling();
			}
		} catch {
			loadError = 'Network error loading channel.';
		} finally {
			loading = false;
		}
	}

	async function submitPassword(e: Event) {
		e.preventDefault();
		if (!password.trim()) return;
		unlocking = true;
		passwordError = null;
		try {
			const res = await fetch(`/api/v1/public/channel/${token}/unlock`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				credentials: 'include',
				body: JSON.stringify({ password }),
			});
			if (res.status === 401) {
				passwordError = 'Incorrect password.';
				return;
			}
			if (!res.ok) {
				passwordError = 'Unable to unlock the stream.';
				return;
			}
			unlocked = true;
			password = '';
			attachPlayer();
		} catch {
			passwordError = 'Network error.';
		} finally {
			unlocking = false;
		}
	}

	function destroyHls() {
		if (hls) {
			hls.destroy();
			hls = null;
		}
	}

	function startPolling() {
		if (pollTimer) return;
		pollTimer = setInterval(() => fetchInfo(false), 4000);
	}

	function stopPolling() {
		if (pollTimer) {
			clearInterval(pollTimer);
			pollTimer = null;
		}
	}

	// Attach hls.js (or native HLS) to the video element and load the playlist.
	// `attaching` stays true for the whole async load so polls don't stack attempts;
	// it clears on success (onReady) or failure (handleOffline).
	function attachPlayer() {
		if (!unlocked || attaching || playing) return;
		const el = videoEl;
		if (!el) {
			// video element not yet mounted; retry shortly
			setTimeout(attachPlayer, 50);
			return;
		}
		attaching = true;
		streamOffline = false;
		destroyHls();

		const onReady = () => {
			attaching = false;
			playing = true;
			streamOffline = false;
			stopPolling();
			el.play().catch(() => {});
		};

		if (Hls.isSupported()) {
			hls = new Hls({ xhrSetup: (xhr) => { xhr.withCredentials = true; } });
			hls.on(Hls.Events.MANIFEST_PARSED, onReady);
			hls.on(Hls.Events.LEVEL_LOADED, (_e, data) => {
				isLive = data.details.live;
			});
			hls.on(Hls.Events.ERROR, (_evt, data) => {
				if (data.fatal) handleOffline();
			});
			hls.loadSource(playlistUrl);
			hls.attachMedia(el);
		} else if (el.canPlayType('application/vnd.apple.mpegurl')) {
			// Native HLS (Safari) — cookies are sent same-origin automatically.
			el.src = playlistUrl;
			el.addEventListener(
				'loadedmetadata',
				() => {
					isLive = !isFinite(el.duration);
					onReady();
				},
				{ once: true },
			);
			el.addEventListener('error', handleOffline, { once: true });
		} else {
			attaching = false;
			streamOffline = true;
		}
	}

	// Attach failed or the stream dropped — show the offline state and poll until it
	// returns. The poll re-attaches once segments are available again.
	function handleOffline() {
		destroyHls();
		attaching = false;
		playing = false;
		streamOffline = true;
		startPolling();
	}

	// --- Player controls ---
	function togglePlay() {
		const el = videoEl;
		if (!el) return;
		if (el.paused) el.play().catch(() => {});
		else el.pause();
	}

	function restart() {
		const el = videoEl;
		if (!el) return;
		el.currentTime = 0;
		el.play().catch(() => {});
	}

	function toggleMute() {
		muted = !muted;
	}

	function seek(e: MouseEvent) {
		const el = videoEl;
		if (!el || !duration || !isFinite(duration)) return;
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const frac = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
		el.currentTime = frac * duration;
	}

	function toggleFullscreen() {
		if (document.fullscreenElement) {
			document.exitFullscreen().catch(() => {});
		} else {
			playerWell?.requestFullscreen().catch(() => {});
		}
	}

	function fmt(s: number): string {
		if (!isFinite(s) || s < 0) return '0:00';
		const m = Math.floor(s / 60);
		const sec = Math.floor(s % 60);
		return `${m}:${String(sec).padStart(2, '0')}`;
	}
</script>

<div class="flex min-h-screen flex-col items-center justify-center p-4">
	{#if loading}
		<span class="label animate-pulse">Loading…</span>
	{:else if loadError}
		<div class="panel w-full max-w-md text-center">
			<div class="panel__body py-10">
				<p class="mb-2 text-lg tracking-widest text-amber-bright">◉ MUXSHED</p>
				<p class="text-sm text-amber-dim">{loadError}</p>
			</div>
		</div>
	{:else if info}
		<div class="w-full max-w-3xl space-y-4">
			<!-- Branding header -->
			<header class="flex flex-col items-center gap-3 text-center">
				{#if info.logo_url}
					<img src={info.logo_url} alt="{info.title} logo" class="max-h-20 max-w-[260px] object-contain" />
				{:else}
					<p class="text-2xl tracking-widest" style="color: {accent}">◉ {info.title}</p>
				{/if}
				{#if info.logo_url}
					<h1 class="text-lg tracking-widest" style="color: {accent}">{info.title}</h1>
				{/if}
				{#if info.live}
					<span class="pill pill--live">● ON AIR</span>
				{:else}
					<span class="pill pill--idle">○ OFFLINE</span>
				{/if}
			</header>

			<!-- Player / states -->
			<div
				bind:this={playerWell}
				class="scanlines-well relative w-full overflow-hidden rounded-md border border-border"
				style="aspect-ratio: 16 / 9"
			>
				{#if info.requires_password && !unlocked}
					<!-- Password gate -->
					<div class="absolute inset-0 flex flex-col items-center justify-center gap-3 p-6">
						<p class="label">This stream is password protected</p>
						<form onsubmit={submitPassword} class="flex w-full max-w-xs flex-col gap-2">
							<label class="field-label sr-only" for="watch-pw">Viewer password</label>
							<input
								id="watch-pw"
								bind:value={password}
								type="password"
								autocomplete="off"
								placeholder="Enter password"
								class="input text-center"
							/>
							{#if passwordError}
								<p class="text-center text-xs text-danger">{passwordError}</p>
							{/if}
							<button type="submit" disabled={unlocking || !password.trim()} class="btn">
								{unlocking ? 'Unlocking…' : 'Watch'}
							</button>
						</form>
					</div>
				{:else if streamOffline}
					<!-- Offline empty state -->
					<div class="absolute inset-0 flex flex-col items-center justify-center gap-2 text-center">
						<p class="text-lg tracking-widest text-amber-muted">○ STREAM OFFLINE</p>
						<p class="text-xs text-amber-dim">Waiting for the broadcast to start…</p>
					</div>
				{/if}

				<!-- Video element (kept mounted once unlocked so hls.js can attach) -->
				<!-- svelte-ignore a11y_media_has_caption -->
				<video
					bind:this={videoEl}
					bind:paused
					bind:muted
					bind:currentTime={curTime}
					bind:duration
					class="h-full w-full bg-black {!unlocked || streamOffline ? 'invisible' : ''}"
					playsinline
					onclick={togglePlay}
					aria-label="{info.title} stream"
				>
					<track kind="captions" label="Captions" />
				</video>

				{#if unlocked && !streamOffline}
					<!-- Custom player controls -->
					<div class="player-bar">
						{#if !isLive && duration > 0}
							<button class="seek" onclick={seek} aria-label="Seek">
								<span class="seek-fill" style="width: {(curTime / duration) * 100}%"></span>
							</button>
						{/if}
						<div class="player-row">
							<button class="pbtn" onclick={togglePlay} aria-label={paused ? 'Play' : 'Pause'}>
								{paused ? '▶' : '▮▮'}
							</button>
							{#if !isLive}
								<button class="pbtn" onclick={restart} aria-label="Restart">↺</button>
								<span class="ptime">{fmt(curTime)} / {fmt(duration)}</span>
							{:else}
								<span class="pill pill--live">● LIVE</span>
							{/if}
							<span class="grow"></span>
							<button class="pbtn" onclick={toggleMute} aria-label={muted ? 'Unmute' : 'Mute'}>
								{muted ? 'MUTED' : 'VOL'}
							</button>
							<button class="pbtn" onclick={toggleFullscreen} aria-label="Toggle fullscreen">
								{isFullscreen ? '><' : '[ ]'}
							</button>
						</div>
					</div>
				{/if}
			</div>

			<p class="text-center text-[11px] text-amber-muted">Powered by Muxshed</p>
		</div>
	{/if}
</div>

<style>
	.player-bar {
		position: absolute;
		inset-inline: 0;
		bottom: 0;
		background: rgba(10, 7, 3, 0.72);
		border-top: 1px solid var(--color-border-dim);
	}
	.player-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 9px;
	}
	.grow {
		flex: 1;
	}
	.pbtn {
		min-width: 30px;
		height: 28px;
		padding: 0 7px;
		font-family: var(--font-mono);
		font-size: 12px;
		letter-spacing: 1px;
		color: var(--color-amber);
		background: rgba(27, 20, 10, 0.7);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: color 0.12s, border-color 0.12s;
	}
	.pbtn:hover {
		color: var(--color-amber-bright);
		border-color: var(--color-amber-bright);
	}
	.ptime {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--color-amber-dim);
	}
	.seek {
		display: block;
		width: 100%;
		height: 6px;
		padding: 0;
		background: var(--color-border-dim);
		border: none;
		cursor: pointer;
	}
	.seek-fill {
		display: block;
		height: 100%;
		background: var(--color-amber);
	}
</style>
