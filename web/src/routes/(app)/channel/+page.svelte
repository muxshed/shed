<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { notify } from '$lib/notify';
	import type { ChannelConfig } from '$lib/types';

	let channel = $state<ChannelConfig | null>(null);
	let loading = $state(true);

	// Editable fields
	let title = $state('');
	let accent = $state('#FFB000');
	let savingTitle = $state(false);
	let togglingEnabled = $state(false);

	// Logo
	let logoInput = $state<HTMLInputElement | null>(null);
	let uploadingLogo = $state(false);

	// Password
	let password = $state('');
	let savingPassword = $state(false);

	// Token regen
	let regenerating = $state(false);

	// Share link copy
	let copied = $state(false);

	const watchUrl = $derived(
		channel ? `${typeof location !== 'undefined' ? location.origin : ''}${channel.watch_path}` : '',
	);

	onMount(load);

	async function load() {
		loading = true;
		try {
			const c = await api.getChannel();
			channel = c;
			title = c.title;
			accent = c.accent ?? '#FFB000';
		} catch (e) {
			notify.error(e);
		} finally {
			loading = false;
		}
	}

	async function toggleEnabled() {
		if (!channel) return;
		togglingEnabled = true;
		try {
			channel = await api.updateChannel({ enabled: !channel.enabled });
			notify.success(channel.enabled ? 'Channel enabled' : 'Channel disabled');
		} catch (e) {
			notify.error(e);
		} finally {
			togglingEnabled = false;
		}
	}

	async function saveBranding() {
		if (!channel) return;
		savingTitle = true;
		try {
			channel = await api.updateChannel({ title: title.trim(), accent: accent || null });
			notify.success('Branding saved');
		} catch (e) {
			notify.error(e);
		} finally {
			savingTitle = false;
		}
	}

	async function onLogoSelected(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		uploadingLogo = true;
		try {
			const formData = new FormData();
			formData.append('file', file);
			const res = await api.uploadChannelLogo(formData);
			if (channel) channel = { ...channel, logo_url: res.logo_url };
			notify.success('Logo uploaded');
		} catch (err) {
			notify.error(err);
		} finally {
			uploadingLogo = false;
			if (logoInput) logoInput.value = '';
		}
	}

	async function removeLogo() {
		try {
			await api.deleteChannelLogo();
			if (channel) channel = { ...channel, logo_url: null };
			notify.success('Logo removed');
		} catch (e) {
			notify.error(e);
		}
	}

	async function savePassword() {
		savingPassword = true;
		try {
			const value = password.trim() || null;
			await api.setChannelPassword(value);
			if (channel) channel = { ...channel, password_protected: value !== null };
			password = '';
			notify.success(value ? 'Viewer password set' : 'Viewer password cleared');
		} catch (e) {
			notify.error(e);
		} finally {
			savingPassword = false;
		}
	}

	async function clearPassword() {
		savingPassword = true;
		try {
			await api.setChannelPassword(null);
			if (channel) channel = { ...channel, password_protected: false };
			password = '';
			notify.success('Viewer password cleared');
		} catch (e) {
			notify.error(e);
		} finally {
			savingPassword = false;
		}
	}

	async function copyLink() {
		if (!watchUrl) return;
		try {
			await navigator.clipboard.writeText(watchUrl);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch (e) {
			notify.error(e);
		}
	}

	async function regenerate() {
		if (!confirm('Regenerate the watch link? The old link will stop working immediately.')) return;
		regenerating = true;
		try {
			channel = await api.regenerateChannelToken();
			notify.success('New watch link generated');
		} catch (e) {
			notify.error(e);
		} finally {
			regenerating = false;
		}
	}
</script>

<div class="mx-auto max-w-2xl space-y-4">
	{#if loading}
		<p class="label animate-pulse">Loading channel…</p>
	{:else if channel}
		<!-- PUBLIC CHANNEL -->
		<section class="panel">
			<header class="panel__head flex items-center justify-between">
				<span>▮ PUBLIC CHANNEL</span>
				<span class="pill {channel.enabled ? 'pill--live' : 'pill--idle'}">
					{channel.enabled ? '● LIVE' : '○ OFFLINE'}
				</span>
			</header>
			<div class="panel__body space-y-4">
				<div class="row">
					<div>
						<span class="text-amber">Public channel</span>
						<p class="text-xs text-amber-dim">Serve a publicly-watchable HLS stream at an unlisted link.</p>
					</div>
					<button
						onclick={toggleEnabled}
						disabled={togglingEnabled}
						class="btn {channel.enabled ? 'btn--go' : ''}"
					>
						{channel.enabled ? '● On' : '○ Off'}
					</button>
				</div>

				<form onsubmit={(e) => { e.preventDefault(); saveBranding(); }} class="space-y-3">
					<div>
						<label class="field-label" for="ch-title">Title</label>
						<input id="ch-title" bind:value={title} placeholder="e.g. My Channel" class="input" />
					</div>
					<div>
						<label class="field-label" for="ch-accent">Accent color</label>
						<div class="flex items-center gap-2">
							<input
								id="ch-accent"
								bind:value={accent}
								type="color"
								class="h-8 w-10 cursor-pointer rounded-sm border border-border bg-well"
								aria-label="Accent color picker"
							/>
							<input
								bind:value={accent}
								placeholder="#FFB000"
								class="input font-mono"
								aria-label="Accent color hex value"
							/>
						</div>
					</div>
					<button type="submit" disabled={savingTitle} class="btn">
						{savingTitle ? 'Saving…' : 'Save branding'}
					</button>
				</form>

				<!-- Logo -->
				<div class="space-y-2">
					<span class="field-label">Logo</span>
					{#if channel.logo_url}
						<div class="row">
							<img src={channel.logo_url} alt="Channel logo" class="max-h-12 max-w-[160px] object-contain" />
							<button onclick={removeLogo} class="btn btn--danger">Remove</button>
						</div>
					{/if}
					<input
						bind:this={logoInput}
						onchange={onLogoSelected}
						type="file"
						accept="image/*"
						class="hidden"
						aria-label="Upload channel logo"
					/>
					<button onclick={() => logoInput?.click()} disabled={uploadingLogo} class="btn btn--ghost">
						{uploadingLogo ? 'Uploading…' : channel.logo_url ? 'Replace logo' : '+ Upload logo'}
					</button>
				</div>

				<!-- Viewer password -->
				<div class="space-y-2">
					<div class="flex items-center justify-between">
						<span class="field-label mb-0">Viewer password</span>
						{#if channel.password_protected}
							<span class="pill pill--warning">PROTECTED</span>
						{:else}
							<span class="pill pill--idle">OPEN</span>
						{/if}
					</div>
					<p class="text-xs text-amber-dim">Optional. Viewers must enter this before the stream plays.</p>
					<div class="flex gap-2">
						<input
							bind:value={password}
							type="password"
							autocomplete="new-password"
							placeholder={channel.password_protected ? '•••••••• (set)' : 'No password'}
							class="input"
						/>
						<button onclick={savePassword} disabled={savingPassword || !password.trim()} class="btn">
							Set
						</button>
						{#if channel.password_protected}
							<button onclick={clearPassword} disabled={savingPassword} class="btn btn--danger">
								Clear
							</button>
						{/if}
					</div>
				</div>
			</div>
		</section>

		<!-- SHARE LINK -->
		<section class="panel">
			<header class="panel__head">▮ SHARE LINK</header>
			<div class="panel__body space-y-3">
				<div>
					<span class="field-label">Watch URL</span>
					<div class="flex items-center gap-2">
						<code
							class="scanlines-well flex-1 overflow-x-auto whitespace-nowrap rounded-sm border border-border px-3 py-2 text-xs text-amber-bright"
						>
							{watchUrl}
						</code>
						<button onclick={copyLink} class="btn btn--icon" aria-label="Copy watch link" title="Copy watch link">
							{copied ? '✓' : '⧉'}
						</button>
					</div>
				</div>

				<div class="flex flex-wrap gap-2">
					<a href={watchUrl} target="_blank" rel="noopener noreferrer" class="btn btn--ghost">
						Open watch page ▸
					</a>
					<button onclick={regenerate} disabled={regenerating} class="btn btn--danger">
						{regenerating ? 'Regenerating…' : 'Regenerate link'}
					</button>
				</div>
				<p class="text-[11px] text-warning">
					Regenerating breaks the existing link — anyone using the old URL will lose access.
				</p>

				<div class="rounded-sm border border-border-dim bg-panel-raised p-3 text-xs text-amber-dim">
					<p>
						Video only plays while you are streaming. The public HLS output requires the
						GStreamer / Docker build of Muxshed.
					</p>
				</div>
			</div>
		</section>
	{/if}
</div>
