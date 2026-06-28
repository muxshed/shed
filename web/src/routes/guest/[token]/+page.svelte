<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/stores';

	const token = $derived($page.params.token);

	type Mode = 'loading' | 'invalid' | 'ready' | 'joining' | 'live' | 'pending' | 'error';
	let mode = $state<Mode>('loading');
	let guestName = $state('');
	let message = $state('');

	let videoEl = $state<HTMLVideoElement | null>(null);
	let stream = $state<MediaStream | null>(null);
	let pc: RTCPeerConnection | null = null;
	let iceServers: RTCIceServer[] = [{ urls: 'stun:stun.l.google.com:19302' }];

	// device pickers
	let cameras = $state<MediaDeviceInfo[]>([]);
	let mics = $state<MediaDeviceInfo[]>([]);
	let camId = $state('');
	let micId = $state('');

	// Keep the preview wired to the active stream whenever either changes — the
	// <video> only exists once we're past 'loading', so bind reactively.
	$effect(() => {
		if (videoEl && stream) videoEl.srcObject = stream;
	});

	onMount(init);
	onDestroy(teardown);

	async function init() {
		mode = 'loading';
		message = '';
		try {
			const res = await fetch(`/api/v1/guest/${token}`);
			if (!res.ok) {
				mode = 'invalid';
				return;
			}
			const info = await res.json();
			guestName = info.name ?? 'Guest';
			if (Array.isArray(info.ice_servers) && info.ice_servers.length > 0) {
				iceServers = info.ice_servers;
			}
			await openMedia();
			await refreshDevices();
			mode = 'ready';
		} catch (e) {
			message = e instanceof Error ? e.message : 'Could not access camera or microphone.';
			mode = 'error';
		}
	}

	async function openMedia() {
		stream?.getTracks().forEach((t) => t.stop());
		stream = await navigator.mediaDevices.getUserMedia({
			video: camId ? { deviceId: { exact: camId } } : true,
			audio: micId ? { deviceId: { exact: micId } } : true
		});
		// remember which devices we actually got
		camId = stream.getVideoTracks()[0]?.getSettings().deviceId ?? camId;
		micId = stream.getAudioTracks()[0]?.getSettings().deviceId ?? micId;
	}

	async function refreshDevices() {
		const devs = await navigator.mediaDevices.enumerateDevices();
		cameras = devs.filter((d) => d.kind === 'videoinput');
		mics = devs.filter((d) => d.kind === 'audioinput');
	}

	// Switch input. If already connected, swap the track on the live sender.
	async function switchDevice() {
		try {
			await openMedia();
			if (pc) {
				for (const sender of pc.getSenders()) {
					const next =
						sender.track?.kind === 'video'
							? stream?.getVideoTracks()[0]
							: stream?.getAudioTracks()[0];
					if (next) await sender.replaceTrack(next);
				}
			}
		} catch (e) {
			message = e instanceof Error ? e.message : 'Could not switch device.';
		}
	}

	function teardown() {
		stream?.getTracks().forEach((t) => t.stop());
		pc?.close();
		pc = null;
	}

	// WHIP is non-trickle — resolve once ICE gathering finishes (or a short
	// timeout) so the offer SDP we POST already contains our candidates.
	function waitForIceGathering(peer: RTCPeerConnection, timeoutMs = 3000): Promise<void> {
		if (peer.iceGatheringState === 'complete') return Promise.resolve();
		return new Promise((resolve) => {
			const finish = () => {
				peer.removeEventListener('icegatheringstatechange', check);
				clearTimeout(timer);
				resolve();
			};
			const check = () => {
				if (peer.iceGatheringState === 'complete') finish();
			};
			const timer = setTimeout(finish, timeoutMs);
			peer.addEventListener('icegatheringstatechange', check);
		});
	}

	// WHIP publish: send camera/mic to the studio as a WebRTC source.
	async function join() {
		if (!stream) return;
		mode = 'joining';
		message = '';
		try {
			pc = new RTCPeerConnection({ iceServers });
			stream.getTracks().forEach((t) => pc!.addTrack(t, stream!));

			pc.onconnectionstatechange = () => {
				const s = pc?.connectionState;
				if (s === 'connected') {
					mode = 'live';
				} else if (s === 'failed') {
					message = 'Connection failed — check your network, or set a TURN server.';
					mode = 'error';
				} else if ((s === 'disconnected' || s === 'closed') && mode === 'live') {
					mode = 'ready';
				}
			};

			await pc.setLocalDescription(await pc.createOffer());
			// WHIP is non-trickle: wait for ICE gathering so the offer we POST
			// carries our candidates — otherwise the server has nothing to pair with.
			await waitForIceGathering(pc);

			const res = await fetch(`/api/v1/guest/${token}/whip`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/sdp' },
				body: pc.localDescription?.sdp ?? ''
			});

			if (res.status === 503) {
				pc.close();
				pc = null;
				mode = 'pending';
				return;
			}
			if (!res.ok) throw new Error(`server returned ${res.status}`);

			const answer = await res.text();
			await pc.setRemoteDescription({ type: 'answer', sdp: answer });
			// stays 'joining' until onconnectionstatechange reports 'connected'
		} catch (e) {
			message = e instanceof Error ? e.message : 'Could not connect.';
			mode = 'error';
		}
	}

	function disconnect() {
		pc?.close();
		pc = null;
		mode = 'ready';
		message = '';
	}

	const busy = $derived(mode === 'joining');
