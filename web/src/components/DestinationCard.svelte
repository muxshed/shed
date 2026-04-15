<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
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
	class="rounded-lg border border-neutral-700 bg-neutral-900 p-4 {destination.enabled
		? ''
		: 'opacity-50'}"
>
	<div class="mb-2 flex items-center justify-between">
		<span class="font-medium">{destination.name}</span>
		<span class="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-400 uppercase">
			{destination.kind.type}
		</span>
	</div>
	<div class="mb-3 truncate text-xs text-neutral-500">{displayUrl()}</div>
	<div class="flex items-center gap-2">
		<button
			onclick={toggleEnabled}
			class="rounded px-3 py-1 text-xs {destination.enabled
				? 'bg-green-900 text-green-400'
				: 'bg-neutral-700 text-neutral-400'} hover:opacity-80"
		>
			{destination.enabled ? 'Enabled' : 'Disabled'}
		</button>
		<button
			onclick={remove}
			class="rounded bg-neutral-700 px-3 py-1 text-xs text-red-400 hover:bg-neutral-600"
		>
			Delete
		</button>
	</div>
</div>
