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
	let stream: MediaStream | null = null;
	let pc: RTCPeerConnection | null = null;
	let iceServers: RTCIceServer[] = [{ urls: 'stun:stun.l.google.com:19302' }];

	onMount(async () => {
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
			await startCamera();
			mode = 'ready';
		} catch {
			mode = 'invalid';
		}
	});

	onDestroy(() => teardown());

	async function startCamera() {
		stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
		if (videoEl) videoEl.srcObject = stream;
	}

	function teardown() {
		stream?.getTracks().forEach((t) => t.stop());
		pc?.close();
		pc = null;
	}

	// WHIP publish: send our camera/mic to the studio as a WebRTC source.
	async function join() {
		if (!stream) return;
		mode = 'joining';
		message = '';
		try {
			pc = new RTCPeerConnection({ iceServers });
			stream.getTracks().forEach((t) => pc!.addTrack(t, stream!));

			const offer = await pc.createOffer();
			await pc.setLocalDescription(offer);

			const res = await fetch(`/api/v1/guest/${token}/whip`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/sdp' },
				body: offer.sdp ?? ''
			});

			if (res.status === 503) {
				// Backend WHIP ingest not enabled yet — handled gracefully.
				pc.close();
				pc = null;
				mode = 'pending';
				return;
			}
			if (!res.ok) {
				throw new Error(`server returned ${res.status}`);
			}

			const answer = await res.text();
			await pc.setRemoteDescription({ type: 'answer', sdp: answer });
			mode = 'live';
		} catch (e) {
			message = e instanceof Error ? e.message : 'Could not connect.';
			mode = 'error';
		}
	}
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
				{:else}
					<span class="pill pill--idle">○ {guestName}</span>
				{/if}
			</header>

			<div class="scanlines-well relative w-full overflow-hidden rounded-md border border-border" style="aspect-ratio: 16 / 9">
				<!-- svelte-ignore a11y_media_has_caption -->
				<video bind:this={videoEl} class="h-full w-full bg-black" autoplay playsinline muted></video>
			</div>

			{#if mode === 'ready'}
				<button class="btn btn--go w-full" onclick={join}>● Join the broadcast</button>
				<p class="text-center text-[12px] text-amber-muted">
					Your camera and mic preview is above. Hit join when you're ready.
				</p>
			{:else if mode === 'joining'}
				<p class="text-center text-sm text-amber-dim animate-pulse">Connecting…</p>
			{:else if mode === 'live'}
				<p class="text-center text-sm text-live-glow">● You are live in the studio.</p>
			{:else if mode === 'pending'}
				<div class="panel"><div class="panel__body text-center">
					<p class="text-sm text-warning">Guest video ingest isn't enabled on this instance yet.</p>
					<p class="mt-1 text-[12px] text-amber-muted">Your link is valid — the host can see you're here.</p>
				</div></div>
			{:else if mode === 'error'}
				<p class="text-center text-sm text-danger-glow">{message}</p>
				<button class="btn w-full" onclick={join}>Try again</button>
			{/if}
		</div>
	{/if}
</div>