</script>

<svelte:head>
	<title>Join as guest — Muxshed</title>
</svelte:head>

<div class="flex min-h-screen flex-col items-center justify-center p-4">
	{#if mode === 'loading'}
		<span class="label animate-pulse">Loading…</span>
	{:else if mode === 'invalid'}
		<div class="panel w-full max-w-md text-center">
			<div class="panel__body py-10">
				<p class="mb-2 text-lg tracking-widest text-amber-bright">◉ MUXSHED</p>
				<p class="text-sm text-amber-dim">This guest link is invalid or has expired.</p>
			</div>
		</div>
	{:else}
		<div class="w-full max-w-2xl space-y-4">
			<header class="flex items-center justify-between">
				<span class="text-lg tracking-widest text-amber-bright glow">◉ MUXSHED · GUEST</span>
				{#if mode === 'live'}
					<span class="pill pill--live"><span class="tally">●</span> ON AIR</span>
				{:else if mode === 'joining'}
					<span class="pill pill--idle animate-pulse">○ connecting</span>
				{:else}
					<span class="pill pill--idle">○ {guestName}</span>
				{/if}
			</header>

			<div
				class="scanlines-well relative w-full overflow-hidden rounded-md border border-border"
				style="aspect-ratio: 16 / 9"
			>
				<!-- svelte-ignore a11y_media_has_caption -->
				<video bind:this={videoEl} class="h-full w-full bg-black" autoplay playsinline muted></video>
			</div>

			<!-- device selection -->
			<div class="grid gap-2 sm:grid-cols-2">
				<label class="block">
					<span class="field-label">Camera</span>
					<select class="input" bind:value={camId} onchange={switchDevice}>
						{#each cameras as c, i}
							<option value={c.deviceId}>{c.label || `Camera ${i + 1}`}</option>
						{/each}
					</select>
				</label>
				<label class="block">
					<span class="field-label">Microphone</span>
					<select class="input" bind:value={micId} onchange={switchDevice}>
						{#each mics as m, i}
							<option value={m.deviceId}>{m.label || `Microphone ${i + 1}`}</option>
						{/each}
					</select>
				</label>
			</div>

			{#if mode === 'ready'}
				<button class="btn btn--go w-full" onclick={join}>● Join the broadcast</button>
				<p class="text-center text-[12px] text-amber-muted">
					Pick your camera and mic above, then join when you're ready.
				</p>
			{:else if mode === 'joining'}
				<p class="text-center text-sm text-amber-dim animate-pulse">Connecting…</p>
				<button class="btn w-full" onclick={disconnect}>Cancel</button>
			{:else if mode === 'live'}
				<p class="text-center text-sm text-live-glow">● You are live in the studio.</p>
				<button class="btn btn--danger w-full" onclick={disconnect}>Disconnect</button>
			{:else if mode === 'pending'}
				<div class="panel">
					<div class="panel__body text-center">
						<p class="text-sm text-warning">Guest video ingest isn't enabled on this instance yet.</p>
						<p class="mt-1 text-[12px] text-amber-muted">Your link is valid — the host can see you're here.</p>
					</div>
				</div>
			{:else if mode === 'error'}
				<p class="text-center text-sm text-danger-glow">{message}</p>
				<button class="btn w-full" disabled={busy} onclick={() => (pc ? join() : init())}>Try again</button>
			{/if}
		</div>
	{/if}
</div>
