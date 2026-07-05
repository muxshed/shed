<!-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE. -->
<script lang="ts">
	import type { Source } from '$lib/types';
	import { api } from '$lib/api';

	let { source, ondelete }: { source: Source; ondelete: () => void } = $props();
	let copied = $state(false);

	function connectionUrl(): string {
		const host = typeof window !== 'undefined' ? window.location.hostname : 'localhost';
		if (source.kind.type === 'rtmp') {
			return `rtmp://${host}:1935/live/${source.kind.stream_key}`;
		}
		if (source.kind.type === 'srt') {
			let url = `srt://${host}:${source.kind.port}`;
			if (source.kind.passphrase) {
				url += `?passphrase=${source.kind.passphrase}`;
			}
			return url;
		}
		if (source.kind.type === 'web_rtc') {
			const proto = typeof window !== 'undefined' ? window.location.protocol : 'http:';
			const port = typeof window !== 'undefined' && window.location.port ? `:${window.location.port}` : '';
			return `${proto}//${host}${port}/api/v1/whip`;
		}
		return '';
	}

	function stateLabel(): string {
		if (source.state === 'live') return 'Live';
		if (source.state === 'connecting') return 'Waiting';
		return 'Offline';
	}

	function statePill(): string {
		if (source.state === 'live') return 'pill--live';
		if (source.state === 'connecting') return 'pill--warning';
		return 'pill--idle';
	}

	function dotStyle(): string {
		if (source.state === 'live') return 'background: var(--color-live-glow)';
		if (source.state === 'connecting') return 'background: var(--color-warning)';
		return 'background: var(--color-amber-muted)';
	}

	async function copyUrl() {
		await navigator.clipboard.writeText(connectionUrl());
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	function bearerToken(): string {
		return source.kind.type === 'web_rtc' ? source.kind.token : '';
	}

	let copiedToken = $state(false);
	async function copyToken() {
		await navigator.clipboard.writeText(bearerToken());
		copiedToken = true;
		setTimeout(() => (copiedToken = false), 2000);
	}

	async function remove() {
		await api.deleteSource(source.id);
		ondelete();
	}
</script>

<div class="rounded-md border border-border-dim bg-panel-raised p-3">
	<div class="mb-2 flex items-center justify-between gap-2">
		<div class="flex items-center gap-2 min-w-0">
			<span class="h-2 w-2 shrink-0 rounded-full" style={dotStyle()}></span>
			<span class="truncate text-amber">{source.name}</span>
			<span class="pill {statePill()}">{stateLabel()}</span>
		</div>
		<span class="label shrink-0">
			{source.kind.type === 'srt'
				? 'SRT'
				: source.kind.type === 'rtmp'
				? 'RTMP'
				: source.kind.type === 'web_rtc'
				? 'WHIP'
				: source.kind.type}
		</span>
	</div>
	{#if source.kind.type === 'rtmp' || source.kind.type === 'srt' || source.kind.type === 'web_rtc'}
		<div class="scanlines-well mt-2 break-all rounded-sm border border-border p-2 text-xs text-amber-dim">
			{connectionUrl()}
		</div>
		{#if source.kind.type === 'web_rtc'}
			<div class="mt-1 text-[0.65rem] uppercase tracking-wide text-amber-muted">Bearer token</div>
			<div class="scanlines-well mt-1 break-all rounded-sm border border-border p-2 text-xs text-amber-dim">
				{bearerToken()}
			</div>
		{/if}
		<div class="mt-2 flex gap-2">
			<button onclick={copyUrl} class="btn btn--ghost">
				{copied ? '✓ Copied' : 'Copy URL'}
			</button>
			{#if source.kind.type === 'web_rtc'}
				<button onclick={copyToken} class="btn btn--ghost">
					{copiedToken ? '✓ Copied' : 'Copy Token'}
				</button>
			{/if}
			<button onclick={remove} class="btn btn--danger">
				Delete
			</button>
		</div>
	{:else}
		<div class="mt-2 flex gap-2">
			<button onclick={remove} class="btn btn--danger">
				Delete
			</button>
		</div>
	{/if}
</div>
