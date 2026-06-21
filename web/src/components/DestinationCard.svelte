<!-- Licensed under the Business Source License 1.1 — see LICENSE. -->
<script lang="ts">
	import type { Destination } from '$lib/types';
	import { api } from '$lib/api';

	let { destination, onupdate }: { destination: Destination; onupdate: () => void } = $props();

	function displayUrl(): string {
		if (destination.kind.type === 'rtmp' || destination.kind.type === 'rtmps') {
			return destination.kind.url;
		}
		if (destination.kind.type === 'srt') return destination.kind.url;
		return '';
	}

	async function toggleEnabled() {
		if (destination.enabled) {
			await api.disableDestination(destination.id);
		} else {
			await api.enableDestination(destination.id);
		}
		onupdate();
	}

	async function remove() {
		await api.deleteDestination(destination.id);
		onupdate();
	}
</script>

<div
	class="rounded-md border border-border-dim bg-panel-raised p-3 {destination.enabled
		? ''
		: 'opacity-50'}"
>
	<div class="mb-2 flex items-center justify-between gap-2">
		<span class="truncate text-amber">{destination.name}</span>
		<span class="label shrink-0">
			{destination.kind.type}
		</span>
	</div>
	<div class="mb-3 truncate text-xs text-amber-dim">{displayUrl()}</div>
	<div class="flex items-center gap-2">
		<button
			onclick={toggleEnabled}
			class="pill {destination.enabled ? 'pill--live' : 'pill--idle'} cursor-pointer"
		>
			{destination.enabled ? '● Enabled' : '○ Disabled'}
		</button>
		<button onclick={remove} class="btn btn--danger">
			Delete
		</button>
	</div>
</div>
