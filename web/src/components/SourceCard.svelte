<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
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
		return '';
	}

	function stateLabel(): string {
		if (source.state === 'live') return 'Live';
		if (source.state === 'connecting') return 'Waiting';
		return 'Offline';
	}

	function stateColor(): string {
		if (source.state === 'live') return 'bg-green-500';
		if (source.state === 'connecting') return 'bg-yellow-500 animate-pulse';
		return 'bg-neutral-600';
	}

	async function copyUrl() {
		await navigator.clipboard.writeText(connectionUrl());
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	async function remove() {
		await api.deleteSource(source.id);
		ondelete();
	}
</script>

<div class="rounded-lg border border-neutral-700 bg-neutral-900 p-4">
	<div class="mb-2 flex items-center justify-between">
		<div class="flex items-center gap-2">
			<span class="h-2 w-2 rounded-full {stateColor()}"></span>
			<span class="font-medium">{source.name}</span>
			<span class="text-xs text-neutral-500">{stateLabel()}</span>
		</div>
		<span class="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-400 uppercase">
			{source.kind.type === 'srt' ? 'SRT' : source.kind.type === 'rtmp' ? 'RTMP' : source.kind.type}
		</span>
	</div>
	{#if source.kind.type === 'rtmp' || source.kind.type === 'srt'}
		<div class="mt-2 rounded bg-neutral-800 p-2 text-xs font-mono text-neutral-300 break-all">
			{connectionUrl()}
		</div>
		<div class="mt-2 flex gap-2">
			<button
				onclick={copyUrl}
				class="rounded bg-neutral-700 px-3 py-1 text-xs text-neutral-300 hover:bg-neutral-600"
			>
				{copied ? 'Copied' : 'Copy URL'}
			</button>
			<button
				onclick={remove}
				class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
			>
				Delete
			</button>
		</div>
	{:else}
		<div class="mt-2 flex gap-2">
			<button
				onclick={remove}
				class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
			>
				Delete
			</button>
		</div>
	{/if}
</div>
